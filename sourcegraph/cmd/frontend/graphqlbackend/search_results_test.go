package graphqlbackend

import (
	"context"
	"fmt"
	"os"
	"sort"
	"strings"
	"testing"
	"time"

	mockrequire "github.com/derision-test/go-mockgen/testutil/require"
	"github.com/google/go-cmp/cmp"
	"github.com/google/zoekt"
	"github.com/hexops/autogold"
	"github.com/stretchr/testify/require"
	"go.uber.org/atomic"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/search"
	searchbackend "github.com/sourcegraph/sourcegraph/internal/search/backend"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/run"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/internal/search/symbol"
	"github.com/sourcegraph/sourcegraph/internal/search/textsearch"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestSearchResults(t *testing.T) {
	if os.Getenv("CI") != "" {
		// #25936: Some unit tests rely on external services that break
		// in CI but not locally. They should be removed or improved.
		t.Skip("TestSearchResults only works in local dev and is not reliable in CI")
	}

	ctx := context.Background()
	db := database.NewMockDB()

	getResults := func(t *testing.T, query, version string) []string {
		r, err := newSchemaResolver(db).Search(ctx, &SearchArgs{Query: query, Version: version})
		require.Nil(t, err)

		results, err := r.Results(ctx)
		require.NoError(t, err)

		resultDescriptions := make([]string, len(results.Matches))
		for i, match := range results.Matches {
			// NOTE: Only supports one match per line. If we need to test other cases,
			// just remove that assumption in the following line of code.
			switch m := match.(type) {
			case *result.RepoMatch:
				resultDescriptions[i] = fmt.Sprintf("repo:%s", m.Name)
			case *result.FileMatch:
				resultDescriptions[i] = fmt.Sprintf("%s:%d", m.Path, m.LineMatches[0].LineNumber)
			default:
				t.Fatal("unexpected result type:", match)
			}
		}
		// dedupe results since we expect our clients to do dedupping
		if len(resultDescriptions) > 1 {
			sort.Strings(resultDescriptions)
			dedup := resultDescriptions[:1]
			for _, s := range resultDescriptions[1:] {
				if s != dedup[len(dedup)-1] {
					dedup = append(dedup, s)
				}
			}
			resultDescriptions = dedup
		}
		return resultDescriptions
	}
	testCallResults := func(t *testing.T, query, version string, want []string) {
		t.Helper()
		results := getResults(t, query, version)
		if d := cmp.Diff(want, results); d != "" {
			t.Errorf("unexpected results (-want, +got):\n%s", d)
		}
	}

	searchVersions := []string{"V1", "V2"}

	t.Run("repo: only", func(t *testing.T) {
		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		repos := database.NewMockRepoStore()
		repos.ListMinimalReposFunc.SetDefaultHook(func(ctx context.Context, opt database.ReposListOptions) ([]types.MinimalRepo, error) {
			require.Equal(t, []string{"r", "p"}, opt.IncludePatterns)
			return []types.MinimalRepo{{ID: 1, Name: "repo"}}, nil
		})
		db.ReposFunc.SetDefaultReturn(repos)

		textsearch.MockSearchFilesInRepos = func() ([]result.Match, *streaming.Stats, error) {
			return nil, &streaming.Stats{}, nil
		}
		defer func() { textsearch.MockSearchFilesInRepos = nil }()

		for _, v := range searchVersions {
			testCallResults(t, `repo:r repo:p`, v, []string{"repo:repo"})
			mockrequire.Called(t, repos.ListMinimalReposFunc)
		}
	})

	t.Run("multiple terms regexp", func(t *testing.T) {
		t.Skip("Skipping because it's currently failing locally")

		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		repos := database.NewMockRepoStore()
		repos.ListMinimalReposFunc.SetDefaultReturn([]types.MinimalRepo{}, nil)
		db.ReposFunc.SetDefaultReturn(repos)

		calledSearchSymbols := false
		symbol.MockSearchSymbols = func(ctx context.Context, args *search.TextParameters, limit int) (res []result.Match, common *streaming.Stats, err error) {
			calledSearchSymbols = true
			if want := `(foo\d).*?(bar\*)`; args.PatternInfo.Pattern != want {
				t.Errorf("got %q, want %q", args.PatternInfo.Pattern, want)
			}
			// TODO return mock results here and assert that they are output as results
			return nil, nil, nil
		}
		defer func() { symbol.MockSearchSymbols = nil }()

		calledSearchFilesInRepos := atomic.NewBool(false)
		textsearch.MockSearchFilesInRepos = func() ([]result.Match, *streaming.Stats, error) {
			calledSearchFilesInRepos.Store(true)
			repo := types.MinimalRepo{ID: 1, Name: "repo"}
			fm := mkFileMatch(repo, "dir/file", 123)
			return []result.Match{fm}, &streaming.Stats{}, nil
		}
		defer func() { textsearch.MockSearchFilesInRepos = nil }()

		testCallResults(t, `foo\d "bar*"`, "V1", []string{"dir/file:123"})
		mockrequire.Called(t, repos.ListMinimalReposFunc)
		if !calledSearchFilesInRepos.Load() {
			t.Error("!calledSearchFilesInRepos")
		}
		if calledSearchSymbols {
			t.Error("calledSearchSymbols")
		}
	})

	t.Run("multiple terms literal", func(t *testing.T) {
		t.Skip("Skipping because it's currently failing locally")

		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		repos := database.NewMockRepoStore()
		repos.ListMinimalReposFunc.SetDefaultReturn([]types.MinimalRepo{}, nil)
		db.ReposFunc.SetDefaultReturn(repos)

		calledSearchSymbols := false
		symbol.MockSearchSymbols = func(ctx context.Context, args *search.TextParameters, limit int) (res []result.Match, common *streaming.Stats, err error) {
			calledSearchSymbols = true
			if want := `"foo\\d \"bar*\""`; args.PatternInfo.Pattern != want {
				t.Errorf("got %q, want %q", args.PatternInfo.Pattern, want)
			}
			// TODO return mock results here and assert that they are output as results
			return nil, nil, nil
		}
		defer func() { symbol.MockSearchSymbols = nil }()

		calledSearchFilesInRepos := atomic.NewBool(false)
		textsearch.MockSearchFilesInRepos = func() ([]result.Match, *streaming.Stats, error) {
			calledSearchFilesInRepos.Store(true)
			repo := types.MinimalRepo{ID: 1, Name: "repo"}
			fm := mkFileMatch(repo, "dir/file", 123)
			return []result.Match{fm}, &streaming.Stats{}, nil
		}
		defer func() { textsearch.MockSearchFilesInRepos = nil }()

		testCallResults(t, `foo\d "bar*"`, "V2", []string{"dir/file:123"})
		mockrequire.Called(t, repos.ListMinimalReposFunc)
		if !calledSearchFilesInRepos.Load() {
			t.Error("!calledSearchFilesInRepos")
		}
		if calledSearchSymbols {
			t.Error("calledSearchSymbols")
		}
	})
}

func TestSearchResolver_DynamicFilters(t *testing.T) {
	repo := types.MinimalRepo{Name: "testRepo"}
	repoMatch := &result.RepoMatch{Name: "testRepo"}
	fileMatch := func(path string) *result.FileMatch {
		return mkFileMatch(repo, path)
	}

	rev := "develop3.0"
	fileMatchRev := fileMatch("/testFile.md")
	fileMatchRev.InputRev = &rev

	type testCase struct {
		descr                           string
		searchResults                   []result.Match
		expectedDynamicFilterStrsRegexp map[string]int
	}

	tests := []testCase{

		{
			descr:         "single repo match",
			searchResults: []result.Match{repoMatch},
			expectedDynamicFilterStrsRegexp: map[string]int{
				`repo:^testRepo$`: 1,
			},
		},

		{
			descr:         "single file match without revision in query",
			searchResults: []result.Match{fileMatch("/testFile.md")},
			expectedDynamicFilterStrsRegexp: map[string]int{
				`repo:^testRepo$`: 1,
				`lang:markdown`:   1,
			},
		},

		{
			descr:         "single file match with specified revision",
			searchResults: []result.Match{fileMatchRev},
			expectedDynamicFilterStrsRegexp: map[string]int{
				`repo:^testRepo$@develop3.0`: 1,
				`lang:markdown`:              1,
			},
		},
		{
			descr:         "file match from a language with two file extensions, using first extension",
			searchResults: []result.Match{fileMatch("/testFile.ts")},
			expectedDynamicFilterStrsRegexp: map[string]int{
				`repo:^testRepo$`: 1,
				`lang:typescript`: 1,
			},
		},
		{
			descr:         "file match from a language with two file extensions, using second extension",
			searchResults: []result.Match{fileMatch("/testFile.tsx")},
			expectedDynamicFilterStrsRegexp: map[string]int{
				`repo:^testRepo$`: 1,
				`lang:typescript`: 1,
			},
		},
		{
			descr:         "file match which matches one of the common file filters",
			searchResults: []result.Match{fileMatch("/anything/node_modules/testFile.md")},
			expectedDynamicFilterStrsRegexp: map[string]int{
				`repo:^testRepo$`:          1,
				`-file:(^|/)node_modules/`: 1,
				`lang:markdown`:            1,
			},
		},
		{
			descr:         "file match which matches one of the common file filters",
			searchResults: []result.Match{fileMatch("/node_modules/testFile.md")},
			expectedDynamicFilterStrsRegexp: map[string]int{
				`repo:^testRepo$`:          1,
				`-file:(^|/)node_modules/`: 1,
				`lang:markdown`:            1,
			},
		},
		{
			descr: "file match which matches one of the common file filters",
			searchResults: []result.Match{
				fileMatch("/foo_test.go"),
				fileMatch("/foo.go"),
			},
			expectedDynamicFilterStrsRegexp: map[string]int{
				`repo:^testRepo$`:  2,
				`-file:_test\.go$`: 1,
				`lang:go`:          2,
			},
		},

		{
			descr: "prefer rust to renderscript",
			searchResults: []result.Match{
				fileMatch("/channel.rs"),
			},
			expectedDynamicFilterStrsRegexp: map[string]int{
				`repo:^testRepo$`: 1,
				`lang:rust`:       1,
			},
		},

		{
			descr: "javascript filters",
			searchResults: []result.Match{
				fileMatch("/jsrender.min.js.map"),
				fileMatch("playground/react/lib/app.js.map"),
				fileMatch("assets/javascripts/bootstrap.min.js"),
			},
			expectedDynamicFilterStrsRegexp: map[string]int{
				`repo:^testRepo$`:  3,
				`-file:\.min\.js$`: 1,
				`-file:\.js\.map$`: 2,
				`lang:javascript`:  1,
			},
		},

		// If there are no search results, no filters should be displayed.
		{
			descr:                           "no results",
			searchResults:                   []result.Match{},
			expectedDynamicFilterStrsRegexp: map[string]int{},
		},
		{
			descr:         "values containing spaces are quoted",
			searchResults: []result.Match{fileMatch("/.gitignore")},
			expectedDynamicFilterStrsRegexp: map[string]int{
				`repo:^testRepo$`:    1,
				`lang:"ignore list"`: 1,
			},
		},
	}

	mockDecodedViewerFinalSettings = &schema.Settings{}
	defer func() { mockDecodedViewerFinalSettings = nil }()

	var expectedDynamicFilterStrs map[string]int
	for _, test := range tests {
		t.Run(test.descr, func(t *testing.T) {
			for _, globbing := range []bool{true, false} {
				mockDecodedViewerFinalSettings.SearchGlobbing = &globbing
				actualDynamicFilters := (&SearchResultsResolver{db: database.NewMockDB(), SearchResults: &SearchResults{Matches: test.searchResults}}).DynamicFilters(context.Background())
				actualDynamicFilterStrs := make(map[string]int)

				for _, filter := range actualDynamicFilters {
					actualDynamicFilterStrs[filter.Value()] = int(filter.Count())
				}

				expectedDynamicFilterStrs = test.expectedDynamicFilterStrsRegexp
				if diff := cmp.Diff(expectedDynamicFilterStrs, actualDynamicFilterStrs); diff != "" {
					t.Errorf("mismatch (-want, +got):\n%s", diff)
				}
			}
		})
	}
}

func TestLonger(t *testing.T) {
	N := 2
	noise := time.Nanosecond
	for dt := time.Millisecond + noise; dt < time.Hour; dt += time.Millisecond {
		dt2 := longer(N, dt)
		if dt2 < time.Duration(N)*dt {
			t.Fatalf("longer(%v)=%v < 2*%v, want more", dt, dt2, dt)
		}
		if strings.Contains(dt2.String(), ".") {
			t.Fatalf("longer(%v).String() = %q contains an unwanted decimal point, want a nice round duration", dt, dt2)
		}
		lowest := 2 * time.Second
		if dt2 < lowest {
			t.Fatalf("longer(%v) = %v < %s, too short", dt, dt2, lowest)
		}
	}
}

func TestSearchResultsHydration(t *testing.T) {
	id := 42
	repoName := "reponame-foobar"
	fileName := "foobar.go"

	repoWithIDs := &types.Repo{
		ID:   api.RepoID(id),
		Name: api.RepoName(repoName),
		ExternalRepo: api.ExternalRepoSpec{
			ID:          repoName,
			ServiceType: extsvc.TypeGitHub,
			ServiceID:   "https://github.com",
		},
	}

	hydratedRepo := &types.Repo{
		ID:           repoWithIDs.ID,
		ExternalRepo: repoWithIDs.ExternalRepo,
		Name:         repoWithIDs.Name,
		URI:          fmt.Sprintf("github.com/my-org/%s", repoWithIDs.Name),
		Description:  "This is a description of a repository",
		Fork:         false,
	}

	db := database.NewMockDB()

	repos := database.NewMockRepoStore()
	repos.GetFunc.SetDefaultReturn(hydratedRepo, nil)
	repos.ListMinimalReposFunc.SetDefaultHook(func(ctx context.Context, opt database.ReposListOptions) ([]types.MinimalRepo, error) {
		if opt.OnlyPrivate {
			return nil, nil
		}
		return []types.MinimalRepo{{ID: repoWithIDs.ID, Name: repoWithIDs.Name}}, nil
	})
	repos.CountFunc.SetDefaultReturn(0, nil)
	db.ReposFunc.SetDefaultReturn(repos)

	zoektRepo := &zoekt.RepoListEntry{
		Repository: zoekt.Repository{
			ID:       uint32(repoWithIDs.ID),
			Name:     string(repoWithIDs.Name),
			Branches: []zoekt.RepositoryBranch{{Name: "HEAD", Version: "deadbeef"}},
		},
	}

	zoektFileMatches := []zoekt.FileMatch{{
		Score:        5.0,
		FileName:     fileName,
		RepositoryID: uint32(repoWithIDs.ID),
		Repository:   string(repoWithIDs.Name), // Important: this needs to match a name in `repos`
		Branches:     []string{"master"},
		LineMatches: []zoekt.LineMatch{
			{
				Line: nil,
			},
		},
		Checksum: []byte{0, 1, 2},
	}}

	z := &searchbackend.FakeSearcher{
		Repos:  []*zoekt.RepoListEntry{zoektRepo},
		Result: &zoekt.SearchResult{Files: zoektFileMatches},
	}

	// Act in a user context
	var ctxUser int32 = 1234
	ctx := actor.WithActor(context.Background(), actor.FromMockUser(ctxUser))

	p, err := query.Pipeline(query.InitLiteral(`foobar index:only count:350`))
	if err != nil {
		t.Fatal(err)
	}

	resolver := &searchResolver{
		db: db,
		SearchInputs: &run.SearchInputs{
			Plan:         p,
			Query:        p.ToParseTree(),
			UserSettings: &schema.Settings{},
		},
		zoekt: z,
	}
	results, err := resolver.Results(ctx)
	if err != nil {
		t.Fatal("Results:", err)
	}
	// We want one file match and one repository match
	wantMatchCount := 2
	if int(results.MatchCount()) != wantMatchCount {
		t.Fatalf("wrong results length. want=%d, have=%d\n", wantMatchCount, results.MatchCount())
	}

	for _, r := range results.Results() {
		switch r := r.(type) {
		case *FileMatchResolver:
			assertRepoResolverHydrated(ctx, t, r.Repository(), hydratedRepo)

		case *RepositoryResolver:
			assertRepoResolverHydrated(ctx, t, r, hydratedRepo)
		}
	}
}

func TestSearchResultsResolver_ApproximateResultCount(t *testing.T) {
	type fields struct {
		results             []result.Match
		searchResultsCommon streaming.Stats
		alert               *search.Alert
	}
	tests := []struct {
		name   string
		fields fields
		want   string
	}{
		{
			name:   "empty",
			fields: fields{},
			want:   "0",
		},

		{
			name: "file matches",
			fields: fields{
				results: []result.Match{&result.FileMatch{}},
			},
			want: "1",
		},

		{
			name: "file matches limit hit",
			fields: fields{
				results:             []result.Match{&result.FileMatch{}},
				searchResultsCommon: streaming.Stats{IsLimitHit: true},
			},
			want: "1+",
		},

		{
			name: "symbol matches",
			fields: fields{
				results: []result.Match{
					&result.FileMatch{
						Symbols: []*result.SymbolMatch{
							// 1
							{},
							// 2
							{},
						},
					},
				},
			},
			want: "2",
		},

		{
			name: "symbol matches limit hit",
			fields: fields{
				results: []result.Match{
					&result.FileMatch{
						Symbols: []*result.SymbolMatch{
							// 1
							{},
							// 2
							{},
						},
					},
				},
				searchResultsCommon: streaming.Stats{IsLimitHit: true},
			},
			want: "2+",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			sr := &SearchResultsResolver{
				db: database.NewMockDB(),
				SearchResults: &SearchResults{
					Stats:   tt.fields.searchResultsCommon,
					Matches: tt.fields.results,
					Alert:   tt.fields.alert,
				},
			}
			if got := sr.ApproximateResultCount(); got != tt.want {
				t.Errorf("searchResultsResolver.ApproximateResultCount() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestCompareSearchResults(t *testing.T) {
	makeResult := func(repo, file string) *result.FileMatch {
		return &result.FileMatch{File: result.File{
			Repo: types.MinimalRepo{Name: api.RepoName(repo)},
			Path: file,
		}}
	}

	tests := []struct {
		name    string
		a       *result.FileMatch
		b       *result.FileMatch
		aIsLess bool
	}{
		{
			name:    "alphabetical order",
			a:       makeResult("arepo", "afile"),
			b:       makeResult("arepo", "bfile"),
			aIsLess: true,
		},
		{
			name:    "same length, different files",
			a:       makeResult("arepo", "bfile"),
			b:       makeResult("arepo", "afile"),
			aIsLess: false,
		},
		{
			name:    "different repo, no exact patterns",
			a:       makeResult("arepo", "file"),
			b:       makeResult("brepo", "afile"),
			aIsLess: true,
		},
		{
			name:    "repo matches only",
			a:       makeResult("arepo", ""),
			b:       makeResult("brepo", ""),
			aIsLess: true,
		},
		{
			name:    "repo match and file match, same repo",
			a:       makeResult("arepo", "file"),
			b:       makeResult("arepo", ""),
			aIsLess: false,
		},
		{
			name:    "repo match and file match, different repos",
			a:       makeResult("arepo", ""),
			b:       makeResult("brepo", "file"),
			aIsLess: true,
		},
	}
	for _, tt := range tests {
		t.Run("test", func(t *testing.T) {
			if got := tt.a.Key().Less(tt.b.Key()); got != tt.aIsLess {
				t.Errorf("compareSearchResults() = %v, aIsLess %v", got, tt.aIsLess)
			}
		})
	}
}

func TestEvaluateAnd(t *testing.T) {
	db := database.NewMockDB()

	tests := []struct {
		name         string
		query        string
		zoektMatches int
		filesSkipped int
		wantAlert    bool
	}{
		{
			name:         "zoekt returns enough matches, exhausted",
			query:        "foo and bar index:only count:5",
			zoektMatches: 5,
			filesSkipped: 0,
			wantAlert:    false,
		},
		{
			name:         "zoekt does not return enough matches, not exhausted",
			query:        "foo and bar index:only count:50",
			zoektMatches: 0,
			filesSkipped: 1,
			wantAlert:    true,
		},
		{
			name:         "zoekt returns enough matches, not exhausted",
			query:        "foo and bar index:only count:50",
			zoektMatches: 50,
			filesSkipped: 1,
			wantAlert:    false,
		},
	}

	minimalRepos, zoektRepos := generateRepos(5000)

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			zoektFileMatches := generateZoektMatches(tt.zoektMatches)
			z := &searchbackend.FakeSearcher{
				Repos:  zoektRepos,
				Result: &zoekt.SearchResult{Files: zoektFileMatches, Stats: zoekt.Stats{FilesSkipped: tt.filesSkipped}},
			}

			ctx := context.Background()

			repos := database.NewMockRepoStore()
			repos.ListMinimalReposFunc.SetDefaultHook(func(ctx context.Context, opt database.ReposListOptions) ([]types.MinimalRepo, error) {
				if len(opt.IncludePatterns) > 0 || len(opt.ExcludePattern) > 0 {
					return nil, nil
				}
				repoNames := make([]types.MinimalRepo, len(minimalRepos))
				for i := range minimalRepos {
					repoNames[i] = types.MinimalRepo{ID: minimalRepos[i].ID, Name: minimalRepos[i].Name}
				}
				return repoNames, nil
			})
			repos.CountFunc.SetDefaultReturn(len(minimalRepos), nil)
			db.ReposFunc.SetDefaultReturn(repos)

			p, err := query.Pipeline(query.InitLiteral(tt.query))
			if err != nil {
				t.Fatal(err)
			}

			resolver := &searchResolver{
				db: db,
				SearchInputs: &run.SearchInputs{
					Plan:         p,
					Query:        p.ToParseTree(),
					UserSettings: &schema.Settings{},
				},
				zoekt: z,
			}
			results, err := resolver.Results(ctx)
			if err != nil {
				t.Fatal("Results:", err)
			}
			if tt.wantAlert {
				if results.SearchResults.Alert == nil {
					t.Errorf("Expected alert")
				}
			} else if int(results.MatchCount()) != len(zoektFileMatches) {
				t.Errorf("wrong results length. want=%d, have=%d\n", len(zoektFileMatches), results.MatchCount())
			}
		})
	}
}

func TestSearchContext(t *testing.T) {
	orig := envvar.SourcegraphDotComMode()
	envvar.MockSourcegraphDotComMode(true)
	defer envvar.MockSourcegraphDotComMode(orig)

	tts := []struct {
		name        string
		searchQuery string
		numContexts int
	}{
		{name: "single search context", searchQuery: "foo context:@userA", numContexts: 1},
		{name: "multiple search contexts", searchQuery: "foo (context:@userA or context:@userB)", numContexts: 2},
	}

	users := map[string]int32{
		"userA": 1,
		"userB": 2,
	}

	mockZoekt := &searchbackend.FakeSearcher{Repos: []*zoekt.RepoListEntry{}}

	for _, tt := range tts {
		t.Run(tt.name, func(t *testing.T) {
			p, err := query.Pipeline(query.InitLiteral(tt.searchQuery))
			if err != nil {
				t.Fatal(err)
			}

			repos := database.NewMockRepoStore()
			repos.ListMinimalReposFunc.SetDefaultReturn([]types.MinimalRepo{}, nil)
			repos.CountFunc.SetDefaultReturn(0, nil)

			ns := database.NewMockNamespaceStore()
			ns.GetByNameFunc.SetDefaultHook(func(ctx context.Context, name string) (*database.Namespace, error) {
				userID, ok := users[name]
				if !ok {
					t.Errorf("User with ID %d not found", userID)
				}
				return &database.Namespace{Name: name, User: userID}, nil
			})

			db := database.NewMockDB()
			db.ReposFunc.SetDefaultReturn(repos)
			db.NamespacesFunc.SetDefaultReturn(ns)

			resolver := searchResolver{
				SearchInputs: &run.SearchInputs{
					Plan:         p,
					Query:        p.ToParseTree(),
					UserSettings: &schema.Settings{},
				},
				zoekt: mockZoekt,
				db:    db,
			}

			_, err = resolver.Results(context.Background())
			if err != nil {
				t.Fatal(err)
			}
		})
	}
}

func Test_toSearchInputs(t *testing.T) {
	orig := envvar.SourcegraphDotComMode()
	envvar.MockSourcegraphDotComMode(true)
	defer envvar.MockSourcegraphDotComMode(orig)

	test := func(input string, parser func(string) (query.Q, error)) string {
		q, _ := parser(input)
		resolver := searchResolver{
			SearchInputs: &run.SearchInputs{
				Query:        q,
				UserSettings: &schema.Settings{},
				PatternType:  query.SearchTypeLiteral,
			},
		}
		job, _ := resolver.toSearchJob(q)
		return job.Name()
	}

	// Job generation for global vs non-global search
	autogold.Want("user search context", "ParallelJob{RepoSubsetText, Repo, ComputeExcludedRepos}").Equal(t, test(`foo context:@userA`, query.ParseLiteral))
	autogold.Want("universal (AKA global) search context", "ParallelJob{RepoUniverseText, Repo, ComputeExcludedRepos}").Equal(t, test(`foo context:global`, query.ParseLiteral))
	autogold.Want("universal (AKA global) search", "ParallelJob{RepoUniverseText, Repo, ComputeExcludedRepos}").Equal(t, test(`foo`, query.ParseLiteral))
	autogold.Want("nonglobal repo", "ParallelJob{RepoSubsetText, Repo, ComputeExcludedRepos}").Equal(t, test(`foo repo:sourcegraph/sourcegraph`, query.ParseLiteral))
	autogold.Want("nonglobal repo contains", "ParallelJob{RepoSubsetText, Repo, ComputeExcludedRepos}").Equal(t, test(`foo repo:contains(bar)`, query.ParseLiteral))

	// Job generation support for implied `type:repo` queries.
	autogold.Want("supported Repo job", "ParallelJob{RepoUniverseText, Repo, ComputeExcludedRepos}").Equal(t, test("ok ok", query.ParseRegexp))
	autogold.Want("supportedRepo job literal", "ParallelJob{RepoUniverseText, Repo, ComputeExcludedRepos}").Equal(t, test("ok @thing", query.ParseLiteral))
	autogold.Want("unsupported Repo job prefix", "ParallelJob{RepoUniverseText, ComputeExcludedRepos}").Equal(t, test("@nope", query.ParseRegexp))
	autogold.Want("unsupported Repo job regexp", "ParallelJob{RepoUniverseText, ComputeExcludedRepos}").Equal(t, test("foo @bar", query.ParseRegexp))

	// Job generation for other types of search
	autogold.Want("symbol", "ParallelJob{RepoUniverseSymbol, ComputeExcludedRepos}").Equal(t, test("type:symbol test", query.ParseRegexp))
	autogold.Want("commit", "ParallelJob{Commit, ComputeExcludedRepos}").Equal(t, test("type:commit test", query.ParseRegexp))
	autogold.Want("diff", "ParallelJob{Diff, ComputeExcludedRepos}").Equal(t, test("type:diff test", query.ParseRegexp))
	autogold.Want("file or commit", "JobWithOptional{Required: ParallelJob{RepoUniverseText, ComputeExcludedRepos}, Optional: Commit}").Equal(t, test("type:file type:commit test", query.ParseRegexp))
	autogold.Want("many types", "JobWithOptional{Required: ParallelJob{RepoSubsetText, Repo, ComputeExcludedRepos}, Optional: ParallelJob{RepoSubsetSymbol, Commit}}").Equal(t, test("type:file type:path type:repo type:commit type:symbol repo:test test", query.ParseRegexp))
}

func TestZeroElapsedMilliseconds(t *testing.T) {
	r := &SearchResultsResolver{}
	if got := r.ElapsedMilliseconds(); got != 0 {
		t.Fatalf("got %d, want %d", got, 0)
	}
}

func TestIsContextError(t *testing.T) {
	cases := []struct {
		err  error
		want bool
	}{
		{
			context.Canceled,
			true,
		},
		{
			context.DeadlineExceeded,
			true,
		},
		{
			errors.Wrap(context.Canceled, "wrapped"),
			true,
		},
		{
			errors.New("not a context error"),
			false,
		},
	}
	ctx := context.Background()
	for _, c := range cases {
		t.Run(c.err.Error(), func(t *testing.T) {
			if got := isContextError(ctx, c.err); got != c.want {
				t.Fatalf("wanted %t, got %t", c.want, got)
			}
		})
	}
}

// Detailed filtering tests are below in TestSubRepoFilterFunc, this test is more
// of an integration test to ensure that things are threaded through correctly
// from the resolver
func TestSubRepoFiltering(t *testing.T) {
	tts := []struct {
		name        string
		searchQuery string
		wantCount   int
		checker     func() authz.SubRepoPermissionChecker
	}{
		{
			name:        "simple search without filtering",
			searchQuery: "foo",
			wantCount:   3,
		},
		{
			name:        "simple search with filtering",
			searchQuery: "foo ",
			wantCount:   2,
			checker: func() authz.SubRepoPermissionChecker {
				checker := authz.NewMockSubRepoPermissionChecker()
				checker.EnabledFunc.SetDefaultHook(func() bool {
					return true
				})
				// We'll just block the third file
				checker.PermissionsFunc.SetDefaultHook(func(ctx context.Context, i int32, content authz.RepoContent) (authz.Perms, error) {
					if strings.Contains(content.Path, "3") {
						return authz.None, nil
					}
					return authz.Read, nil
				})
				return checker
			},
		},
	}

	zoektFileMatches := generateZoektMatches(3)
	mockZoekt := &searchbackend.FakeSearcher{
		Repos: []*zoekt.RepoListEntry{},
		Result: &zoekt.SearchResult{
			Files: zoektFileMatches,
		},
	}

	for _, tt := range tts {
		t.Run(tt.name, func(t *testing.T) {
			if tt.checker != nil {
				old := authz.DefaultSubRepoPermsChecker
				t.Cleanup(func() { authz.DefaultSubRepoPermsChecker = old })
				authz.DefaultSubRepoPermsChecker = tt.checker()
			}

			p, err := query.Pipeline(query.InitLiteral(tt.searchQuery))
			if err != nil {
				t.Fatal(err)
			}

			repos := database.NewMockRepoStore()
			repos.ListMinimalReposFunc.SetDefaultReturn([]types.MinimalRepo{}, nil)
			repos.CountFunc.SetDefaultReturn(0, nil)

			db := database.NewMockDB()
			db.ReposFunc.SetDefaultReturn(repos)
			db.EventLogsFunc.SetDefaultHook(func() database.EventLogStore {
				return database.NewMockEventLogStore()
			})

			resolver := searchResolver{
				SearchInputs: &run.SearchInputs{
					Plan:         p,
					Query:        p.ToParseTree(),
					UserSettings: &schema.Settings{},
				},
				zoekt: mockZoekt,
				db:    db,
			}

			ctx := context.Background()
			ctx = actor.WithActor(ctx, &actor.Actor{
				UID: 1,
			})
			rr, err := resolver.Results(ctx)
			if err != nil {
				t.Fatal(err)
			}

			if len(rr.Matches) != tt.wantCount {
				t.Fatalf("Want %d matches, got %d", tt.wantCount, len(rr.Matches))
			}
		})
	}
}

func Test_searchResultsToRepoNodes(t *testing.T) {
	cases := []struct {
		matches []result.Match
		res     string
		err     string
	}{{
		matches: []result.Match{
			&result.RepoMatch{Name: "repo_a"},
		},
		res: `"repo:^repo_a$"`,
	}, {
		matches: []result.Match{
			&result.RepoMatch{Name: "repo_a", Rev: "main"},
		},
		res: `"repo:^repo_a$@main"`,
	}, {
		matches: []result.Match{
			&result.FileMatch{},
		},
		err: "expected type",
	}}

	for _, tc := range cases {
		t.Run(tc.res, func(t *testing.T) {
			nodes, err := searchResultsToRepoNodes(tc.matches)
			if err != nil {
				require.Contains(t, err.Error(), tc.err)
				return
			}
			require.Equal(t, tc.res, query.Q(nodes).String())
		})
	}
}

func Test_searchResultsToFileNodes(t *testing.T) {
	cases := []struct {
		matches []result.Match
		res     string
		err     string
	}{{
		matches: []result.Match{
			&result.FileMatch{
				File: result.File{
					Repo: types.MinimalRepo{
						Name: "repo_a",
					},
					Path: "my/file/path.txt",
				},
			},
		},
		res: `(and "repo:^repo_a$" "file:^my/file/path\\.txt$")`,
	}, {
		matches: []result.Match{
			&result.FileMatch{
				File: result.File{
					Repo: types.MinimalRepo{
						Name: "repo_a",
					},
					InputRev: func() *string { s := "main"; return &s }(),
					Path:     "my/file/path1.txt",
				},
			},
			&result.FileMatch{
				File: result.File{
					Repo: types.MinimalRepo{
						Name: "repo_b",
					},
					Path: "my/file/path2.txt",
				},
			},
		},
		res: `(and "repo:^repo_a$@main" "file:^my/file/path1\\.txt$") (and "repo:^repo_b$" "file:^my/file/path2\\.txt$")`,
	}}

	for _, tc := range cases {
		t.Run(tc.res, func(t *testing.T) {
			nodes, err := searchResultsToFileNodes(tc.matches)
			if err != nil {
				require.Contains(t, err.Error(), tc.err)
				return
			}
			require.Equal(t, tc.res, query.Q(nodes).String())
		})
	}
}
