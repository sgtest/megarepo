package updatecheck

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"io/ioutil"
	"math/rand"
	"net/http"
	"net/url"
	"strconv"
	"sync"
	"time"

	"github.com/inconshreveable/log15"
	"github.com/prometheus/client_golang/prometheus"
	"golang.org/x/net/context/ctxhttp"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/siteid"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/usagestatsdeprecated"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/usagestats"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/version"
)

// recorder records operational metrics for methods.
var recorder = metrics.NewOperationMetrics(prometheus.DefaultRegisterer, "updatecheck", metrics.WithLabels("method"))

//
var updateCheckHistogram = prometheus.NewHistogram(prometheus.HistogramOpts{
	Name: "update_check_req",
	Help: "metrics for update_check",
})

func init() {
	prometheus.MustRegister(updateCheckHistogram)
}

// Status of the check for software updates for Sourcegraph.
type Status struct {
	Date          time.Time // the time that the last check completed
	Err           error     // the error that occurred, if any. When present, indicates the instance is offline / unable to contact Sourcegraph.com
	UpdateVersion string    // the version string of the updated version, if any
}

// HasUpdate reports whether the status indicates an update is available.
func (s Status) HasUpdate() bool { return s.UpdateVersion != "" }

var (
	mu         sync.Mutex
	startedAt  *time.Time
	lastStatus *Status
)

// Last returns the status of the last-completed software update check.
func Last() *Status {
	mu.Lock()
	defer mu.Unlock()
	if lastStatus == nil {
		return nil
	}
	tmp := *lastStatus
	return &tmp
}

// IsPending returns whether an update check is in progress.
func IsPending() bool {
	mu.Lock()
	defer mu.Unlock()
	return startedAt != nil
}

var baseURL = &url.URL{
	Scheme: "https",
	Host:   "sourcegraph.com",
	Path:   "/.api/updates",
}

// recordOperation returns a record fn that is called on any given return err. If an error is encountered
// it will register the err metric. The err is never altered.
func recordOperation(method string) func(*error) {
	start := time.Now()
	return func(err *error) {
		recorder.Observe(time.Since(start).Seconds(), 1, err, method)
	}
}

func getAndMarshalSiteActivityJSON(ctx context.Context, criticalOnly bool) (_ json.RawMessage, err error) {
	defer recordOperation("getAndMarshalSiteActivityJSON")(&err)
	siteActivity, err := usagestats.GetSiteUsageStats(ctx, criticalOnly)

	if err != nil {
		return nil, err
	}

	return json.Marshal(siteActivity)
}

func hasSearchOccurred(ctx context.Context) (_ bool, err error) {
	defer recordOperation("hasSearchOccurred")(&err)
	return usagestats.HasSearchOccurred()
}

func hasFindRefsOccurred(ctx context.Context) (_ bool, err error) {
	defer recordOperation("hasSearchOccured")(&err)
	return usagestats.HasFindRefsOccurred()
}

func getTotalUsersCount(ctx context.Context) (_ int, err error) {
	defer recordOperation("getTotalUsersCount")(&err)
	return db.Users.Count(ctx, &db.UsersListOptions{})
}

func getTotalReposCount(ctx context.Context) (_ int, err error) {
	defer recordOperation("getTotalReposCount")(&err)
	return db.Repos.Count(ctx, db.ReposListOptions{})
}

func getUsersActiveTodayCount(ctx context.Context) (_ int, err error) {
	defer recordOperation("getUsersActiveTodayCount")(&err)
	return usagestatsdeprecated.GetUsersActiveTodayCount()
}

func getInitialSiteAdminEmail(ctx context.Context) (_ string, err error) {
	defer recordOperation("getInitialSiteAdminEmail")(&err)
	return db.UserEmails.GetInitialSiteAdminEmail(ctx)
}

func getAndMarshalCampaignsUsageJSON(ctx context.Context) (_ json.RawMessage, err error) {
	defer recordOperation("getAndMarshalCampaignsUsageJSON")(&err)

	campaignsUsage, err := usagestats.GetCampaignsUsageStatistics(ctx)
	if err != nil {
		return nil, err
	}
	return json.Marshal(campaignsUsage)
}

func getAndMarshalAggregatedUsageJSON(ctx context.Context) (_ json.RawMessage, _ json.RawMessage, err error) {
	defer recordOperation("getAndMarshalAggregatedUsageJSON")(&err)

	codeIntelUsage, searchUsage, err := usagestats.GetAggregatedStats(ctx)
	if err != nil {
		return nil, nil, err
	}

	serializedCodeIntelUsage, err := json.Marshal(codeIntelUsage)
	if err != nil {
		return nil, nil, err
	}

	serializedSearchUsage, err := json.Marshal(searchUsage)
	if err != nil {
		return nil, nil, err
	}

	return serializedCodeIntelUsage, serializedSearchUsage, nil
}

func updateURL(ctx context.Context) string {
	return baseURL.String()
}

func updateBody(ctx context.Context) (io.Reader, error) {
	logFunc := log15.Debug
	if envvar.SourcegraphDotComMode() {
		logFunc = log15.Warn
	}

	r := &pingRequest{
		ClientSiteID:        siteid.Get(),
		DeployType:          conf.DeployType(),
		ClientVersionString: version.Version(),
		LicenseKey:          conf.Get().LicenseKey,
		CodeIntelUsage:      []byte("{}"),
		SearchUsage:         []byte("{}"),
		CampaignsUsage:      []byte("{}"),
	}

	totalUsers, err := getTotalUsersCount(ctx)
	if err != nil {
		logFunc("telemetry: db.Users.Count failed", "error", err)
	}
	r.TotalUsers = int32(totalUsers)
	r.InitialAdminEmail, err = getInitialSiteAdminEmail(ctx)
	if err != nil {
		logFunc("telemetry: db.UserEmails.GetInitialSiteAdminEmail failed", "error", err)
	}

	if !conf.Get().DisableNonCriticalTelemetry {
		// TODO(Dan): migrate this to the new usagestats package.
		//
		// For the time being, instances will report daily active users through the legacy package via this argument,
		// as well as using the new package through the `act` argument below. This will allow comparison during the
		// transition.
		count, err := getUsersActiveTodayCount(ctx)
		if err != nil {
			logFunc("telemetry: updatecheck.getUsersActiveToday failed", "error", err)
		}
		r.UniqueUsers = int32(count)
		totalRepos, err := getTotalReposCount(ctx)
		if err != nil {
			logFunc("telemetry: updatecheck.getTotalReposCount failed", "error", err)
		}
		r.HasRepos = totalRepos > 0

		r.EverSearched, err = hasSearchOccurred(ctx)
		if err != nil {
			logFunc("telemetry: updatecheck.hasSearchOccurred failed", "error", err)
		}
		r.EverFindRefs, err = hasFindRefsOccurred(ctx)
		if err != nil {
			logFunc("telemetry: updatecheck.hasFindRefsOccurred failed", "error", err)
		}
		r.CampaignsUsage, err = getAndMarshalCampaignsUsageJSON(ctx)
		if err != nil {
			logFunc("telemetry: updatecheck.getAndMarshalCampaignsUsageJSON failed", "error", err)
		}
		r.ExternalServices, err = externalServiceKinds(ctx)
		if err != nil {
			logFunc("telemetry: externalServicesKinds failed", "error", err)
		}

		r.HasExtURL = conf.UsingExternalURL()
		r.BuiltinSignupAllowed = conf.IsBuiltinSignupAllowed()
		r.AuthProviders = authProviderTypes()

		// The following methods are the most expensive to calculate, so we do them in
		// parallel.

		var wg sync.WaitGroup

		wg.Add(1)
		go func() {
			defer wg.Done()
			r.Activity, err = getAndMarshalSiteActivityJSON(ctx, false)
			if err != nil {
				logFunc("telemetry: updatecheck.getAndMarshalSiteActivityJSON failed", "error", err)
			}
		}()

		wg.Add(1)
		go func() {
			defer wg.Done()
			r.CodeIntelUsage, r.SearchUsage, err = getAndMarshalAggregatedUsageJSON(ctx)
			if err != nil {
				logFunc("telemetry: updatecheck.getAndMarshalAggregatedUsageJSON failed", "error", err)
			}
		}()
		wg.Wait()
	} else {
		r.Activity, err = getAndMarshalSiteActivityJSON(ctx, true)
		if err != nil {
			logFunc("telemetry: updatecheck.getAndMarshalSiteActivityJSON failed", "error", err)
		}
	}

	contents, err := json.Marshal(r)
	if err != nil {
		return nil, err
	}

	return bytes.NewReader(contents), nil
}

func authProviderTypes() []string {
	ps := conf.Get().AuthProviders
	types := make([]string, len(ps))
	for i, p := range ps {
		types[i] = conf.AuthProviderType(p)
	}
	return types
}

func externalServiceKinds(ctx context.Context) (_ []string, err error) {
	defer recordOperation("externalServiceKinds")(&err)

	services, err := db.ExternalServices.List(ctx, db.ExternalServicesListOptions{})
	if err != nil {
		return nil, err
	}
	kinds := make([]string, len(services))
	for i, s := range services {
		kinds[i] = s.Kind
	}
	return kinds, nil
}

// check performs an update check. It returns the result and updates the global state
// (returned by Last and IsPending).
func check(ctx context.Context) (*Status, error) {
	doCheck := func() (updateVersion string, err error) {
		body, err := updateBody(ctx)
		if err != nil {
			return "", err
		}
		resp, err := ctxhttp.Post(ctx, nil, updateURL(ctx), "application/json", body)
		if err != nil {
			return "", err
		}
		defer resp.Body.Close()
		if resp.StatusCode != http.StatusOK && resp.StatusCode != http.StatusNoContent {
			var description string
			if body, err := ioutil.ReadAll(io.LimitReader(resp.Body, 30)); err != nil {
				description = err.Error()
			} else if len(body) == 0 {
				description = "(no response body)"
			} else {
				description = strconv.Quote(string(bytes.TrimSpace(body)))
			}
			return "", fmt.Errorf("update endpoint returned HTTP error %d: %s", resp.StatusCode, description)
		}

		if resp.StatusCode == http.StatusNoContent {
			return "", nil // no update available
		}

		var latestBuild build
		if err := json.NewDecoder(resp.Body).Decode(&latestBuild); err != nil {
			return "", err
		}
		return latestBuild.Version.String(), nil
	}

	mu.Lock()
	thisCheckStartedAt := time.Now()
	startedAt = &thisCheckStartedAt
	mu.Unlock()

	updateVersion, err := doCheck()

	mu.Lock()
	if startedAt != nil && !startedAt.After(thisCheckStartedAt) {
		startedAt = nil
	}
	lastStatus = &Status{
		Date:          time.Now(),
		Err:           err,
		UpdateVersion: updateVersion,
	}
	mu.Unlock()

	return lastStatus, err
}

var started bool

// Start starts checking for software updates periodically.
func Start() {
	if started {
		panic("already started")
	}
	started = true

	if channel := conf.UpdateChannel(); channel != "release" {
		return // no update check
	}

	ctx := context.Background()
	const delay = 30 * time.Minute
	for {
		ctx, cancel := context.WithTimeout(ctx, 300*time.Second)
		_, _ = check(ctx) // updates global state on its own, can safely ignore return value
		cancel()

		// Randomize sleep to prevent thundering herds.
		randomDelay := time.Duration(rand.Intn(600)) * time.Second
		time.Sleep(delay + randomDelay)
	}
}
