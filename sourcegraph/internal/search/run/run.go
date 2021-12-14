package run

import (
	"context"

	"github.com/sourcegraph/sourcegraph/internal/featureflag"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	searchrepos "github.com/sourcegraph/sourcegraph/internal/search/repos"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/schema"
)

// SearchInputs contains fields we set before kicking off search.
type SearchInputs struct {
	Plan          query.Plan // the comprehensive query plan
	Query         query.Q    // the current basic query being evaluated, one part of query.Plan
	OriginalQuery string     // the raw string of the original search query
	PatternType   query.SearchType
	UserSettings  *schema.Settings
	Features      featureflag.FlagSet

	// DefaultLimit is the default limit to use if not specified in query.
	DefaultLimit int
}

// Job is an interface shared by all search backends. Calling Run on a job
// object runs a search. The relation with SearchInputs and Jobs is that
// SearchInputs are static values, parsed and validated, to produce Jobs. Jobs
// express semantic behavior at runtime across different backends and system
// architecture. The third argument accepts resolved repositories (which may or
// may not be required, depending on the job. E.g., a global search job does not
// require upfront repository resolution).
type Job interface {
	Run(context.Context, streaming.Sender, searchrepos.Pager) error
	Name() string
}

// MaxResults computes the limit for the query.
func (inputs SearchInputs) MaxResults() int {
	if inputs.Query == nil {
		return 0
	}

	if count := inputs.Query.Count(); count != nil {
		return *count
	}

	if inputs.DefaultLimit != 0 {
		return inputs.DefaultLimit
	}

	return search.DefaultMaxSearchResults
}
