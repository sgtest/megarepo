package result

import (
	"net/url"
	"path"
	"strings"
	"unicode/utf8"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/search/filter"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

// File represents all the information we need to identify a file in a repository
type File struct {
	// InputRev is the Git revspec that the user originally requested to search. It is used to
	// preserve the original revision specifier from the user instead of navigating them to the
	// absolute commit ID when they select a result.
	InputRev *string           `json:"-"`
	Repo     types.MinimalRepo `json:"-"`
	CommitID api.CommitID      `json:"-"`
	Path     string
}

func (f *File) URL() *url.URL {
	var path strings.Builder
	path.Grow(len("/@/-/blob/") + len(f.Repo.Name) + len(f.Path) + 20)
	path.WriteRune('/')
	path.WriteString(string(f.Repo.Name))
	if f.InputRev != nil && len(*f.InputRev) > 0 {
		path.WriteRune('@')
		path.WriteString(*f.InputRev)
	}
	path.WriteString("/-/blob/")
	path.WriteString(f.Path)
	return &url.URL{Path: path.String()}
}

// FileMatch represents either:
// - A collection of symbol results (len(Symbols) > 0)
// - A collection of text content results (len(LineMatches) > 0)
// - A result representing the whole file (len(Symbols) == 0 && len(LineMatches) == 0)
type FileMatch struct {
	File

	ChunkMatches ChunkMatches
	Symbols      []*SymbolMatch `json:"-"`

	LimitHit bool
}

func (fm *FileMatch) RepoName() types.MinimalRepo {
	return fm.File.Repo
}

func (fm *FileMatch) searchResultMarker() {}

func (fm *FileMatch) ResultCount() int {
	rc := len(fm.Symbols) + fm.ChunkMatches.MatchCount()
	if rc == 0 {
		return 1 // 1 to count "empty" results like type:path results
	}
	return rc
}

// IsPathMatch returns true if a `FileMatch` has no line or symbol matches. In
// the absence of a true `PathMatch` type, we use this function as a proxy
// signal to drive `select:file` logic that deduplicates path results.
func (fm *FileMatch) IsPathMatch() bool {
	return len(fm.ChunkMatches) == 0 && len(fm.Symbols) == 0
}

func (fm *FileMatch) Select(selectPath filter.SelectPath) Match {
	switch selectPath.Root() {
	case filter.Repository:
		return &RepoMatch{
			Name: fm.Repo.Name,
			ID:   fm.Repo.ID,
		}
	case filter.File:
		fm.ChunkMatches = nil
		fm.Symbols = nil
		if len(selectPath) > 1 && selectPath[1] == "directory" {
			fm.Path = path.Clean(path.Dir(fm.Path)) + "/" // Add trailing slash for clarity.
		}
		return fm
	case filter.Symbol:
		if len(fm.Symbols) > 0 {
			fm.ChunkMatches = nil // Only return symbol match if symbols exist
			if len(selectPath) > 1 {
				filteredSymbols := SelectSymbolKind(fm.Symbols, selectPath[1])
				if len(filteredSymbols) == 0 {
					return nil // Remove file match if there are no symbol results after filtering
				}
				fm.Symbols = filteredSymbols
			}
			return fm
		}
		return nil
	case filter.Content:
		// Only return file match if line matches exist
		if len(fm.ChunkMatches) > 0 {
			fm.Symbols = nil
			return fm
		}
		return nil
	case filter.Commit:
		return nil
	}
	return nil
}

// AppendMatches appends the line matches from src as well as updating match
// counts and limit.
func (fm *FileMatch) AppendMatches(src *FileMatch) {
	// TODO merge hunk matches smartly
	fm.ChunkMatches = append(fm.ChunkMatches, src.ChunkMatches...)
	fm.Symbols = append(fm.Symbols, src.Symbols...)
	fm.LimitHit = fm.LimitHit || src.LimitHit
}

// Limit will mutate fm such that it only has limit results. limit is a number
// greater than 0.
//
//   if limit >= ResultCount then nothing is done and we return limit - ResultCount.
//   if limit < ResultCount then ResultCount becomes limit and we return 0.
func (fm *FileMatch) Limit(limit int) int {
	matchCount := fm.ChunkMatches.MatchCount()
	symbolCount := len(fm.Symbols)

	// An empty FileMatch should still count against the limit -- see *FileMatch.ResultCount()
	if matchCount == 0 && symbolCount == 0 {
		return limit - 1
	}

	if limit < matchCount {
		fm.ChunkMatches.Limit(limit)
		limit = 0
		fm.LimitHit = true
	} else {
		limit -= matchCount
	}

	if limit < symbolCount {
		fm.Symbols = fm.Symbols[:limit]
		limit = 0
		fm.LimitHit = true
	} else {
		limit -= symbolCount
	}
	return limit
}

func (fm *FileMatch) Key() Key {
	k := Key{
		TypeRank: rankFileMatch,
		Repo:     fm.Repo.Name,
		Commit:   fm.CommitID,
		Path:     fm.Path,
	}

	if fm.InputRev != nil {
		k.Rev = *fm.InputRev
	}

	return k
}

// ChunkMatch stores the smallest (and contiguous) line range of file content
// corresponding to the set of ranges. We represent it this way so we always
// have the complete line available to clients for display purposes and we
// aways have the complete content of the matched range available for further
// computation.
type ChunkMatch struct {
	// Content contains the lines overlapped by Ranges. Content will always
	// contain full lines. This means the slice of file content contained
	// in Content will always be:
	// 1) preceded by the beginning of the file or a newline, and
	// 2) succeeded by the end of the file or a newline.
	Content string

	// ContentStart is the location of the first character in Content. Since
	// Content always starts at the beginning of a line, Column should always
	// be set to zero.
	ContentStart Location

	// Ranges is the set of matches for this hunk. Each represents a range of
	// the matched file that is fully contained by the range represented by
	// Content. Ranges are relative to the beginning of the file, not the
	// beginning of Content. This type provides no guarantees about the
	// ordering of ranges, and also does not guarantee that the ranges are
	// non-overlapping.
	Ranges Ranges
}

// MatchedContent returns the content matched by the ranges in this ChunkMatch.
func (h ChunkMatch) MatchedContent() []string {
	// Create a new set of ranges whose offsets are
	// relative to the start of the content.
	relRanges := h.Ranges.Sub(h.ContentStart)
	res := make([]string, 0, len(relRanges))
	for _, rr := range relRanges {
		res = append(res, h.Content[rr.Start.Offset:rr.End.Offset])
	}
	return res
}

// AsLineMatches facilitates converting from ChunkMatch to a set of LineMatches.
// This loses information like byte offsets and the logical relationship
// between lines in a multiline match, but it allows us to keep providing the
// LineMatch representation for clients without breaking backwards compatibility.
func (h ChunkMatch) AsLineMatches() []*LineMatch {
	lines := strings.Split(h.Content, "\n")
	lineMatches := make([]*LineMatch, len(lines))
	for i, line := range lines {
		lineNumber := h.ContentStart.Line + i
		var offsetAndLengths [][2]int32
		for _, rr := range h.Ranges {
			for rangeLine := rr.Start.Line; rangeLine <= rr.End.Line; rangeLine++ {
				if rangeLine == lineNumber {
					start := 0
					if rangeLine == rr.Start.Line {
						start = rr.Start.Column
					}

					end := utf8.RuneCountInString(line)
					if rangeLine == rr.End.Line {
						end = rr.End.Column
					}

					offsetAndLengths = append(offsetAndLengths, [2]int32{int32(start), int32(end - start)})
				}
			}
		}
		lineMatches[i] = &LineMatch{
			Preview:          line,
			LineNumber:       int32(lineNumber),
			OffsetAndLengths: offsetAndLengths,
		}
	}
	return lineMatches
}

type ChunkMatches []ChunkMatch

func (hs ChunkMatches) AsLineMatches() []*LineMatch {
	res := make([]*LineMatch, 0, len(hs))
	for _, h := range hs {
		res = append(res, h.AsLineMatches()...)
	}
	return res
}

func (hs ChunkMatches) MatchCount() int {
	count := 0
	for _, h := range hs {
		count += len(h.Ranges)
	}
	return count
}

func (hs *ChunkMatches) Limit(limit int) {
	matches := *hs
	for i, match := range matches {
		if len(match.Ranges) >= limit {
			matches[i].Ranges = match.Ranges[:limit]
			*hs = matches[:i+1]
			return
		}
		limit -= len(match.Ranges)
	}
}

type LineMatch struct {
	// Preview is the full single line these offsets belong to
	Preview          string
	OffsetAndLengths [][2]int32
	LineNumber       int32
}
