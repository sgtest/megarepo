package graphqlbackend

import (
	"fmt"
	"strconv"

	"github.com/sourcegraph/go-langserver/pkg/lsp"
)

type locationResolver struct {
	resource *gitTreeEntryResolver
	lspRange *lsp.Range
}

func (r *locationResolver) Resource() *gitTreeEntryResolver { return r.resource }

func (r *locationResolver) Range() *rangeResolver {
	if r.lspRange == nil {
		return nil
	}
	return &rangeResolver{*r.lspRange}
}

func (r *locationResolver) URL() string { return r.urlPath(r.resource.URL()) }

func (r *locationResolver) CanonicalURL() string { return r.urlPath(r.resource.CanonicalURL()) }

func (r *locationResolver) urlPath(prefix string) string {
	url := prefix
	if r.lspRange != nil {
		url += "#L" + r.Range().urlFragment()
	}
	return url
}

type rangeResolver struct{ lspRange lsp.Range }

func (r *rangeResolver) Start() *positionResolver { return &positionResolver{r.lspRange.Start} }
func (r *rangeResolver) End() *positionResolver   { return &positionResolver{r.lspRange.End} }

func (r *rangeResolver) urlFragment() string {
	if r.lspRange.Start == r.lspRange.End {
		return r.Start().urlFragment(false)
	}
	hasCharacter := r.lspRange.Start.Character != 0 || r.lspRange.End.Character != 0
	return r.Start().urlFragment(hasCharacter) + "-" + r.End().urlFragment(hasCharacter)
}

type positionResolver struct{ pos lsp.Position }

func (r *positionResolver) Line() int32      { return int32(r.pos.Line) }
func (r *positionResolver) Character() int32 { return int32(r.pos.Character) }

func (r *positionResolver) urlFragment(forceIncludeCharacter bool) string {
	if !forceIncludeCharacter && r.pos.Character == 0 {
		return strconv.Itoa(r.pos.Line + 1)
	}
	return fmt.Sprintf("%d:%d", r.pos.Line+1, r.pos.Character+1)
}
