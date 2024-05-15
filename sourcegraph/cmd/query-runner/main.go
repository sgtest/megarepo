//docker:user sourcegraph

package main

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"
	"time"

	"github.com/pkg/errors"
	log15 "gopkg.in/inconshreveable/log15.v2"

	"github.com/sourcegraph/sourcegraph/cmd/query-runner/queryrunnerapi"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/debugserver"
	"github.com/sourcegraph/sourcegraph/pkg/env"
	"github.com/sourcegraph/sourcegraph/pkg/eventlogger"
	"github.com/sourcegraph/sourcegraph/pkg/tracer"
)

var (
	forceRunInterval = env.Get("FORCE_RUN_INTERVAL", "", "Force an interval to run saved queries at, instead of assuming query execution time * 30 (query that takes 2s to run, runs every 60s)")
	insecureDev, _   = strconv.ParseBool(env.Get("INSECURE_DEV", "false", "Running in insecure dev (local laptop) mode"))
)

const port = "3183"

func main() {
	env.Lock()
	env.HandleHelpFlag()

	tracer.Init()

	go func() {
		c := make(chan os.Signal, 1)
		signal.Notify(c, syscall.SIGINT, syscall.SIGHUP)
		<-c
		os.Exit(0)
	}()

	go debugserver.Start()

	http.HandleFunc(queryrunnerapi.PathSavedQueryWasCreatedOrUpdated, serveSavedQueryWasCreatedOrUpdated)
	http.HandleFunc(queryrunnerapi.PathSavedQueryWasDeleted, serveSavedQueryWasDeleted)
	http.HandleFunc(queryrunnerapi.PathTestNotification, serveTestNotification)

	ctx := context.Background()

	go func() {
		err := executor.run(ctx)
		if err != nil {
			log15.Error("executor: failed to run due to error", "error", err)
		}
	}()

	host := ""
	if insecureDev {
		host = "127.0.0.1"
	}
	addr := net.JoinHostPort(host, port)
	log15.Info("query-runner: listening", "addr", addr)
	log.Fatalf("Fatal error serving: %s", http.ListenAndServe(addr, nil))
}

func writeError(w http.ResponseWriter, err error) {
	w.WriteHeader(http.StatusInternalServerError)
	err2 := json.NewEncoder(w).Encode(&queryrunnerapi.ErrorResponse{
		Message: err.Error(),
	})
	if err2 != nil {
		log15.Error("error encoding HTTP error response", "error", err2.Error(), "writing_error", err.Error())
	}
}

// Useful for debugging e.g. email and slack notifications. Set it to true and
// it will send one notification on server startup, effectively.
var debugPretendSavedQueryResultsExist = false

var executor = &executorT{}

type executorT struct {
	forceRunInterval *time.Duration
}

func (e *executorT) run(ctx context.Context) error {
	// Parse FORCE_RUN_INTERVAL value.
	if forceRunInterval != "" {
		forceRunInterval, err := time.ParseDuration(forceRunInterval)
		if err != nil {
			log15.Error("executor: failed to parse FORCE_RUN_INTERVAL", "error", err)
			return nil
		}
		e.forceRunInterval = &forceRunInterval
	}

	// Kick off fetching of the full list of saved queries from the frontend.
	// Important to do this early on in case we get created/updated/deleted
	// notifications for saved queries.
	allSavedQueries.fetchInitialListFromFrontend()

	// TODO(slimsag): Make gitserver notify us about repositories being updated
	// as we could avoid executing queries if repositories haven't updated
	// (impossible for new results to exist).
	for {
		allSavedQueries := allSavedQueries.get()
		start := time.Now()
		for _, query := range allSavedQueries {
			err := e.runQuery(ctx, query.Spec, query.Config)
			if err != nil {
				log15.Error("executor: failed to run query", "error", err, "query_description", query.Config.Description)
			}
		}

		// If running all the queries didn't take very long (due to them
		// erroring out quickly, or if we had zero to run, or if they very
		// quickly produced zero results), then sleep for a few second to
		// prevent busy waiting and needlessly polling the DB.
		if time.Since(start) < time.Second {
			time.Sleep(5 * time.Second)
		}
	}
}

// runQuery runs the given query if an appropriate amount of time has elapsed
// since it last ran.
func (e *executorT) runQuery(ctx context.Context, spec api.SavedQueryIDSpec, query api.ConfigSavedQuery) error {
	if !query.Notify && !query.NotifySlack {
		// No need to run this query because there will be nobody to notify.
		return nil
	}
	if !strings.Contains(query.Query, "type:diff") && !strings.Contains(query.Query, "type:commit") {
		// TODO(slimsag): we temporarily do not support non-commit search
		// queries, since those do not support the after:"time" operator.
		return nil
	}

	info, err := api.InternalClient.SavedQueriesGetInfo(ctx, query.Query)
	if err != nil {
		return errors.Wrap(err, "SavedQueriesGetInfo")
	}

	// If the saved query was executed recently in the past, then skip it to
	// avoid putting too much pressure on searcher/gitserver.
	if info != nil {
		// We assume a run interval of 30x that which it takes to execute the
		// query. For example, a query which takes 2s to execute will run (2s*30)
		// every minute.
		//
		// Additionally, in case queries run very quickly (e.g. our after:
		// queries with no results often return in ~15ms), we impose a minimum
		// run interval of 10s.
		runInterval := info.ExecDuration * 30
		if runInterval < 10*time.Second {
			runInterval = 10 * time.Second
		}
		if e.forceRunInterval != nil {
			runInterval = *e.forceRunInterval
		}
		if time.Since(info.LastExecuted) < runInterval {
			return nil // too early to run the query
		}
	}

	// Construct a new query which finds search results introduced after the
	// last time we queried.
	var latestKnownResult time.Time
	if info != nil {
		latestKnownResult = info.LatestResult
	} else {
		// We've never executed this search query before, so use the current
		// time. We'll most certainly find nothing, which is okay.
		latestKnownResult = time.Now()
	}
	afterTime := latestKnownResult.UTC().Format(time.RFC3339)
	newQuery := strings.Join([]string{query.Query, fmt.Sprintf(`after:"%s"`, afterTime)}, " ")
	if debugPretendSavedQueryResultsExist {
		debugPretendSavedQueryResultsExist = false
		newQuery = query.Query
	}

	// Perform the search and mark the saved query as having been executed in
	// the database. We do this regardless of whether or not the search query
	// fails in order to avoid e.g. failed saved queries from executing
	// constantly and potentially causing harm to the system. We'll retry at
	// our normal interval, regardless of errors.
	v, searchErr, execDuration := performSearch(ctx, newQuery)
	if err := api.InternalClient.SavedQueriesSetInfo(ctx, &api.SavedQueryInfo{
		Query:        query.Query,
		LastExecuted: time.Now(),
		LatestResult: latestResultTime(info, v, searchErr),
		ExecDuration: execDuration,
	}); err != nil {
		return errors.Wrap(err, "SavedQueriesSetInfo")
	}

	if searchErr != nil {
		return searchErr
	}

	// Send notifications for new search results in a separate goroutine, so
	// that we don't block other search queries from running in sequence (which
	// is done intentionally, to ensure no overloading of searcher/gitserver).
	go func() {
		if err := notify(context.Background(), spec, query, newQuery, v); err != nil {
			log15.Error("executor: failed to send notifications", "error", err)
		}
	}()
	return nil
}

func performSearch(ctx context.Context, query string) (v *gqlSearchResponse, err error, execDuration time.Duration) {
	attempts := 0
	for {
		// Query for search results.
		start := time.Now()
		v, err := search(ctx, query)
		execDuration := time.Since(start)
		if err != nil {
			return nil, errors.Wrap(err, "search"), execDuration
		}
		if len(v.Data.Search.Results.Results) > 0 {
			return v, nil, execDuration // We have at least some search results, so we're done.
		}

		cloning := len(v.Data.Search.Results.Cloning)
		timedout := len(v.Data.Search.Results.Timedout)
		if cloning == 0 && timedout == 0 {
			return v, nil, execDuration // zero results, but no cloning or timed out repos. No point in retrying.
		}

		if attempts > 5 {
			return nil, fmt.Errorf("found 0 results due to %d cloning %d timedout repos", cloning, timedout), execDuration
		}

		// We didn't find any search results. Some repos are cloning or timed
		// out, so try again in a few seconds.
		attempts++
		log15.Warn("executor: failed to run query found 0 search results due to cloning or timed out repos (retrying in 5s)", "cloning", cloning, "timedout", timedout)
		time.Sleep(5 * time.Second)
	}
}

func latestResultTime(prevInfo *api.SavedQueryInfo, v *gqlSearchResponse, searchErr error) time.Time {
	if searchErr != nil || len(v.Data.Search.Results.Results) == 0 {
		// Error performing the search, or there were no results. Assume the
		// previous info's result time.
		if prevInfo != nil {
			return prevInfo.LatestResult
		}
		return time.Now()
	}

	// Results are ordered chronologically, so first result is the latest.
	t, err := extractTime(v.Data.Search.Results.Results[0])
	if err != nil {
		// Error already logged by extractTime.
		return time.Now()
	}
	return *t
}

var appURL *url.URL

// notify handles sending notifications for new search results.
func notify(ctx context.Context, spec api.SavedQueryIDSpec, query api.ConfigSavedQuery, newQuery string, results *gqlSearchResponse) error {
	if len(results.Data.Search.Results.Results) == 0 {
		return nil
	}
	log15.Info("sending notifications", "new_results", len(results.Data.Search.Results.Results), "description", query.Description)

	// Determine which users to notify.
	recipients, err := getNotificationRecipients(ctx, spec, query)
	if err != nil {
		return err
	}

	// Send slack notifications.
	n := &notifier{
		spec:       spec,
		query:      query,
		newQuery:   newQuery,
		results:    results,
		recipients: recipients,
	}

	// Send Slack and email notifications.
	n.slackNotify(ctx)
	n.emailNotify(ctx)
	return nil
}

type notifier struct {
	spec       api.SavedQueryIDSpec
	query      api.ConfigSavedQuery
	newQuery   string
	results    *gqlSearchResponse
	recipients recipients
}

const (
	utmSourceEmail = "saved-search-email"
	utmSourceSlack = "saved-search-slack"
)

func searchURL(query, utmSource string) string {
	if appURL == nil {
		// Determine the app URL.
		appURLStr, err := api.InternalClient.AppURL(context.Background())
		if err != nil {
			log15.Error("failed to get AppURL", err)
			return ""
		}
		appURL, err = url.Parse(appURLStr)
		if err != nil {
			log15.Error("failed to parse AppURL", err)
			return ""
		}
	}

	// Construct URL to the search query.
	u := appURL.ResolveReference(&url.URL{Path: "search"})
	q := u.Query()
	q.Set("q", query)
	q.Set("utm_source", utmSource)
	u.RawQuery = q.Encode()
	return u.String()
}

func logEvent(email, eventName, eventType string) {
	eventlogger.LogEvent(email, eventName, json.RawMessage(fmt.Sprintf(`{"saved_searches": {"event_type": "%s"}}`, eventType)))
}
