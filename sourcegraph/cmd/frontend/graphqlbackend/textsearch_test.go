package graphqlbackend

import (
	"context"
	"reflect"
	"sort"
	"testing"
	"time"

	zoektquery "github.com/google/zoekt/query"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/search"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/search/query"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/errcode"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver"
	"github.com/sourcegraph/sourcegraph/pkg/vcs"
	"github.com/sourcegraph/sourcegraph/pkg/vcs/git"
)

func TestQueryToZoektQuery(t *testing.T) {
	cases := []struct {
		Name    string
		Pattern *search.PatternInfo
		Query   string
	}{
		{
			Name: "substr",
			Pattern: &search.PatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              false,
				Pattern:                      "foo",
				IncludePatterns:              nil,
				ExcludePattern:               "",
				PathPatternsAreRegExps:       true,
				PathPatternsAreCaseSensitive: false,
			},
			Query: "foo case:no",
		},
		{
			Name: "regex",
			Pattern: &search.PatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              false,
				Pattern:                      "(foo).*?(bar)",
				IncludePatterns:              nil,
				ExcludePattern:               "",
				PathPatternsAreRegExps:       true,
				PathPatternsAreCaseSensitive: false,
			},
			Query: "(foo).*?(bar) case:no",
		},
		{
			Name: "path",
			Pattern: &search.PatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              false,
				Pattern:                      "foo",
				IncludePatterns:              []string{`\.go$`, `\.yaml$`},
				ExcludePattern:               `\bvendor\b`,
				PathPatternsAreRegExps:       true,
				PathPatternsAreCaseSensitive: false,
			},
			Query: `foo case:no f:\.go$ f:\.yaml$ -f:\bvendor\b`,
		},
		{
			Name: "case",
			Pattern: &search.PatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              true,
				Pattern:                      "foo",
				IncludePatterns:              []string{`\.go$`, `yaml`},
				ExcludePattern:               "",
				PathPatternsAreRegExps:       true,
				PathPatternsAreCaseSensitive: true,
			},
			Query: `foo case:yes f:\.go$ f:yaml`,
		},
		{
			Name: "casepath",
			Pattern: &search.PatternInfo{
				IsRegExp:                     true,
				IsCaseSensitive:              true,
				Pattern:                      "foo",
				IncludePatterns:              []string{`\.go$`, `\.yaml$`},
				ExcludePattern:               `\bvendor\b`,
				PathPatternsAreRegExps:       true,
				PathPatternsAreCaseSensitive: true,
			},
			Query: `foo case:yes f:\.go$ f:\.yaml$ -f:\bvendor\b`,
		},
	}
	for _, tt := range cases {
		t.Run(tt.Name, func(t *testing.T) {
			q, err := zoektquery.Parse(tt.Query)
			if err != nil {
				t.Fatalf("failed to parse %q: %v", tt.Query, err)
			}
			got, err := queryToZoektQuery(tt.Pattern)
			if err != nil {
				t.Fatal("queryToZoektQuery failed:", err)
			}
			if !queryEqual(got, q) {
				t.Fatalf("mismatched queries\ngot  %s\nwant %s", got.String(), q.String())
			}
		})
	}
}

func queryEqual(a zoektquery.Q, b zoektquery.Q) bool {
	sortChildren := func(q zoektquery.Q) zoektquery.Q {
		switch s := q.(type) {
		case *zoektquery.And:
			sort.Slice(s.Children, func(i, j int) bool {
				return s.Children[i].String() < s.Children[j].String()
			})
		case *zoektquery.Or:
			sort.Slice(s.Children, func(i, j int) bool {
				return s.Children[i].String() < s.Children[j].String()
			})
		}
		return q
	}
	return zoektquery.Map(a, sortChildren).String() == zoektquery.Map(b, sortChildren).String()
}

func TestSearchFilesInRepos(t *testing.T) {
	mockSearchFilesInRepo = func(ctx context.Context, repo *types.Repo, gitserverRepo gitserver.Repo, rev string, info *search.PatternInfo, fetchTimeout time.Duration) (matches []*fileMatchResolver, limitHit bool, err error) {
		repoName := repo.URI
		switch repoName {
		case "foo/one":
			return []*fileMatchResolver{
				{
					uri: "git://" + string(repoName) + "?" + rev + "#" + "main.go",
				},
			}, false, nil
		case "foo/two":
			return []*fileMatchResolver{
				{
					uri: "git://" + string(repoName) + "?" + rev + "#" + "main.go",
				},
			}, false, nil
		case "foo/empty":
			return nil, false, nil
		case "foo/cloning":
			return nil, false, &vcs.RepoNotExistError{Repo: repoName, CloneInProgress: true}
		case "foo/missing":
			return nil, false, &vcs.RepoNotExistError{Repo: repoName}
		case "foo/missing-db":
			return nil, false, &errcode.Mock{Message: "repo not found: foo/missing-db", IsNotFound: true}
		case "foo/timedout":
			return nil, false, context.DeadlineExceeded
		case "foo/no-rev":
			return nil, false, &git.RevisionNotFoundError{Repo: repoName, Spec: "missing"}
		default:
			return nil, false, errors.New("Unexpected repo")
		}
	}
	defer func() { mockSearchFilesInRepo = nil }()

	q, err := query.ParseAndCheck("foo")
	if err != nil {
		t.Fatal(err)
	}
	args := &search.Args{
		Pattern: &search.PatternInfo{
			FileMatchLimit: defaultMaxSearchResults,
			Pattern:        "foo",
		},
		Repos: makeRepositoryRevisions("foo/one", "foo/two", "foo/empty", "foo/cloning", "foo/missing", "foo/missing-db", "foo/timedout", "foo/no-rev"),
		Query: q,
	}
	results, common, err := searchFilesInRepos(context.Background(), args)
	if err != nil {
		t.Fatal(err)
	}
	if len(results) != 2 {
		t.Errorf("expected two results, got %d", len(results))
	}
	if v := toRepoURIs(common.cloning); !reflect.DeepEqual(v, []api.RepoURI{"foo/cloning"}) {
		t.Errorf("unexpected cloning: %v", v)
	}
	sort.Slice(common.missing, func(i, j int) bool { return common.missing[i].URI < common.missing[j].URI }) // to make deterministic
	if v := toRepoURIs(common.missing); !reflect.DeepEqual(v, []api.RepoURI{"foo/missing", "foo/missing-db"}) {
		t.Errorf("unexpected missing: %v", v)
	}
	if v := toRepoURIs(common.timedout); !reflect.DeepEqual(v, []api.RepoURI{"foo/timedout"}) {
		t.Errorf("unexpected timedout: %v", v)
	}

	// If we specify a rev and it isn't found, we fail the whole search since
	// that should be checked earlier.
	args = &search.Args{
		Pattern: &search.PatternInfo{
			FileMatchLimit: defaultMaxSearchResults,
			Pattern:        "foo",
		},
		Repos: makeRepositoryRevisions("foo/no-rev@dev"),
		Query: q,
	}
	_, _, err = searchFilesInRepos(context.Background(), args)
	if !git.IsRevisionNotFound(errors.Cause(err)) {
		t.Fatalf("searching non-existent rev expected to fail with RevisionNotFoundError got: %v", err)
	}
}

func makeRepositoryRevisions(repos ...string) []*search.RepositoryRevisions {
	r := make([]*search.RepositoryRevisions, len(repos))
	for i, urispec := range repos {
		uri, revs := search.ParseRepositoryRevisions(urispec)
		if len(revs) == 0 {
			// treat empty list as preferring master
			revs = []search.RevisionSpecifier{{RevSpec: ""}}
		}
		r[i] = &search.RepositoryRevisions{Repo: &types.Repo{URI: uri}, Revs: revs}
	}
	return r
}
