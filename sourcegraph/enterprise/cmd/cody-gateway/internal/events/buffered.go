package events

import (
	"context"
	"sync/atomic"
	"time"

	"github.com/sourcegraph/log"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/trace"

	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	sgtrace "github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type bufferedEvent struct {
	spanCtx context.Context
	Event
}

type BufferedLogger struct {
	log log.Logger

	// handler is the underlying event logger to which events are submitted.
	handler Logger

	// bufferC is a buffered channel of events to be logged.
	bufferC chan bufferedEvent
	// timeout is the max duration to wait to submit an event.
	timeout time.Duration

	// bufferClosed indicates if the buffer has been closed.
	bufferClosed *atomic.Bool
	// flushedC is a channel that is closed when the buffer is emptied.
	flushedC chan struct{}
}

var _ Logger = &BufferedLogger{}
var _ goroutine.BackgroundRoutine = &BufferedLogger{}

// defaultTimeout is the default timeout to wait for an event to be submitted,
// configured on NewBufferedLogger. The goal is to never block for long enough
// for the delay to become noticeable to the user - bufferSize is generally
// quite large, so we should never hit timeout in a normal situation.
var defaultTimeout = 150 * time.Millisecond

// NewBufferedLogger wraps handler with a buffered logger that submits events
// in the background instead of in the hot-path of a request. It implements
// goroutine.BackgroundRoutine that must be started.
func NewBufferedLogger(logger log.Logger, handler Logger, bufferSize int) *BufferedLogger {
	return &BufferedLogger{
		log: logger.Scoped("bufferedLogger", "buffered events logger"),

		handler: handler,

		bufferC: make(chan bufferedEvent, bufferSize),
		timeout: defaultTimeout,

		bufferClosed: &atomic.Bool{},
		flushedC:     make(chan struct{}),
	}
}

// LogEvent implements event.Logger by submitting the event to a buffer for processing.
func (l *BufferedLogger) LogEvent(spanCtx context.Context, event Event) error {
	// Track whether or not the event buffered, and how long it took.
	_, span := tracer.Start(backgroundContextWithSpan(spanCtx), "bufferedLogger.LogEvent",
		trace.WithAttributes(
			attribute.String("source", event.Source),
			attribute.String("event.name", string(event.Name))))
	var buffered bool
	defer func() {
		span.SetAttributes(
			attribute.Bool("event.buffered", buffered),
			attribute.Int("buffer.backlog", len(l.bufferC)))
		span.End()
	}()

	// If buffer is closed, make a best-effort attempt to log the event directly.
	if l.bufferClosed.Load() {
		sgtrace.Logger(spanCtx, l.log).Warn("buffer is closed: logging event directly")
		return l.handler.LogEvent(spanCtx, event)
	}

	select {
	case l.bufferC <- bufferedEvent{spanCtx: spanCtx, Event: event}:
		buffered = true
		return nil

	case <-time.After(l.timeout):
		// The buffer is full, which is indicative of a problem. We try to
		// submit the event immediately anyway, because we don't want to
		// silently drop anything, and log an error so that we ge notified.
		sgtrace.Logger(spanCtx, l.log).
			Error("failed to queue event within timeout, submitting event directly",
				log.Error(errors.New("buffer is full")), // real error needed for Sentry
				log.Int("buffer.capacity", cap(l.bufferC)),
				log.Int("buffer.backlog", len(l.bufferC)),
				log.Duration("timeout", l.timeout))
		return l.handler.LogEvent(spanCtx, event)
	}
}

// Start begins working by procssing the logger's buffer, blocking until stop
// is called and the backlog is cleared.
func (l *BufferedLogger) Start() {
	for event := range l.bufferC {
		if err := l.handler.LogEvent(event.spanCtx, event.Event); err != nil {
			sgtrace.Logger(event.spanCtx, l.log).
				Error("failed to log buffered event", log.Error(err))
		}
	}

	l.log.Info("all events flushed")
	close(l.flushedC)
}

// Stop stops buffered logger's background processing job and flushes its buffer.
func (l *BufferedLogger) Stop() {
	l.bufferClosed.Store(true)
	close(l.bufferC)
	l.log.Info("buffer closed - waiting for events to flush")

	start := time.Now()
	select {
	case <-l.flushedC:
		l.log.Info("shutdown complete",
			log.Duration("elapsed", time.Since(start)))

	// We may lose some events, but it won't be a lot since traffic should
	// already be routing to new instances when work is stopping.
	case <-time.After(10 * time.Second):
		l.log.Error("failed to shut down within shutdown deadline",
			log.Error(errors.Newf("unflushed events: %d", len(l.bufferC)))) // real error for Sentry
	}
}
