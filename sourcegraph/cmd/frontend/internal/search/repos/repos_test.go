package repos

import (
	"context"
	"fmt"
	"reflect"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/google/zoekt"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/search"
	searchbackend "github.com/sourcegraph/sourcegraph/internal/search/backend"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
)

type mockNamespaceStore struct {
	GetByNameMock func(ctx context.Context, name string) (*database.Namespace, error)
}

func (ns *mockNamespaceStore) GetByName(ctx context.Context, name string) (*database.Namespace, error) {
	return ns.GetByNameMock(ctx, name)
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

	database.Mocks.Repos.ListRepoNames = func(ctx context.Context, opts database.ReposListOptions) ([]*types.RepoName, error) {
		return []*types.RepoName{{Name: "repoFoo"}}, nil
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
				Repo: &types.RepoName{Name: "repoFoo"},
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
				Repo: &types.RepoName{Name: "repoFoo"},
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
				Repo: &types.RepoName{Name: "repoFoo"},
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
				Repo: &types.RepoName{Name: "repoFoo"},
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
				Repo: &types.RepoName{Name: "repoFoo"},
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
			repositoryResolver := &Resolver{NamespaceStore: &mockNamespaceStore{}}
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

			var drs []*types.RepoName
			for i, name := range tc.defaultsInDb {
				r := &types.RepoName{
					ID:   api.RepoID(i),
					Name: api.RepoName(name),
				}
				drs = append(drs, r)
			}
			getRawDefaultRepos := func(ctx context.Context) ([]*types.RepoName, error) {
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

func TestHasTypeRepo(t *testing.T) {
	tests := []struct {
		query           string
		wantHasTypeRepo bool
	}{
		{
			query:           "sourcegraph type:repo",
			wantHasTypeRepo: true,
		},
		{
			query:           "sourcegraph type:symbol type:repo",
			wantHasTypeRepo: true,
		},
		{
			query:           "(sourcegraph type:repo) or (goreman type:repo)",
			wantHasTypeRepo: true,
		},
		{
			query:           "sourcegraph repohasfile:Dockerfile type:repo",
			wantHasTypeRepo: true,
		},
		{
			query:           "repo:sourcegraph type:repo",
			wantHasTypeRepo: true,
		},
		{
			query:           "repo:sourcegraph",
			wantHasTypeRepo: false,
		},
		{
			query:           "repository",
			wantHasTypeRepo: false,
		},
		{
			query:           "",
			wantHasTypeRepo: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.query, func(t *testing.T) {
			q, err := query.ProcessAndOr(tt.query, query.ParserOptions{SearchType: query.SearchTypeLiteral})
			if err != nil {
				t.Fatal(err)
			}
			if got := hasTypeRepo(q); got != tt.wantHasTypeRepo {
				t.Fatalf("got %t, expected %t", got, tt.wantHasTypeRepo)
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
	defaultRepos := make([]*types.RepoName, len(wantDefaultRepoNames))
	zoektRepoListEntries := make([]*zoekt.RepoListEntry, len(wantDefaultRepoNames))
	mockDefaultReposFunc := func(_ context.Context) ([]*types.RepoName, error) {
		return defaultRepos, nil
	}

	for idx, name := range wantDefaultRepoNames {
		defaultRepos[idx] = &types.RepoName{Name: api.RepoName(name)}
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
			repositoryResolver := &Resolver{Zoekt: mockZoekt, DefaultReposFunc: mockDefaultReposFunc, NamespaceStore: &mockNamespaceStore{}}
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
	orig := envvar.SourcegraphDotComMode()
	envvar.MockSourcegraphDotComMode(true)
	defer envvar.MockSourcegraphDotComMode(orig)

	const (
		wantName   = "alice"
		wantUserID = 123
	)
	queryInfo, err := query.ParseLiteral("foo")
	if err != nil {
		t.Fatal(err)
	}

	database.Mocks.Repos.ListRepoNames = func(ctx context.Context, op database.ReposListOptions) ([]*types.RepoName, error) {
		if op.UserID != wantUserID {
			t.Errorf("got %q, want %q", op.UserID, wantUserID)
		}
		return []*types.RepoName{}, nil
	}
	database.Mocks.Repos.Count = func(ctx context.Context, op database.ReposListOptions) (int, error) { return 0, nil }
	defer func() {
		database.Mocks.Repos.ListRepoNames = nil
		database.Mocks.Repos.Count = nil
	}()

	getNamespaceByName := func(ctx context.Context, name string) (*database.Namespace, error) {
		if name != wantName {
			t.Errorf("got %q, want %q", name, wantName)
		}
		return &database.Namespace{Name: wantName, User: wantUserID}, nil
	}
	namespaceStore := &mockNamespaceStore{GetByNameMock: getNamespaceByName}

	op := Options{
		Query:             queryInfo,
		SearchContextSpec: "@" + wantName,
	}
	repositoryResolver := &Resolver{NamespaceStore: namespaceStore}
	_, err = repositoryResolver.Resolve(context.Background(), op)
	if err != nil {
		t.Fatal(err)
	}
}
