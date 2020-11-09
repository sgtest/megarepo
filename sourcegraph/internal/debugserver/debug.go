package debugserver

import (
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"net/http/pprof"
	"os"
	"path/filepath"
	"strings"

	"github.com/felixge/fgprof"
	"github.com/gorilla/mux"
	"github.com/prometheus/client_golang/prometheus/promhttp"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/httpserver"
	"golang.org/x/net/trace"
)

var addr = env.Get("SRC_PROF_HTTP", ":6060", "net/http/pprof http bind address.")

func init() {
	err := json.Unmarshal([]byte(env.Get("SRC_PROF_SERVICES", "[]", "list of net/http/pprof http bind address.")), &Services)
	if err != nil {
		panic("failed to JSON unmarshal SRC_PROF_SERVICES: " + err.Error())
	}

	if addr == "" {
		// Look for our binname in the services list
		name := filepath.Base(os.Args[0])
		for _, svc := range Services {
			if svc.Name == name {
				addr = svc.Host
				break
			}
		}
	}
}

// Endpoint is a handler for the debug server. It will be displayed on the
// debug index page.
type Endpoint struct {
	// Name is the name shown on the index page for the endpoint
	Name string
	// Path is passed to http.Mux.Handle as the pattern.
	Path string
	// Handler is the debug handler
	Handler http.Handler
}

// Services is the list of registered services' debug addresses. Populated
// from SRC_PROF_MAP.
var Services []Service

// Service is a service's debug addr (host:port).
type Service struct {
	// Name of the service. Always the binary name. example: "gitserver"
	Name string

	// Host is the host:port for the services SRC_PROF_HTTP. example:
	// "127.0.0.1:6060"
	Host string

	// DefaultPath is the path to the service we should link to.
	DefaultPath string
}

// Dumper is a service which can dump its state for debugging.
type Dumper interface {
	// DebugDump returns a snapshot of the current state.
	DebugDump() interface{}
}

// NewServerRoutine returns a background routine that exposes pprof and metrics endpoints.
func NewServerRoutine(extra ...Endpoint) (goroutine.BackgroundRoutine, error) {
	if addr == "" {
		return goroutine.NoopRoutine(), nil
	}

	// we're protected by adminOnly on the front of this
	trace.AuthRequest = func(req *http.Request) (any, sensitive bool) {
		return true, true
	}

	handler := httpserver.NewHandler(func(router *mux.Router) {
		index := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			_, _ = w.Write([]byte(`
				<a href="vars">Vars</a><br>
				<a href="debug/pprof/">PProf</a><br>
				<a href="metrics">Metrics</a><br>
				<a href="debug/requests">Requests</a><br>
				<a href="debug/events">Events</a><br>
			`))

			for _, e := range extra {
				fmt.Fprintf(w, `<a href="%s">%s</a><br>`, strings.TrimPrefix(e.Path, "/"), e.Name)
			}

			_, _ = w.Write([]byte(`
				<br>
				<form method="post" action="gc" style="display: inline;"><input type="submit" value="GC"></form>
				<form method="post" action="freeosmemory" style="display: inline;"><input type="submit" value="Free OS Memory"></form>
			`))
		})

		router.Handle("/", index)
		router.Handle("/debug", index)
		router.Handle("/vars", http.HandlerFunc(expvarHandler))
		router.Handle("/gc", http.HandlerFunc(gcHandler))
		router.Handle("/freeosmemory", http.HandlerFunc(freeOSMemoryHandler))
		router.Handle("/debug/fgprof", fgprof.Handler())
		router.Handle("/debug/pprof/cmdline", http.HandlerFunc(pprof.Cmdline))
		router.Handle("/debug/pprof/profile", http.HandlerFunc(pprof.Profile))
		router.Handle("/debug/pprof/symbol", http.HandlerFunc(pprof.Symbol))
		router.Handle("/debug/pprof/trace", http.HandlerFunc(pprof.Trace))
		router.Handle("/debug/requests", http.HandlerFunc(trace.Traces))
		router.Handle("/debug/events", http.HandlerFunc(trace.Events))
		router.Handle("/metrics", promhttp.Handler())

		// This path acts as a wildcard and should appear after more specific entries.
		router.PathPrefix("/debug/pprof").HandlerFunc(pprof.Index)

		for _, e := range extra {
			router.Handle(e.Path, e.Handler)
		}
	})

	return httpserver.NewFromAddr(addr, handler, httpserver.Options{})
}

// Start runs a debug server (pprof, prometheus, etc) if it is configured (via
// SRC_PROF_HTTP environment variable). It is blocking.
func Start(extra ...Endpoint) {
	debugServer, err := NewServerRoutine(extra...)
	if err != nil {
		log.Fatalf("Failed to create listener: %s", err)
	}

	debugServer.Start()
}
