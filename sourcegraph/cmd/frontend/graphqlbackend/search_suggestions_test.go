package graphqlbackend

import (
	"context"
	"os"
	"reflect"
	"sync"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/stretchr/testify/require"
	"go.uber.org/atomic"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtesting"
	"github.com/sourcegraph/sourcegraph/internal/inventory"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/internal/search/symbol"
	"github.com/sourcegraph/sourcegraph/internal/search/unindexed"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestSearchSuggestions(t *testing.T) {
	if os.Getenv("CI") != "" {
		// #25936: Some unit tests rely on external services that break
		// in CI but not locally. They should be removed or improved.
		t.Skip("TestSeachSuggestions only works in local dev and is not reliable in CI")
	}
	db := new(dbtesting.MockDB)

	getSuggestions := func(t *testing.T, query, version string) []string {
		t.Helper()
		r, err := (&schemaResolver{db: database.NewDB(db)}).Search(context.Background(), &SearchArgs{Query: query, Version: version})
		if err != nil {
			t.Fatal("Search:", err)
		}
		results, err := r.Suggestions(context.Background(), &searchSuggestionsArgs{})
		if err != nil {
			t.Fatal("Suggestions:", err)
		}
		resultDescriptions := make([]string, len(results))
		for i, result := range results {
			resultDescriptions[i] = testStringResult(result)
		}
		return resultDescriptions
	}
	testSuggestions := func(t *testing.T, query, version string, want []string) {
		t.Helper()
		got := getSuggestions(t, query, version)
		if !reflect.DeepEqual(got, want) {
			t.Errorf("got != want\ngot:  %v\nwant: %v", got, want)
		}
	}

	symbol.MockSearchSymbols = func(ctx context.Context, args *search.TextParameters, limit int) (res []result.Match, common *streaming.Stats, err error) {
		// TODO test symbol suggestions
		return nil, nil, nil
	}
	defer func() { symbol.MockSearchSymbols = nil }()

	mockDecodedViewerFinalSettings = &schema.Settings{}
	defer func() { mockDecodedViewerFinalSettings = nil }()

	searchVersions := []string{"V1", "V2"}

	t.Run("empty", func(t *testing.T) {
		for _, v := range searchVersions {
			testSuggestions(t, "", v, []string{})
		}
	})

	t.Run("whitespace", func(t *testing.T) {
		for _, v := range searchVersions {
			testSuggestions(t, " ", v, []string{})
		}
	})

	t.Run("no suggestions for predicate syntax", func(t *testing.T) {
		for _, v := range searchVersions {
			testSuggestions(t, "repo:contains(file:foo)", v, []string{})
		}
	})

	t.Run("no suggestions for search expressions", func(t *testing.T) {
		for _, v := range searchVersions {
			testSuggestions(t, "file:foo or file:bar", v, []string{})
		}
	})

	t.Run("single term", func(t *testing.T) {
		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		mu := sync.Mutex{}
		var calledReposListNamesAll, calledReposListFoo bool

		database.Mocks.Repos.ListMinimalRepos = func(_ context.Context, op database.ReposListOptions) ([]types.MinimalRepo, error) {
			mu.Lock()
			defer mu.Unlock()
			if reflect.DeepEqual(op.IncludePatterns, []string{"foo"}) {
				// when treating term as repo: field
				calledReposListFoo = true
				return []types.MinimalRepo{{Name: "foo-repo"}}, nil
			} else {
				// when treating term as text query
				calledReposListNamesAll = true
				return []types.MinimalRepo{{Name: "bar-repo"}}, nil
			}
		}
		database.Mocks.Repos.Count = mockCount
		database.Mocks.Repos.MockGetByName(t, "repo", 1)
		backend.Mocks.Repos.MockResolveRev_NoCheck(t, api.CommitID("deadbeef"))

		defer func() { database.Mocks = database.MockStores{} }()
		git.Mocks.ResolveRevision = func(rev string, opt git.ResolveRevisionOptions) (api.CommitID, error) {
			return api.CommitID("deadbeef"), nil
		}
		defer git.ResetMocks()

		calledSearchFilesInRepos := atomic.NewBool(false)
		unindexed.MockSearchFilesInRepos = func() ([]result.Match, *streaming.Stats, error) {
			calledSearchFilesInRepos.Store(true)
			fm := mkFileMatch(types.MinimalRepo{Name: "repo"}, "dir/file")
			rev := "rev"
			fm.CommitID = "rev"
			fm.InputRev = &rev
			return []result.Match{fm}, &streaming.Stats{}, nil
		}
		defer func() { unindexed.MockSearchFilesInRepos = nil }()
		for _, v := range searchVersions {
			testSuggestions(t, "foo", v, []string{"repo:foo-repo", "file:dir/file"})
			if !calledReposListNamesAll {
				t.Error("!calledReposListNamesAll")
			}
			if !calledReposListFoo {
				t.Error("!calledReposListFoo")
			}
			if !calledSearchFilesInRepos.Load() {
				t.Error("!calledSearchFilesInRepos")
			}
		}
	})

	t.Run("repo: field", func(t *testing.T) {
		var mu sync.Mutex

		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		calledReposListMinimalRepos := false
		database.Mocks.Repos.ListMinimalRepos = func(_ context.Context, op database.ReposListOptions) ([]types.MinimalRepo, error) {
			mu.Lock()
			defer mu.Unlock()
			calledReposListMinimalRepos = true

			require.Equal(t, []string{"foo"}, op.IncludePatterns)

			return []types.MinimalRepo{{Name: "foo-repo"}}, nil
		}
		database.Mocks.Repos.Count = mockCount
		defer func() { database.Mocks.Repos.ListMinimalRepos = nil }()

		// Mock to bypass language suggestions.
		mockShowLangSuggestions = func() ([]SearchSuggestionResolver, error) { return nil, nil }
		defer func() { mockShowLangSuggestions = nil }()

		calledSearchFilesInRepos := atomic.NewBool(false)
		unindexed.MockSearchFilesInRepos = func() ([]result.Match, *streaming.Stats, error) {
			mu.Lock()
			defer mu.Unlock()
			calledSearchFilesInRepos.Store(true)
			return []result.Match{&result.RepoMatch{Name: "foo-repo", ID: 23}},
				&streaming.Stats{},
				nil
		}
		defer func() { unindexed.MockSearchFilesInRepos = nil }()

		for _, v := range searchVersions {
			testSuggestions(t, "repo:foo", v, []string{"repo:foo-repo"})
			if !calledReposListMinimalRepos {
				t.Error("!calledReposListMinimalRepos")
			}
		}
	})

	t.Run("repo: field for language suggestions", func(t *testing.T) {
		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		database.Mocks.Repos.List = func(_ context.Context, have database.ReposListOptions) ([]*types.Repo, error) {
			want := database.ReposListOptions{
				IncludePatterns: []string{"foo"},
				LimitOffset: &database.LimitOffset{
					Limit: 1,
				},
			}
			if diff := cmp.Diff(have, want, cmp.AllowUnexported(database.ReposListOptions{})); diff != "" {
				t.Error(diff)
			}
			return []*types.Repo{{Name: "foo-repo"}}, nil
		}
		database.Mocks.Repos.ListMinimalRepos = func(_ context.Context, have database.ReposListOptions) ([]types.MinimalRepo, error) {
			want := database.ReposListOptions{
				IncludePatterns: []string{"foo"},
				LimitOffset: &database.LimitOffset{
					Limit: 1,
				},
			}
			if diff := cmp.Diff(have, want, cmp.AllowUnexported(database.ReposListOptions{})); diff != "" {
				t.Error(diff)
			}
			return []types.MinimalRepo{{Name: "foo-repo"}}, nil
		}
		database.Mocks.Repos.Count = mockCount
		defer func() { database.Mocks.Repos.List = nil }()
		defer func() { database.Mocks.Repos.ListMinimalRepos = nil }()
		git.Mocks.ResolveRevision = func(rev string, opt git.ResolveRevisionOptions) (api.CommitID, error) {
			return api.CommitID("deadbeef"), nil
		}
		defer git.ResetMocks()

		calledReposGetInventory := false
		backend.Mocks.Repos.GetInventory = func(_ context.Context, _ *types.Repo, _ api.CommitID) (*inventory.Inventory, error) {
			calledReposGetInventory = true
			return &inventory.Inventory{
				Languages: []inventory.Lang{
					{Name: "Go"},
					{Name: "TypeScript"},
					{Name: "Java"},
				},
			}, nil
		}
		defer func() { backend.Mocks.Repos.GetInventory = nil }()

		// Mock to bypass other suggestions.
		mockShowRepoSuggestions = func() ([]SearchSuggestionResolver, error) { return nil, nil }
		defer func() { mockShowRepoSuggestions = nil }()
		mockShowFileSuggestions = func() ([]SearchSuggestionResolver, error) { return nil, nil }
		defer func() { mockShowFileSuggestions = nil }()
		mockShowSymbolMatches = func() ([]SearchSuggestionResolver, error) { return nil, nil }
		defer func() { mockShowSymbolMatches = nil }()

		for _, v := range searchVersions {
			testSuggestions(t, "repo:foo@master", v, []string{"lang:go", "lang:java", "lang:typescript"})
			if !calledReposGetInventory {
				t.Error("!calledReposGetInventory")
			}
		}
	})

	t.Run("repo: and file: field", func(t *testing.T) {
		var mu sync.Mutex

		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		calledReposListMinimalRepos := false
		database.Mocks.Repos.ListMinimalRepos = func(_ context.Context, op database.ReposListOptions) ([]types.MinimalRepo, error) {
			mu.Lock()
			defer mu.Unlock()
			calledReposListMinimalRepos = true

			require.Equal(t, []string{"foo"}, op.IncludePatterns)

			return []types.MinimalRepo{{Name: "foo-repo"}}, nil
		}
		database.Mocks.Repos.Count = mockCount
		defer func() { database.Mocks.Repos.ListMinimalRepos = nil }()

		// Mock to bypass language suggestions.
		mockShowLangSuggestions = func() ([]SearchSuggestionResolver, error) { return nil, nil }
		defer func() { mockShowLangSuggestions = nil }()

		calledSearchFilesInRepos := atomic.NewBool(false)
		unindexed.MockSearchFilesInRepos = func() ([]result.Match, *streaming.Stats, error) {
			mu.Lock()
			defer mu.Unlock()
			calledSearchFilesInRepos.Store(true)
			return []result.Match{mkFileMatch(types.MinimalRepo{Name: "foo-repo"}, "dir/bar-file")}, &streaming.Stats{}, nil
		}
		defer func() { unindexed.MockSearchFilesInRepos = nil }()

		for _, v := range searchVersions {
			testSuggestions(t, "repo:foo file:bar", v, []string{"file:dir/bar-file"})
			if !calledReposListMinimalRepos {
				t.Error("!calledReposListMinimalRepos")
			}
			if !calledSearchFilesInRepos.Load() {
				t.Error("!calledSearchFilesInRepos")
			}
		}
	})
}
