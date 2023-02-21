package result

import (
	"strings"
	"testing"
	"time"

	"github.com/hexops/autogold/v2"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/search/filter"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestSelect(t *testing.T) {
	t.Run("FileMatch", func(t *testing.T) {
		t.Run("symbols", func(t *testing.T) {
			data := &FileMatch{
				Symbols: []*SymbolMatch{
					{Symbol: Symbol{Name: "a()", Kind: "func"}},
					{Symbol: Symbol{Name: "b()", Kind: "function"}},
					{Symbol: Symbol{Name: "var c", Kind: "variable"}},
				},
			}

			test := func(input string) string {
				selectPath, _ := filter.SelectPathFromString(input)
				symbols := data.Select(selectPath).(*FileMatch).Symbols
				var values []string
				for _, s := range symbols {
					values = append(values, s.Symbol.Name+":"+s.Symbol.Kind)
				}
				return strings.Join(values, ", ")
			}

			autogold.Expect("a():func, b():function, var c:variable").Equal(t, test("symbol"))
			autogold.Expect("var c:variable").Equal(t, test("symbol.variable"))
		})

		t.Run("path match", func(t *testing.T) {
			fm := &FileMatch{
				PathMatches:  []Range{{}},
				ChunkMatches: []ChunkMatch{{}},
			}

			selected := fm.Select([]string{filter.Content})
			require.Empty(t, selected.(*FileMatch).PathMatches)
		})
	})

	t.Run("CommitMatch", func(t *testing.T) {
		type commitMatchTestCase struct {
			input      CommitMatch
			selectPath filter.SelectPath
			output     Match
		}

		t.Run("Message", func(t *testing.T) {
			testMessageMatch := CommitMatch{
				Repo:           types.MinimalRepo{Name: "testrepo"},
				MessagePreview: &MatchedString{Content: "test"},
			}

			cases := []commitMatchTestCase{{
				input:      testMessageMatch,
				selectPath: []string{filter.Commit},
				output:     &testMessageMatch,
			}, {
				input:      testMessageMatch,
				selectPath: []string{filter.Repository},
				output:     &RepoMatch{Name: "testrepo"},
			}, {
				input:      testMessageMatch,
				selectPath: []string{filter.File},
				output:     nil,
			}, {
				input:      testMessageMatch,
				selectPath: []string{filter.Commit, "diff", "added"},
				output:     nil,
			}, {
				input:      testMessageMatch,
				selectPath: []string{filter.Symbol},
				output:     nil,
			}, {
				input:      testMessageMatch,
				selectPath: []string{filter.Content},
				output:     nil,
			}}

			for _, tc := range cases {
				t.Run(tc.selectPath.String(), func(t *testing.T) {
					result := tc.input.Select(tc.selectPath)
					require.Equal(t, tc.output, result)
				})
			}
		})

		t.Run("Diff", func(t *testing.T) {
			diffContent := "file1 file2\n@@ -969,3 +969,2 @@ functioncontext\ncontextbefore\n-removed\n+added\ncontextafter\n"
			removedRange := Range{Start: Location{Offset: 63, Line: 3, Column: 1}, End: Location{Offset: 67, Line: 3, Column: 5}}
			addedRange := Range{Start: Location{Offset: 73, Line: 4, Column: 2}, End: Location{Offset: 77, Line: 4, Column: 6}}

			testDiffMatch := func() CommitMatch {
				return CommitMatch{
					Repo: types.MinimalRepo{Name: "testrepo"},
					DiffPreview: &MatchedString{
						Content:       diffContent,
						MatchedRanges: Ranges{addedRange, removedRange},
					},
				}
			}

			cases := []commitMatchTestCase{{
				input:      testDiffMatch(),
				selectPath: []string{filter.Commit},
				output:     func() *CommitMatch { c := testDiffMatch(); return &c }(),
			}, {
				input:      testDiffMatch(),
				selectPath: []string{filter.Repository},
				output:     &RepoMatch{Name: "testrepo"},
			}, {
				input:      testDiffMatch(),
				selectPath: []string{filter.File},
				output:     nil,
			}, {
				input:      testDiffMatch(),
				selectPath: []string{filter.Symbol},
				output:     nil,
			}, {
				input:      testDiffMatch(),
				selectPath: []string{filter.Content},
				output:     nil,
			}, {
				input:      testDiffMatch(),
				selectPath: []string{filter.Commit, "diff", "added"},
				output: &CommitMatch{
					Repo: types.MinimalRepo{Name: "testrepo"},
					DiffPreview: &MatchedString{
						Content:       diffContent,
						MatchedRanges: Ranges{addedRange},
					},
				},
			}, {
				input:      testDiffMatch(),
				selectPath: []string{filter.Commit, "diff", "removed"},
				output: &CommitMatch{
					Repo: types.MinimalRepo{Name: "testrepo"},
					DiffPreview: &MatchedString{
						Content:       diffContent,
						MatchedRanges: Ranges{removedRange},
					},
				},
			}}

			for _, tc := range cases {
				t.Run(tc.selectPath.String(), func(t *testing.T) {
					result := tc.input.Select(tc.selectPath)
					require.Equal(t, tc.output, result)
				})
			}
		})
	})
}

func TestKeyEquality(t *testing.T) {
	time1 := time.Now()
	time2 := time1
	time3 := time1.Add(10 * time.Second)

	cases := []struct {
		match1   Match
		match2   Match
		areEqual bool
	}{{
		match1:   &CommitMatch{Commit: gitdomain.Commit{ID: "test", Author: gitdomain.Signature{Date: time1}}},
		match2:   &CommitMatch{Commit: gitdomain.Commit{ID: "test", Author: gitdomain.Signature{Date: time2}}},
		areEqual: true,
	}, {
		match1:   &CommitMatch{Commit: gitdomain.Commit{ID: "test", Author: gitdomain.Signature{Date: time1}}},
		match2:   &CommitMatch{Commit: gitdomain.Commit{ID: "test", Author: gitdomain.Signature{Date: time3}}},
		areEqual: false,
	}, {
		match1:   &CommitMatch{Commit: gitdomain.Commit{ID: "test1"}},
		match2:   &CommitMatch{Commit: gitdomain.Commit{ID: "test2"}},
		areEqual: false,
	}}

	for _, tc := range cases {
		t.Run("", func(t *testing.T) {
			require.Equal(t, tc.areEqual, tc.match1.Key() == tc.match2.Key())
		})
	}
}
