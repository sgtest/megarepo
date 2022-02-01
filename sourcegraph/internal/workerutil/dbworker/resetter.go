package dbworker

import (
	"context"
	"time"

	"github.com/cockroachdb/errors"
	"github.com/derision-test/glock"
	"github.com/inconshreveable/log15"
	"github.com/prometheus/client_golang/prometheus"

	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
)

// Resetter periodically moves all unlocked records that have been in the processing state
// for a while back to queued.
//
// An unlocked record signifies that it is not actively being processed and records in this
// state for more than a few seconds are very likely to be stuck after the worker processing
// them has crashed.
type Resetter struct {
	store    store.Store
	options  ResetterOptions
	clock    glock.Clock
	ctx      context.Context // root context passed to the database
	cancel   func()          // cancels the root context
	finished chan struct{}   // signals that Start has finished
}

type ResetterOptions struct {
	Name     string
	Interval time.Duration
	Metrics  ResetterMetrics
}

type ResetterMetrics struct {
	RecordResets        prometheus.Counter
	RecordResetFailures prometheus.Counter
	Errors              prometheus.Counter
}

// NewMetrics returns a metrics object for a resetter that follows standard naming convention. The base metric name should be
// the same metric name provided to a `worker` ex. my_job_queue. Do not provide prefix "src" or postfix "_record...".
func NewMetrics(observationContext *observation.Context, metricNameRoot string) *ResetterMetrics {
	resets := prometheus.NewCounter(prometheus.CounterOpts{
		Name: "src_" + metricNameRoot + "_record_resets_total",
		Help: "The number of stalled record resets.",
	})
	observationContext.Registerer.MustRegister(resets)

	resetFailures := prometheus.NewCounter(prometheus.CounterOpts{
		Name: "src_" + metricNameRoot + "_record_reset_failures_total",
		Help: "The number of stalled record resets marked as failure.",
	})
	observationContext.Registerer.MustRegister(resetFailures)

	resetErrors := prometheus.NewCounter(prometheus.CounterOpts{
		Name: "src_" + metricNameRoot + "_record_reset_errors_total",
		Help: "The number of errors that occur during stalled " +
			"record reset.",
	})
	observationContext.Registerer.MustRegister(resetErrors)

	return &ResetterMetrics{
		RecordResets:        resets,
		RecordResetFailures: resetFailures,
		Errors:              resetErrors,
	}
}

func NewResetter(store store.Store, options ResetterOptions) *Resetter {
	return newResetter(store, options, glock.NewRealClock())
}

func newResetter(store store.Store, options ResetterOptions, clock glock.Clock) *Resetter {
	if options.Name == "" {
		panic("no name supplied to github.com/sourcegraph/sourcegraph/internal/dbworker/newResetter")
	}

	ctx, cancel := context.WithCancel(context.Background())

	return &Resetter{
		store:    store,
		options:  options,
		clock:    clock,
		ctx:      ctx,
		cancel:   cancel,
		finished: make(chan struct{}),
	}
}

// Start begins periodically calling reset stalled on the underlying store.
func (r *Resetter) Start() {
	defer close(r.finished)

loop:
	for {
		resetLastHeartbeatsByIDs, failedLastHeartbeatsByIDs, err := r.store.ResetStalled(r.ctx)
		if err != nil {
			if r.ctx.Err() != nil && errors.Is(err, r.ctx.Err()) {
				// If the error is due to the loop being shut down, just break
				break loop
			}

			r.options.Metrics.Errors.Inc()
			log15.Error("Failed to reset stalled records", "name", r.options.Name, "error", err)
		}

		for id, lastHeartbeatAge := range resetLastHeartbeatsByIDs {
			log15.Warn("Reset stalled record back to 'queued' state", "name", r.options.Name, "id", id, "timeSinceLastHeartbeat", lastHeartbeatAge)
		}
		for id, lastHeartbeatAge := range failedLastHeartbeatsByIDs {
			log15.Warn("Reset stalled record to 'failed' state", "name", r.options.Name, "id", id, "timeSinceLastHeartbeat", lastHeartbeatAge)
		}

		r.options.Metrics.RecordResets.Add(float64(len(resetLastHeartbeatsByIDs)))
		r.options.Metrics.RecordResetFailures.Add(float64(len(failedLastHeartbeatsByIDs)))

		select {
		case <-r.clock.After(r.options.Interval):
		case <-r.ctx.Done():
			return
		}
	}
}

// Stop will cause the resetter loop to exit after the current iteration.
func (r *Resetter) Stop() {
	r.cancel()
	<-r.finished
}
