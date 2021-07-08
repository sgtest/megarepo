package result

import (
	"fmt"
	"sort"
	"strings"
	"testing"

	"github.com/hexops/autogold"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
)

func commitResult(repo, commit string) *CommitMatch {
	return &CommitMatch{
		Repo: types.RepoName{Name: api.RepoName(repo)},
		Commit: git.Commit{
			ID: api.CommitID(commit),
		},
	}
}

func diffResult(repo, commit string) *CommitMatch {
	return &CommitMatch{
		DiffPreview: &HighlightedString{},
		Repo:        types.RepoName{Name: api.RepoName(repo)},
		Commit: git.Commit{
			ID: api.CommitID(commit),
		},
	}
}

func repoResult(name string) *RepoMatch {
	return &RepoMatch{
		Name: api.RepoName(name),
	}
}

func fileResult(repo string, lineMatches []*LineMatch, symbolMatches []*SymbolMatch) *FileMatch {
	return &FileMatch{
		File: File{
			Repo: types.RepoName{Name: api.RepoName(repo)},
		},
		Symbols:     symbolMatches,
		LineMatches: lineMatches,
	}
}

func resultsToString(matches []Match) string {
	toString := func(match Match) string {
		switch v := match.(type) {
		case *FileMatch:
			symbols := []string{}
			for _, symbol := range v.Symbols {
				symbols = append(symbols, symbol.Symbol.Name)
			}
			lines := []string{}
			for _, line := range v.LineMatches {
				lines = append(lines, line.Preview)
			}
			return fmt.Sprintf("File{url:%s/%s,symbols:[%s],lineMatches:[%s]}", v.Repo.Name, v.Path, strings.Join(symbols, ","), strings.Join(lines, ","))
		case *CommitMatch:
			if v.DiffPreview != nil {
				return fmt.Sprintf("Diff:%s", v.URL())
			}
			return fmt.Sprintf("Commit:%s", v.URL())
		case *RepoMatch:
			return fmt.Sprintf("Repo:%s", v.URL())
		}
		return ""
	}

	var searchResultStrings []string
	for _, srr := range matches {
		searchResultStrings = append(searchResultStrings, toString(srr))
	}
	return strings.Join(searchResultStrings, ", ")
}

func TestUnionMerge(t *testing.T) {
	cases := []struct {
		left  []Match
		right []Match
		want  autogold.Value
	}{
		{
			left: []Match{
				diffResult("a", "a"),
				commitResult("a", "a"),
				repoResult("a"),
				fileResult("a", nil, nil),
			},
			right: []Match{},
			want:  autogold.Want("LeftOnly", "File{url:a/,symbols:[],lineMatches:[]}, Repo:/a, Commit:/a/-/commit/a, Diff:/a/-/commit/a"),
		},
		{
			left: []Match{
				diffResult("a", "a"),
				commitResult("a", "a"),
				repoResult("a"),
				fileResult("a", nil, nil),
			},
			want: autogold.Want("RightOnly", "File{url:a/,symbols:[],lineMatches:[]}, Repo:/a, Commit:/a/-/commit/a, Diff:/a/-/commit/a"),
		},
		{
			left: []Match{
				diffResult("a", "a"),
				commitResult("a", "a"),
				repoResult("a"),
				fileResult("a", nil, nil),
			},
			right: []Match{
				diffResult("b", "b"),
				commitResult("b", "b"),
				repoResult("b"),
				fileResult("b", nil, nil),
			},
			want: autogold.Want("MergeAllDifferent", "File{url:a/,symbols:[],lineMatches:[]}, Repo:/a, Commit:/a/-/commit/a, Diff:/a/-/commit/a, File{url:b/,symbols:[],lineMatches:[]}, Repo:/b, Commit:/b/-/commit/b, Diff:/b/-/commit/b"),
		},
		{
			left: []Match{
				fileResult("b", []*LineMatch{
					{Preview: "a"},
					{Preview: "b"},
				}, nil),
			},
			right: []Match{
				fileResult("b", []*LineMatch{
					{Preview: "c"},
					{Preview: "d"},
				}, nil),
			},
			want: autogold.Want("MergeFileLineMatches", "File{url:b/,symbols:[],lineMatches:[a,b,c,d]}"),
		},
		{
			left: []Match{
				fileResult("a", []*LineMatch{
					{Preview: "a"},
					{Preview: "b"},
				}, nil),
			},
			right: []Match{
				fileResult("b", []*LineMatch{
					{Preview: "c"},
					{Preview: "d"},
				}, nil),
			},
			want: autogold.Want("NoMergeFileSymbols", "File{url:a/,symbols:[],lineMatches:[a,b]}, File{url:b/,symbols:[],lineMatches:[c,d]}"),
		},
		{
			left: []Match{
				fileResult("a", nil, []*SymbolMatch{
					{Symbol: Symbol{Name: "a"}},
					{Symbol: Symbol{Name: "b"}},
				}),
			},
			right: []Match{
				fileResult("a", nil, []*SymbolMatch{
					{Symbol: Symbol{Name: "c"}},
					{Symbol: Symbol{Name: "d"}},
				}),
			},
			want: autogold.Want("MergeFileSymbols", "File{url:a/,symbols:[a,b,c,d],lineMatches:[]}"),
		},
	}

	for _, tc := range cases {
		t.Run("", func(t *testing.T) {
			got := Union(tc.left, tc.right)
			sort.Sort(Matches(got))
			tc.want.Equal(t, resultsToString(got))
		})
	}
}
