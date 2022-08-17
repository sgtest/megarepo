package streaming

import (
	"testing"
	"time"

	"github.com/hexops/autogold"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/types"
	streamhttp "github.com/sourcegraph/sourcegraph/internal/search/streaming/http"
)

type testAggregator struct {
	results map[string]int
}

func (r *testAggregator) AddResult(result *AggregationMatchResult, err error) {
	if err != nil {
		return
	}
	current, _ := r.results[result.Key.Group]
	r.results[result.Key.Group] = result.Count + current
}

func contentMatch(repo, path string, repoID int32, chunks ...string) *streamhttp.EventContentMatch {
	matches := make([]streamhttp.ChunkMatch, 0, len(chunks))
	for _, content := range chunks {
		matches = append(matches, streamhttp.ChunkMatch{
			Content:      content,
			ContentStart: streamhttp.Location{Offset: 0, Line: 1, Column: 0},
			Ranges: []streamhttp.Range{{
				Start: streamhttp.Location{Offset: 0, Line: 1, Column: 0},
				End:   streamhttp.Location{Offset: len(content), Line: 1, Column: len(content)},
			}},
		})
	}

	return &streamhttp.EventContentMatch{
		Type:         streamhttp.ContentMatchType,
		Path:         path,
		RepositoryID: repoID,
		Repository:   repo,
		ChunkMatches: matches,
	}
}

func repoMatch(repo string, repoID int32) *streamhttp.EventRepoMatch {
	return &streamhttp.EventRepoMatch{
		Type:         streamhttp.RepoMatchType,
		RepositoryID: repoID,
		Repository:   repo,
	}
}

func pathMatch(repo, path string, repoID int32) *streamhttp.EventPathMatch {
	return &streamhttp.EventPathMatch{
		Type:         streamhttp.PathMatchType,
		RepositoryID: repoID,
		Repository:   repo,
		Path:         path,
	}
}

func symbolMatch(repo, path string, repoID int32, symbols ...string) *streamhttp.EventSymbolMatch {
	eventSymbols := []streamhttp.Symbol{}
	for _, s := range symbols {
		eventSymbols = append(eventSymbols, streamhttp.Symbol{Name: s})
	}

	return &streamhttp.EventSymbolMatch{
		Type:         streamhttp.SymbolMatchType,
		RepositoryID: repoID,
		Repository:   repo,
		Path:         path,
		Symbols:      eventSymbols,
	}
}

func commitMatch(repo, author string, date time.Time, repoID, numRanges int32, content string) *streamhttp.EventCommitMatch {
	ranges := [][3]int32{}
	for i := 0; i < int(numRanges); i++ {
		ranges = append(ranges, [3]int32{1, 1, int32(len(content))})
	}

	return &streamhttp.EventCommitMatch{
		Type:         streamhttp.CommitMatchType,
		RepositoryID: repoID,
		Repository:   repo,
		AuthorName:   author,
		AuthorDate:   date,
		Content:      content,
		Ranges:       ranges,
	}
}

var sampleDate time.Time = time.Date(2022, time.April, 1, 0, 0, 0, 0, time.UTC)

func TestRepoAggregation(t *testing.T) {
	testCases := []struct {
		mode          types.SearchAggregationMode
		searchResults []streamhttp.EventMatch
		want          autogold.Value
	}{
		{types.REPO_AGGREGATION_MODE, []streamhttp.EventMatch{}, autogold.Want("No results", map[string]int{})},
		{
			types.REPO_AGGREGATION_MODE,
			[]streamhttp.EventMatch{contentMatch("myRepo", "file.go", 1, "a", "b")},
			autogold.Want("Single file match multiple results", map[string]int{"myRepo": 2}),
		},
		{
			types.REPO_AGGREGATION_MODE,
			[]streamhttp.EventMatch{
				contentMatch("myRepo", "file.go", 1, "a", "b"),
				contentMatch("myRepo", "file2.go", 1, "d", "e"),
			},
			autogold.Want("Multiple file match multiple results", map[string]int{"myRepo": 4}),
		},
		{
			types.REPO_AGGREGATION_MODE,
			[]streamhttp.EventMatch{
				contentMatch("myRepo", "file.go", 1, "a", "b"),
				contentMatch("myRepo2", "file2.go", 2, "a", "b"),
			},
			autogold.Want("Multiple repos multiple match", map[string]int{"myRepo": 2, "myRepo2": 2}),
		},
		{
			types.REPO_AGGREGATION_MODE,
			[]streamhttp.EventMatch{
				commitMatch("myRepo", "Author A", sampleDate, 1, 2, "a"),
				commitMatch("myRepo", "Author B", sampleDate, 1, 2, "b"),
			},
			autogold.Want("Count repos on commit matches", map[string]int{"myRepo": 2}),
		},
		{
			types.REPO_AGGREGATION_MODE,
			[]streamhttp.EventMatch{
				repoMatch("myRepo", 1),
				repoMatch("myRepo2", 2),
			},
			autogold.Want("Count repos on repo match", map[string]int{"myRepo": 1, "myRepo2": 1}),
		},
		{
			types.REPO_AGGREGATION_MODE,
			[]streamhttp.EventMatch{
				pathMatch("myRepo", "file1.go", 1),
				pathMatch("myRepo", "file2.go", 1),
				pathMatch("myRepoB", "file3.go", 2),
			},
			autogold.Want("Count repos on path matches", map[string]int{"myRepo": 2, "myRepoB": 1}),
		},
		{
			types.REPO_AGGREGATION_MODE,
			[]streamhttp.EventMatch{
				symbolMatch("myRepo", "file1.go", 1, "a", "b"),
				symbolMatch("myRepo", "file2.go", 1, "c", "d"),
			},
			autogold.Want("Count repos on symbol matches", map[string]int{"myRepo": 4}),
		},
	}
	for _, tc := range testCases {
		t.Run(tc.want.Name(), func(t *testing.T) {
			aggregator := testAggregator{results: make(map[string]int)}
			onMatch, _ := TabulateAggregationMatches(aggregator.AddResult, tc.mode)
			onMatch(tc.searchResults)
			tc.want.Equal(t, aggregator.results)
		})
	}
}

func TestAuthorAggregation(t *testing.T) {
	testCases := []struct {
		mode          types.SearchAggregationMode
		searchResults []streamhttp.EventMatch
		want          autogold.Value
	}{
		{types.AUTHOR_AGGREGATION_MODE, []streamhttp.EventMatch{}, autogold.Want("No results", map[string]int{})},
		{
			types.AUTHOR_AGGREGATION_MODE,
			[]streamhttp.EventMatch{contentMatch("myRepo", "file.go", 1, "a", "b")},
			autogold.Want("No author for content match", map[string]int{}),
		},
		{
			types.AUTHOR_AGGREGATION_MODE,
			[]streamhttp.EventMatch{symbolMatch("myRepo", "file.go", 1, "a", "b")},
			autogold.Want("No author for symbol match", map[string]int{}),
		},
		{
			types.AUTHOR_AGGREGATION_MODE,
			[]streamhttp.EventMatch{pathMatch("myRepo", "file.go", 1)},
			autogold.Want("No author for path match", map[string]int{}),
		},
		{
			types.AUTHOR_AGGREGATION_MODE,
			[]streamhttp.EventMatch{
				commitMatch("repoA", "Author A", sampleDate, 1, 2, "a"),
				commitMatch("repoA", "Author B", sampleDate, 1, 2, "a"),
				commitMatch("repoB", "Author B", sampleDate, 2, 2, "a"),
				commitMatch("repoB", "Author C", sampleDate, 2, 2, "a"),
			},
			autogold.Want("counts by author", map[string]int{"Author A": 1, "Author B": 2, "Author C": 1}),
		},
	}
	for _, tc := range testCases {
		t.Run(tc.want.Name(), func(t *testing.T) {
			aggregator := testAggregator{results: make(map[string]int)}
			onMatch, _ := TabulateAggregationMatches(aggregator.AddResult, tc.mode)
			onMatch(tc.searchResults)
			tc.want.Equal(t, aggregator.results)
		})
	}
}

func TestPathAggregation(t *testing.T) {
	testCases := []struct {
		mode          types.SearchAggregationMode
		searchResults []streamhttp.EventMatch
		want          autogold.Value
	}{
		{types.PATH_AGGREGATION_MODE, []streamhttp.EventMatch{}, autogold.Want("No results", map[string]int{})},
		{
			types.PATH_AGGREGATION_MODE,
			[]streamhttp.EventMatch{
				commitMatch("repoA", "Author A", sampleDate, 1, 2, "a"),
			},
			autogold.Want("no path for commit", map[string]int{}),
		},
		{
			types.PATH_AGGREGATION_MODE,
			[]streamhttp.EventMatch{
				repoMatch("myRepo", 1),
			},
			autogold.Want("no path on repo match", map[string]int{}),
		},
		{
			types.PATH_AGGREGATION_MODE,
			[]streamhttp.EventMatch{contentMatch("myRepo", "file.go", 1, "a", "b")},
			autogold.Want("Single file match multiple results", map[string]int{"file.go": 2}),
		},
		{
			types.PATH_AGGREGATION_MODE,
			[]streamhttp.EventMatch{
				contentMatch("myRepo", "file.go", 1, "a", "b"),
				contentMatch("myRepo", "file2.go", 1, "d", "e"),
			},
			autogold.Want("Multiple file match multiple results", map[string]int{"file.go": 2, "file2.go": 2}),
		},
		{
			types.PATH_AGGREGATION_MODE,
			[]streamhttp.EventMatch{
				contentMatch("myRepo", "file.go", 1, "a", "b"),
				contentMatch("myRepo2", "file.go", 2, "a", "b"),
			},
			autogold.Want("Multiple repos same file", map[string]int{"file.go": 4}),
		},
		{
			types.PATH_AGGREGATION_MODE,
			[]streamhttp.EventMatch{
				pathMatch("myRepo", "file1.go", 1),
				pathMatch("myRepo", "file2.go", 1),
				pathMatch("myRepoB", "file3.go", 2),
			},
			autogold.Want("Count paths on path matches", map[string]int{"file1.go": 1, "file2.go": 1, "file3.go": 1}),
		},
		{
			types.PATH_AGGREGATION_MODE,
			[]streamhttp.EventMatch{
				symbolMatch("myRepo", "file1.go", 1, "a", "b"),
				symbolMatch("myRepo", "file2.go", 1, "c", "d"),
			},
			autogold.Want("Count paths on symbol matches", map[string]int{"file1.go": 2, "file2.go": 2}),
		},
		{
			types.PATH_AGGREGATION_MODE,
			[]streamhttp.EventMatch{
				repoMatch("myRepo", 1),
				pathMatch("myRepo", "file1.go", 1),
				symbolMatch("myRepo", "file1.go", 1, "c", "d"),
				contentMatch("myRepo", "file.go", 1, "a", "b"),
			},
			autogold.Want("Count paths on multiple matche types", map[string]int{"file.go": 2, "file1.go": 3}),
		},
	}
	for _, tc := range testCases {
		t.Run(tc.want.Name(), func(t *testing.T) {
			aggregator := testAggregator{results: make(map[string]int)}
			onMatch, _ := TabulateAggregationMatches(aggregator.AddResult, tc.mode)
			onMatch(tc.searchResults)
			tc.want.Equal(t, aggregator.results)
		})
	}
}
