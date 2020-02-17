package cli

import (
	"context"
	"errors"
	"fmt"
	"log"
	"net"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/keegancsmith/tmpfriend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/globals"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/httpapi"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/pkg/updatecheck"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/bg"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/cli/loghandlers"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/discussions/mailreply"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/siteid"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/db/confdb"
	"github.com/sourcegraph/sourcegraph/internal/db/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/db/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/debugserver"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/processrestart"
	"github.com/sourcegraph/sourcegraph/internal/sysreq"
	"github.com/sourcegraph/sourcegraph/internal/tracer"
	"github.com/sourcegraph/sourcegraph/internal/version"
	"github.com/sourcegraph/sourcegraph/internal/vfsutil"
	"gopkg.in/inconshreveable/log15.v2"
)

var (
	trace          = env.Get("SRC_LOG_TRACE", "HTTP", "space separated list of trace logs to show. Options: all, HTTP, build, github")
	traceThreshold = env.Get("SRC_LOG_TRACE_THRESHOLD", "", "show traces that take longer than this")

	printLogo, _ = strconv.ParseBool(env.Get("LOGO", "false", "print Sourcegraph logo upon startup"))

	httpAddr         = env.Get("SRC_HTTP_ADDR", ":3080", "HTTP listen address for app and HTTP API")
	httpAddrInternal = env.Get("SRC_HTTP_ADDR_INTERNAL", ":3090", "HTTP listen address for internal HTTP API. This should never be exposed externally, as it lacks certain authz checks.")

	nginxAddr = env.Get("SRC_NGINX_HTTP_ADDR", "", "HTTP listen address for nginx reverse proxy to SRC_HTTP_ADDR. Has preference over SRC_HTTP_ADDR for ExternalURL.")

	// dev browser browser extension ID. You can find this by going to chrome://extensions
	devExtension = "chrome-extension://bmfbcejdknlknpncfpeloejonjoledha"
	// production browser extension ID. This is found by viewing our extension in the chrome store.
	prodExtension = "chrome-extension://dgjhfomjieaadpoljlnidmbgkdffpack"
)

func init() {
	// If CACHE_DIR is specified, use that
	cacheDir := env.Get("CACHE_DIR", "/tmp", "directory to store cached archives.")
	vfsutil.ArchiveCacheDir = filepath.Join(cacheDir, "frontend-archive-cache")
}

// defaultExternalURL returns the default external URL of the application.
func defaultExternalURL(nginxAddr, httpAddr string) *url.URL {
	addr := nginxAddr
	if addr == "" {
		addr = httpAddr
	}

	var hostPort string
	if strings.HasPrefix(addr, ":") {
		// Prepend localhost if HTTP listen addr is just a port.
		hostPort = "127.0.0.1" + addr
	} else {
		hostPort = addr
	}

	return &url.URL{Scheme: "http", Host: hostPort}
}

// InitDB initializes the global database connection and sets the
// version of the frontend in our versions table.
func InitDB() error {
	if err := dbconn.ConnectToDB(""); err != nil {
		return err
	}

	ctx := context.Background()
	migrate := true

	for {
		// We need this loop so that we handle the missing versions table,
		// which would be added by running the migrations. Once we detect that
		// it's missing, we run the migrations and try to update the version again.

		err := backend.UpdateServiceVersion(ctx, "frontend", version.Version())
		if err != nil && !dbutil.IsPostgresError(err, "undefined_table") {
			return err
		}

		if !migrate {
			return nil
		}

		if err := dbconn.MigrateDB(dbconn.Global, ""); err != nil {
			return err
		}

		migrate = false
	}
}

// Main is the main entrypoint for the frontend server program.
func Main(githubWebhook, bitbucketServerWebhook http.Handler) error {
	log.SetFlags(0)
	log.SetPrefix("")

	if dbconn.Global == nil {
		if err := InitDB(); err != nil {
			log.Fatalf("ERROR: %v", err)
		}
	}

	if err := handleConfigOverrides(); err != nil {
		log.Fatal("applying config overrides:", err)
	}

	globals.ConfigurationServerFrontendOnly = conf.InitConfigurationServerFrontendOnly(&configurationSource{})
	conf.MustValidateDefaults()
	if err := confdb.RunMigrations(context.Background()); err != nil {
		log.Fatal(err)
	}

	// Filter trace logs
	d, _ := time.ParseDuration(traceThreshold)
	tracer.Init(tracer.Filter(loghandlers.Trace(strings.Fields(trace), d)))

	if len(os.Args) >= 2 {
		switch os.Args[1] {
		case "help", "-h", "--help":
			log.Printf("Version: %s", version.Version())
			log.Print()

			env.PrintHelp()

			log.Print()
			ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancel()
			for _, st := range sysreq.Check(ctx, skippedSysReqs()) {
				log.Printf("%s:", st.Name)
				if st.OK() {
					log.Print("\tOK")
					continue
				}
				if st.Skipped {
					log.Print("\tSkipped")
					continue
				}
				if st.Problem != "" {
					log.Print("\t" + st.Problem)
				}
				if st.Err != nil {
					log.Printf("\tError: %s", st.Err)
				}
				if st.Fix != "" {
					log.Printf("\tPossible fix: %s", st.Fix)
				}
			}

			return nil
		}
	}

	printConfigValidation()

	cleanup := tmpfriend.SetupOrNOOP()
	defer cleanup()

	// Don't proceed if system requirements are missing, to avoid
	// presenting users with a half-working experience.
	if err := checkSysReqs(context.Background(), os.Stderr); err != nil {
		return err
	}

	go debugserver.Start()

	siteid.Init()

	globals.WatchExternalURL(defaultExternalURL(nginxAddr, httpAddr))
	globals.WatchPermissionsUserMapping()

	goroutine.Go(func() { bg.MigrateAllSettingsMOTDToNotices(context.Background()) })
	goroutine.Go(func() { bg.MigrateSavedQueriesAndSlackWebhookURLsFromSettingsToDatabase(context.Background()) })
	goroutine.Go(func() { bg.CheckRedisCacheEvictionPolicy() })
	goroutine.Go(func() { bg.DeleteOldCacheDataInRedis() })
	goroutine.Go(func() { bg.DeleteOldEventLogsInPostgres(context.Background()) })
	goroutine.Go(mailreply.StartWorker)
	go updatecheck.Start()

	// Parse GraphQL schema and set up resolvers that depend on dbconn.Global
	// being initialized
	if dbconn.Global == nil {
		return errors.New("dbconn.Global is nil when trying to parse GraphQL schema")
	}

	// graphqlbackend.A8NResolver is set by enterprise frontend
	var a8nResolver graphqlbackend.A8NResolver
	if graphqlbackend.NewA8NResolver != nil {
		a8nResolver = graphqlbackend.NewA8NResolver(dbconn.Global)
	}

	// graphqlbackend.CodeIntelResolver is set by enterprise frontend
	var codeIntelResolver graphqlbackend.CodeIntelResolver
	if graphqlbackend.NewCodeIntelResolver != nil {
		codeIntelResolver = graphqlbackend.NewCodeIntelResolver()
	}

	// graphqlbackend.AuthzResolver is set by enterprise frontend
	var authzResolver graphqlbackend.AuthzResolver
	if graphqlbackend.NewAuthzResolver != nil {
		authzResolver = graphqlbackend.NewAuthzResolver()
	}

	schema, err := graphqlbackend.NewSchema(a8nResolver, codeIntelResolver, authzResolver)
	if err != nil {
		return err
	}

	// httpapi.NewLSIFServerProxy is set by enterprise frontend
	var lsifServerProxy *httpapi.LSIFServerProxy
	if httpapi.NewLSIFServerProxy != nil {
		var err error
		if lsifServerProxy, err = httpapi.NewLSIFServerProxy(); err != nil {
			return err
		}
	}

	// Create the external HTTP handler.
	externalHandler, err := newExternalHTTPHandler(schema, githubWebhook, bitbucketServerWebhook, lsifServerProxy)
	if err != nil {
		return err
	}

	// The internal HTTP handler does not include the auth handlers.
	internalHandler := newInternalHTTPHandler(schema)

	// serve will serve externalHandler on l. It additionally handles graceful restarts.
	srv := &httpServers{}

	// Start HTTP server.
	l, err := net.Listen("tcp", httpAddr)
	if err != nil {
		return err
	}
	log15.Debug("HTTP running", "on", httpAddr)
	srv.GoServe(l, &http.Server{
		Handler:      externalHandler,
		ReadTimeout:  75 * time.Second,
		WriteTimeout: 60 * time.Second,
	})

	if httpAddrInternal != "" {
		l, err := net.Listen("tcp", httpAddrInternal)
		if err != nil {
			return err
		}

		log15.Debug("HTTP (internal) running", "on", httpAddrInternal)
		srv.GoServe(l, &http.Server{
			Handler:     internalHandler,
			ReadTimeout: 75 * time.Second,
			// Higher since for internal RPCs which can have large responses
			// (eg git archive). Should match the timeout used for git archive
			// in gitserver.
			WriteTimeout: time.Hour,
		})
	}

	go func() {
		<-processrestart.WillRestart
		// Block forever so we don't return from main func and exit this process. Package processrestart takes care
		// of killing and restarting this process externally.
		srv.wg.Add(1)

		log15.Debug("Stopping HTTP server due to imminent restart")
		srv.Close()
	}()

	if printLogo {
		fmt.Println(" ")
		fmt.Println(logoColor)
		fmt.Println(" ")
	}
	fmt.Printf("✱ Sourcegraph is ready at: %s\n", globals.ExternalURL())

	srv.Wait()
	return nil
}

type httpServers struct {
	mu      sync.Mutex
	wg      sync.WaitGroup
	servers []*http.Server
	wrapper func(http.Handler) http.Handler
}

// SetWrapper will set the wrapper for serve. All handlers served by are
// passed through w.
func (s *httpServers) SetWrapper(w func(http.Handler) http.Handler) {
	s.mu.Lock()
	s.wrapper = w
	s.mu.Unlock()
}

// GoServe serves srv in a new goroutine. If serve returns an error other than
// http.ErrServerClosed it will fatal.
func (s *httpServers) GoServe(l net.Listener, srv *http.Server) {
	s.addServer(srv)
	s.wg.Add(1)
	go func() {
		defer s.wg.Done()
		if err := srv.Serve(l); err != http.ErrServerClosed {
			log.Fatal(err)
		}
	}()
}

func (s *httpServers) addServer(srv *http.Server) *http.Server {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.wrapper != nil {
		srv.Handler = s.wrapper(srv.Handler)
	}
	s.servers = append(s.servers, srv)
	return srv
}

// Close closes all servers added
func (s *httpServers) Close() {
	s.mu.Lock()
	defer s.mu.Unlock()
	for _, srv := range s.servers {
		srv.Close()
	}
	s.servers = nil
}

// Wait waits until all servers are closed.
func (s *httpServers) Wait() {
	s.wg.Wait()
}

func isAllowedOrigin(origin string, allowedOrigins []string) bool {
	for _, o := range allowedOrigins {
		if o == "*" || o == origin {
			return true
		}
	}
	return false
}
