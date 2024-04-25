package codycontext

import (
	"context"
	"strings"
	"sync"

	"github.com/sourcegraph/conc/pool"
	"go.opentelemetry.io/otel/attribute"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/embeddings/embed"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/job"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const DefaultCodeResultsCount = 12
const DefaultTextResultsCount = 3

// NewSearchJob creates a new job for Cody context searches. It maps the query into a keyword query by breaking
// it into terms, applying light stemming, then combining the terms through an OR operator.
//
// When the job is run, it executes two child jobs: one for code and one for text. Each search is limited to a small
// number of file matches. The match limits can be adjusted by passing `codyCodeCount` and `codyTextCount` parameters
// (which are not user-facing and only intended for internal use).
//
// The job blocks until all results are collected, then streams them back to the caller. This gives flexibility to
// combine and reorder the results in any way.
func NewSearchJob(plan query.Plan, inputs *search.Inputs, newJob func(query.Basic) (job.Job, error)) (job.Job, error) {
	if len(plan) > 1 {
		return nil, errors.New("The 'codycontext' patterntype does not support multiple clauses")
	}

	codeCount, textCount := getResultLimits(inputs)
	fileMatcher := getFileMatcher(inputs)
	basicQuery := plan[0].ToParseTree()

	q, err := queryStringToKeywordQuery(query.StringHuman(basicQuery))

	if err != nil {
		return nil, err
	}

	params := q.query.Parameters
	patterns := q.patterns

	// If there are no patterns left, this query was entirely composed of stopwords, so we return no results.
	// ⚠️ We must return a no-op job instead of nil, since the job framework assumes all jobs are non-nil.
	if len(patterns) == 0 {
		return newNoopJob(), nil
	}

	codeQuery := q.query.MapParameters(append(params, query.Parameter{Field: query.FieldFile, Value: textFileFilter, Negated: true}))
	codeJob, err := newJob(codeQuery)
	if err != nil {
		return nil, err
	}

	textQuery := q.query.MapParameters(append(params, query.Parameter{Field: query.FieldFile, Value: textFileFilter}))
	textJob, err := newJob(textQuery)
	if err != nil {
		return nil, err
	}

	return &searchJob{codeJob, codeCount, textJob, textCount, fileMatcher, patterns}, nil
}

type codyFileMatcher = func(id api.RepoID, s string) bool

func getFileMatcher(inputs *search.Inputs) codyFileMatcher {
	if inputs.Features == nil || inputs.Features.CodyFileMatcher == nil {
		return func(id api.RepoID, s string) bool {
			return true
		}
	}
	return inputs.Features.CodyFileMatcher
}

func getResultLimits(inputs *search.Inputs) (codeCount, textCount int) {
	codeCount = DefaultCodeResultsCount
	textCount = DefaultTextResultsCount
	if inputs.Features == nil {
		return
	}

	if inputs.Features.CodyContextCodeCount > 0 {
		codeCount = inputs.Features.CodyContextCodeCount
	}
	if inputs.Features.CodyContextTextCount > 0 {
		textCount = inputs.Features.CodyContextTextCount
	}
	return
}

var textFileFilter = func() string {
	var extensions []string
	for extension := range embed.TextFileExtensions {
		extensions = append(extensions, extension)
	}
	return `(` + strings.Join(extensions, "|") + `)$`
}()

type searchJob struct {
	codeJob   job.Job
	codeCount int

	textJob   job.Job
	textCount int

	fileMatcher codyFileMatcher
	patterns    []string
}

func (j *searchJob) Run(ctx context.Context, clients job.RuntimeClients, stream streaming.Sender) (alert *search.Alert, err error) {
	_, ctx, stream, finish := job.StartSpan(ctx, stream, j)
	defer func() { finish(alert, err) }()

	wg := pool.NewWithResults[response]()
	wg.Go(func() response {
		return j.doSearch(ctx, clients, j.textJob, j.textCount)
	})
	wg.Go(func() response {
		return j.doSearch(ctx, clients, j.codeJob, j.codeCount)
	})
	responses := wg.Wait()

	for _, r := range responses {
		stream.Send(streaming.SearchEvent{
			Results: r.matches,
		})

		alert = search.MaxPriorityAlert(alert, r.alert)
		if r.err != nil {
			err = errors.Append(err, r.err)
		}
	}

	return alert, err
}

type response struct {
	matches result.Matches
	err     error
	alert   *search.Alert
}

func (j *searchJob) doSearch(ctx context.Context, clients job.RuntimeClients, job job.Job, limit int) response {
	var (
		mu        sync.Mutex
		collected result.Matches
	)

	ctx, cancel := context.WithCancel(ctx)
	defer cancel()

	stream := streaming.StreamFunc(func(e streaming.SearchEvent) {
		mu.Lock()
		defer mu.Unlock()

		if len(collected) >= limit {
			return
		}

		for _, res := range e.Results {
			if fm, ok := res.(*result.FileMatch); ok {
				if !j.fileMatcher(fm.Repo.ID, fm.Path) {
					continue
				}

				collected = append(collected, fm)
				if len(collected) >= limit {
					cancel()
					return
				}
			}
		}
	})

	alert, err := job.Run(ctx, clients, stream)
	return response{collected, err, alert}
}

func (j *searchJob) Name() string {
	return "CodyContextSearchJob"
}

func (j *searchJob) Attributes(v job.Verbosity) (res []attribute.KeyValue) {
	switch v {
	case job.VerbosityMax:
		fallthrough
	case job.VerbosityBasic:
		res = append(res,
			attribute.StringSlice("patterns", j.patterns),
			attribute.Int("codeCount", j.codeCount),
			attribute.Int("textCount", j.textCount),
		)
	}
	return res
}

func (j *searchJob) Children() []job.Describer {
	return nil
}

func (j *searchJob) MapChildren(job.MapFunc) job.Job {
	return j
}

func newNoopJob() *noopJob {
	return &noopJob{}
}

type noopJob struct{}

func (e *noopJob) Run(context.Context, job.RuntimeClients, streaming.Sender) (*search.Alert, error) {
	return nil, nil
}

func (e *noopJob) Name() string                                  { return "NoopJob" }
func (e *noopJob) Attributes(job.Verbosity) []attribute.KeyValue { return nil }
func (e *noopJob) Children() []job.Describer                     { return nil }
func (e *noopJob) MapChildren(job.MapFunc) job.Job               { return e }
