package scheduler

import (
	"context"
	"time"

	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types/scheduler/config"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
)

// Scheduler provides a scheduling service that moves changesets from the
// scheduled state to the queued state based on the current rate limit, if
// anything. Changesets are processed in a FIFO manner.
type Scheduler struct {
	ctx   context.Context
	done  chan struct{}
	store *store.Store
}

var _ goroutine.BackgroundRoutine = &Scheduler{}

func NewScheduler(ctx context.Context, bstore *store.Store) *Scheduler {
	return &Scheduler{
		ctx:   ctx,
		done:  make(chan struct{}),
		store: bstore,
	}
}

func (s *Scheduler) Start() {
	// Set up a global backoff strategy where we start at 5 seconds, up to a
	// minute, when we don't have any changesets to enqueue. Without this, an
	// unlimited schedule will essentially busy-wait calling Take().
	backoff := newBackoff(5*time.Second, 2, 1*time.Minute)

	// Set up our configuration listener.
	cfg := config.Subscribe()
	defer config.Unsubscribe(cfg)

	for {
		schedule := config.ActiveWindow().Schedule()
		ticker := newTicker(schedule)
		validity := time.NewTimer(time.Until(schedule.ValidUntil()))

		log15.Debug("applying batch change schedule", "schedule", schedule, "until", schedule.ValidUntil())

	scheduleloop:
		for {
			select {
			case delay := <-ticker.C:
				// We can enqueue a changeset. Let's try to do so, ensuring that
				// we always return a duration back down the delay channel.
				if err := s.enqueueChangeset(); err != nil {
					// If we get an error back, we need to increment the backoff
					// delay and return that. enqueueChangeset will have handled
					// any logging we need to do.
					delay <- backoff.next()
				} else {
					// All is well, so we should reset the backoff delay and
					// loop immediately.
					backoff.reset()
					delay <- time.Duration(0)
				}

			case <-validity.C:
				// The schedule is no longer valid, so let's break out of this
				// loop and build a new schedule.
				break scheduleloop

			case <-cfg:
				// The batch change rollout window configuration was updated, so
				// let's break out of this loop and build a new schedule.
				break scheduleloop

			case <-s.done:
				// The scheduler service has been asked to stop, so let's stop.
				log15.Debug("stopping the batch change scheduler")
				ticker.stop()
				return
			}
		}

		ticker.stop()
	}
}

func (s *Scheduler) Stop() {
	s.done <- struct{}{}
	close(s.done)
}

func (s *Scheduler) enqueueChangeset() error {
	_, err := s.store.EnqueueNextScheduledChangeset(s.ctx)

	// Let's see if this is an error caused by there being no changesets to
	// enqueue (which is fine), or something less expected, in which case we
	// should log the error.
	if err != nil && err != store.ErrNoResults {
		log15.Warn("error enqueueing the next scheduled changeset", "err", err)
	}

	return err
}

// backoff implements a very simple bounded exponential backoff strategy.
type backoff struct {
	init       time.Duration
	multiplier int
	limit      time.Duration

	current time.Duration
}

func newBackoff(init time.Duration, multiplier int, limit time.Duration) *backoff {
	return &backoff{
		init:       init,
		multiplier: multiplier,
		limit:      limit,
		current:    init,
	}
}

func (b *backoff) next() time.Duration {
	curr := b.current

	b.current *= time.Duration(b.multiplier)
	if b.current > b.limit {
		b.current = b.limit
	}

	return curr
}

func (b *backoff) reset() {
	b.current = b.init
}
