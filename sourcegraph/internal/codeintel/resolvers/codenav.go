package resolvers

import (
	"context"
	"fmt"
	"strings"

	"github.com/graph-gophers/graphql-go"
	"go.opentelemetry.io/otel/attribute"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/uploads/shared"
	"github.com/sourcegraph/sourcegraph/internal/markdown"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

type CodeNavServiceResolver interface {
	GitBlobLSIFData(ctx context.Context, args *GitBlobLSIFDataArgs) (GitBlobLSIFDataResolver, error)
	// CodeGraphData is a newer API that is more SCIP-oriented.
	// The second parameter is called 'opts' and not 'args' to reflect
	// that it is not what is exactly provided as input from the GraphQL
	// client.
	CodeGraphData(ctx context.Context, opts *CodeGraphDataOpts) (*[]CodeGraphDataResolver, error)
	UsagesForSymbol(ctx context.Context, args *UsagesForSymbolArgs) (UsageConnectionResolver, error)
}

type GitBlobLSIFDataArgs struct {
	Repo      *types.Repo
	Commit    api.CommitID
	Path      string
	ExactPath bool
	ToolName  string
}

func (a *GitBlobLSIFDataArgs) Options() shared.UploadMatchingOptions {
	matching := shared.RootMustEnclosePath
	if !a.ExactPath {
		matching = shared.RootEnclosesPathOrPathEnclosesRoot
	}
	return shared.UploadMatchingOptions{
		RepositoryID:       int(a.Repo.ID),
		Commit:             string(a.Commit),
		Path:               a.Path,
		RootToPathMatching: matching,
		Indexer:            a.ToolName,
	}
}

type GitBlobLSIFDataResolver interface {
	GitTreeLSIFDataResolver
	ToGitTreeLSIFData() (GitTreeLSIFDataResolver, bool)
	ToGitBlobLSIFData() (GitBlobLSIFDataResolver, bool)

	Stencil(ctx context.Context) ([]RangeResolver, error)
	Ranges(ctx context.Context, args *LSIFRangesArgs) (CodeIntelligenceRangeConnectionResolver, error)
	Definitions(ctx context.Context, args *LSIFQueryPositionArgs) (LocationConnectionResolver, error)
	References(ctx context.Context, args *LSIFPagedQueryPositionArgs) (LocationConnectionResolver, error)
	Implementations(ctx context.Context, args *LSIFPagedQueryPositionArgs) (LocationConnectionResolver, error)
	Prototypes(ctx context.Context, args *LSIFPagedQueryPositionArgs) (LocationConnectionResolver, error)
	Hover(ctx context.Context, args *LSIFQueryPositionArgs) (HoverResolver, error)
	VisibleIndexes(ctx context.Context) (_ *[]PreciseIndexResolver, err error)
	Snapshot(ctx context.Context, args *struct{ IndexID graphql.ID }) (_ *[]SnapshotDataResolver, err error)
}

type SnapshotDataResolver interface {
	Offset() int32
	Data() string
	Additional() *[]string
}

type LSIFRangesArgs struct {
	StartLine int32
	EndLine   int32
}

type LSIFQueryPositionArgs struct {
	Line      int32
	Character int32
	Filter    *string
}

type LSIFPagedQueryPositionArgs struct {
	LSIFQueryPositionArgs
	PagedConnectionArgs
	Filter *string
}

type (
	CodeIntelligenceRangeConnectionResolver = ConnectionResolver[CodeIntelligenceRangeResolver]
)

type CodeIntelligenceRangeResolver interface {
	Range(ctx context.Context) (RangeResolver, error)
	Definitions(ctx context.Context) (LocationConnectionResolver, error)
	References(ctx context.Context) (LocationConnectionResolver, error)
	Implementations(ctx context.Context) (LocationConnectionResolver, error)
	Hover(ctx context.Context) (HoverResolver, error)
}

type RangeResolver interface {
	Start() PositionResolver
	End() PositionResolver
}

type PositionResolver interface {
	Line() int32
	Character() int32
}

type (
	LocationConnectionResolver = PagedConnectionResolver[LocationResolver]
)

type LocationResolver interface {
	Resource() GitTreeEntryResolver
	Range() RangeResolver
	URL(ctx context.Context) (string, error)
	CanonicalURL() string
}

type HoverResolver interface {
	Markdown() Markdown
	Range() RangeResolver
}

type Markdown string

func (m Markdown) Text() string {
	return string(m)
}

func (m Markdown) HTML() (string, error) {
	return markdown.Render(string(m))
}

type GitTreeLSIFDataResolver interface {
	Diagnostics(ctx context.Context, args *LSIFDiagnosticsArgs) (DiagnosticConnectionResolver, error)
}

type (
	LSIFDiagnosticsArgs          = ConnectionArgs
	DiagnosticConnectionResolver = PagedConnectionWithTotalCountResolver[DiagnosticResolver]
)

type DiagnosticResolver interface {
	Severity() (*string, error)
	Code() (*string, error)
	Source() (*string, error)
	Message() (*string, error)
	Location(ctx context.Context) (LocationResolver, error)
}

type CodeGraphDataResolver interface {
	Provenance(ctx context.Context) (CodeGraphDataProvenance, error)
	Commit(ctx context.Context) (string, error)
	ToolInfo(ctx context.Context) (*CodeGraphToolInfo, error)
	// Pre-condition: args are Normalized.
	Occurrences(ctx context.Context, args *OccurrencesArgs) (SCIPOccurrenceConnectionResolver, error)
}

type CodeGraphDataProvenance string

const (
	ProvenancePrecise     CodeGraphDataProvenance = "PRECISE"
	ProvenanceSyntactic   CodeGraphDataProvenance = "SYNTACTIC"
	ProvenanceSearchBased CodeGraphDataProvenance = "SEARCH_BASED"
)

type CodeGraphDataProvenanceComparator struct {
	Equals *CodeGraphDataProvenance
}

type CodeGraphDataFilter struct {
	Provenance *CodeGraphDataProvenanceComparator
}

// String is meant as a debugging-only representation without round-trippability
func (f *CodeGraphDataFilter) String() string {
	if f != nil && f.Provenance != nil && f.Provenance.Equals != nil {
		return fmt.Sprintf("provenance == %s", string(*f.Provenance.Equals))
	}
	return ""
}

type CodeGraphDataArgs struct {
	Filter *CodeGraphDataFilter
}

func (args *CodeGraphDataArgs) Attrs() []attribute.KeyValue {
	if args == nil {
		return nil
	}
	return []attribute.KeyValue{attribute.String("args.filter", args.Filter.String())}
}

type ForEachProvenance[T any] struct {
	SearchBased T
	Syntactic   T
	Precise     T
}

func (a *CodeGraphDataArgs) ProvenancesForSCIPData() ForEachProvenance[bool] {
	var out ForEachProvenance[bool]
	if a == nil || a.Filter == nil || a.Filter.Provenance == nil || a.Filter.Provenance.Equals == nil {
		out.Syntactic = true
		out.Precise = true
	} else {
		p := *a.Filter.Provenance.Equals
		switch p {
		case ProvenancePrecise:
			out.Precise = true
		case ProvenanceSyntactic:
			out.Syntactic = true
		case ProvenanceSearchBased:
		}
	}
	return out
}

type CodeGraphDataOpts struct {
	Args   *CodeGraphDataArgs
	Repo   *types.Repo
	Commit api.CommitID
	Path   string
}

func (opts *CodeGraphDataOpts) Attrs() []attribute.KeyValue {
	return append([]attribute.KeyValue{attribute.String("repo", opts.Repo.String()),
		opts.Commit.Attr(),
		attribute.String("path", opts.Path)}, opts.Args.Attrs()...)
}

type CodeGraphToolInfo struct {
	Name_    *string
	Version_ *string
}

func (ti *CodeGraphToolInfo) Name() *string {
	return ti.Name_
}

func (ti *CodeGraphToolInfo) Version() *string {
	return ti.Version_
}

type OccurrencesArgs struct {
	First *int32
	After *string
}

// Normalize returns args for convenience of chaining
func (args *OccurrencesArgs) Normalize(maxPageSize int32) *OccurrencesArgs {
	if args == nil {
		*args = OccurrencesArgs{}
	}
	if args.First == nil || *args.First > maxPageSize {
		args.First = &maxPageSize
	}
	return args
}

type SCIPOccurrenceConnectionResolver interface {
	ConnectionResolver[SCIPOccurrenceResolver]
	PageInfo(ctx context.Context) (*graphqlutil.ConnectionPageInfo[SCIPOccurrenceResolver], error)
}

type SCIPOccurrenceResolver interface {
	Symbol() (*string, error)
	Range() (RangeResolver, error)
	Roles() (*[]SymbolRole, error)
}

type SymbolRole string

// ⚠️ CAUTION: These constants are part of the public GraphQL API
const (
	SymbolRoleDefinition        SymbolRole = "DEFINITION"
	SymbolRoleReference         SymbolRole = "REFERENCE"
	SymbolRoleForwardDefinition SymbolRole = "FORWARD_DEFINITION"
)

type UsagesForSymbolArgs struct {
	Symbol *SymbolComparator
	Range  RangeInput
	Filter *UsagesFilter
	First  *int32
	After  *string
}

func (args *UsagesForSymbolArgs) ProvenancesForSCIPData() ForEachProvenance[bool] {
	var out ForEachProvenance[bool]
	if args == nil || args.Symbol == nil || args.Symbol.Provenance.Equals == nil {
		out.Precise = true
		out.Syntactic = true
		out.SearchBased = true
	} else {
		switch p := *args.Symbol.Provenance.Equals; p {
		case ProvenancePrecise:
			out.Precise = true
		case ProvenanceSyntactic:
			out.Syntactic = true
		case ProvenanceSearchBased:
			out.SearchBased = true
		}
	}
	return out
}

// Normalize sets the First field to a non-null value.
func (args *UsagesForSymbolArgs) Normalize(maxPageSize int32) {
	if args == nil {
		*args = UsagesForSymbolArgs{}
	}
	if args.First == nil || *args.First > maxPageSize {
		args.First = &maxPageSize
	}
}

func (args *UsagesForSymbolArgs) Attrs() (out []attribute.KeyValue) {
	out = append(append(args.Symbol.Attrs(), args.Range.Attrs()...), attribute.String("filter", args.Filter.DebugString()))
	if args.First != nil {
		out = append(out, attribute.Int("first", int(*args.First)))
	}
	out = append(out, attribute.Bool("hasAfter", args.After != nil))
	return out
}

type SymbolComparator struct {
	Name       SymbolNameComparator
	Provenance CodeGraphDataProvenanceComparator
}

func (c *SymbolComparator) Attrs() (out []attribute.KeyValue) {
	if c == nil {
		return nil
	}
	if c.Name.Equals != nil {
		out = append(out, attribute.String("symbol.name.equals", *c.Name.Equals))
	}
	if c.Provenance.Equals != nil {
		out = append(out, attribute.String("symbol.provenance.equals", string(*c.Provenance.Equals)))
	}
	return out
}

type SymbolNameComparator struct {
	Equals *string
}

type RangeInput struct {
	Repository string
	Revision   *string
	Path       string
	Start      *PositionInput
	End        *PositionInput
}

func (r *RangeInput) Attrs() (out []attribute.KeyValue) {
	out = append(out, attribute.String("range.repository", r.Repository))
	if r.Revision != nil {
		out = append(out, attribute.String("range.revision", *r.Revision))
	}
	out = append(out, attribute.String("range.path", r.Path))
	if r.Start != nil {
		out = append(out, attribute.Int("range.start.line", int(r.Start.Line)))
		out = append(out, attribute.Int("range.start.character", int(r.Start.Character)))
	}
	if r.End != nil {
		out = append(out, attribute.Int("range.end.line", int(r.End.Line)))
		out = append(out, attribute.Int("range.end.character", int(r.End.Character)))
	}
	return out
}

type PositionInput struct {
	// Zero-based line number
	Line int32
	// Zero-based UTF-16 code unit offset
	Character int32
}

type UsagesFilter struct {
	Not        *UsagesFilter
	Repository *RepositoryFilter
}

func (f *UsagesFilter) DebugString() string {
	if f == nil {
		return ""
	}
	result := []string{}
	if f.Not != nil {
		result = append(result, fmt.Sprintf("(not %s)", f.Not.DebugString()))
	}
	if f.Repository != nil && f.Repository.Name.Equals != nil {
		result = append(result, fmt.Sprintf("(repo == %s)", *f.Repository.Name.Equals))
	}
	return strings.Join(result, " and ")
}

type RepositoryFilter struct {
	Name StringComparator
}

type StringComparator struct {
	Equals *string
}

type UsageConnectionResolver interface {
	ConnectionResolver[UsageResolver]
	PageInfo(ctx context.Context) (*graphqlutil.ConnectionPageInfo[UsageResolver], error)
}

type UsageResolver interface {
	Symbol(context.Context) (SymbolInformationResolver, error)
	UsageRange(context.Context) (UsageRangeResolver, error)
	SurroundingContent(_ context.Context, args *struct {
		*SurroundingLines `json:"surroundingLines"`
	}) (*string, error)
}

type SymbolInformationResolver interface {
	Name() (string, error)
	Documentation() (*[]string, error)
	Provenance() (CodeGraphDataProvenance, error)
	DataSource() *string
}

type UsageRangeResolver interface {
	Repository() string
	Revision() string
	Path() string
	Range() RangeResolver
}

type SurroundingLines struct {
	LinesBefore *int32 `json:"linesBefore"`
	LinesAfter  *int32 `json:"linesAfter"`
}
