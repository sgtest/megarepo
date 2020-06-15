// Command grafana-wrapper provides a wrapper command for Grafana that
// also handles Sourcegraph configuration changes and making changes to Grafana.
package main

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httputil"
	"os"
	"os/exec"
	"time"

	"github.com/gorilla/mux"
	"github.com/grafana-tools/sdk"
	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/internal/env"
)

var noConfig = os.Getenv("DISABLE_SOURCEGRAPH_CONFIG")
var exportPort = env.Get("EXPORT_PORT", "3370", "port that should be used to access grafana and custom endpoints externally")
var grafanaPort = env.Get("GRAFANA_INTERNAL_PORT", "3371", "internal grafana port")
var grafanaCredentials = env.Get("GRAFANA_INTERNAL_CREDENTIALS", "admin:admin", "credentials for accessing the grafana server")

func main() {
	log := log15.New("cmd", "grafana-wrapper")
	ctx := context.Background()

	// spin up grafana
	grafanaErrs := make(chan error)
	go func() {
		grafanaErrs <- newGrafanaRunCmd().Run()
	}()

	// router serves endpoints accessible from outside the container (defined by `exportPort`)
	// this includes any endpoints from `siteConfigSubscriber`, reverse-proxying Grafana, etc.
	router := mux.NewRouter()

	// subscribe to configuration
	if noConfig == "true" {
		log.Info("DISABLE_SOURCEGRAPH_CONFIG=true; configuration syncing is disabled")
	} else {
		log.Info("initializing configuration")
		grafanaClient := sdk.NewClient(fmt.Sprintf("http://127.0.0.1:%s", grafanaPort), grafanaCredentials, http.DefaultClient)

		// limit the amount of time we spend spinning up the subscriber before erroring
		newSubscriberCtx, cancel := context.WithTimeout(ctx, 30*time.Second)
		config, err := newSiteConfigSubscriber(newSubscriberCtx, log, grafanaClient)
		if err != nil {
			log.Crit("failed to initialize configuration", "error", err)
			os.Exit(1)
		}
		cancel()

		// watch for configuration updates in the background
		config.Subscribe(ctx)

		// serve subscriber status
		router.PathPrefix("/grafana-wrapper/config-subscriber").Handler(config.Handler())
	}

	// serve grafana via reverse proxy - place last so other prefixes get served first
	router.PathPrefix("/").Handler(&httputil.ReverseProxy{
		Director: func(req *http.Request) {
			req.URL.Scheme = "http"
			req.URL.Host = fmt.Sprintf(":%s", grafanaPort)
		},
	})
	go func() {
		log.Debug("serving endpoints and reverse proxy")
		if err := http.ListenAndServe(fmt.Sprintf(":%s", exportPort), router); err != nil && !errors.Is(err, http.ErrServerClosed) {
			log.Crit("error serving reverse proxy", "error", err)
			os.Exit(1)
		}
		os.Exit(0)
	}()

	// wait for grafana to exit
	err := <-grafanaErrs
	if err != nil {
		log.Crit("grafana exited", "error", err)
		var exitErr *exec.ExitError
		if errors.As(err, &exitErr) {
			os.Exit(exitErr.ProcessState.ExitCode())
		}
		os.Exit(1)
	} else {
		log.Info("grafana exited")
		os.Exit(0)
	}
}
