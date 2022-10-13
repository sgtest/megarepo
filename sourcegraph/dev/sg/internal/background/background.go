package background

import (
	"bytes"
	"context"
	"strings"
	"sync"
	"time"

	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/trace"
	"go.uber.org/atomic"

	"github.com/sourcegraph/sourcegraph/dev/sg/internal/analytics"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/std"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

type key int

var jobsKey key

type backgroundJobs struct {
	wg    sync.WaitGroup
	count atomic.Int32

	verbose bool
	output  chan string
}

// Context creates a context that can have background jobs added to it with background.Run
func Context(ctx context.Context, verbose bool) context.Context {
	return context.WithValue(ctx, jobsKey, &backgroundJobs{
		verbose: verbose,
		output:  make(chan string, 10), // reasonable default
	})
}

func loadFromContext(ctx context.Context) *backgroundJobs {
	return ctx.Value(jobsKey).(*backgroundJobs)
}

// Run starts the given job and registers it so that background.Wait does not exit until
// this job is complete.
//
// Jobs get a context timeout of 30 seconds.
func Run(ctx context.Context, job func(ctx context.Context, out *std.Output)) {
	jobs := loadFromContext(ctx)
	jobs.wg.Add(1)
	jobs.count.Add(1)

	b := new(bytes.Buffer)
	out := std.NewOutput(b, jobs.verbose)
	go func() {
		// Do not let the job run forever
		jobCtx, cancel := context.WithTimeout(ctx, 30*time.Second)
		defer cancel()
		// Execute job
		job(jobCtx, out)
		// Signal the completion of this job
		jobs.count.Dec()
		jobs.output <- strings.TrimSpace(b.String())
	}()
}

// Wait blocks until jobs registered in context are complete, rendering their results as
// they complete.
func Wait(ctx context.Context, out *std.Output) {
	jobs := loadFromContext(ctx)
	count := int(jobs.count.Load())
	if count == 0 {
		return // no jobs left
	}

	_, span := analytics.StartSpan(ctx, "background_wait", "",
		trace.WithAttributes(attribute.Int("jobs", count)))
	defer span.End()

	firstResultWithOutput := true
	if jobs.verbose {
		out.WriteLine(output.Styledf(output.StylePending, "Waiting for %d remaining background %s to complete...",
			count, pluralize("job", "jobs", count)))
	}
	go func() {
		// Stream job output as they complete
		for jobOutput := range jobs.output {
			if jobOutput != "" {
				if firstResultWithOutput {
					out.Write("") // separator for background output
					firstResultWithOutput = false
				}
				out.Write(jobOutput)
			}
			jobs.wg.Done()
		}
	}()
	jobs.wg.Wait()

	// Done!
	close(jobs.output)
	if jobs.verbose {
		out.WriteLine(output.Line(output.EmojiSuccess, output.StyleSuccess, "Background jobs done!"))
	}
	span.Succeeded()
}

func pluralize(single, plural string, count int) string {
	if count != 1 {
		return plural
	}
	return single
}
