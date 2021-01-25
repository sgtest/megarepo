package graphqlbackend

import (
	"context"
	"fmt"
	"reflect"
	"sort"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/google/zoekt"
	"go.uber.org/atomic"

	searchrepos "github.com/sourcegraph/sourcegraph/cmd/frontend/internal/search/repos"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/search"
	searchbackend "github.com/sourcegraph/sourcegraph/internal/search/backend"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/schema"
)

var mockCount = func(_ context.Context, options database.ReposListOptions) (int, error) { return 0, nil }

func assertEqual(t *testing.T, got, want interface{}) {
	t.Helper()

	if diff := cmp.Diff(got, want); diff != "" {
		t.Fatalf("(-want +got):\n%s", diff)
	}
}

func TestSearchResults(t *testing.T) {
	limitOffset := &database.LimitOffset{Limit: searchrepos.SearchLimits().MaxRepos + 1}

	getResults := func(t *testing.T, query, version string) []string {
		r, err := (&schemaResolver{}).Search(context.Background(), &SearchArgs{Query: query, Version: version})
		if err != nil {
			t.Fatal("Search:", err)
		}
		results, err := r.Results(context.Background())
		if err != nil {
			t.Fatal("Results:", err)
		}
		resultDescriptions := make([]string, len(results.SearchResults))
		for i, result := range results.SearchResults {
			// NOTE: Only supports one match per line. If we need to test other cases,
			// just remove that assumption in the following line of code.
			switch m := result.(type) {
			case *RepositoryResolver:
				resultDescriptions[i] = fmt.Sprintf("repo:%s", m.innerRepo.Name)
			case *FileMatchResolver:
				resultDescriptions[i] = fmt.Sprintf("%s:%d", m.JPath, m.JLineMatches[0].JLineNumber)
			default:
				t.Fatal("unexpected result type", result)
			}
		}
		// dedup results since we expect our clients to do dedupping
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

		var calledReposListRepoNames bool
		database.Mocks.Repos.ListRepoNames = func(_ context.Context, op database.ReposListOptions) ([]*types.RepoName, error) {
			calledReposListRepoNames = true

			// Validate that the following options are invariant
			// when calling the DB through Repos.ListRepoNames, no matter how
			// many times it is called for a single Search(...) operation.
			assertEqual(t, op.LimitOffset, limitOffset)
			assertEqual(t, op.IncludePatterns, []string{"r", "p"})

			return []*types.RepoName{{ID: 1, Name: "repo"}}, nil
		}
		database.Mocks.Repos.MockGetByName(t, "repo", 1)
		database.Mocks.Repos.MockGet(t, 1)
		database.Mocks.Repos.Count = mockCount

		mockSearchFilesInRepos = func(args *search.TextParameters) ([]*FileMatchResolver, *streaming.Stats, error) {
			return nil, &streaming.Stats{}, nil
		}
		defer func() { mockSearchFilesInRepos = nil }()

		for _, v := range searchVersions {
			testCallResults(t, `repo:r repo:p`, v, []string{"repo:repo"})
			if !calledReposListRepoNames {
				t.Error("!calledReposListRepoNames")
			}
		}

	})

	t.Run("multiple terms regexp", func(t *testing.T) {
		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		var calledReposListRepoNames bool
		database.Mocks.Repos.ListRepoNames = func(_ context.Context, op database.ReposListOptions) ([]*types.RepoName, error) {
			calledReposListRepoNames = true

			// Validate that the following options are invariant
			// when calling the DB through Repos.List, no matter how
			// many times it is called for a single Search(...) operation.
			assertEqual(t, op.LimitOffset, limitOffset)

			return []*types.RepoName{{ID: 1, Name: "repo"}}, nil
		}
		defer func() { database.Mocks = database.MockStores{} }()
		database.Mocks.Repos.MockGetByName(t, "repo", 1)
		database.Mocks.Repos.MockGet(t, 1)
		database.Mocks.Repos.Count = mockCount

		calledSearchRepositories := false
		mockSearchRepositories = func(args *search.TextParameters) ([]SearchResultResolver, *streaming.Stats, error) {
			calledSearchRepositories = true
			return nil, &streaming.Stats{}, nil
		}
		defer func() { mockSearchRepositories = nil }()

		calledSearchSymbols := false
		mockSearchSymbols = func(ctx context.Context, args *search.TextParameters, limit int) (res []*FileMatchResolver, common *streaming.Stats, err error) {
			calledSearchSymbols = true
			if want := `(foo\d).*?(bar\*)`; args.PatternInfo.Pattern != want {
				t.Errorf("got %q, want %q", args.PatternInfo.Pattern, want)
			}
			// TODO return mock results here and assert that they are output as results
			return nil, nil, nil
		}
		defer func() { mockSearchSymbols = nil }()

		calledSearchFilesInRepos := atomic.NewBool(false)
		mockSearchFilesInRepos = func(args *search.TextParameters) ([]*FileMatchResolver, *streaming.Stats, error) {
			calledSearchFilesInRepos.Store(true)
			if want := `(foo\d).*?(bar\*)`; args.PatternInfo.Pattern != want {
				t.Errorf("got %q, want %q", args.PatternInfo.Pattern, want)
			}
			repo := &types.RepoName{ID: 1, Name: "repo"}
			fm := mkFileMatch(repo, "dir/file", 123)
			return []*FileMatchResolver{fm}, &streaming.Stats{}, nil
		}
		defer func() { mockSearchFilesInRepos = nil }()

		testCallResults(t, `foo\d "bar*"`, "V1", []string{"dir/file:123"})
		if !calledReposListRepoNames {
			t.Error("!calledReposListRepoNames")
		}
		if !calledSearchRepositories {
			t.Error("!calledSearchRepositories")
		}
		if !calledSearchFilesInRepos.Load() {
			t.Error("!calledSearchFilesInRepos")
		}
		if calledSearchSymbols {
			t.Error("calledSearchSymbols")
		}
	})

	t.Run("multiple terms literal", func(t *testing.T) {
		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		var calledReposListRepoNames bool
		database.Mocks.Repos.ListRepoNames = func(_ context.Context, op database.ReposListOptions) ([]*types.RepoName, error) {
			calledReposListRepoNames = true

			// Validate that the following options are invariant
			// when calling the DB through Repos.List, no matter how
			// many times it is called for a single Search(...) operation.
			assertEqual(t, op.LimitOffset, limitOffset)

			return []*types.RepoName{{ID: 1, Name: "repo"}}, nil
		}
		defer func() { database.Mocks = database.MockStores{} }()
		database.Mocks.Repos.MockGetByName(t, "repo", 1)
		database.Mocks.Repos.MockGet(t, 1)
		database.Mocks.Repos.Count = mockCount

		calledSearchRepositories := false
		mockSearchRepositories = func(args *search.TextParameters) ([]SearchResultResolver, *streaming.Stats, error) {
			calledSearchRepositories = true
			return nil, &streaming.Stats{}, nil
		}
		defer func() { mockSearchRepositories = nil }()

		calledSearchSymbols := false
		mockSearchSymbols = func(ctx context.Context, args *search.TextParameters, limit int) (res []*FileMatchResolver, common *streaming.Stats, err error) {
			calledSearchSymbols = true
			if want := `"foo\\d \"bar*\""`; args.PatternInfo.Pattern != want {
				t.Errorf("got %q, want %q", args.PatternInfo.Pattern, want)
			}
			// TODO return mock results here and assert that they are output as results
			return nil, nil, nil
		}
		defer func() { mockSearchSymbols = nil }()

		calledSearchFilesInRepos := atomic.NewBool(false)
		mockSearchFilesInRepos = func(args *search.TextParameters) ([]*FileMatchResolver, *streaming.Stats, error) {
			calledSearchFilesInRepos.Store(true)
			if want := `foo\\d "bar\*"`; args.PatternInfo.Pattern != want {
				t.Errorf("got %q, want %q", args.PatternInfo.Pattern, want)
			}
			repo := &types.RepoName{ID: 1, Name: "repo"}
			fm := mkFileMatch(repo, "dir/file", 123)
			return []*FileMatchResolver{fm}, &streaming.Stats{}, nil
		}
		defer func() { mockSearchFilesInRepos = nil }()

		testCallResults(t, `foo\d "bar*"`, "V2", []string{"dir/file:123"})
		if !calledReposListRepoNames {
			t.Error("!calledReposListRepoNames")
		}
		if !calledSearchRepositories {
			t.Error("!calledSearchRepositories")
		}
		if !calledSearchFilesInRepos.Load() {
			t.Error("!calledSearchFilesInRepos")
		}
		if calledSearchSymbols {
			t.Error("calledSearchSymbols")
		}
	})

	t.Run("test start time is not null when alert thrown", func(t *testing.T) {
		mockDecodedViewerFinalSettings = &schema.Settings{}
		defer func() { mockDecodedViewerFinalSettings = nil }()

		for _, v := range searchVersions {
			r, err := (&schemaResolver{}).Search(context.Background(), &SearchArgs{Query: `repo:*`, Version: v})
			if err != nil {
				t.Fatal("Search:", err)
			}

			results, err := r.Results(context.Background())
			if err != nil {
				t.Fatal("Search: ", err)
			}

			if results.start.IsZero() {
				t.Error("Start value is not set")
			}
		}
	})
}

func TestOrderedFuzzyRegexp(t *testing.T) {
	got := orderedFuzzyRegexp([]string{})
	if want := ""; got != want {
		t.Errorf("got %q, want %q", got, want)
	}

	got = orderedFuzzyRegexp([]string{"a"})
	if want := "a"; got != want {
		t.Errorf("got %q, want %q", got, want)
	}

	got = orderedFuzzyRegexp([]string{"a", "b|c"})
	if want := "(a).*?(b|c)"; got != want {
		t.Errorf("got %q, want %q", got, want)
	}
}

func TestProcessSearchPattern(t *testing.T) {
	cases := []struct {
		Name    string
		Pattern string
		Opts    *getPatternInfoOptions
		Want    string
	}{
		{
			Name:    "Regexp, no content field",
			Pattern: `search me`,
			Opts:    &getPatternInfoOptions{},
			Want:    "(search).*?(me)",
		},
		{
			Name:    "Regexp with content field",
			Pattern: `content:search`,
			Opts:    &getPatternInfoOptions{},
			Want:    "search",
		},
		{
			Name:    "Regexp with quoted content field",
			Pattern: `content:"search me"`,
			Opts:    &getPatternInfoOptions{},
			Want:    "search me",
		},
		{
			Name:    "Regexp with content field ignores default pattern",
			Pattern: `content:"search me" ignored`,
			Opts:    &getPatternInfoOptions{},
			Want:    "search me",
		},
		{
			Name:    "Literal with quoted content field means double quotes are not part of the pattern",
			Pattern: `content:"content:"`,
			Opts:    &getPatternInfoOptions{performLiteralSearch: true},
			Want:    "content:",
		},
		{
			Name:    "Literal with quoted content field containing quotes",
			Pattern: `content:"\"content:\""`,
			Opts:    &getPatternInfoOptions{performLiteralSearch: true},
			Want:    "\"content:\"",
		},
	}
	for _, tt := range cases {
		t.Run(tt.Name, func(t *testing.T) {
			q, _ := query.ParseAndCheck(tt.Pattern)
			got, _, _, _ := processSearchPattern(q, tt.Opts)
			if got != tt.Want {
				t.Fatalf("got %s\nwant %s", got, tt.Want)
			}
		})
	}
}

func TestIsPatternNegated(t *testing.T) {
	cases := []struct {
		name    string
		pattern string
		want    bool
	}{
		{
			name:    "simple negated pattern",
			pattern: "-content:foo",
			want:    true,
		},
		{
			name:    "compound query with negated content as first term",
			pattern: "-content:foo and bar",
			want:    false,
		},
		{
			name:    "compound query with negated content as last term",
			pattern: "bar and -content:foo",
			want:    false,
		},
		{
			name:    "simple query with content field but without negation",
			pattern: "content:foo",
			want:    false,
		},
	}

	for _, tt := range cases {
		t.Run(tt.name, func(t *testing.T) {
			q, err := query.ProcessAndOr(tt.pattern,
				query.ParserOptions{SearchType: query.SearchTypeLiteral, Globbing: false})
			if err != nil {
				t.Fatalf(err.Error())
			}
			got := isPatternNegated(q.(*query.AndOrQuery).Query)
			if got != tt.want {
				t.Fatalf("got %t\nwant %t", got, tt.want)
			}
		})
	}
}

func TestProcessSearchPatternAndOr(t *testing.T) {
	cases := []struct {
		name                string
		pattern             string
		searchType          query.SearchType
		opts                *getPatternInfoOptions
		wantPattern         string
		wantIsRegExp        bool
		wantIsStructuralPat bool
		wantIsNegated       bool
	}{
		{
			name:                "Simple content",
			pattern:             `content:foo`,
			searchType:          query.SearchTypeLiteral,
			opts:                &getPatternInfoOptions{},
			wantPattern:         "foo",
			wantIsRegExp:        true,
			wantIsStructuralPat: false,
			wantIsNegated:       false,
		},
		{
			name:                "Negated content",
			pattern:             `-content:foo`,
			searchType:          query.SearchTypeLiteral,
			opts:                &getPatternInfoOptions{},
			wantPattern:         "foo",
			wantIsRegExp:        true,
			wantIsStructuralPat: false,
			wantIsNegated:       true,
		},
	}
	for _, tt := range cases {
		t.Run(tt.name, func(t *testing.T) {
			q, err := query.ProcessAndOr(tt.pattern,
				query.ParserOptions{SearchType: tt.searchType, Globbing: false})
			if err != nil {
				t.Fatalf(err.Error())
			}

			pattern, isRegExp, isStructuralPat, isNegated := processSearchPattern(q, tt.opts)

			if want := tt.wantPattern; pattern != want {
				t.Fatalf("got %s\nwant %s", pattern, want)
			}

			if want := tt.wantIsRegExp; isRegExp != want {
				t.Fatalf("got %t\nwant %t", isRegExp, want)
			}

			if want := tt.wantIsStructuralPat; isStructuralPat != want {
				t.Fatalf("got %t\nwant %t", isStructuralPat, want)
			}

			if want := tt.wantIsNegated; isNegated != want {
				t.Fatalf("got %t\nwant %t", isNegated, want)
			}
		})
	}
}

func TestSearchResolver_getPatternInfo(t *testing.T) {
	normalize := func(p *search.TextPatternInfo) {
		if len(p.IncludePatterns) == 0 {
			p.IncludePatterns = nil
		}
		if p.FileMatchLimit == 0 {
			p.FileMatchLimit = defaultMaxSearchResults
		}
	}

	tests := map[string]search.TextPatternInfo{
		"p": {
			Pattern:  "p",
			IsRegExp: true,
		},
		"p1 p2": {
			Pattern:  "(p1).*?(p2)",
			IsRegExp: true,
		},
		"p case:yes": {
			Pattern:                      "p",
			IsRegExp:                     true,
			IsCaseSensitive:              true,
			PathPatternsAreCaseSensitive: true,
		},
		"p file:f": {
			Pattern:         "p",
			IsRegExp:        true,
			IncludePatterns: []string{"f"},
		},
		"p file:f1 file:f2": {
			Pattern:         "p",
			IsRegExp:        true,
			IncludePatterns: []string{"f1", "f2"},
		},
		"p -file:f": {
			Pattern:        "p",
			IsRegExp:       true,
			ExcludePattern: "f",
		},
		"p -file:f1 -file:f2": {
			Pattern:        "p",
			IsRegExp:       true,
			ExcludePattern: "f1|f2",
		},
		"p lang:graphql": {
			Pattern:         "p",
			IsRegExp:        true,
			IncludePatterns: []string{`\.graphql$|\.gql$|\.graphqls$`},
			Languages:       []string{"graphql"},
		},
		"p lang:graphql file:f": {
			Pattern:         "p",
			IsRegExp:        true,
			IncludePatterns: []string{"f", `\.graphql$|\.gql$|\.graphqls$`},
			Languages:       []string{"graphql"},
		},
		"p -lang:graphql file:f": {
			Pattern:         "p",
			IsRegExp:        true,
			IncludePatterns: []string{"f"},
			ExcludePattern:  `\.graphql$|\.gql$|\.graphqls$`,
		},
		"p -lang:graphql -file:f": {
			Pattern:        "p",
			IsRegExp:       true,
			ExcludePattern: `f|(\.graphql$|\.gql$|\.graphqls$)`,
		},
	}
	for queryStr, want := range tests {
		t.Run(queryStr, func(t *testing.T) {
			query, err := query.ParseRegexp(queryStr)
			if err != nil {
				t.Fatal(err)
			}
			sr := searchResolver{query: query}
			p, err := sr.getPatternInfo(nil)
			if err != nil {
				t.Fatal(err)
			}
			normalize(p)
			normalize(&want)
			if !reflect.DeepEqual(*p, want) {
				t.Errorf("\ngot  %+v\nwant %+v", *p, want)
			}
		})
	}
}

func TestSearchResolver_DynamicFilters(t *testing.T) {
	repo := &types.RepoName{Name: "testRepo"}
	repoMatch := &RepositoryResolver{
		innerRepo: repo.ToRepo(),
	}
	fileMatch := func(path string) *FileMatchResolver {
		return mkFileMatch(repo, path)
	}

	rev := "develop3.0"
	fileMatchRev := fileMatch("/testFile.md")
	fileMatchRev.InputRev = &rev

	type testCase struct {
		descr                             string
		searchResults                     []SearchResultResolver
		expectedDynamicFilterStrsRegexp   map[string]struct{}
		expectedDynamicFilterStrsGlobbing map[string]struct{}
	}

	tests := []testCase{

		{
			descr:         "single repo match",
			searchResults: []SearchResultResolver{repoMatch},
			expectedDynamicFilterStrsRegexp: map[string]struct{}{
				`repo:^testRepo$`: {},
			},
			expectedDynamicFilterStrsGlobbing: map[string]struct{}{
				`repo:testRepo`: {},
			},
		},

		{
			descr:         "single file match without revision in query",
			searchResults: []SearchResultResolver{fileMatch("/testFile.md")},
			expectedDynamicFilterStrsRegexp: map[string]struct{}{
				`repo:^testRepo$`: {},
				`lang:markdown`:   {},
			},
			expectedDynamicFilterStrsGlobbing: map[string]struct{}{
				`repo:testRepo`: {},
				`lang:markdown`: {},
			},
		},

		{
			descr:         "single file match with specified revision",
			searchResults: []SearchResultResolver{fileMatchRev},
			expectedDynamicFilterStrsRegexp: map[string]struct{}{
				`repo:^testRepo$@develop3.0`: {},
				`lang:markdown`:              {},
			},
			expectedDynamicFilterStrsGlobbing: map[string]struct{}{
				`repo:testRepo@develop3.0`: {},
				`lang:markdown`:            {},
			},
		},
		{
			descr:         "file match from a language with two file extensions, using first extension",
			searchResults: []SearchResultResolver{fileMatch("/testFile.ts")},
			expectedDynamicFilterStrsRegexp: map[string]struct{}{
				`repo:^testRepo$`: {},
				`lang:typescript`: {},
			},
			expectedDynamicFilterStrsGlobbing: map[string]struct{}{
				`repo:testRepo`:   {},
				`lang:typescript`: {},
			},
		},
		{
			descr:         "file match from a language with two file extensions, using second extension",
			searchResults: []SearchResultResolver{fileMatch("/testFile.tsx")},
			expectedDynamicFilterStrsRegexp: map[string]struct{}{
				`repo:^testRepo$`: {},
				`lang:typescript`: {},
			},
			expectedDynamicFilterStrsGlobbing: map[string]struct{}{
				`repo:testRepo`:   {},
				`lang:typescript`: {},
			},
		},
		{
			descr:         "file match which matches one of the common file filters",
			searchResults: []SearchResultResolver{fileMatch("/anything/node_modules/testFile.md")},
			expectedDynamicFilterStrsRegexp: map[string]struct{}{
				`repo:^testRepo$`:          {},
				`-file:(^|/)node_modules/`: {},
				`lang:markdown`:            {},
			},
			expectedDynamicFilterStrsGlobbing: map[string]struct{}{
				`repo:testRepo`: {},
				`-file:node_modules/** -file:**/node_modules/**`: {},
				`lang:markdown`: {},
			},
		},
		{
			descr:         "file match which matches one of the common file filters",
			searchResults: []SearchResultResolver{fileMatch("/node_modules/testFile.md")},
			expectedDynamicFilterStrsRegexp: map[string]struct{}{
				`repo:^testRepo$`:          {},
				`-file:(^|/)node_modules/`: {},
				`lang:markdown`:            {},
			},
			expectedDynamicFilterStrsGlobbing: map[string]struct{}{
				`repo:testRepo`: {},
				`-file:node_modules/** -file:**/node_modules/**`: {},
				`lang:markdown`: {},
			},
		},
		{
			descr:         "file match which matches one of the common file filters",
			searchResults: []SearchResultResolver{fileMatch("/foo_test.go")},
			expectedDynamicFilterStrsRegexp: map[string]struct{}{
				`repo:^testRepo$`:  {},
				`-file:_test\.go$`: {},
				`lang:go`:          {},
			},
			expectedDynamicFilterStrsGlobbing: map[string]struct{}{
				`repo:testRepo`:    {},
				`-file:**_test.go`: {},
				`lang:go`:          {},
			},
		},

		// If there are no search results, no filters should be displayed.
		{
			descr:                             "no results",
			searchResults:                     []SearchResultResolver{},
			expectedDynamicFilterStrsRegexp:   map[string]struct{}{},
			expectedDynamicFilterStrsGlobbing: map[string]struct{}{},
		},
		{
			descr:         "values containing spaces are quoted",
			searchResults: []SearchResultResolver{fileMatch("/.gitignore")},
			expectedDynamicFilterStrsRegexp: map[string]struct{}{
				`repo:^testRepo$`:    {},
				`lang:"ignore list"`: {},
			},
			expectedDynamicFilterStrsGlobbing: map[string]struct{}{
				`repo:testRepo`:      {},
				`lang:"ignore list"`: {},
			},
		},
	}

	mockDecodedViewerFinalSettings = &schema.Settings{}
	defer func() { mockDecodedViewerFinalSettings = nil }()

	var expectedDynamicFilterStrs map[string]struct{}
	for _, test := range tests {
		t.Run(test.descr, func(t *testing.T) {
			for _, globbing := range []bool{true, false} {
				mockDecodedViewerFinalSettings.SearchGlobbing = &globbing
				actualDynamicFilters := (&SearchResultsResolver{SearchResults: test.searchResults}).DynamicFilters(context.Background())
				actualDynamicFilterStrs := make(map[string]struct{})

				for _, filter := range actualDynamicFilters {
					actualDynamicFilterStrs[filter.Value()] = struct{}{}
				}

				if globbing {
					expectedDynamicFilterStrs = test.expectedDynamicFilterStrsGlobbing
				} else {
					expectedDynamicFilterStrs = test.expectedDynamicFilterStrsRegexp
				}

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

func TestRoundStr(t *testing.T) {
	tests := []struct {
		name string
		s    string
		want string
	}{
		{
			name: "empty",
			s:    "",
			want: "",
		},
		{
			name: "simple",
			s:    "19s",
			want: "19s",
		},
		{
			name: "decimal",
			s:    "19.99s",
			want: "20s",
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := roundStr(tt.s); got != tt.want {
				t.Errorf("roundStr() = %v, want %v", got, tt.want)
			}
		})
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
		}}

	hydratedRepo := &types.Repo{

		ID:           repoWithIDs.ID,
		ExternalRepo: repoWithIDs.ExternalRepo,
		Name:         repoWithIDs.Name,
		URI:          fmt.Sprintf("github.com/my-org/%s", repoWithIDs.Name),
		Description:  "This is a description of a repository",
		Fork:         false,
	}

	database.Mocks.Repos.Get = func(ctx context.Context, id api.RepoID) (*types.Repo, error) {
		return hydratedRepo, nil
	}

	database.Mocks.Repos.ListRepoNames = func(_ context.Context, op database.ReposListOptions) ([]*types.RepoName, error) {
		return []*types.RepoName{{ID: repoWithIDs.ID, Name: repoWithIDs.Name}}, nil
	}
	database.Mocks.Repos.Count = mockCount

	defer func() { database.Mocks = database.MockStores{} }()

	zoektRepo := &zoekt.RepoListEntry{
		Repository: zoekt.Repository{
			Name:     string(repoWithIDs.Name),
			Branches: []zoekt.RepositoryBranch{{Name: "HEAD", Version: "deadbeef"}},
		},
	}

	zoektFileMatches := []zoekt.FileMatch{{
		Score:      5.0,
		FileName:   fileName,
		Repository: string(repoWithIDs.Name), // Important: this needs to match a name in `repos`
		Branches:   []string{"master"},
		LineMatches: []zoekt.LineMatch{
			{
				Line: nil,
			},
		},
		Checksum: []byte{0, 1, 2},
	}}

	z := &searchbackend.Zoekt{
		Client: &searchbackend.FakeSearcher{
			Repos:  []*zoekt.RepoListEntry{zoektRepo},
			Result: &zoekt.SearchResult{Files: zoektFileMatches},
		},
		DisableCache: true,
	}

	ctx := context.Background()

	q, err := query.ParseLiteral(`foobar index:only count:350`)
	if err != nil {
		t.Fatal(err)
	}
	resolver := &searchResolver{query: q, zoekt: z, userSettings: &schema.Settings{}, reposMu: &sync.Mutex{}, resolved: &searchrepos.Resolved{}}
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

func TestCheckDiffCommitSearchLimits(t *testing.T) {
	cases := []struct {
		name        string
		resultType  string
		numRepoRevs int
		fields      []query.Node
		wantError   error
	}{
		{
			name:        "diff_search_warns_on_repos_greater_than_search_limit",
			resultType:  "diff",
			numRepoRevs: 51,
			wantError:   RepoLimitErr{ResultType: "diff", Max: 50},
		},
		{
			name:        "commit_search_warns_on_repos_greater_than_search_limit",
			resultType:  "commit",
			numRepoRevs: 51,
			wantError:   RepoLimitErr{ResultType: "commit", Max: 50},
		},
		{
			name:        "commit_search_warns_on_repos_greater_than_search_limit_with_time_filter",
			fields:      []query.Node{query.Parameter{Field: "after"}},
			resultType:  "commit",
			numRepoRevs: 20000,
			wantError:   TimeLimitErr{ResultType: "commit", Max: 10000},
		},
		{
			name:        "no_warning_when_commit_search_within_search_limit",
			resultType:  "commit",
			numRepoRevs: 50,
			wantError:   nil,
		},
		{
			name:        "no_search_limit_on_queries_including_after_filter",
			fields:      []query.Node{query.Parameter{Field: "after"}},
			resultType:  "commit",
			numRepoRevs: 200,
			wantError:   nil,
		},
		{
			name:        "no_search_limit_on_queries_including_before_filter",
			fields:      []query.Node{query.Parameter{Field: "before"}},
			resultType:  "commit",
			numRepoRevs: 200,
			wantError:   nil,
		},
	}

	for _, test := range cases {
		repoRevs := make([]*search.RepositoryRevisions, test.numRepoRevs)
		for i := range repoRevs {
			repoRevs[i] = &search.RepositoryRevisions{
				Repo: &types.RepoName{ID: api.RepoID(i)},
			}
		}

		haveErr := checkDiffCommitSearchLimits(
			context.Background(),
			&search.TextParameters{
				RepoPromise: (&search.Promise{}).Resolve(repoRevs),
				Query:       &query.AndOrQuery{Query: test.fields},
			},
			test.resultType)

		if diff := cmp.Diff(test.wantError, haveErr); diff != "" {
			t.Fatalf("test %s, mismatched error (-want, +got):\n%s", test.name, diff)
		}
	}
}

func Test_SearchResultsResolver_ApproximateResultCount(t *testing.T) {
	type fields struct {
		results             []SearchResultResolver
		searchResultsCommon streaming.Stats
		alert               *searchAlert
		start               time.Time
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
				results: []SearchResultResolver{&FileMatchResolver{}},
			},
			want: "1",
		},

		{
			name: "file matches limit hit",
			fields: fields{
				results:             []SearchResultResolver{&FileMatchResolver{}},
				searchResultsCommon: streaming.Stats{IsLimitHit: true},
			},
			want: "1+",
		},

		{
			name: "symbol matches",
			fields: fields{
				results: []SearchResultResolver{
					&FileMatchResolver{
						symbols: []*searchSymbolResult{
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
				results: []SearchResultResolver{
					&FileMatchResolver{
						symbols: []*searchSymbolResult{
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
				SearchResults: tt.fields.results,
				Stats:         tt.fields.searchResultsCommon,
				alert:         tt.fields.alert,
				start:         tt.fields.start,
			}
			if got := sr.ApproximateResultCount(); got != tt.want {
				t.Errorf("searchResultsResolver.ApproximateResultCount() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestSearchResolver_evaluateWarning(t *testing.T) {
	q, _ := query.ProcessAndOr("file:foo or file:bar", query.ParserOptions{SearchType: query.SearchTypeRegex, Globbing: false})
	wantPrefix := "I'm having trouble understanding that query."
	andOrQuery, _ := q.(*query.AndOrQuery)
	got, _ := (&searchResolver{}).evaluate(context.Background(), andOrQuery.Query)
	t.Run("warn for unsupported and/or query", func(t *testing.T) {
		if !strings.HasPrefix(got.alert.description, wantPrefix) {
			t.Fatalf("got alert description %s, want %s", got.alert.description, wantPrefix)
		}
	})

	_, err := query.ProcessAndOr("file:foo or or or", query.ParserOptions{SearchType: query.SearchTypeRegex, Globbing: false})
	gotAlert := alertForQuery("", err)
	t.Run("warn for unsupported ambiguous and/or query", func(t *testing.T) {
		if !strings.HasPrefix(gotAlert.description, wantPrefix) {
			t.Fatalf("got alert description %s, want %s", got.alert.description, wantPrefix)
		}
	})
}

func TestGetExactFilePatterns(t *testing.T) {
	tests := []struct {
		in   string
		want map[string]struct{}
	}{
		{
			in:   "file:foo.bar file:*.bas",
			want: map[string]struct{}{"foo.bar": {}},
		},
		{
			in:   "file:foo.bar file:foo.bas",
			want: map[string]struct{}{"foo.bar": {}, "foo.bas": {}},
		},
		{
			in:   "file:*.bar",
			want: map[string]struct{}{},
		},
		{
			in:   "repo:github.com/foo/bar file:foo.bar",
			want: map[string]struct{}{"foo.bar": {}},
		},
	}
	for _, tt := range tests {
		t.Run(tt.in, func(t *testing.T) {
			q, err := query.ProcessAndOr(tt.in, query.ParserOptions{Globbing: true, SearchType: query.SearchTypeLiteral})
			if err != nil {
				t.Fatal(err)
			}
			r := searchResolver{query: q, originalQuery: tt.in}
			if got := r.getExactFilePatterns(); !reflect.DeepEqual(got, tt.want) {
				t.Errorf("getExactFilePatterns() = %v, want %v", got, tt.want)
			}
		})
	}
}

func TestCompareSearchResults(t *testing.T) {
	makeResult := func(repo, file string) *FileMatchResolver {
		return &FileMatchResolver{
			Repo:  &RepositoryResolver{innerRepo: &types.Repo{Name: api.RepoName(repo)}},
			JPath: file,
		}
	}

	tests := []struct {
		name              string
		a                 *FileMatchResolver
		b                 *FileMatchResolver
		exactFilePatterns map[string]struct{}
		aIsLess           bool
	}{
		{
			name:              "prefer exact match",
			a:                 makeResult("arepo", "afile"),
			b:                 makeResult("arepo", "file"),
			exactFilePatterns: map[string]struct{}{"file": {}},
			aIsLess:           false,
		},
		{
			name:              "reverse a and b",
			a:                 makeResult("arepo", "file"),
			b:                 makeResult("arepo", "afile"),
			exactFilePatterns: map[string]struct{}{"file": {}},
			aIsLess:           true,
		},
		{
			name:              "alphabetical order if exactFilePatterns is empty",
			a:                 makeResult("arepo", "afile"),
			b:                 makeResult("arepo", "file"),
			exactFilePatterns: map[string]struct{}{},
			aIsLess:           true,
		},
		{
			name:              "alphabetical order if exactFilePatterns is nil",
			a:                 makeResult("arepo", "afile"),
			b:                 makeResult("arepo", "bfile"),
			exactFilePatterns: nil,
			aIsLess:           true,
		},
		{
			name:              "same length, different files",
			a:                 makeResult("arepo", "bfile"),
			b:                 makeResult("arepo", "afile"),
			exactFilePatterns: nil,
			aIsLess:           false,
		},
		{
			name:              "exact matches with different length",
			a:                 makeResult("arepo", "adir1/file"),
			b:                 makeResult("arepo", "dir1/file"),
			exactFilePatterns: map[string]struct{}{"file": {}},
			aIsLess:           false,
		},
		{
			name:              "exact matches with same length",
			a:                 makeResult("arepo", "dir2/file"),
			b:                 makeResult("arepo", "dir1/file"),
			exactFilePatterns: map[string]struct{}{"file": {}},
			aIsLess:           false,
		},
		{
			name:              "no match",
			a:                 makeResult("arepo", "afile"),
			b:                 makeResult("arepo", "bfile"),
			exactFilePatterns: map[string]struct{}{"file": {}},
			aIsLess:           true,
		},
		{
			name:              "different repo, 1 exact match",
			a:                 makeResult("arepo", "file"),
			b:                 makeResult("brepo", "afile"),
			exactFilePatterns: map[string]struct{}{"file": {}},
			aIsLess:           true,
		},
		{
			name:              "different repo, no exact patterns",
			a:                 makeResult("arepo", "file"),
			b:                 makeResult("brepo", "afile"),
			exactFilePatterns: nil,
			aIsLess:           true,
		},
		{
			name:              "different repo, 2 exact matches",
			a:                 makeResult("arepo", "file"),
			b:                 makeResult("brepo", "file"),
			exactFilePatterns: map[string]struct{}{"file": {}},
			aIsLess:           true,
		},
		{
			name:              "repo matches only",
			a:                 makeResult("arepo", ""),
			b:                 makeResult("brepo", ""),
			exactFilePatterns: nil,
			aIsLess:           true,
		},
		{
			name:              "repo match and file match, same repo",
			a:                 makeResult("arepo", "file"),
			b:                 makeResult("arepo", ""),
			exactFilePatterns: nil,
			aIsLess:           false,
		},
		{
			name:              "repo match and file match, different repos",
			a:                 makeResult("arepo", ""),
			b:                 makeResult("brepo", "file"),
			exactFilePatterns: nil,
			aIsLess:           true,
		},
		{
			name:              "prefer repo matches",
			a:                 makeResult("arepo", ""),
			b:                 makeResult("brepo", "file"),
			exactFilePatterns: map[string]struct{}{"file": {}},
			aIsLess:           true,
		},
	}
	for _, tt := range tests {
		t.Run("test", func(t *testing.T) {
			if got := compareSearchResults(tt.a, tt.b, tt.exactFilePatterns); got != tt.aIsLess {
				t.Errorf("compareSearchResults() = %v, aIsLess %v", got, tt.aIsLess)
			}
		})
	}
}

func TestEvaluateAnd(t *testing.T) {
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
			zoektMatches: 10,
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

	minimalRepos, _, zoektRepos := generateRepos(5000)

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			zoektFileMatches := generateZoektMatches(tt.zoektMatches)
			z := &searchbackend.Zoekt{
				Client: &searchbackend.FakeSearcher{
					Repos:  zoektRepos,
					Result: &zoekt.SearchResult{Files: zoektFileMatches, Stats: zoekt.Stats{FilesSkipped: tt.filesSkipped}},
				},
				DisableCache: true,
			}

			ctx := context.Background()

			database.Mocks.Repos.ListRepoNames = func(_ context.Context, op database.ReposListOptions) ([]*types.RepoName, error) {
				repoNames := make([]*types.RepoName, len(minimalRepos))
				for i := range minimalRepos {
					repoNames[i] = &types.RepoName{ID: minimalRepos[i].ID, Name: minimalRepos[i].Name}
				}
				return repoNames, nil
			}
			database.Mocks.Repos.Count = func(ctx context.Context, opt database.ReposListOptions) (int, error) {
				return len(minimalRepos), nil
			}
			defer func() { database.Mocks = database.MockStores{} }()

			q, err := query.ProcessAndOr(tt.query, query.ParserOptions{SearchType: query.SearchTypeLiteral})
			if err != nil {
				t.Fatal(err)
			}
			resolver := &searchResolver{query: q, zoekt: z, userSettings: &schema.Settings{}, reposMu: &sync.Mutex{}, resolved: &searchrepos.Resolved{}}
			results, err := resolver.Results(ctx)
			if err != nil {
				t.Fatal("Results:", err)
			}
			if tt.wantAlert {
				if results.alert == nil {
					t.Errorf("Expected results")
				}
			} else if int(results.MatchCount()) != len(zoektFileMatches) {
				t.Errorf("wrong results length. want=%d, have=%d\n", len(zoektFileMatches), results.MatchCount())
			}
		})
	}
}
