package updatecheck

import (
	"encoding/json"
	"fmt"
	"io"
	"io/ioutil"
	"net/http"
	"net/url"
	"strconv"
	"strings"
	"time"

	"github.com/coreos/go-semver/semver"
	"github.com/inconshreveable/log15"
	"github.com/pkg/errors"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/tracking"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/hubspot"
	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"
	"github.com/sourcegraph/sourcegraph/internal/pubsub/pubsubutil"
)

// pubSubPingsTopicID is the topic ID of the topic that forwards messages to Pings' pub/sub subscribers.
var pubSubPingsTopicID = env.Get("PUBSUB_TOPIC_ID", "", "Pub/sub pings topic ID is the pub/sub topic id where pings are published.")

var (
	// latestReleaseDockerServerImageBuild is only used by sourcegraph.com to tell existing
	// non-cluster, non-docker-compose, and non-pure-docker installations what the latest
	//version is. The version here _must_ be available at https://hub.docker.com/r/sourcegraph/server/tags/
	// before landing in master.
	latestReleaseDockerServerImageBuild = newBuild("3.14.1")

	// latestReleaseKubernetesBuild is only used by sourcegraph.com to tell existing Sourcegraph
	// cluster deployments what the latest version is. The version here _must_ be available in
	// a tag at https://github.com/sourcegraph/deploy-sourcegraph before landing in master.
	latestReleaseKubernetesBuild = newBuild("3.14.1")

	// latestReleaseDockerComposeOrPureDocker is only used by sourcegraph.com to tell existing Sourcegraph
	// Docker Compose or Pure Docker deployments what the latest version is. The version here _must_ be
	// available in a tag at https://github.com/sourcegraph/deploy-sourcegraph-docker before landing in master.
	latestReleaseDockerComposeOrPureDocker = newBuild("3.14.0-1")
)

func getLatestRelease(deployType string) build {
	switch {
	case conf.IsDeployTypeKubernetes(deployType):
		return latestReleaseKubernetesBuild
	case conf.IsDeployTypeDockerCompose(deployType), conf.IsDeployTypePureDocker(deployType):
		return latestReleaseDockerComposeOrPureDocker
	default:
		return latestReleaseDockerServerImageBuild
	}
}

// Handler is an HTTP handler that responds with information about software updates
// for Sourcegraph.
func Handler(w http.ResponseWriter, r *http.Request) {
	requestCounter.Inc()

	pr, err := readPingRequest(r)
	if err != nil {
		log15.Error("updatecheck: malformed request", "error", err)
		w.WriteHeader(http.StatusBadRequest)
		return
	}

	if pr.ClientSiteID == "" {
		log15.Error("updatecheck: no site ID specified")
		http.Error(w, "no site ID specified", http.StatusBadRequest)
		return
	}
	if pr.ClientVersionString == "" {
		log15.Error("updatecheck: no version specified")
		http.Error(w, "no version specified", http.StatusBadRequest)
		return
	}
	if pr.ClientVersionString == "dev" {
		// No updates for dev servers.
		w.WriteHeader(http.StatusNoContent)
		return
	}

	latestReleaseBuild := getLatestRelease(pr.DeployType)
	hasUpdate, err := canUpdate(pr.ClientVersionString, latestReleaseBuild)
	if err != nil {
		// Still log pings on malformed version strings.
		logPing(r, pr, false)

		http.Error(w, pr.ClientVersionString+" is a bad version string: "+err.Error(), http.StatusBadRequest)
		return
	}

	logPing(r, pr, hasUpdate)

	if !hasUpdate {
		// No newer version.
		w.WriteHeader(http.StatusNoContent)
		return
	}

	w.Header().Set("content-type", "application/json; charset=utf-8")
	body, err := json.Marshal(latestReleaseBuild)
	if err != nil {
		log15.Error("updatecheck: error preparing update check response", "error", err)
		http.Error(w, "", http.StatusInternalServerError)
		return
	}
	requestHasUpdateCounter.Inc()
	_, _ = w.Write(body)
}

// canUpdate returns true if the latestReleaseBuild is newer than the clientVersionString.
func canUpdate(clientVersionString string, latestReleaseBuild build) (bool, error) {
	// Check for a date in the version string to handle developer builds that don't have a semver.
	// If there is an error parsing a date out of the version string, then we ignore the error
	// and parse it as a semver.
	if hasDateUpdate, err := canUpdateDate(clientVersionString); err == nil {
		return hasDateUpdate, nil
	}

	// Released builds will have a semantic version that we can compare.
	return canUpdateVersion(clientVersionString, latestReleaseBuild)
}

// canUpdateVersion returns true if the latest released build is newer than
// the clientVersionString. It returns an error if clientVersionString is not a semver.
func canUpdateVersion(clientVersionString string, latestReleaseBuild build) (bool, error) {
	clientVersionString = strings.TrimPrefix(clientVersionString, "v")
	clientVersion, err := semver.NewVersion(clientVersionString)
	if err != nil {
		return false, err
	}
	return clientVersion.LessThan(latestReleaseBuild.Version), nil
}

var dateRegex = lazyregexp.New("_([0-9]{4}-[0-9]{2}-[0-9]{2})_")
var timeNow = time.Now

// canUpdateDate returns true if clientVersionString contains a date
// more than 40 days in the past. It returns an error if there is no
// parsable date in clientVersionString
func canUpdateDate(clientVersionString string) (bool, error) {
	match := dateRegex.FindStringSubmatch(clientVersionString)
	if len(match) != 2 {
		return false, fmt.Errorf("no date in version string %q", clientVersionString)
	}

	t, err := time.ParseInLocation("2006-01-02", match[1], time.UTC)
	if err != nil {
		// This shouldn't ever happen if the above code is correct.
		return false, err
	}

	// Assume that we release a new version at least every 40 days.
	return timeNow().After(t.Add(40 * 24 * time.Hour)), nil
}

// pingRequest is the payload of the update check request. These values either
// supplied via query string or by a JSON body (when the request method is POST).
// We need to maintain backwards compatibility with the GET-only update checks
// while expanding the payload size for newer instance versions (via HTTP body).
type pingRequest struct {
	ClientSiteID         string          `json:"site"`
	DeployType           string          `json:"deployType"`
	ClientVersionString  string          `json:"version"`
	AuthProviders        []string        `json:"auth"`
	ExternalServices     []string        `json:"extsvcs"`
	BuiltinSignupAllowed bool            `json:"signup"`
	HasExtURL            bool            `json:"hasExtURL"`
	UniqueUsers          int32           `json:"u"`
	Activity             json.RawMessage `json:"act"`
	CampaignsUsage       json.RawMessage `json:"automationUsage"`
	CodeIntelUsage       json.RawMessage `json:"codeIntelUsage"`
	SearchUsage          json.RawMessage `json:"searchUsage"`
	InitialAdminEmail    string          `json:"initAdmin"`
	TotalUsers           int32           `json:"totalUsers"`
	HasRepos             bool            `json:"repos"`
	EverSearched         bool            `json:"searched"`
	EverFindRefs         bool            `json:"refs"`
}

// readPingRequest reads the ping request payload from the request. If the
// request method is GET, it will read all parameters from the query string.
// If the request method is POST, it will read the parameters via a JSON
// encoded HTTP body.
func readPingRequest(r *http.Request) (*pingRequest, error) {
	if r.Method == "GET" {
		return readPingRequestFromQuery(r.URL.Query())
	}

	return readPingRequestFromBody(r.Body)
}

func readPingRequestFromQuery(q url.Values) (*pingRequest, error) {
	return &pingRequest{
		ClientSiteID:         q.Get("site"),
		DeployType:           q.Get("deployType"),
		ClientVersionString:  q.Get("version"),
		AuthProviders:        strings.Split(q.Get("auth"), ","),
		ExternalServices:     strings.Split(q.Get("extsvcs"), ","),
		BuiltinSignupAllowed: toBool(q.Get("signup")),
		HasExtURL:            toBool(q.Get("hasExtURL")),
		UniqueUsers:          toInt(q.Get("u")),
		Activity:             toRawMessage(q.Get("act")),
		InitialAdminEmail:    q.Get("initAdmin"),
		TotalUsers:           toInt(q.Get("totalUsers")),
		HasRepos:             toBool(q.Get("repos")),
		EverSearched:         toBool(q.Get("searched")),
		EverFindRefs:         toBool(q.Get("refs")),
	}, nil
}

func readPingRequestFromBody(body io.ReadCloser) (*pingRequest, error) {
	defer body.Close()
	contents, err := ioutil.ReadAll(body)
	if err != nil {
		return nil, err
	}

	var payload *pingRequest
	if err := json.Unmarshal(contents, &payload); err != nil {
		return nil, err
	}
	return payload, nil
}

func toInt(val string) int32 {
	value, err := strconv.ParseInt(val, 10, 32)
	if err != nil {
		return 0
	}
	return int32(value)
}

func toBool(val string) bool {
	value, err := strconv.ParseBool(val)
	return err == nil && value
}

func toRawMessage(val string) json.RawMessage {
	if val == "" {
		return nil
	}
	var payload json.RawMessage
	_ = json.Unmarshal([]byte(val), &payload)
	return payload
}

type pingPayload struct {
	RemoteIP             string          `json:"remote_ip"`
	RemoteSiteVersion    string          `json:"remote_site_version"`
	RemoteSiteID         string          `json:"remote_site_id"`
	HasUpdate            string          `json:"has_update"`
	UniqueUsersToday     string          `json:"unique_users_today"`
	SiteActivity         json.RawMessage `json:"site_activity"`
	CampaignsUsage       json.RawMessage `json:"automation_usage"`
	CodeIntelUsage       json.RawMessage `json:"code_intel_usage"`
	SearchUsage          json.RawMessage `json:"search_usage"`
	InstallerEmail       string          `json:"installer_email"`
	AuthProviders        string          `json:"auth_providers"`
	ExtServices          string          `json:"ext_services"`
	BuiltinSignupAllowed string          `json:"builtin_signup_allowed"`
	DeployType           string          `json:"deploy_type"`
	TotalUserAccounts    string          `json:"total_user_accounts"`
	HasExternalURL       string          `json:"has_external_url"`
	HasRepos             string          `json:"has_repos"`
	EverSearched         string          `json:"ever_searched"`
	EverFindRefs         string          `json:"ever_find_refs"`
	Timestamp            string          `json:"timestamp"`
}

func logPing(r *http.Request, pr *pingRequest, hasUpdate bool) {
	var clientAddr string
	if v := r.Header.Get("x-forwarded-for"); v != "" {
		clientAddr = v
	} else {
		clientAddr = r.RemoteAddr
	}

	message, err := marshalPing(pr, hasUpdate, clientAddr, time.Now())
	if err != nil {
		log15.Warn("logPing.Marshal: failed to Marshal payload", "error", err)
	} else {
		if pubsubutil.Enabled() {
			err := pubsubutil.Publish(pubSubPingsTopicID, string(message))
			if err != nil {
				log15.Warn("pubsubutil.Publish: failed to Publish", "message", message, "error", err)
			}
		}
	}

	// Sync the initial administrator email in HubSpot.
	if pr.InitialAdminEmail != "" && strings.Contains(pr.InitialAdminEmail, "@") {
		// Hubspot requires the timestamp to be rounded to the nearest day at midnight.
		now := time.Now().UTC()
		rounded := time.Date(now.Year(), now.Month(), now.Day(), 0, 0, 0, 0, now.Location())
		millis := rounded.UnixNano() / (int64(time.Millisecond) / int64(time.Nanosecond))
		go tracking.SyncUser(pr.InitialAdminEmail, "", &hubspot.ContactProperties{IsServerAdmin: true, LatestPing: millis})
	}
}

func marshalPing(pr *pingRequest, hasUpdate bool, clientAddr string, now time.Time) ([]byte, error) {
	codeIntelUsage, err := reserializeCodeIntelUsage(pr.CodeIntelUsage)
	if err != nil {
		return nil, errors.Wrap(err, "malformed code intel usage")
	}

	searchUsage, err := reserializeSearchUsage(pr.SearchUsage)
	if err != nil {
		return nil, errors.Wrap(err, "malformed search usage")
	}

	return json.Marshal(&pingPayload{
		RemoteIP:             clientAddr,
		RemoteSiteVersion:    pr.ClientVersionString,
		RemoteSiteID:         pr.ClientSiteID,
		HasUpdate:            strconv.FormatBool(hasUpdate),
		UniqueUsersToday:     strconv.FormatInt(int64(pr.UniqueUsers), 10),
		SiteActivity:         pr.Activity,       // no change in schema
		CampaignsUsage:       pr.CampaignsUsage, // no change in schema
		CodeIntelUsage:       codeIntelUsage,
		SearchUsage:          searchUsage,
		InstallerEmail:       pr.InitialAdminEmail,
		AuthProviders:        strings.Join(pr.AuthProviders, ","),
		ExtServices:          strings.Join(pr.ExternalServices, ","),
		BuiltinSignupAllowed: strconv.FormatBool(pr.BuiltinSignupAllowed),
		DeployType:           pr.DeployType,
		TotalUserAccounts:    strconv.FormatInt(int64(pr.TotalUsers), 10),
		HasExternalURL:       strconv.FormatBool(pr.HasExtURL),
		HasRepos:             strconv.FormatBool(pr.HasRepos),
		EverSearched:         strconv.FormatBool(pr.EverSearched),
		EverFindRefs:         strconv.FormatBool(pr.EverFindRefs),
		Timestamp:            now.UTC().Format(time.RFC3339),
	})
}

// reserializeCodeIntelUsage will reserialize a code intel usage statistics
// struct with only the first period in each period type. This reduces the
// complexity required in the BigQuery schema and downstream ETL transform
// logic.
func reserializeCodeIntelUsage(payload json.RawMessage) (json.RawMessage, error) {
	if len(payload) == 0 {
		return nil, nil
	}

	var codeIntelUsage *types.CodeIntelUsageStatistics
	if err := json.Unmarshal(payload, &codeIntelUsage); err != nil {
		return nil, err
	}

	singlePeriodUsage := struct {
		Daily   *types.CodeIntelUsagePeriod
		Weekly  *types.CodeIntelUsagePeriod
		Monthly *types.CodeIntelUsagePeriod
	}{}

	if len(codeIntelUsage.Daily) > 0 {
		singlePeriodUsage.Daily = codeIntelUsage.Daily[0]
	}
	if len(codeIntelUsage.Weekly) > 0 {
		singlePeriodUsage.Weekly = codeIntelUsage.Weekly[0]
	}
	if len(codeIntelUsage.Monthly) > 0 {
		singlePeriodUsage.Monthly = codeIntelUsage.Monthly[0]
	}

	return json.Marshal(singlePeriodUsage)
}

// reserializeSearchUsage will reserialize a code intel usage statistics
// struct with only the first period in each period type. This reduces the
// complexity required in the BigQuery schema and downstream ETL transform
// logic.
func reserializeSearchUsage(payload json.RawMessage) (json.RawMessage, error) {
	if len(payload) == 0 {
		return nil, nil
	}

	var searchUsage *types.SearchUsageStatistics
	if err := json.Unmarshal(payload, &searchUsage); err != nil {
		return nil, err
	}

	singlePeriodUsage := struct {
		Daily   *types.SearchUsagePeriod
		Weekly  *types.SearchUsagePeriod
		Monthly *types.SearchUsagePeriod
	}{}

	if len(searchUsage.Daily) > 0 {
		singlePeriodUsage.Daily = searchUsage.Daily[0]
	}
	if len(searchUsage.Weekly) > 0 {
		singlePeriodUsage.Weekly = searchUsage.Weekly[0]
	}
	if len(searchUsage.Monthly) > 0 {
		singlePeriodUsage.Monthly = searchUsage.Monthly[0]
	}

	return json.Marshal(singlePeriodUsage)
}

var (
	requestCounter = prometheus.NewCounter(prometheus.CounterOpts{
		Namespace: "src",
		Subsystem: "updatecheck",
		Name:      "requests",
		Help:      "Number of requests to the update check handler.",
	})
	requestHasUpdateCounter = prometheus.NewCounter(prometheus.CounterOpts{
		Namespace: "src",
		Subsystem: "updatecheck",
		Name:      "requests_has_update",
		Help:      "Number of requests to the update check handler where an update is available.",
	})
)

func init() {
	prometheus.MustRegister(requestCounter)
	prometheus.MustRegister(requestHasUpdateCounter)
}
