// package search is search specific logic for the frontend. Also see
// github.com/sourcegraph/sourcegraph/internal/search for more generic search
// code.
package search

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"

	"github.com/inconshreveable/log15"
	"github.com/opentracing/opentracing-go"
	otlog "github.com/opentracing/opentracing-go/log"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	searchlogs "github.com/sourcegraph/sourcegraph/cmd/frontend/internal/search/logs"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/honey"
	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	streamhttp "github.com/sourcegraph/sourcegraph/internal/search/streaming/http"
	"github.com/sourcegraph/sourcegraph/internal/trace"
)

// StreamHandler is an http handler which streams back search results.
func StreamHandler(db dbutil.DB) http.Handler {
	return &streamHandler{
		db:                  db,
		newSearchResolver:   defaultNewSearchResolver,
		flushTickerInternal: 100 * time.Millisecond,
		pingTickerInterval:  5 * time.Second,
	}
}

type streamHandler struct {
	db                  dbutil.DB
	newSearchResolver   func(context.Context, dbutil.DB, *graphqlbackend.SearchArgs) (searchResolver, error)
	flushTickerInternal time.Duration
	pingTickerInterval  time.Duration
}

func (h *streamHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithCancel(r.Context())
	defer cancel()

	args, err := parseURLQuery(r.URL.Query())
	if err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	tr, ctx := trace.New(ctx, "search.ServeStream", args.Query,
		trace.Tag{Key: "version", Value: args.Version},
		trace.Tag{Key: "pattern_type", Value: args.PatternType},
		trace.Tag{Key: "version_context", Value: args.VersionContext},
	)
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

	events, inputs, results := h.startSearch(ctx, args)

	traceURL := ""
	if span := opentracing.SpanFromContext(ctx); span != nil {
		spanURL := trace.SpanURL(span)
		// URLs starting with # don't have a trace. eg
		// "#tracer-not-enabled"
		if !strings.HasPrefix(spanURL, "#") {
			traceURL = spanURL
		}
	}

	// Display is the number of results we send down. If display is < 0 we
	// want to send everything we find before hitting a limit. Otherwise we
	// can only send up to limit results.
	display := args.Display
	limit := inputs.MaxResults()
	if display < 0 || display > limit {
		display = limit
	}

	start := time.Now()

	displayLimit := display
	if display < 0 {
		displayLimit = math.MaxInt32
	}
	progress := progressAggregator{
		Start:        start,
		Limit:        inputs.MaxResults(),
		Trace:        traceURL,
		DisplayLimit: displayLimit,
	}

	sendProgress := func() {
		_ = eventWriter.Event("progress", progress.Current())
	}

	filters := &graphqlbackend.SearchFilters{
		Globbing: false, // TODO
	}

	// Store marshalled matches and flush periodically or when we go over
	// 32kb.
	matchesBuf := &jsonArrayBuf{
		// 32kb chosen to be smaller than bufio.MaxTokenSize. Note: we can
		// still write more than that.
		FlushSize: 32 * 1024,
		Write: func(data []byte) error {
			return eventWriter.EventBytes("matches", data)
		},
	}
	matchesFlush := func() {
		if err := matchesBuf.Flush(); err != nil {
			// EOF
			return
		}

		if progress.Dirty {
			sendProgress()
		}
	}
	matchesAppend := func(m streamhttp.EventMatch) {
		// Only possible error is EOF, ignore
		_ = matchesBuf.Append(m)
	}

	flushTicker := time.NewTicker(h.flushTickerInternal)
	defer flushTicker.Stop()

	pingTicker := time.NewTicker(h.pingTickerInterval)
	defer pingTicker.Stop()

	first := true

	for {
		var event graphqlbackend.SearchEvent
		var ok bool
		select {
		case event, ok = <-events:
		case <-flushTicker.C:
			ok = true
			matchesFlush()
		case <-pingTicker.C:
			ok = true
			sendProgress()
		}

		if !ok {
			break
		}

		progress.Update(event)
		filters.Update(event)

		for _, result := range event.Results {
			if display <= 0 {
				break
			}

			if fm, ok := result.ToFileMatch(); ok {
				display = fm.Limit(display)

				if syms := fm.Symbols(); len(syms) > 0 {
					// Inlining to avoid exporting a bunch of stuff from
					// graphqlbackend
					symbols := make([]streamhttp.Symbol, 0, len(syms))
					for _, sym := range syms {
						u, err := sym.URL(ctx)
						if err != nil {
							continue
						}
						symbols = append(symbols, streamhttp.Symbol{
							URL:           u,
							Name:          sym.Name(),
							ContainerName: fromStrPtr(sym.ContainerName()),
							Kind:          sym.Kind(),
						})
					}
					matchesAppend(fromSymbolMatch(fm, symbols))
				} else {
					matchesAppend(fromFileMatch(&fm.FileMatch))
				}
			}
			if repo, ok := result.ToRepository(); ok {
				display = repo.Limit(display)

				matchesAppend(fromRepository(repo.RepoMatch))
			}
			if commit, ok := result.ToCommitSearchResult(); ok {
				display = commit.Limit(display)

				matchesAppend(fromCommit(commit))
			}
		}

		// Instantly send results if we have not sent any yet.
		if first && matchesBuf.Len() > 0 {
			first = false
			matchesFlush()

			metricLatency.WithLabelValues(string(GuessSource(r))).
				Observe(time.Since(start).Seconds())

			graphqlbackend.LogSearchLatency(ctx, h.db, &inputs, int32(time.Since(start).Milliseconds()))
		}
	}

	matchesFlush()

	// Send dynamic filters once.
	if filters := filters.Compute(); len(filters) > 0 {
		buf := make([]streamhttp.EventFilter, 0, len(filters))
		for _, f := range filters {
			buf = append(buf, streamhttp.EventFilter{
				Value:    f.Value,
				Label:    f.Label,
				Count:    f.Count,
				LimitHit: f.IsLimitHit,
				Kind:     f.Kind,
			})
		}

		if err := eventWriter.Event("filters", buf); err != nil {
			// EOF
			return
		}
	}

	resultsResolver, err := results()
	if err != nil {
		_ = eventWriter.Event("error", streamhttp.EventError{Message: err.Error()})
		return
	}

	alert := resultsResolver.Alert()
	if alert != nil {
		var pqs []streamhttp.ProposedQuery
		if proposed := alert.ProposedQueries(); proposed != nil {
			for _, pq := range *proposed {
				pqs = append(pqs, streamhttp.ProposedQuery{
					Description: fromStrPtr(pq.Description()),
					Query:       pq.Query(),
				})
			}
		}
		_ = eventWriter.Event("alert", streamhttp.EventAlert{
			Title:           alert.Title(),
			Description:     fromStrPtr(alert.Description()),
			ProposedQueries: pqs,
		})
	}

	_ = eventWriter.Event("progress", progress.Final())

	var status, alertType string
	status = graphqlbackend.DetermineStatusForLogs(resultsResolver, err)
	if alert != nil {
		alertType = alert.PrometheusType()
	}

	isSlow := time.Since(start) > searchlogs.LogSlowSearchesThreshold()
	if honey.Enabled() || isSlow {
		ev := honey.SearchEvent(ctx, honey.SearchEventArgs{
			OriginalQuery: inputs.OriginalQuery,
			Typ:           "stream",
			Source:        string(trace.RequestSource(ctx)),
			Status:        status,
			AlertType:     alertType,
			DurationMs:    time.Since(start).Milliseconds(),
			ResultSize:    progress.MatchCount,
		})

		if honey.Enabled() {
			_ = ev.Send()
		}

		if isSlow {
			log15.Warn("streaming: slow search request", searchlogs.MapToLog15Ctx(ev.Fields())...)
		}
	}
}

// startSearch will start a search. It returns the events channel which
// streams out search events. Once events is closed you can call results which
// will return the results resolver and error.
func (h *streamHandler) startSearch(ctx context.Context, a *args) (events <-chan graphqlbackend.SearchEvent, inputs graphqlbackend.SearchInputs, results func() (*graphqlbackend.SearchResultsResolver, error)) {
	eventsC := make(chan graphqlbackend.SearchEvent)

	search, err := h.newSearchResolver(ctx, h.db, &graphqlbackend.SearchArgs{
		Query:          a.Query,
		Version:        a.Version,
		PatternType:    strPtr(a.PatternType),
		VersionContext: strPtr(a.VersionContext),

		Stream: graphqlbackend.StreamFunc(func(event graphqlbackend.SearchEvent) {
			eventsC <- event
		}),
	})
	if err != nil {
		close(eventsC)
		return eventsC, graphqlbackend.SearchInputs{}, func() (*graphqlbackend.SearchResultsResolver, error) {
			return nil, err
		}
	}

	type finalResult struct {
		resultsResolver *graphqlbackend.SearchResultsResolver
		err             error
	}
	final := make(chan finalResult, 1)
	go func() {
		defer close(final)
		defer close(eventsC)

		r, err := search.Results(ctx)
		final <- finalResult{resultsResolver: r, err: err}
	}()

	return eventsC, search.Inputs(), func() (*graphqlbackend.SearchResultsResolver, error) {
		f := <-final
		return f.resultsResolver, f.err
	}
}

type searchResolver interface {
	Results(context.Context) (*graphqlbackend.SearchResultsResolver, error)
	Inputs() graphqlbackend.SearchInputs
}

func defaultNewSearchResolver(ctx context.Context, db dbutil.DB, args *graphqlbackend.SearchArgs) (searchResolver, error) {
	return graphqlbackend.NewSearchImplementer(ctx, db, args)
}

type args struct {
	Query          string
	Version        string
	PatternType    string
	VersionContext string
	Display        int
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
		Query:          get("q", ""),
		Version:        get("v", "V2"),
		PatternType:    get("t", ""),
		VersionContext: get("vc", ""),
	}

	if a.Query == "" {
		return nil, errors.New("no query found")
	}

	display := get("display", "-1")
	var err error
	if a.Display, err = strconv.Atoi(display); err != nil {
		return nil, fmt.Errorf("display must be an integer, got %q: %w", display, err)
	}

	return &a, nil
}

func strPtr(s string) *string {
	if s == "" {
		return nil
	}
	return &s
}

func fromStrPtr(s *string) string {
	if s == nil {
		return ""
	}
	return *s
}

func fromFileMatch(fm *result.FileMatch) *streamhttp.EventFileMatch {
	lineMatches := make([]streamhttp.EventLineMatch, 0, len(fm.LineMatches))
	for _, lm := range fm.LineMatches {
		lineMatches = append(lineMatches, streamhttp.EventLineMatch{
			Line:             lm.Preview,
			LineNumber:       lm.LineNumber,
			OffsetAndLengths: lm.OffsetAndLengths,
		})
	}

	var branches []string
	if fm.InputRev != nil {
		branches = []string{*fm.InputRev}
	}

	return &streamhttp.EventFileMatch{
		Type:        streamhttp.FileMatchType,
		Path:        fm.Path,
		Repository:  string(fm.Repo.Name),
		Branches:    branches,
		Version:     string(fm.CommitID),
		LineMatches: lineMatches,
	}
}

func fromSymbolMatch(fm *graphqlbackend.FileMatchResolver, symbols []streamhttp.Symbol) *streamhttp.EventSymbolMatch {
	var branches []string
	if fm.InputRev != nil {
		branches = []string{*fm.InputRev}
	}

	return &streamhttp.EventSymbolMatch{
		Type:       streamhttp.SymbolMatchType,
		Path:       fm.Path,
		Repository: string(fm.Repo.Name),
		Branches:   branches,
		Version:    string(fm.CommitID),
		Symbols:    symbols,
	}
}

func fromRepository(rm result.RepoMatch) *streamhttp.EventRepoMatch {
	var branches []string
	if rev := rm.Rev; rev != "" {
		branches = []string{rev}
	}

	return &streamhttp.EventRepoMatch{
		Type:       streamhttp.RepoMatchType,
		Repository: string(rm.Name),
		Branches:   branches,
	}
}

func fromCommit(commit *graphqlbackend.CommitSearchResultResolver) *streamhttp.EventCommitMatch {
	var content string
	var ranges [][3]int32
	if matches := commit.Matches(); len(matches) == 1 {
		match := matches[0]
		content = match.Body().Text()
		highlights := match.Highlights()
		ranges = make([][3]int32, len(highlights))
		for i, h := range highlights {
			ranges[i] = [3]int32{h.Line(), h.Character(), h.Length()}
		}
	}
	return &streamhttp.EventCommitMatch{
		Type:    streamhttp.CommitMatchType,
		Label:   commit.Label().Text(),
		URL:     commit.URL(),
		Detail:  commit.Detail().Text(),
		Content: content,
		Ranges:  ranges,
	}
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

// jsonArrayBuf builds up a JSON array by marshalling per item. Once the array
// has reached FlushSize it will be written out via Write and the buffer will
// be reset.
type jsonArrayBuf struct {
	FlushSize int
	Write     func([]byte) error

	buf bytes.Buffer
}

// Append marshals v and adds it to the json array buffer. If the size of the
// buffer exceed FlushSize the buffer is written out.
func (j *jsonArrayBuf) Append(v interface{}) error {
	b, err := json.Marshal(v)
	if err != nil {
		return err
	}

	if j.buf.Len() == 0 {
		j.buf.WriteByte('[')
	} else {
		j.buf.WriteByte(',')
	}

	// err is always nil for a bytes.Buffer
	_, _ = j.buf.Write(b)

	if j.buf.Len() >= j.FlushSize {
		return j.Flush()
	}
	return nil
}

// Flush writes and resets the buffer if there is data to write.
func (j *jsonArrayBuf) Flush() error {
	if j.buf.Len() == 0 {
		return nil
	}

	// Terminate array
	j.buf.WriteByte(']')

	buf := j.buf.Bytes()
	j.buf.Reset()
	return j.Write(buf)
}

func (j *jsonArrayBuf) Len() int {
	return j.buf.Len()
}

var metricLatency = promauto.NewHistogramVec(prometheus.HistogramOpts{
	Name:    "src_search_streaming_latency_seconds",
	Help:    "Histogram with time to first result in seconds",
	Buckets: []float64{0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 30},
}, []string{"source"})

var searchBlitzUserAgentRegexp = lazyregexp.New(`^SearchBlitz \(([^\)]+)\)$`)

// GuessSource guesses the source the request came from (browser, other HTTP client, etc.)
func GuessSource(r *http.Request) trace.SourceType {
	userAgent := r.UserAgent()
	for _, guess := range []string{
		"Mozilla",
		"WebKit",
		"Gecko",
		"Chrome",
		"Firefox",
		"Safari",
		"Edge",
	} {
		if strings.Contains(userAgent, guess) {
			return trace.SourceBrowser
		}
	}

	// We send some automated search requests in order to measure baseline search perf. Track the source of these.
	if match := searchBlitzUserAgentRegexp.FindStringSubmatch(userAgent); match != nil {
		return trace.SourceType("searchblitz_" + match[1])
	}

	return trace.SourceOther
}
