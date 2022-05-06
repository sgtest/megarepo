package jobutil

import (
	"context"
	"time"

	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/job"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// NewSequentialJob will create a job that sequentially runs a list of jobs.
// This is used to implement logic where we might like to order independent
// search operations, favoring results returns by jobs earlier in the list to
// those appearing later in the list. If this job sees a cancellation for a
// child job, it stops executing additional jobs and returns.
func NewSequentialJob(children ...job.Job) job.Job {
	if len(children) == 0 {
		return &NoopJob{}
	}
	if len(children) == 1 {
		return children[0]
	}
	return &SequentialJob{children: children}
}

type SequentialJob struct {
	children []job.Job
}

func (s *SequentialJob) Name() string {
	return "SequentialJob"
}

func (s *SequentialJob) Run(ctx context.Context, clients job.RuntimeClients, stream streaming.Sender) (alert *search.Alert, err error) {
	var maxAlerter search.MaxAlerter
	var errs errors.MultiError

	for _, child := range s.children {
		alert, err := child.Run(ctx, clients, stream)
		if ctx.Err() != nil {
			// Cancellation or Deadline hit implies it's time to stop running jobs.
			return maxAlerter.Alert, errs
		}
		maxAlerter.Add(alert)
		errs = errors.Append(errs, err)
	}
	return maxAlerter.Alert, errs
}

// NewParallelJob will create a job that runs all its child jobs in separate
// goroutines, then waits for all to complete. It returns an aggregated error
// if any of the child jobs failed.
func NewParallelJob(children ...job.Job) job.Job {
	if len(children) == 0 {
		return &NoopJob{}
	}
	if len(children) == 1 {
		return children[0]
	}
	return &ParallelJob{children: children}
}

type ParallelJob struct {
	children []job.Job
}

func (p *ParallelJob) Name() string {
	return "ParallelJob"
}

func (p *ParallelJob) Run(ctx context.Context, clients job.RuntimeClients, s streaming.Sender) (alert *search.Alert, err error) {
	_, ctx, s, finish := job.StartSpan(ctx, s, p)
	defer func() { finish(alert, err) }()

	var (
		g          errors.Group
		maxAlerter search.MaxAlerter
	)
	for _, child := range p.children {
		child := child
		g.Go(func() error {
			alert, err := child.Run(ctx, clients, s)
			maxAlerter.Add(alert)
			return err
		})
	}
	return maxAlerter.Alert, g.Wait()
}

// NewTimeoutJob creates a new job that is canceled after the
// timeout is hit. The timer starts with `Run()` is called.
func NewTimeoutJob(timeout time.Duration, child job.Job) job.Job {
	if _, ok := child.(*NoopJob); ok {
		return child
	}
	return &TimeoutJob{
		timeout: timeout,
		child:   child,
	}
}

type TimeoutJob struct {
	child   job.Job
	timeout time.Duration
}

func (t *TimeoutJob) Run(ctx context.Context, clients job.RuntimeClients, s streaming.Sender) (alert *search.Alert, err error) {
	_, ctx, s, finish := job.StartSpan(ctx, s, t)
	defer func() { finish(alert, err) }()

	ctx, cancel := context.WithTimeout(ctx, t.timeout)
	defer cancel()

	return t.child.Run(ctx, clients, s)
}

func (t *TimeoutJob) Name() string {
	return "TimeoutJob"
}

// NewLimitJob creates a new job that is canceled after the result limit
// is hit. Whenever an event is sent down the stream, the result count
// is incremented by the number of results in that event, and if it reaches
// the limit, the context is canceled.
func NewLimitJob(limit int, child job.Job) job.Job {
	if _, ok := child.(*NoopJob); ok {
		return child
	}
	return &LimitJob{
		limit: limit,
		child: child,
	}
}

type LimitJob struct {
	child job.Job
	limit int
}

func (l *LimitJob) Run(ctx context.Context, clients job.RuntimeClients, s streaming.Sender) (alert *search.Alert, err error) {
	_, ctx, s, finish := job.StartSpan(ctx, s, l)
	defer func() { finish(alert, err) }()

	ctx, s, cancel := streaming.WithLimit(ctx, s, l.limit)
	defer cancel()

	alert, err = l.child.Run(ctx, clients, s)
	if errors.Is(err, context.Canceled) {
		// Ignore context canceled errors
		err = nil
	}
	return alert, err

}

func (l *LimitJob) Name() string {
	return "LimitJob"
}

func NewNoopJob() *NoopJob {
	return &NoopJob{}
}

type NoopJob struct{}

func (e *NoopJob) Run(context.Context, job.RuntimeClients, streaming.Sender) (*search.Alert, error) {
	return nil, nil
}

func (e *NoopJob) Name() string { return "NoopJob" }
