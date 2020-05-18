package graphqlbackend

import (
	"context"
	"html/template"

	"github.com/sourcegraph/sourcegraph/internal/highlight"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
)

type highlightedRange struct {
	line      int32
	character int32
	length    int32
}

func (h *highlightedRange) Line() int32      { return h.line }
func (h *highlightedRange) Character() int32 { return h.character }
func (h *highlightedRange) Length() int32    { return h.length }

type highlightedString struct {
	value      string
	highlights []*highlightedRange
}

func (s *highlightedString) Value() string                   { return s.value }
func (s *highlightedString) Highlights() []*highlightedRange { return s.highlights }

func fromVCSHighlights(vcsHighlights []git.Highlight) []*highlightedRange {
	highlights := make([]*highlightedRange, len(vcsHighlights))
	for i, vh := range vcsHighlights {
		highlights[i] = &highlightedRange{
			line:      int32(vh.Line),
			character: int32(vh.Character),
			length:    int32(vh.Length),
		}
	}
	return highlights
}

type HighlightArgs struct {
	DisableTimeout     bool
	IsLightTheme       bool
	HighlightLongLines bool
}

type highlightedFileResolver struct {
	aborted bool
	html    string
}

func (h *highlightedFileResolver) Aborted() bool { return h.aborted }
func (h *highlightedFileResolver) HTML() string  { return h.html }

func highlightContent(ctx context.Context, args *HighlightArgs, content, path string, metadata highlight.Metadata) (*highlightedFileResolver, error) {
	var (
		html            template.HTML
		result          = &highlightedFileResolver{}
		err             error
		simulateTimeout = metadata.RepoName == "github.com/sourcegraph/AlwaysHighlightTimeoutTest"
	)
	html, result.aborted, err = highlight.Code(ctx, highlight.Params{
		Content:            []byte(content),
		Filepath:           path,
		DisableTimeout:     args.DisableTimeout,
		IsLightTheme:       args.IsLightTheme,
		HighlightLongLines: args.HighlightLongLines,
		SimulateTimeout:    simulateTimeout,
		Metadata:           metadata,
	})
	if err != nil {
		return nil, err
	}
	result.html = string(html)
	return result, nil
}
