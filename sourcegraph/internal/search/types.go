package search

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"github.com/grafana/regexp"
	"github.com/sourcegraph/log"
	"github.com/sourcegraph/zoekt"
	zoektquery "github.com/sourcegraph/zoekt/query"
	"go.opentelemetry.io/otel/attribute"

	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/trace/policy"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/featureflag"
	"github.com/sourcegraph/sourcegraph/internal/search/filter"
	"github.com/sourcegraph/sourcegraph/internal/search/limits"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/schema"
)

// Inputs contains fields we set before kicking off search.
type Inputs struct {
	Plan                   query.Plan // the comprehensive query plan
	Query                  query.Q    // the current basic query being evaluated, one part of query.Plan
	OriginalQuery          string     // the raw string of the original search query
	SearchMode             Mode
	PatternType            query.SearchType
	UserSettings           *schema.Settings
	OnSourcegraphDotCom    bool
	Features               *Features
	Protocol               Protocol
	SanitizeSearchPatterns []*regexp.Regexp
}

// MaxResults computes the limit for the query.
func (inputs Inputs) MaxResults() int {
	return inputs.Query.MaxResults(inputs.DefaultLimit())
}

// DefaultLimit is the default limit to use if not specified in query.
func (inputs Inputs) DefaultLimit() int {
	if inputs.Protocol == Batch {
		return limits.DefaultMaxSearchResults
	}
	return limits.DefaultMaxSearchResultsStreaming
}

type Mode int

const (
	Precise     Mode = 0
	SmartSearch      = 1 << (iota - 1)
)

type Protocol int

const (
	Streaming Protocol = iota
	Batch
)

func (p Protocol) String() string {
	switch p {
	case Streaming:
		return "Streaming"
	case Batch:
		return "Batch"
	default:
		return fmt.Sprintf("unknown{%d}", p)
	}
}

type SymbolsParameters struct {
	// Repo is the name of the repository to search in.
	Repo api.RepoName `json:"repo"`

	// CommitID is the commit to search in.
	CommitID api.CommitID `json:"commitID"`

	// Query is the search query.
	Query string

	// IsRegExp if true will treat the Pattern as a regular expression.
	IsRegExp bool

	// IsCaseSensitive if false will ignore the case of query and file pattern
	// when finding matches.
	IsCaseSensitive bool

	// IncludePatterns is a list of regexes that symbol's file paths
	// need to match to get included in the result
	//
	// The patterns are ANDed together; a file's path must match all patterns
	// for it to be kept. That is also why it is a list (unlike the singular
	// ExcludePattern); it is not possible in general to construct a single
	// glob or Go regexp that represents multiple such patterns ANDed together.
	IncludePatterns []string

	// ExcludePattern is an optional regex that symbol's file paths
	// need to match to get included in the result
	ExcludePattern string

	// First indicates that only the first n symbols should be returned.
	First int

	// Timeout is the maximum amount of time the symbols search should take.
	//
	// If Timeout isn't specified, a default timeout of 60 seconds is used.
	Timeout time.Duration
}

type SymbolsResponse struct {
	Symbols result.Symbols `json:"symbols,omitempty"`
	Err     string         `json:"error,omitempty"`
}

// GlobalSearchMode designates code paths which optimize performance for global
// searches, i.e., literal or regexp, indexed searches without repo: filter.
type GlobalSearchMode int

const (
	DefaultMode GlobalSearchMode = iota

	// ZoektGlobalSearch designates a performance optimised code path for indexed
	// searches. For a global search we don't need to resolve repos before searching
	// shards on Zoekt, instead we can resolve repos and call Zoekt concurrently.
	//
	// Note: Even for a global search we have to resolve repos to filter search results
	// returned by Zoekt.
	ZoektGlobalSearch

	// SearcherOnly designated a code path on which we skip indexed search, even if
	// the user specified index:yes. SearcherOnly is used in conjunction with
	// ZoektGlobalSearch and designates the non-indexed part of the performance
	// optimised code path.
	SearcherOnly

	// SkipUnindexed disables content, path, and symbol search. Used:
	// (1) in conjunction with ZoektGlobalSearch on Sourcegraph.com.
	// (2) when a query does not specify any patterns, include patterns, or exclude pattern.
	SkipUnindexed
)

var globalSearchModeStrings = map[GlobalSearchMode]string{
	ZoektGlobalSearch: "ZoektGlobalSearch",
	SearcherOnly:      "SearcherOnly",
	SkipUnindexed:     "SkipUnindexed",
}

func (m GlobalSearchMode) String() string {
	if s, ok := globalSearchModeStrings[m]; ok {
		return s
	}
	return "None"
}

type IndexedRequestType string

const (
	TextRequest   IndexedRequestType = "text"
	SymbolRequest IndexedRequestType = "symbol"
)

// ZoektParameters contains all the inputs to run a Zoekt indexed search.
type ZoektParameters struct {
	Query          zoektquery.Q
	Typ            IndexedRequestType
	FileMatchLimit int32
	Select         filter.SelectPath

	// Features are feature flags that can affect behaviour of searcher.
	Features Features

	// EXPERIMENTAL: If true, use keyword-style scoring instead of Zoekt's default scoring formula.
	KeywordScoring bool
}

// ToSearchOptions converts the parameters to options for the Zoekt search API.
func (o *ZoektParameters) ToSearchOptions(ctx context.Context, logger log.Logger) *zoekt.SearchOptions {
	defaultTimeout := 20 * time.Second
	searchOpts := &zoekt.SearchOptions{
		Trace:             policy.ShouldTrace(ctx),
		MaxWallTime:       defaultTimeout,
		ChunkMatches:      true,
		UseKeywordScoring: o.KeywordScoring,
	}

	// These are reasonable default amounts of work to do per shard and
	// replica respectively.
	searchOpts.ShardMaxMatchCount = 10_000
	searchOpts.TotalMaxMatchCount = 100_000

	// Tell each zoekt replica to not send back more than limit results.
	limit := int(o.FileMatchLimit)
	searchOpts.MaxDocDisplayCount = limit

	// If we are searching for large limits, raise the amount of work we
	// are willing to do per shard and zoekt replica respectively.
	if limit > searchOpts.ShardMaxMatchCount {
		searchOpts.ShardMaxMatchCount = limit
	}
	if limit > searchOpts.TotalMaxMatchCount {
		searchOpts.TotalMaxMatchCount = limit
	}

	// If we're searching repos, ignore the other options and only check one file per repo
	if o.Select.Root() == filter.Repository {
		searchOpts.ShardRepoMaxMatchCount = 1
		return searchOpts
	}

	if o.Features.Debug {
		searchOpts.DebugScore = true
	}

	if o.Features.Ranking {
		// This enables our stream based ranking, where we wait a certain amount
		// of time to collect results before ranking.
		searchOpts.FlushWallTime = conf.SearchFlushWallTime()

		// This enables the use of document ranks in scoring, if they are available.
		searchOpts.UseDocumentRanks = true
		searchOpts.DocumentRanksWeight = conf.SearchDocumentRanksWeight()
	}

	return searchOpts
}

// SearcherParameters the inputs for a search fulfilled by the Searcher service
// (cmd/searcher). Searcher fulfills (1) unindexed literal and regexp searches
// and (2) structural search requests.
type SearcherParameters struct {
	PatternInfo *TextPatternInfo

	// UseFullDeadline indicates that the search should try do as much work as
	// it can within context.Deadline. If false the search should try and be
	// as fast as possible, even if a "slow" deadline is set.
	//
	// For example searcher will wait to full its archive cache for a
	// repository if this field is true. Another example is we set this field
	// to true if the user requests a specific timeout or maximum result size.
	UseFullDeadline bool

	// Features are feature flags that can affect behaviour of searcher.
	Features Features
}

// TextPatternInfo is the struct used by vscode pass on search queries. Keep it in
// sync with pkg/searcher/protocol.PatternInfo.
type TextPatternInfo struct {
	Pattern         string
	IsNegated       bool
	IsRegExp        bool
	IsStructuralPat bool
	CombyRule       string
	IsWordMatch     bool
	IsCaseSensitive bool
	FileMatchLimit  int32
	Index           query.YesNoOnly
	Select          filter.SelectPath

	// We do not support IsMultiline
	// IsMultiline     bool
	IncludePatterns []string
	ExcludePattern  string

	PathPatternsAreCaseSensitive bool

	PatternMatchesContent bool
	PatternMatchesPath    bool

	Languages []string
}

func (p *TextPatternInfo) Fields() []attribute.KeyValue {
	res := make([]attribute.KeyValue, 0, 4)
	add := func(fs ...attribute.KeyValue) {
		res = append(res, fs...)
	}

	add(attribute.String("pattern", p.Pattern))

	if p.IsNegated {
		add(attribute.Bool("isNegated", p.IsNegated))
	}
	if p.IsRegExp {
		add(attribute.Bool("isRegexp", p.IsRegExp))
	}
	if p.IsStructuralPat {
		add(attribute.Bool("isStructural", p.IsStructuralPat))
	}
	if p.CombyRule != "" {
		add(attribute.String("combyRule", p.CombyRule))
	}
	if p.IsWordMatch {
		add(attribute.Bool("isWordMatch", p.IsWordMatch))
	}
	if p.IsCaseSensitive {
		add(attribute.Bool("isCaseSensitive", p.IsCaseSensitive))
	}
	add(attribute.Int("fileMatchLimit", int(p.FileMatchLimit)))

	if p.Index != query.Yes {
		add(attribute.String("index", string(p.Index)))
	}
	if len(p.Select) > 0 {
		add(attribute.StringSlice("select", p.Select))
	}
	if len(p.IncludePatterns) > 0 {
		add(attribute.StringSlice("includePatterns", p.IncludePatterns))
	}
	if p.ExcludePattern != "" {
		add(attribute.String("excludePattern", p.ExcludePattern))
	}
	if p.PathPatternsAreCaseSensitive {
		add(attribute.Bool("pathPatternsAreCaseSensitive", p.PathPatternsAreCaseSensitive))
	}
	if p.PatternMatchesPath {
		add(attribute.Bool("patternMatchesPath", p.PatternMatchesPath))
	}
	if len(p.Languages) > 0 {
		add(attribute.StringSlice("languages", p.Languages))
	}
	return res
}

func (p *TextPatternInfo) String() string {
	args := []string{fmt.Sprintf("%q", p.Pattern)}
	if p.IsRegExp {
		args = append(args, "re")
	}
	if p.IsStructuralPat {
		if p.CombyRule != "" {
			args = append(args, fmt.Sprintf("comby:%s", p.CombyRule))
		} else {
			args = append(args, "comby")
		}
	}
	if p.IsWordMatch {
		args = append(args, "word")
	}
	if p.IsCaseSensitive {
		args = append(args, "case")
	}
	if !p.PatternMatchesContent {
		args = append(args, "nocontent")
	}
	if !p.PatternMatchesPath {
		args = append(args, "nopath")
	}
	if p.FileMatchLimit > 0 {
		args = append(args, fmt.Sprintf("filematchlimit:%d", p.FileMatchLimit))
	}
	for _, lang := range p.Languages {
		args = append(args, fmt.Sprintf("lang:%s", lang))
	}

	path := "f"
	if p.PathPatternsAreCaseSensitive {
		path = "F"
	}
	if p.ExcludePattern != "" {
		args = append(args, fmt.Sprintf("-%s:%q", path, p.ExcludePattern))
	}
	for _, inc := range p.IncludePatterns {
		args = append(args, fmt.Sprintf("%s:%q", path, inc))
	}

	return fmt.Sprintf("TextPatternInfo{%s}", strings.Join(args, ","))
}

// Features describe feature flags for a request. This is state that differs
// across users and time. It is created based on user feature flags and
// configuration.
//
// The Feature struct should be initialized once per search request early on.
//
// The default value for a Feature should be the go zero value, such that
// creating an empty Feature struct represents the usual search
// experience. This is to avoid needing to update a large number of tests when
// a new feature flag is introduced, and instead changes are localized to this
// struct and read sites of a flag.
type Features struct {
	// ContentBasedLangFilters when true will use the language detected from
	// the content of the file, rather than just file name patterns. This is
	// currently just supported by Zoekt.
	ContentBasedLangFilters bool `json:"search-content-based-lang-detection"`

	// HybridSearch when true will consult the Zoekt index when running
	// unindexed searches. Searcher (unindexed search) will the only search
	// what has changed since the indexed commit.
	HybridSearch bool `json:"search-hybrid"`

	// Ranking when true will use a our new #ranking signals and code paths
	// for ranking results from Zoekt.
	Ranking bool `json:"ranking"`

	// Debug when true will set the Debug field on FileMatches. This may grow
	// from here. For now we treat this like a feature flag for convenience.
	Debug bool `json:"debug"`

	// CodeOwnershipSearch when true will enable searching through code ownership
	// using `file:has.owner({owner})` and `select:file.owners` filters.
	CodeOwnershipSearch bool `json:"codeownership"`
}

func (f *Features) String() string {
	jsonObject, err := json.Marshal(f)
	if err != nil {
		return "error encoding features as string"
	}
	flagMap := featureflag.EvaluatedFlagSet{}
	if err := json.Unmarshal(jsonObject, &flagMap); err != nil {
		return "error decoding features"
	}
	return flagMap.String()
}

// RepoOptions is the source of truth for the options a user specified
// in their search query that affect which repos should be searched.
// When adding fields to this struct, be sure to update IsGlobal().
type RepoOptions struct {
	RepoFilters         []query.ParsedRepoFilter
	MinusRepoFilters    []string
	DescriptionPatterns []string

	CaseSensitiveRepoFilters bool
	SearchContextSpec        string

	CommitAfter *query.RepoHasCommitAfterArgs
	Visibility  query.RepoVisibility
	Limit       int
	Cursors     []*types.Cursor

	// Whether we should depend on Zoekt for resolving repositories
	UseIndex       query.YesNoOnly
	HasFileContent []query.RepoHasFileContentArgs
	HasKVPs        []query.RepoKVPFilter
	HasTopics      []query.RepoHasTopicPredicate

	// ForkSet indicates whether `fork:` was set explicitly in the query,
	// or whether the values were set from defaults.
	ForkSet   bool
	NoForks   bool
	OnlyForks bool

	OnlyCloned bool

	// ArchivedSet indicates whether `archived:` was set explicitly in the query,
	// or whether the values were set from defaults.
	ArchivedSet  bool
	NoArchived   bool
	OnlyArchived bool
}

func (op *RepoOptions) Attributes() []attribute.KeyValue {
	res := make([]attribute.KeyValue, 0, 8)
	add := func(f ...attribute.KeyValue) {
		res = append(res, f...)
	}

	if len(op.RepoFilters) > 0 {
		add(attribute.String("repoFilters", fmt.Sprintf("%v", op.RepoFilters)))
	}
	if len(op.MinusRepoFilters) > 0 {
		add(attribute.StringSlice("minusRepoFilters", op.MinusRepoFilters))
	}
	if len(op.DescriptionPatterns) > 0 {
		add(attribute.StringSlice("descriptionPatterns", op.DescriptionPatterns))
	}
	if op.CaseSensitiveRepoFilters {
		add(attribute.Bool("caseSensitiveRepoFilters", true))
	}
	if op.SearchContextSpec != "" {
		add(attribute.String("searchContextSpec", op.SearchContextSpec))
	}
	if op.CommitAfter != nil {
		add(attribute.String("commitAfter.time", op.CommitAfter.TimeRef))
		add(attribute.Bool("commitAfter.negated", op.CommitAfter.Negated))
	}
	if op.Visibility != query.Any {
		add(attribute.String("visibility", string(op.Visibility)))
	}
	if op.Limit > 0 {
		add(attribute.Int("limit", op.Limit))
	}
	if len(op.Cursors) > 0 {
		add(attribute.String("cursors", fmt.Sprintf("%+v", op.Cursors)))
	}
	if op.UseIndex != query.Yes {
		add(attribute.String("useIndex", string(op.UseIndex)))
	}
	if len(op.HasFileContent) > 0 {
		for i, arg := range op.HasFileContent {
			nondefault := []attribute.KeyValue{}
			if arg.Path != "" {
				nondefault = append(nondefault, attribute.String("path", arg.Path))
			}
			if arg.Content != "" {
				nondefault = append(nondefault, attribute.String("content", arg.Content))
			}
			if arg.Negated {
				nondefault = append(nondefault, attribute.Bool("negated", arg.Negated))
			}
			add(trace.Scoped(fmt.Sprintf("hasFileContent[%d]", i), nondefault...)...)
		}
	}
	if len(op.HasKVPs) > 0 {
		for i, arg := range op.HasKVPs {
			nondefault := []attribute.KeyValue{}
			if arg.Key != "" {
				nondefault = append(nondefault, attribute.String("key", arg.Key))
			}
			if arg.Value != nil {
				nondefault = append(nondefault, attribute.String("value", *arg.Value))
			}
			if arg.Negated {
				nondefault = append(nondefault, attribute.Bool("negated", arg.Negated))
			}
			add(trace.Scoped(fmt.Sprintf("hasKVPs[%d]", i), nondefault...)...)
		}
	}
	if len(op.HasTopics) > 0 {
		for i, arg := range op.HasTopics {
			nondefault := []attribute.KeyValue{}
			if arg.Topic != "" {
				nondefault = append(nondefault, attribute.String("topic", arg.Topic))
			}
			if arg.Negated {
				nondefault = append(nondefault, attribute.Bool("negated", arg.Negated))
			}
			add(trace.Scoped(fmt.Sprintf("hasTopics[%d]", i), nondefault...)...)
		}
	}
	if op.ForkSet {
		add(attribute.Bool("forkSet", op.ForkSet))
	}
	if !op.NoForks { // default value is true
		add(attribute.Bool("noForks", op.NoForks))
	}
	if op.OnlyForks {
		add(attribute.Bool("onlyForks", op.OnlyForks))
	}
	if op.OnlyCloned {
		add(attribute.Bool("onlyCloned", op.OnlyCloned))
	}
	if op.ArchivedSet {
		add(attribute.Bool("archivedSet", op.ArchivedSet))
	}
	if !op.NoArchived { // default value is true
		add(attribute.Bool("noArchived", op.NoArchived))
	}
	if op.OnlyArchived {
		add(attribute.Bool("onlyArchived", op.OnlyArchived))
	}
	return res
}

func (op *RepoOptions) String() string {
	var b strings.Builder

	if len(op.RepoFilters) > 0 {
		fmt.Fprintf(&b, "RepoFilters: %q\n", op.RepoFilters)
	} else {
		b.WriteString("RepoFilters: []\n")
	}
	if len(op.MinusRepoFilters) > 0 {
		fmt.Fprintf(&b, "MinusRepoFilters: %q\n", op.MinusRepoFilters)
	} else {
		b.WriteString("MinusRepoFilters: []\n")
	}

	if len(op.DescriptionPatterns) > 0 {
		fmt.Fprintf(&b, "DescriptionPatterns: %q\n", op.DescriptionPatterns)
	}

	if op.CommitAfter != nil {
		fmt.Fprintf(&b, "CommitAfter: %s\n", op.CommitAfter.TimeRef)
	}
	fmt.Fprintf(&b, "Visibility: %s\n", string(op.Visibility))

	if op.UseIndex != query.Yes {
		fmt.Fprintf(&b, "UseIndex: %s\n", string(op.UseIndex))
	}
	if len(op.HasFileContent) > 0 {
		for i, arg := range op.HasFileContent {
			if arg.Path != "" {
				fmt.Fprintf(&b, "HasFileContent[%d].path: %s\n", i, arg.Path)
			}
			if arg.Content != "" {
				fmt.Fprintf(&b, "HasFileContent[%d].content: %s\n", i, arg.Content)
			}
			if arg.Negated {
				fmt.Fprintf(&b, "HasFileContent[%d].negated: %t\n", i, arg.Negated)
			}
		}
	}
	if len(op.HasKVPs) > 0 {
		for i, arg := range op.HasKVPs {
			if arg.Key != "" {
				fmt.Fprintf(&b, "HasKVPs[%d].key: %s\n", i, arg.Key)
			}
			if arg.Value != nil {
				fmt.Fprintf(&b, "HasKVPs[%d].value: %s\n", i, *arg.Value)
			}
			if arg.Negated {
				fmt.Fprintf(&b, "HasKVPs[%d].negated: %t\n", i, arg.Negated)
			}
		}
	}
	if len(op.HasTopics) > 0 {
		for i, arg := range op.HasTopics {
			if arg.Topic != "" {
				fmt.Fprintf(&b, "HasTopics[%d].topic: %s\n", i, arg.Topic)
			}
			if arg.Negated {
				fmt.Fprintf(&b, "HasTopics[%d].negated: %t\n", i, arg.Negated)
			}
		}
	}

	if op.CaseSensitiveRepoFilters {
		fmt.Fprintf(&b, "CaseSensitiveRepoFilters: %t\n", op.CaseSensitiveRepoFilters)
	}
	if op.ForkSet {
		fmt.Fprintf(&b, "ForkSet: %t\n", op.ForkSet)
	}
	if op.NoForks {
		fmt.Fprintf(&b, "NoForks: %t\n", op.NoForks)
	}
	if op.OnlyForks {
		fmt.Fprintf(&b, "OnlyForks: %t\n", op.OnlyForks)
	}
	if op.OnlyCloned {
		fmt.Fprintf(&b, "OnlyCloned: %t\n", op.OnlyCloned)
	}
	if op.ArchivedSet {
		fmt.Fprintf(&b, "ArchivedSet: %t\n", op.ArchivedSet)
	}
	if op.NoArchived {
		fmt.Fprintf(&b, "NoArchived: %t\n", op.NoArchived)
	}
	if op.OnlyArchived {
		fmt.Fprintf(&b, "OnlyArchived: %t\n", op.OnlyArchived)
	}

	return b.String()
}
