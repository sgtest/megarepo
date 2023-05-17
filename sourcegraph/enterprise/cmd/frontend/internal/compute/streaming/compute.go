package streaming

import (
	"context"

	"github.com/sourcegraph/conc/stream"
	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/compute"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/client"
	"github.com/sourcegraph/sourcegraph/internal/search/job/jobutil"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
)

func toComputeResult(ctx context.Context, cmd compute.Command, match result.Match) (out []compute.Result, _ error) {
	if v, ok := match.(*result.CommitMatch); ok && v.DiffPreview != nil {
		for _, diffMatch := range v.CommitToDiffMatches() {
			runResult, err := cmd.Run(ctx, diffMatch)
			if err != nil {
				return nil, err
			}
			out = append(out, runResult)
		}
	} else {
		runResult, err := cmd.Run(ctx, match)
		if err != nil {
			return nil, err
		}
		out = append(out, runResult)
	}
	return out, nil
}

func NewComputeStream(ctx context.Context, logger log.Logger, db database.DB, enterpriseJobs jobutil.EnterpriseJobs, searchQuery string, computeCommand compute.Command) (<-chan Event, func() (*search.Alert, error)) {
	eventsC := make(chan Event, 8)
	errorC := make(chan error, 1)
	s := stream.New().WithMaxGoroutines(8)
	cb := func(ev Event, err error) stream.Callback {
		return func() {
			if err != nil {
				select {
				case errorC <- err:
				default:
				}
			} else {
				eventsC <- ev
			}
		}
	}
	stream := streaming.StreamFunc(func(event streaming.SearchEvent) {
		if !event.Stats.Zero() {
			s.Go(func() stream.Callback {
				return cb(Event{nil, event.Stats}, nil)
			})
		}
		for _, match := range event.Results {
			match := match
			s.Go(func() stream.Callback {
				results, err := toComputeResult(ctx, computeCommand, match)
				return cb(Event{results, streaming.Stats{}}, err)
			})
		}
	})

	settings, err := graphqlbackend.DecodedViewerFinalSettings(ctx, db)
	if err != nil {
		close(eventsC)
		close(errorC)
		return eventsC, func() (*search.Alert, error) { return nil, err }
	}

	patternType := "regexp"
	searchClient := client.NewSearchClient(logger, db, search.Indexed(), search.SearcherURLs(), search.SearcherGRPCConnectionCache(), enterpriseJobs)
	inputs, err := searchClient.Plan(
		ctx,
		"",
		&patternType,
		searchQuery,
		search.Precise,
		search.Streaming,
		settings,
		envvar.SourcegraphDotComMode(),
	)
	if err != nil {
		close(eventsC)
		close(errorC)

		return eventsC, func() (*search.Alert, error) { return nil, err }
	}

	type finalResult struct {
		alert *search.Alert
		err   error
	}
	final := make(chan finalResult, 1)
	go func() {
		defer close(final)
		defer close(eventsC)
		defer close(errorC)
		defer s.Wait()

		alert, err := searchClient.Execute(ctx, stream, inputs)
		final <- finalResult{alert: alert, err: err}
	}()

	return eventsC, func() (*search.Alert, error) {
		computeErr := <-errorC
		if computeErr != nil {
			return nil, computeErr
		}
		f := <-final
		return f.alert, f.err
	}
}
