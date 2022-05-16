package zoekt

import (
	"context"
	"time"

	zoektquery "github.com/google/zoekt/query"
	"github.com/opentracing/opentracing-go/log"

	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/filter"
	"github.com/sourcegraph/sourcegraph/internal/search/job"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/internal/trace"
)

type ZoektSymbolSearchJob struct {
	Repos          *IndexedRepoRevs // the set of indexed repository revisions to search.
	Query          zoektquery.Q
	FileMatchLimit int32
	Select         filter.SelectPath
	Since          func(time.Time) time.Duration `json:"-"` // since if non-nil will be used instead of time.Since. For tests
}

// Run calls the zoekt backend to search symbols
func (z *ZoektSymbolSearchJob) Run(ctx context.Context, clients job.RuntimeClients, stream streaming.Sender) (alert *search.Alert, err error) {
	tr, ctx, stream, finish := job.StartSpan(ctx, stream, z)
	defer func() { finish(alert, err) }()
	tr.TagFields(trace.LazyFields(z.Tags))

	if z.Repos == nil {
		return nil, nil
	}
	if len(z.Repos.RepoRevs) == 0 {
		return nil, nil
	}

	since := time.Since
	if z.Since != nil {
		since = z.Since
	}

	ctx, cancel := context.WithCancel(ctx)
	defer cancel()

	err = zoektSearch(ctx, z.Repos, z.Query, search.SymbolRequest, clients.Zoekt, z.FileMatchLimit, z.Select, since, stream)
	if err != nil {
		tr.LogFields(log.Error(err))
		// Only record error if we haven't timed out.
		if ctx.Err() == nil {
			cancel()
			return nil, err
		}
	}
	return nil, nil
}

func (z *ZoektSymbolSearchJob) Name() string {
	return "ZoektSymbolSearchJob"
}

func (z *ZoektSymbolSearchJob) Tags() []log.Field {
	tags := []log.Field{
		trace.Stringer("query", z.Query),
		log.Int32("fileMatchLimit", z.FileMatchLimit),
		trace.Stringer("select", z.Select),
	}
	// z.Repos is nil for un-indexed search
	if z.Repos != nil {
		tags = append(tags, log.Int("numRepoRevs", len(z.Repos.RepoRevs)))
		tags = append(tags, log.Int("numBranchRepos", len(z.Repos.branchRepos)))
	}
	return tags
}
