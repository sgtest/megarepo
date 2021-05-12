package repos

import (
	"context"
	"flag"
	"fmt"
	"os"
	"reflect"
	"sort"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/google/zoekt"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/search"
	searchbackend "github.com/sourcegraph/sourcegraph/internal/search/backend"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
)

var dsn = flag.String("dsn", "", "Database connection string to use in integration tests")

func TestMain(m *testing.M) {
	flag.Parse()
	os.Exit(m.Run())
}

func TestRevisionValidation(t *testing.T) {
	// mocks a repo repoFoo with revisions revBar and revBas
	git.Mocks.ResolveRevision = func(spec string, opt git.ResolveRevisionOptions) (api.CommitID, error) {
		// trigger errors
		if spec == "bad_commit" {
			return "", git.BadCommitError{}
		}
		if spec == "deadline_exceeded" {
			return "", context.DeadlineExceeded
		}

		// known revisions
		m := map[string]struct{}{
			"revBar": {},
			"revBas": {},
		}
		if _, ok := m[spec]; ok {
			return "", nil
		}
		return "", &gitserver.RevisionNotFoundError{Repo: "repoFoo", Spec: spec}
	}
	defer func() { git.Mocks.ResolveRevision = nil }()

	database.Mocks.Repos.ListRepoNames = func(ctx context.Context, opts database.ReposListOptions) ([]types.RepoName, error) {
		return []types.RepoName{{Name: "repoFoo"}}, nil
	}
	defer func() { database.Mocks.Repos.List = nil }()

	tests := []struct {
		repoFilters              []string
		wantRepoRevs             []*search.RepositoryRevisions
		wantMissingRepoRevisions []*search.RepositoryRevisions
		wantErr                  error
	}{
		{
			repoFilters: []string{"repoFoo@revBar:^revBas"},
			wantRepoRevs: []*search.RepositoryRevisions{{
				Repo: types.RepoName{Name: "repoFoo"},
				Revs: []search.RevisionSpecifier{
					{
						RevSpec:        "revBar",
						RefGlob:        "",
						ExcludeRefGlob: "",
					},
					{
						RevSpec:        "^revBas",
						RefGlob:        "",
						ExcludeRefGlob: "",
					},
				},
			}},
			wantMissingRepoRevisions: nil,
		},
		{
			repoFilters: []string{"repoFoo@*revBar:*!revBas"},
			wantRepoRevs: []*search.RepositoryRevisions{{
				Repo: types.RepoName{Name: "repoFoo"},
				Revs: []search.RevisionSpecifier{
					{
						RevSpec:        "",
						RefGlob:        "revBar",
						ExcludeRefGlob: "",
					},
					{
						RevSpec:        "",
						RefGlob:        "",
						ExcludeRefGlob: "revBas",
					},
				},
			}},
			wantMissingRepoRevisions: nil,
		},
		{
			repoFilters: []string{"repoFoo@revBar:^revQux"},
			wantRepoRevs: []*search.RepositoryRevisions{{
				Repo: types.RepoName{Name: "repoFoo"},
				Revs: []search.RevisionSpecifier{
					{
						RevSpec:        "revBar",
						RefGlob:        "",
						ExcludeRefGlob: "",
					},
				},
				ListRefs: nil,
			}},
			wantMissingRepoRevisions: []*search.RepositoryRevisions{{
				Repo: types.RepoName{Name: "repoFoo"},
				Revs: []search.RevisionSpecifier{
					{
						RevSpec:        "^revQux",
						RefGlob:        "",
						ExcludeRefGlob: "",
					},
				},
			}},
		},
		{
			repoFilters:              []string{"repoFoo@revBar:bad_commit"},
			wantRepoRevs:             nil,
			wantMissingRepoRevisions: nil,
			wantErr:                  git.BadCommitError{},
		},
		{
			repoFilters:              []string{"repoFoo@revBar:^bad_commit"},
			wantRepoRevs:             nil,
			wantMissingRepoRevisions: nil,
			wantErr:                  git.BadCommitError{},
		},
		{
			repoFilters:              []string{"repoFoo@revBar:deadline_exceeded"},
			wantRepoRevs:             nil,
			wantMissingRepoRevisions: nil,
			wantErr:                  context.DeadlineExceeded,
		},
		{
			repoFilters: []string{"repoFoo"},
			wantRepoRevs: []*search.RepositoryRevisions{{
				Repo: types.RepoName{Name: "repoFoo"},
				Revs: []search.RevisionSpecifier{
					{
						RevSpec:        "",
						RefGlob:        "",
						ExcludeRefGlob: "",
					},
				},
			}},
			wantMissingRepoRevisions: nil,
			wantErr:                  nil,
		},
	}

	for _, tt := range tests {
		t.Run(tt.repoFilters[0], func(t *testing.T) {

			op := Options{RepoFilters: tt.repoFilters}
			repositoryResolver := &Resolver{}
			resolved, err := repositoryResolver.Resolve(context.Background(), op)

			if diff := cmp.Diff(tt.wantRepoRevs, resolved.RepoRevs); diff != "" {
				t.Error(diff)
			}
			if diff := cmp.Diff(tt.wantMissingRepoRevisions, resolved.MissingRepoRevs); diff != "" {
				t.Error(diff)
			}
			if tt.wantErr != err {
				t.Errorf("got: %v, expected: %v", err, tt.wantErr)
			}
		})
	}
}

// TestSearchRevspecs tests a repository name against a list of
// repository specs with optional revspecs, and determines whether
// we get the expected error, list of matching rev specs, or list
// of clashing revspecs (if no matching rev specs were found)
func TestSearchRevspecs(t *testing.T) {
	type testCase struct {
		descr    string
		specs    []string
		repo     string
		err      error
		matched  []search.RevisionSpecifier
		clashing []search.RevisionSpecifier
	}

	tests := []testCase{
		{
			descr:    "simple match",
			specs:    []string{"foo"},
			repo:     "foo",
			err:      nil,
			matched:  []search.RevisionSpecifier{{RevSpec: ""}},
			clashing: nil,
		},
		{
			descr:    "single revspec",
			specs:    []string{".*o@123456"},
			repo:     "foo",
			err:      nil,
			matched:  []search.RevisionSpecifier{{RevSpec: "123456"}},
			clashing: nil,
		},
		{
			descr:    "revspec plus unspecified rev",
			specs:    []string{".*o@123456", "foo"},
			repo:     "foo",
			err:      nil,
			matched:  []search.RevisionSpecifier{{RevSpec: "123456"}},
			clashing: nil,
		},
		{
			descr:    "revspec plus unspecified rev, but backwards",
			specs:    []string{".*o", "foo@123456"},
			repo:     "foo",
			err:      nil,
			matched:  []search.RevisionSpecifier{{RevSpec: "123456"}},
			clashing: nil,
		},
		{
			descr:    "conflicting revspecs",
			specs:    []string{".*o@123456", "foo@234567"},
			repo:     "foo",
			err:      nil,
			matched:  nil,
			clashing: []search.RevisionSpecifier{{RevSpec: "123456"}, {RevSpec: "234567"}},
		},
		{
			descr:    "overlapping revspecs",
			specs:    []string{".*o@a:b", "foo@b:c"},
			repo:     "foo",
			err:      nil,
			matched:  []search.RevisionSpecifier{{RevSpec: "b"}},
			clashing: nil,
		},
		{
			descr:    "multiple overlapping revspecs",
			specs:    []string{".*o@a:b:c", "foo@b:c:d"},
			repo:     "foo",
			err:      nil,
			matched:  []search.RevisionSpecifier{{RevSpec: "b"}, {RevSpec: "c"}},
			clashing: nil,
		},
		{
			descr:    "invalid regexp",
			specs:    []string{"*o@a:b"},
			repo:     "foo",
			err:      fmt.Errorf("%s", "bad request: error parsing regexp: missing argument to repetition operator: `*`"),
			matched:  nil,
			clashing: nil,
		},
	}
	for _, test := range tests {
		t.Run(test.descr, func(t *testing.T) {
			pats, err := findPatternRevs(test.specs)
			if err != nil {
				if test.err == nil {
					t.Errorf("unexpected error: '%s'", err)
				}
				if test.err != nil && err.Error() != test.err.Error() {
					t.Errorf("incorrect error: got '%s', expected '%s'", err, test.err)
				}
				// don't try to use the pattern list if we got an error
				return
			}
			if test.err != nil {
				t.Errorf("missing expected error: wanted '%s'", test.err.Error())
			}
			matched, clashing := getRevsForMatchedRepo(api.RepoName(test.repo), pats)
			if !reflect.DeepEqual(matched, test.matched) {
				t.Errorf("matched repo mismatch: actual: %#v, expected: %#v", matched, test.matched)
			}
			if !reflect.DeepEqual(clashing, test.clashing) {
				t.Errorf("clashing repo mismatch: actual: %#v, expected: %#v", clashing, test.clashing)
			}
		})
	}
}

func BenchmarkGetRevsForMatchedRepo(b *testing.B) {
	b.Run("2 conflicting", func(b *testing.B) {
		pats, _ := findPatternRevs([]string{".*o@123456", "foo@234567"})
		for i := 0; i < b.N; i++ {
			_, _ = getRevsForMatchedRepo("foo", pats)
		}
	})

	b.Run("multiple overlapping", func(b *testing.B) {
		pats, _ := findPatternRevs([]string{".*o@a:b:c:d", "foo@b:c:d:e", "foo@c:d:e:f"})
		for i := 0; i < b.N; i++ {
			_, _ = getRevsForMatchedRepo("foo", pats)
		}
	})
}

func TestDefaultRepositories(t *testing.T) {
	tcs := []struct {
		name             string
		defaultsInDb     []string
		indexedRepoNames map[string]bool
		want             []string
		excludePatterns  []string
	}{
		{
			name:             "none in database => none returned",
			defaultsInDb:     nil,
			indexedRepoNames: nil,
			want:             nil,
		},
		{
			name:             "two in database, one indexed => indexed repo returned",
			defaultsInDb:     []string{"unindexedrepo", "indexedrepo"},
			indexedRepoNames: map[string]bool{"indexedrepo": true},
			want:             []string{"indexedrepo"},
		},
		{
			name:             "should not return excluded repo",
			defaultsInDb:     []string{"unindexedrepo1", "indexedrepo1", "indexedrepo2", "indexedrepo3"},
			indexedRepoNames: map[string]bool{"indexedrepo1": true, "indexedrepo2": true, "indexedrepo3": true},
			excludePatterns:  []string{"indexedrepo3"},
			want:             []string{"indexedrepo1", "indexedrepo2"},
		},
		{
			name:             "should not return excluded repo (case insensitive)",
			defaultsInDb:     []string{"unindexedrepo1", "indexedrepo1", "indexedrepo2", "Indexedrepo3"},
			indexedRepoNames: map[string]bool{"indexedrepo1": true, "indexedrepo2": true, "Indexedrepo3": true},
			excludePatterns:  []string{"indexedrepo3"},
			want:             []string{"indexedrepo1", "indexedrepo2"},
		},
		{
			name:             "should not return excluded repos ending in `test`",
			defaultsInDb:     []string{"repo1", "repo2", "repo-test", "repoTEST"},
			indexedRepoNames: map[string]bool{"repo1": true, "repo2": true, "repo-test": true, "repoTEST": true},
			excludePatterns:  []string{"test$"},
			want:             []string{"repo1", "repo2"},
		},
	}
	for _, tc := range tcs {
		t.Run(tc.name, func(t *testing.T) {

			var drs []types.RepoName
			for i, name := range tc.defaultsInDb {
				r := types.RepoName{
					ID:   api.RepoID(i),
					Name: api.RepoName(name),
				}
				drs = append(drs, r)
			}
			getRawDefaultRepos := func(ctx context.Context) ([]types.RepoName, error) {
				return drs, nil
			}

			var indexed []*zoekt.RepoListEntry
			for name := range tc.indexedRepoNames {
				indexed = append(indexed, &zoekt.RepoListEntry{Repository: zoekt.Repository{Name: name}})
			}
			z := &searchbackend.Zoekt{
				Client:       &searchbackend.FakeSearcher{Repos: indexed},
				DisableCache: true,
			}

			ctx := context.Background()
			drs, err := defaultRepositories(ctx, getRawDefaultRepos, z, tc.excludePatterns)
			if err != nil {
				t.Fatal(err)
			}
			var drNames []string
			for _, dr := range drs {
				drNames = append(drNames, string(dr.Name))
			}
			if !reflect.DeepEqual(drNames, tc.want) {
				t.Errorf("names of default repos = %v, want %v", drNames, tc.want)
			}
		})
	}
}

func TestUseDefaultReposIfMissingOrGlobalSearchContext(t *testing.T) {
	orig := envvar.SourcegraphDotComMode()
	envvar.MockSourcegraphDotComMode(true)
	defer envvar.MockSourcegraphDotComMode(orig)

	queryInfo, err := query.ParseLiteral("foo")
	if err != nil {
		t.Fatal(err)
	}

	wantDefaultRepoNames := []string{
		"default/one",
		"default/two",
		"default/three",
	}
	defaultRepos := make([]types.RepoName, len(wantDefaultRepoNames))
	zoektRepoListEntries := make([]*zoekt.RepoListEntry, len(wantDefaultRepoNames))
	mockDefaultReposFunc := func(_ context.Context) ([]types.RepoName, error) {
		return defaultRepos, nil
	}

	for idx, name := range wantDefaultRepoNames {
		defaultRepos[idx] = types.RepoName{Name: api.RepoName(name)}
		zoektRepoListEntries[idx] = &zoekt.RepoListEntry{
			Repository: zoekt.Repository{
				Name:     name,
				Branches: []zoekt.RepositoryBranch{{Name: "HEAD", Version: "deadbeef"}},
			},
		}
	}

	mockZoekt := &searchbackend.Zoekt{
		Client:       &searchbackend.FakeSearcher{Repos: zoektRepoListEntries},
		DisableCache: true,
	}

	tests := []struct {
		name              string
		searchContextSpec string
	}{
		{name: "use default repos if missing search context", searchContextSpec: ""},
		{name: "use default repos with global search context", searchContextSpec: "global"},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			op := Options{
				SearchContextSpec: tt.searchContextSpec,
				Query:             queryInfo,
			}
			repositoryResolver := &Resolver{Zoekt: mockZoekt, DefaultReposFunc: mockDefaultReposFunc}
			resolved, err := repositoryResolver.Resolve(context.Background(), op)
			if err != nil {
				t.Fatal(err)
			}
			var repoNames []string
			for _, repoRev := range resolved.RepoRevs {
				repoNames = append(repoNames, string(repoRev.Repo.Name))
			}
			if !reflect.DeepEqual(repoNames, wantDefaultRepoNames) {
				t.Errorf("names of default repos = %v, want %v", repoNames, wantDefaultRepoNames)
			}
		})
	}
}

func TestResolveRepositoriesWithUserSearchContext(t *testing.T) {
	db := dbtest.NewDB(t, *dsn)

	const (
		wantName   = "alice"
		wantUserID = 123
	)
	queryInfo, err := query.ParseLiteral("foo")
	if err != nil {
		t.Fatal(err)
	}

	database.Mocks.Repos.ListRepoNames = func(ctx context.Context, op database.ReposListOptions) ([]types.RepoName, error) {
		if op.UserID != wantUserID {
			t.Fatalf("got %q, want %q", op.UserID, wantUserID)
		}
		return []types.RepoName{
			{
				ID:   1,
				Name: "example.com/a",
			},
			{
				ID:   2,
				Name: "example.com/b",
			},
			{
				ID:   3,
				Name: "example.com/c",
			},
			{
				ID:   4,
				Name: "external.com/a",
			},
			{
				ID:   5,
				Name: "external.com/b",
			},
			{
				ID:   6,
				Name: "external.com/c",
			},
		}, nil
	}
	database.Mocks.Repos.Count = func(ctx context.Context, op database.ReposListOptions) (int, error) { return 6, nil }
	database.Mocks.Namespaces.GetByName = func(ctx context.Context, name string) (*database.Namespace, error) {
		if name != wantName {
			t.Fatalf("got %q, want %q", name, wantName)
		}
		return &database.Namespace{Name: wantName, User: wantUserID}, nil
	}
	defer func() {
		database.Mocks.Repos.ListRepoNames = nil
		database.Mocks.Repos.Count = nil
		database.Mocks.Namespaces.GetByName = nil
	}()

	op := Options{
		Query:             queryInfo,
		SearchContextSpec: "@" + wantName,
	}
	repositoryResolver := &Resolver{DB: db}
	resolved, err := repositoryResolver.Resolve(context.Background(), op)
	if err != nil {
		t.Fatal(err)
	}
	var got []api.RepoName
	for _, rev := range resolved.RepoRevs {
		got = append(got, rev.Repo.Name)
	}
	sort.Slice(got, func(i, j int) bool {
		return got[i] < got[j]
	})
	want := []api.RepoName{
		"example.com/a",
		"example.com/b",
		"example.com/c",
		"external.com/a",
		"external.com/b",
		"external.com/c",
	}
	if diff := cmp.Diff(got, want, nil); diff != "" {
		t.Errorf("unexpected diff: %s", diff)
	}
}

func stringSliceToRevisionSpecifiers(revisions []string) []search.RevisionSpecifier {
	revisionSpecs := make([]search.RevisionSpecifier, 0, len(revisions))
	for _, revision := range revisions {
		revisionSpecs = append(revisionSpecs, search.RevisionSpecifier{RevSpec: revision})
	}
	return revisionSpecs
}

func TestResolveRepositoriesWithSearchContext(t *testing.T) {
	db := dbtest.NewDB(t, *dsn)
	searchContext := &types.SearchContext{ID: 1, Name: "searchcontext"}
	repoA := types.RepoName{ID: 1, Name: "example.com/a"}
	repoB := types.RepoName{ID: 2, Name: "example.com/b"}
	searchContextRepositoryRevisions := []*types.SearchContextRepositoryRevisions{
		{Repo: repoA, Revisions: []string{"branch-1", "branch-3"}},
		{Repo: repoB, Revisions: []string{"branch-2"}},
	}

	git.Mocks.ResolveRevision = func(spec string, opt git.ResolveRevisionOptions) (api.CommitID, error) {
		return api.CommitID(spec), nil
	}
	database.Mocks.Repos.ListRepoNames = func(ctx context.Context, op database.ReposListOptions) ([]types.RepoName, error) {
		if op.SearchContextID != searchContext.ID {
			t.Fatalf("got %q, want %q", op.SearchContextID, searchContext.ID)
		}
		return []types.RepoName{repoA, repoB}, nil
	}
	database.Mocks.Repos.Count = func(ctx context.Context, op database.ReposListOptions) (int, error) { return 2, nil }
	database.Mocks.SearchContexts.GetSearchContext = func(ctx context.Context, opts database.GetSearchContextOptions) (*types.SearchContext, error) {
		if opts.Name != searchContext.Name {
			t.Fatalf("got %q, want %q", opts.Name, searchContext.Name)
		}
		return searchContext, nil
	}
	database.Mocks.SearchContexts.GetSearchContextRepositoryRevisions = func(ctx context.Context, searchContextID int64) ([]*types.SearchContextRepositoryRevisions, error) {
		if searchContextID != searchContext.ID {
			t.Fatalf("got %q, want %q", searchContextID, searchContext.ID)
		}
		return searchContextRepositoryRevisions, nil
	}
	defer func() {
		git.Mocks.ResolveRevision = nil
		database.Mocks.Repos.ListRepoNames = nil
		database.Mocks.Repos.Count = nil
		database.Mocks.SearchContexts.GetSearchContext = nil
		database.Mocks.SearchContexts.GetSearchContextRepositoryRevisions = nil
	}()

	queryInfo, err := query.ParseLiteral("foo")
	if err != nil {
		t.Fatal(err)
	}
	op := Options{
		Query:             queryInfo,
		SearchContextSpec: "searchcontext",
	}
	repositoryResolver := &Resolver{DB: db}
	resolved, err := repositoryResolver.Resolve(context.Background(), op)
	if err != nil {
		t.Fatal(err)
	}
	wantRepositoryRevisions := []*search.RepositoryRevisions{
		{Repo: repoA, Revs: stringSliceToRevisionSpecifiers(searchContextRepositoryRevisions[0].Revisions)},
		{Repo: repoB, Revs: stringSliceToRevisionSpecifiers(searchContextRepositoryRevisions[1].Revisions)},
	}
	if !reflect.DeepEqual(resolved.RepoRevs, wantRepositoryRevisions) {
		t.Errorf("got repository revisions %+v, want %+v", resolved.RepoRevs, wantRepositoryRevisions)
	}
}
