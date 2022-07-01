// Package job contains the definitions and helpers for search jobs.
// This imports of this package should stay minimal so it can be referenced
// by other packages without pulling in a large set of transitive dependencies.
package job

import (
	"context"

	"github.com/google/zoekt"
	otlog "github.com/opentracing/opentracing-go/log"
	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/endpoint"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
)

// Job is an interface shared by all individual search operations in the
// backend (e.g., text vs commit vs symbol search are represented as different
// jobs) as well as combinations over those searches (run a set in parallel,
// timeout). Calling Run on a job object runs a search.
type Job interface {
	Run(context.Context, RuntimeClients, streaming.Sender) (*search.Alert, error)
	Name() string
	Tags() []otlog.Field
}

type RuntimeClients struct {
	Logger       log.Logger
	DB           database.DB
	Zoekt        zoekt.Streamer
	SearcherURLs *endpoint.Map
	Gitserver    gitserver.Client
}
