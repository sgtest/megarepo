package streaming

import (
	"context"
	"net/http"
	"net/url"
	"strconv"
	"time"

	"github.com/inconshreveable/log15"
	otlog "github.com/opentracing/opentracing-go/log"

	"github.com/sourcegraph/sourcegraph/internal/database"
	streamhttp "github.com/sourcegraph/sourcegraph/internal/search/streaming/http"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// maxRequestDuration clamps any compute queries to run for at most 1 minute.
// It's possible to trigger longer-running queries with expensive operations,
// and this is best avoided on large instances like Sourcegraph.com
const maxRequestDuration = time.Minute

// NewComputeStreamHandler is an http handler which streams back compute results.
func NewComputeStreamHandler(db database.DB) http.Handler {
	return &streamHandler{
		db:                  db,
		flushTickerInternal: 100 * time.Millisecond,
	}
}

type streamHandler struct {
	db                  database.DB
	flushTickerInternal time.Duration
}

func (h *streamHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithTimeout(r.Context(), maxRequestDuration)
	defer cancel()

	args, err := parseURLQuery(r.URL.Query())
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	tr, ctx := trace.New(ctx, "compute.ServeStream", args.Query)
	defer func() {
		tr.SetError(err)
		tr.Finish()
	}()

	eventWriter, err := streamhttp.NewWriter(w)
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	// Always send a final done event so clients know the stream is shutting
	// down.
	defer eventWriter.Event("done", map[string]interface{}{})

	// Log events to trace
	eventWriter.StatHook = eventStreamOTHook(tr.LogFields)

	events, getErr := NewComputeStream(ctx, h.db, args.Query)
	events = batchEvents(events, 50*time.Millisecond)

	// Store marshalled matches and flush periodically or when we go over
	// 32kb. 32kb chosen to be smaller than bufio.MaxTokenSize. Note: we can
	// still write more than that.
	matchesBuf := streamhttp.NewJSONArrayBuf(32*1024, func(data []byte) error {
		return eventWriter.EventBytes("results", data)
	})
	matchesFlush := func() {
		if err := matchesBuf.Flush(); err != nil {
			// EOF
			return
		}
	}
	flushTicker := time.NewTicker(h.flushTickerInternal)
	defer flushTicker.Stop()

	first := true
	handleEvent := func(event Event) {
		for _, result := range event.Results {
			_ = matchesBuf.Append(result)
		}

		// Instantly send results if we have not sent any yet.
		if first && matchesBuf.Len() > 0 {
			log15.Info("flushing first now")
			first = false
			matchesFlush()
		}

	}

LOOP:
	for {
		select {
		case event, ok := <-events:
			if !ok {
				break LOOP
			}
			handleEvent(event)
		case <-flushTicker.C:
			matchesFlush()
		}
	}

	matchesFlush()

	if err = getErr(); err != nil {
		_ = eventWriter.Event("error", streamhttp.EventError{Message: err.Error()})
		return
	}

	if err := ctx.Err(); errors.Is(err, context.DeadlineExceeded) {
		_ = eventWriter.Event("alert", streamhttp.EventAlert{
			Title:       "Heads up",
			Description: "This data is incomplete! We ran this query for 1 minute and we'll need more time to compute all the results. Ask in #compute for more Compute Credits™",
		})
	}
}

type args struct {
	Query   string
	Display int
}

func parseURLQuery(q url.Values) (*args, error) {
	get := func(k, def string) string {
		v := q.Get(k)
		if v == "" {
			return def
		}
		return v
	}

	a := args{
		Query: get("q", ""),
	}

	if a.Query == "" {
		return nil, errors.New("no query found")
	}

	display := get("display", "-1") // TODO(rvantonder): Currently unused; implement a limit for compute results.
	var err error
	if a.Display, err = strconv.Atoi(display); err != nil {
		return nil, errors.Errorf("display must be an integer, got %q: %w", display, err)
	}

	return &a, nil
}

// batchEvents takes an event stream and merges events that come through close in time into a single event.
// This makes downstream database and network operations more efficient by enabling batch reads.
func batchEvents(source <-chan Event, delay time.Duration) <-chan Event {
	results := make(chan Event)
	go func() {
		defer close(results)

		// Send the first event without a delay
		firstEvent, ok := <-source
		if !ok {
			return
		}
		results <- firstEvent

	OUTER:
		for {
			// Wait for a first event
			event, ok := <-source
			if !ok {
				return
			}

			// Wait up to the delay for more events to come through,
			// and merge any that do into the first event
			timer := time.After(delay)
			for {
				select {
				case newEvent, ok := <-source:
					if !ok {
						// Flush the buffered event and exit
						results <- event
						return
					}
					event.Results = append(event.Results, newEvent.Results...)
				case <-timer:
					results <- event
					continue OUTER
				}
			}
		}

	}()
	return results
}

// eventStreamOTHook returns a StatHook which logs to log.
func eventStreamOTHook(log func(...otlog.Field)) func(streamhttp.WriterStat) {
	return func(stat streamhttp.WriterStat) {
		fields := []otlog.Field{
			otlog.String("streamhttp.Event", stat.Event),
			otlog.Int("bytes", stat.Bytes),
			otlog.Int64("duration_ms", stat.Duration.Milliseconds()),
		}
		if stat.Error != nil {
			fields = append(fields, otlog.Error(stat.Error))
		}
		log(fields...)
	}
}
