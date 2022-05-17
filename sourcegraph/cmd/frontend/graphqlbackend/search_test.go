package graphqlbackend

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http/httptest"
	"reflect"
	"strings"
	"testing"

	"github.com/google/zoekt"
	"github.com/google/zoekt/web"
	"github.com/graph-gophers/graphql-go"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/search/backend"
	searchbackend "github.com/sourcegraph/sourcegraph/internal/search/backend"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	searchrepos "github.com/sourcegraph/sourcegraph/internal/search/repos"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/run"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestSearch(t *testing.T) {
	type Results struct {
		Results    []any
		MatchCount int
	}
	tcs := []struct {
		name                         string
		searchQuery                  string
		searchVersion                string
		reposListMock                func(v0 context.Context, v1 database.ReposListOptions) ([]*types.Repo, error)
		repoRevsMock                 func(spec string, opt gitserver.ResolveRevisionOptions) (api.CommitID, error)
		externalServicesListMock     func(_ context.Context, opt database.ExternalServicesListOptions) ([]*types.ExternalService, error)
		phabricatorGetRepoByNameMock func(_ context.Context, repo api.RepoName) (*types.PhabricatorRepo, error)
		wantResults                  Results
	}{
		{
			name:        "empty query against no repos gets no results",
			searchQuery: "",
			reposListMock: func(v0 context.Context, v1 database.ReposListOptions) ([]*types.Repo, error) {
				return nil, nil
			},
			repoRevsMock: func(spec string, opt gitserver.ResolveRevisionOptions) (api.CommitID, error) {
				return "", nil
			},
			externalServicesListMock: func(_ context.Context, opt database.ExternalServicesListOptions) ([]*types.ExternalService, error) {
				return nil, nil
			},
			phabricatorGetRepoByNameMock: func(_ context.Context, repo api.RepoName) (*types.PhabricatorRepo, error) {
				return nil, nil
			},
			wantResults: Results{
				Results:    nil,
				MatchCount: 0,
			},
			searchVersion: "V1",
		},
		{
			name:        "empty query against empty repo gets no results",
			searchQuery: "",
			reposListMock: func(v0 context.Context, v1 database.ReposListOptions) ([]*types.Repo, error) {
				return []*types.Repo{{Name: "test"}},

					nil
			},
			repoRevsMock: func(spec string, opt gitserver.ResolveRevisionOptions) (api.CommitID, error) {
				return "", nil
			},
			externalServicesListMock: func(_ context.Context, opt database.ExternalServicesListOptions) ([]*types.ExternalService, error) {
				return nil, nil
			},
			phabricatorGetRepoByNameMock: func(_ context.Context, repo api.RepoName) (*types.PhabricatorRepo, error) {
				return nil, nil
			},
			wantResults: Results{
				Results:    nil,
				MatchCount: 0,
			},
			searchVersion: "V1",
		},
	}
	for _, tc := range tcs {
		t.Run(tc.name, func(t *testing.T) {
			conf.Mock(&conf.Unified{})
			defer conf.Mock(nil)
			vars := map[string]any{"query": tc.searchQuery, "version": tc.searchVersion}

			MockDecodedViewerFinalSettings = &schema.Settings{}
			defer func() { MockDecodedViewerFinalSettings = nil }()

			repos := database.NewMockRepoStore()
			repos.ListFunc.SetDefaultHook(tc.reposListMock)

			ext := database.NewMockExternalServiceStore()
			ext.ListFunc.SetDefaultHook(tc.externalServicesListMock)

			phabricator := database.NewMockPhabricatorStore()
			phabricator.GetByNameFunc.SetDefaultHook(tc.phabricatorGetRepoByNameMock)

			db := database.NewMockDB()
			db.ReposFunc.SetDefaultReturn(repos)
			db.ExternalServicesFunc.SetDefaultReturn(ext)
			db.PhabricatorFunc.SetDefaultReturn(phabricator)

			sr := &schemaResolver{db: db}
			schema, err := graphql.ParseSchema(mainSchema, sr, graphql.Tracer(&prometheusTracer{}))
			if err != nil {
				t.Fatal(err)
			}

			gitserver.Mocks.ResolveRevision = tc.repoRevsMock
			result := schema.Exec(context.Background(), testSearchGQLQuery, "", vars)
			if len(result.Errors) > 0 {
				t.Fatalf("graphQL query returned errors: %+v", result.Errors)
			}
			var search struct {
				Results Results
			}
			if err := json.Unmarshal(result.Data, &search); err != nil {
				t.Fatalf("parsing JSON response: %v", err)
			}
			gotResults := search.Results
			if !reflect.DeepEqual(gotResults, tc.wantResults) {
				t.Fatalf("results = %+v, want %+v", gotResults, tc.wantResults)
			}
		})
	}
}

var testSearchGQLQuery = `
		fragment FileMatchFields on FileMatch {
			repository {
				name
				url
			}
			file {
				name
				path
				url
				commit {
					oid
				}
			}
			lineMatches {
				preview
				lineNumber
				offsetAndLengths
			}
		}

		fragment CommitSearchResultFields on CommitSearchResult {
			messagePreview {
				value
				highlights{
					line
					character
					length
				}
			}
			diffPreview {
				value
				highlights {
					line
					character
					length
				}
			}
			label {
				html
			}
			url
			matches {
				url
				body {
					html
					text
				}
				highlights {
					character
					line
					length
				}
			}
			commit {
				repository {
					name
				}
				oid
				url
				subject
				author {
					date
					person {
						displayName
					}
				}
			}
		}

		fragment RepositoryFields on Repository {
			name
			url
			externalURLs {
				serviceKind
				url
			}
			label {
				html
			}
		}

		query ($query: String!, $version: SearchVersion!, $patternType: SearchPatternType) {
			site {
				buildVersion
			}
			search(query: $query, version: $version, patternType: $patternType) {
				results {
					results{
						__typename
						... on FileMatch {
						...FileMatchFields
					}
						... on CommitSearchResult {
						...CommitSearchResultFields
					}
						... on Repository {
						...RepositoryFields
					}
					}
					limitHit
					cloning {
						name
					}
					missing {
						name
					}
					timedout {
						name
					}
					matchCount
					elapsedMilliseconds
				}
			}
		}
`

func TestExactlyOneRepo(t *testing.T) {
	cases := []struct {
		repoFilters []string
		want        bool
	}{
		{
			repoFilters: []string{`^github\.com/sourcegraph/zoekt$`},
			want:        true,
		},
		{
			repoFilters: []string{`^github\.com/sourcegraph/zoekt$@ef3ec23`},
			want:        true,
		},
		{
			repoFilters: []string{`^github\.com/sourcegraph/zoekt$@ef3ec23:deadbeef`},
			want:        true,
		},
		{
			repoFilters: []string{`^.*$`},
			want:        false,
		},

		{
			repoFilters: []string{`^github\.com/sourcegraph/zoekt`},
			want:        false,
		},
		{
			repoFilters: []string{`^github\.com/sourcegraph/zoekt$`, `github\.com/sourcegraph/sourcegraph`},
			want:        false,
		},
	}
	for _, c := range cases {
		t.Run("exactly one repo", func(t *testing.T) {
			if got := searchrepos.ExactlyOneRepo(c.repoFilters); got != c.want {
				t.Errorf("got %t, want %t", got, c.want)
			}
		})
	}
}

func mkFileMatch(repo types.MinimalRepo, path string, lineNumbers ...int32) *result.FileMatch {
	var lines []*result.LineMatch
	for _, n := range lineNumbers {
		lines = append(lines, &result.LineMatch{LineNumber: n})
	}
	return &result.FileMatch{
		File: result.File{
			Path: path,
			Repo: repo,
		},
		LineMatches: lines,
	}
}

func BenchmarkSearchResults(b *testing.B) {
	minimalRepos, zoektRepos := generateRepos(500_000)
	zoektFileMatches := generateZoektMatches(1000)

	z := zoektRPC(b, &searchbackend.FakeSearcher{
		Repos:  zoektRepos,
		Result: &zoekt.SearchResult{Files: zoektFileMatches},
	})

	ctx := context.Background()
	db := database.NewMockDB()

	repos := database.NewMockRepoStore()
	repos.ListMinimalReposFunc.SetDefaultReturn(minimalRepos, nil)
	repos.CountFunc.SetDefaultReturn(len(minimalRepos), nil)
	db.ReposFunc.SetDefaultReturn(repos)

	b.ResetTimer()
	b.ReportAllocs()

	for n := 0; n < b.N; n++ {
		plan, err := query.Pipeline(query.InitLiteral(`print repo:foo index:only count:1000`))
		if err != nil {
			b.Fatal(err)
		}
		resolver := &searchResolver{
			db: db,
			SearchInputs: &run.SearchInputs{
				Plan:         plan,
				Query:        plan.ToQ(),
				UserSettings: &schema.Settings{},
			},
			zoekt: z,
		}
		results, err := resolver.Results(ctx)
		if err != nil {
			b.Fatal("Results:", err)
		}
		if int(results.MatchCount()) != len(zoektFileMatches) {
			b.Fatalf("wrong results length. want=%d, have=%d\n", len(zoektFileMatches), results.MatchCount())
		}
	}
}

func generateRepos(count int) ([]types.MinimalRepo, []*zoekt.RepoListEntry) {
	repos := make([]types.MinimalRepo, 0, count)
	zoektRepos := make([]*zoekt.RepoListEntry, 0, count)

	for i := 1; i <= count; i++ {
		name := fmt.Sprintf("repo-%d", i)

		repoWithIDs := types.MinimalRepo{
			ID:   api.RepoID(i),
			Name: api.RepoName(name),
		}

		repos = append(repos, repoWithIDs)

		zoektRepos = append(zoektRepos, &zoekt.RepoListEntry{
			Repository: zoekt.Repository{
				ID:       uint32(i),
				Name:     name,
				Branches: []zoekt.RepositoryBranch{{Name: "HEAD", Version: "deadbeef"}},
			},
		})
	}
	return repos, zoektRepos
}

func generateZoektMatches(count int) []zoekt.FileMatch {
	var zoektFileMatches []zoekt.FileMatch
	for i := 1; i <= count; i++ {
		repoName := fmt.Sprintf("repo-%d", i)
		fileName := fmt.Sprintf("foobar-%d.go", i)

		zoektFileMatches = append(zoektFileMatches, zoekt.FileMatch{
			Score:        5.0,
			FileName:     fileName,
			RepositoryID: uint32(i),
			Repository:   repoName, // Important: this needs to match a name in `repos`
			Branches:     []string{"master"},
			LineMatches: []zoekt.LineMatch{
				{
					Line: nil,
				},
			},
			Checksum: []byte{0, 1, 2},
		})
	}
	return zoektFileMatches
}

// zoektRPC starts zoekts rpc interface and returns a client to
// searcher. Useful for capturing CPU/memory usage when benchmarking the zoekt
// client.
func zoektRPC(t testing.TB, s zoekt.Streamer) zoekt.Streamer {
	srv, err := web.NewMux(&web.Server{
		Searcher: s,
		RPC:      true,
		Top:      web.Top,
	})
	if err != nil {
		t.Fatal(err)
	}
	ts := httptest.NewServer(srv)
	cl := backend.ZoektDial(strings.TrimPrefix(ts.URL, "http://"))
	t.Cleanup(func() {
		cl.Close()
		ts.Close()
	})
	return cl
}
