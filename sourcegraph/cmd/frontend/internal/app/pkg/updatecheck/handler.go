package updatecheck

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"strings"

	"github.com/sourcegraph/sourcegraph/pkg/hubspot"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/tracking"

	"github.com/sourcegraph/sourcegraph/pkg/conf"
	"github.com/sourcegraph/sourcegraph/pkg/eventlogger"

	"github.com/coreos/go-semver/semver"
	"github.com/prometheus/client_golang/prometheus"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

var (
	// latestReleaseServerBuild is only used by sourcegraph.com to tell existing
	// Server installations what the latest version is. The version here _must_ be
	// available at https://hub.docker.com/r/sourcegraph/server/tags/ before
	// landing in master.
	latestReleaseServerBuild = newBuild("2.11.0")

	// latestReleaseDataCenterBuild is only used by sourcegraph.com to tell existing
	// Data Center installations what the latest version is. The version here _must_ be
	// available at https://storage.googleapis.com/sourcegraph-assets/sourcegraph-server-gen/
	// before landing in master.
	latestReleaseDataCenterBuild = newBuild("2.10.0")
)

func getLatestRelease(deployType string) build {
	if conf.IsDataCenter(deployType) {
		return latestReleaseDataCenterBuild
	}
	return latestReleaseServerBuild
}

// Handler is an HTTP handler that responds with information about software updates
// for Sourcegraph.
func Handler(w http.ResponseWriter, r *http.Request) {
	requestCounter.Inc()

	q := r.URL.Query()
	clientSiteID := q.Get("site")
	deployType := q.Get("deployType")
	clientVersionString := q.Get("version")
	if clientSiteID == "" {
		log15.Error("updatecheck: no site ID specified")
		http.Error(w, "no site ID specified", http.StatusBadRequest)
		return
	}
	if clientVersionString == "" {
		log15.Error("updatecheck: no version specified")
		http.Error(w, "no version specified", http.StatusBadRequest)
		return
	}
	if clientVersionString == "dev" {
		// No updates for dev servers.
		w.WriteHeader(http.StatusNoContent)
		return
	}
	clientVersionString = strings.TrimPrefix(clientVersionString, "v")
	clientVersion, err := semver.NewVersion(clientVersionString)
	if err != nil {
		log15.Error("updatecheck: encountered bad version string", "version", clientVersionString, "error", err)

		// Still log telemetry on malformed version strings.
		logPing(r, clientVersionString, false)

		http.Error(w, clientVersionString+" is a bad version string: "+err.Error(), http.StatusBadRequest)
		return
	}

	latestReleaseBuild := getLatestRelease(deployType)
	hasUpdate := clientVersion.LessThan(latestReleaseBuild.Version)
	logPing(r, clientVersionString, hasUpdate)

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

func logPing(r *http.Request, clientVersionString string, hasUpdate bool) {
	q := r.URL.Query()
	clientSiteID := q.Get("site")
	authProviders := q.Get("auth")
	uniqueUsers := q.Get("u")
	activity := q.Get("act")
	initialAdminEmail := q.Get("initAdmin")
	hasCodeIntelligence := q.Get("codeintel")
	deployType := q.Get("deployType")

	// Log update check.
	var clientAddr string
	if v := r.Header.Get("x-forwarded-for"); v != "" {
		clientAddr = v
	} else {
		clientAddr = r.RemoteAddr
	}

	// Prevent nil activity data (i.e., "") from breaking json marshaling.
	// This is an issue with all Server and Data Center instances at versions < 2.7.0.
	if activity == "" {
		activity = `{}`
	}

	eventlogger.LogEvent("", "ServerUpdateCheck", json.RawMessage(fmt.Sprintf(`{
		"remote_ip": "%s",
		"remote_site_version": "%s",
		"remote_site_id": "%s",
		"has_update": "%s",
		"unique_users_today": "%s",
		"has_code_intelligence": "%s",
		"site_activity": %s,
		"installer_email": "%s",
		"auth_providers": "%s",
		"deploy_type": "%s"
	}`,
		clientAddr,
		clientVersionString,
		clientSiteID,
		strconv.FormatBool(hasUpdate),
		uniqueUsers,
		hasCodeIntelligence,
		activity,
		initialAdminEmail,
		authProviders,
		deployType,
	)))

	// Sync the initial administrator email in HubSpot.
	if initialAdminEmail != "" && strings.Contains(initialAdminEmail, "@") {
		go tracking.SyncUser(initialAdminEmail, "", &hubspot.ContactProperties{IsServerAdmin: true})
	}
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
