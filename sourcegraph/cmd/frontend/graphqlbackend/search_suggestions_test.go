package graphqlbackend

import (
	"context"
	"reflect"
	"sync"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/inventory"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/db"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestSearchSuggestions(t *testing.T) {
	limitOffset := &db.LimitOffset{Limit: maxReposToSearch() + 1}

	getSuggestions := func(t *testing.T, query, version string) []string {
		t.Helper()
		r, err := (&schemaResolver{}).Search(context.Background(), &SearchArgs{Query: query, Version: version})
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

	mockSearchSymbols = func(ctx context.Context, args *search.TextParameters, limit int) (res []*FileMatchResolver, common *searchResultsCommon, err error) {
		// TODO test symbol suggestions
		return nil, nil, nil
	}
	defer func() { mockSearchSymbols = nil }()

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

	t.Run("single term", func(t *testing.T) {
		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		var calledReposListAll, calledReposListFoo bool
		db.Mocks.Repos.List = func(_ context.Context, op db.ReposListOptions) ([]*types.Repo, error) {

			// Validate that the following options are invariant
			// when calling the DB through Repos.List, no matter how
			// many times it is called for a single Search(...) operation.
			assertEqual(t, op.OnlyRepoIDs, true)
			assertEqual(t, op.LimitOffset, limitOffset)

			if reflect.DeepEqual(op.IncludePatterns, []string{"foo"}) {
				// when treating term as repo: field
				calledReposListFoo = true
				return []*types.Repo{{Name: "foo-repo"}}, nil
			} else {
				// when treating term as text query
				calledReposListAll = true
				return []*types.Repo{{Name: "bar-repo"}}, nil
			}
			return nil, nil
		}
		db.Mocks.Repos.Count = mockCount
		db.Mocks.Repos.MockGetByName(t, "repo", 1)
		backend.Mocks.Repos.MockResolveRev_NoCheck(t, api.CommitID("deadbeef"))

		defer func() { db.Mocks = db.MockStores{} }()
		git.Mocks.ResolveRevision = func(rev string, opt git.ResolveRevisionOptions) (api.CommitID, error) {
			return api.CommitID("deadbeef"), nil
		}
		defer git.ResetMocks()

		calledSearchFilesInRepos := false
		mockSearchFilesInRepos = func(args *search.TextParameters) ([]*FileMatchResolver, *searchResultsCommon, error) {
			calledSearchFilesInRepos = true
			if want := "foo"; args.PatternInfo.Pattern != want {
				t.Errorf("got %q, want %q", args.PatternInfo.Pattern, want)
			}
			return []*FileMatchResolver{
				{uri: "git://repo?rev#dir/file", JPath: "dir/file", Repo: &RepositoryResolver{repo: &types.Repo{Name: "repo"}}, CommitID: "rev"},
			}, &searchResultsCommon{}, nil
		}
		defer func() { mockSearchFilesInRepos = nil }()
		for _, v := range searchVersions {
			testSuggestions(t, "foo", v, []string{"repo:foo-repo", "file:dir/file"})
			if !calledReposListAll {
				t.Error("!calledReposListAll")
			}
			if !calledReposListFoo {
				t.Error("!calledReposListFoo")
			}
			if !calledSearchFilesInRepos {
				t.Error("!calledSearchFilesInRepos")
			}
		}
	})

	// This test is only valid for Regexp searches. Literal searches won't return suggestions for an invalid regexp.
	t.Run("single term invalid regex", func(t *testing.T) {
		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		sr, err := (&schemaResolver{}).Search(context.Background(), &SearchArgs{Query: "[foo", PatternType: nil, Version: "V1"})
		if err != nil {
			t.Fatal(err)
		}
		srr, err := sr.Results(context.Background())
		if err != nil {
			t.Fatal(err)
		}
		if len(srr.alert.proposedQueries) == 0 {
			t.Errorf("want an alert with some query suggestions")
		}
	})

	t.Run("repogroup: and single term", func(t *testing.T) {
		t.Skip("TODO(slimsag): this test is not reliable")
		var mu sync.Mutex

		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		var calledReposListReposInGroup, calledReposListFooRepo3 bool
		db.Mocks.Repos.List = func(_ context.Context, op db.ReposListOptions) ([]*types.Repo, error) {
			mu.Lock()
			defer mu.Unlock()
			wantReposInGroup := db.ReposListOptions{IncludePatterns: []string{`^foo-repo1$|^repo3$`}, LimitOffset: limitOffset}    // when treating term as repo: field
			wantFooRepo3 := db.ReposListOptions{IncludePatterns: []string{"foo", `^foo-repo1$|^repo3$`}, LimitOffset: limitOffset} // when treating term as repo: field
			if reflect.DeepEqual(op, wantReposInGroup) {
				calledReposListReposInGroup = true
				return []*types.Repo{
					{Name: "foo-repo1"},
					{Name: "repo3"},
				}, nil
			} else if reflect.DeepEqual(op, wantFooRepo3) {
				calledReposListFooRepo3 = true
				return []*types.Repo{{Name: "foo-repo1"}}, nil
			}
			t.Errorf("got %+v, want %+v or %+v", op, wantReposInGroup, wantFooRepo3)
			return nil, nil
		}
		db.Mocks.Repos.Count = mockCount
		defer func() { db.Mocks = db.MockStores{} }()
		db.Mocks.Repos.MockGetByName(t, "repo", 1)
		backend.Mocks.Repos.MockResolveRev_NoCheck(t, api.CommitID("deadbeef"))

		calledSearchFilesInRepos := false
		mockSearchFilesInRepos = func(args *search.TextParameters) ([]*FileMatchResolver, *searchResultsCommon, error) {
			mu.Lock()
			defer mu.Unlock()
			calledSearchFilesInRepos = true
			if args.PatternInfo.Pattern != "." && args.PatternInfo.Pattern != "foo" {
				t.Errorf("got %q, want %q", args.PatternInfo.Pattern, `"foo" or "."`)
			}
			return []*FileMatchResolver{
				{uri: "git://repo?rev#dir/foo-repo3-file-name-match", JPath: "dir/foo-repo3-file-name-match", Repo: &RepositoryResolver{repo: &types.Repo{Name: "repo3"}}, CommitID: "rev"},
				{uri: "git://repo?rev#dir/foo-repo1-file-name-match", JPath: "dir/foo-repo1-file-name-match", Repo: &RepositoryResolver{repo: &types.Repo{Name: "repo1"}}, CommitID: "rev"},
				{uri: "git://repo?rev#dir/file-content-match", JPath: "dir/file-content-match", Repo: &RepositoryResolver{repo: &types.Repo{Name: "repo"}}, CommitID: "rev"},
			}, &searchResultsCommon{}, nil
		}
		defer func() { mockSearchFilesInRepos = nil }()

		calledResolveRepoGroups := false
		mockResolveRepoGroups = func() (map[string][]*types.Repo, error) {
			mu.Lock()
			defer mu.Unlock()
			calledResolveRepoGroups = true
			return map[string][]*types.Repo{
				"baz": {
					&types.Repo{Name: "foo-repo1"},
					&types.Repo{Name: "repo3"},
				},
			}, nil
		}
		defer func() { mockResolveRepoGroups = nil }()
		for _, v := range searchVersions {
			testSuggestions(t, "repogroup:baz foo", v, []string{"repo:foo-repo1", "file:dir/foo-repo3-file-name-match", "file:dir/foo-repo1-file-name-match", "file:dir/file-content-match"})
			if !calledReposListReposInGroup {
				t.Error("!calledReposListReposInGroup")
			}
			if !calledReposListFooRepo3 {
				t.Error("!calledReposListFooRepo3")
			}
			if !calledSearchFilesInRepos {
				t.Error("!calledSearchFilesInRepos")
			}
			if !calledResolveRepoGroups {
				t.Error("!calledResolveRepoGroups")
			}

		}
	})

	t.Run("repo: field", func(t *testing.T) {
		var mu sync.Mutex

		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		calledReposList := false
		db.Mocks.Repos.List = func(_ context.Context, op db.ReposListOptions) ([]*types.Repo, error) {
			mu.Lock()
			defer mu.Unlock()
			calledReposList = true

			// Validate that the following options are invariant
			// when calling the DB through Repos.List, no matter how
			// many times it is called for a single Search(...) operation.
			assertEqual(t, op.OnlyRepoIDs, true)
			assertEqual(t, op.LimitOffset, limitOffset)
			assertEqual(t, op.IncludePatterns, []string{"foo"})

			return []*types.Repo{{Name: "foo-repo"}}, nil
		}
		db.Mocks.Repos.Count = mockCount
		defer func() { db.Mocks.Repos.List = nil }()

		// Mock to bypass language suggestions.
		mockShowLangSuggestions = func() ([]*searchSuggestionResolver, error) { return nil, nil }
		defer func() { mockShowLangSuggestions = nil }()

		calledSearchFilesInRepos := false
		mockSearchFilesInRepos = func(args *search.TextParameters) ([]*FileMatchResolver, *searchResultsCommon, error) {
			mu.Lock()
			defer mu.Unlock()
			calledSearchFilesInRepos = true
			if want := "foo-repo"; len(args.Repos) != 1 || string(args.Repos[0].Repo.Name) != want {
				t.Errorf("got %q, want %q", args.Repos, want)
			}
			return []*FileMatchResolver{
				{uri: "git://foo-repo?rev#dir/file", JPath: "dir/file", Repo: &RepositoryResolver{repo: &types.Repo{Name: "foo-repo"}}, CommitID: ""},
			}, &searchResultsCommon{}, nil
		}
		defer func() { mockSearchFilesInRepos = nil }()

		for _, v := range searchVersions {
			testSuggestions(t, "repo:foo", v, []string{"repo:foo-repo", "file:dir/file"})
			if !calledReposList {
				t.Error("!calledReposList")
			}
			if !calledSearchFilesInRepos {
				t.Error("!calledSearchFilesInRepos")
			}
		}
	})

	t.Run("repo: field for language suggestions", func(t *testing.T) {
		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		db.Mocks.Repos.List = func(_ context.Context, have db.ReposListOptions) ([]*types.Repo, error) {
			want := db.ReposListOptions{
				IncludePatterns: []string{"foo"},
				OnlyRepoIDs:     true,
				LimitOffset: &db.LimitOffset{
					Limit: 1,
				},
			}
			if !reflect.DeepEqual(have, want) {
				t.Error(cmp.Diff(have, want))
			}
			return []*types.Repo{{Name: "foo-repo"}}, nil
		}
		db.Mocks.Repos.Count = mockCount
		defer func() { db.Mocks.Repos.List = nil }()

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
		mockShowRepoSuggestions = func() ([]*searchSuggestionResolver, error) { return nil, nil }
		defer func() { mockShowRepoSuggestions = nil }()
		mockShowFileSuggestions = func() ([]*searchSuggestionResolver, error) { return nil, nil }
		defer func() { mockShowFileSuggestions = nil }()
		mockShowSymbolMatches = func() ([]*searchSuggestionResolver, error) { return nil, nil }
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

		calledReposList := false
		db.Mocks.Repos.List = func(_ context.Context, op db.ReposListOptions) ([]*types.Repo, error) {
			mu.Lock()
			defer mu.Unlock()
			calledReposList = true

			// Validate that the following options are invariant
			// when calling the DB through Repos.List, no matter how
			// many times it is called for a single Search(...) operation.
			assertEqual(t, op.OnlyRepoIDs, true)
			assertEqual(t, op.LimitOffset, limitOffset)
			assertEqual(t, op.IncludePatterns, []string{"foo"})

			return []*types.Repo{{Name: "foo-repo"}}, nil
		}
		db.Mocks.Repos.Count = mockCount
		defer func() { db.Mocks.Repos.List = nil }()

		// Mock to bypass language suggestions.
		mockShowLangSuggestions = func() ([]*searchSuggestionResolver, error) { return nil, nil }
		defer func() { mockShowLangSuggestions = nil }()

		calledSearchFilesInRepos := false
		mockSearchFilesInRepos = func(args *search.TextParameters) ([]*FileMatchResolver, *searchResultsCommon, error) {
			mu.Lock()
			defer mu.Unlock()
			calledSearchFilesInRepos = true
			if want := "foo-repo"; len(args.Repos) != 1 || string(args.Repos[0].Repo.Name) != want {
				t.Errorf("got %q, want %q", args.Repos, want)
			}
			return []*FileMatchResolver{
				{uri: "git://foo-repo?rev#dir/bar-file", JPath: "dir/bar-file", Repo: &RepositoryResolver{repo: &types.Repo{Name: "foo-repo"}}, CommitID: ""},
			}, &searchResultsCommon{}, nil
		}
		defer func() { mockSearchFilesInRepos = nil }()

		for _, v := range searchVersions {
			testSuggestions(t, "repo:foo file:bar", v, []string{"file:dir/bar-file"})
			if !calledReposList {
				t.Error("!calledReposList")
			}
			if !calledSearchFilesInRepos {
				t.Error("!calledSearchFilesInRepos")
			}
		}
	})
}
