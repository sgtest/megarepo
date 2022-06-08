// gitserver is the gitserver server.
package main // import "github.com/sourcegraph/sourcegraph/cmd/gitserver"

import (
	"container/list"
	"context"
	"database/sql"
	"encoding/base64"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/signal"
	"syscall"
	"time"

	jsoniter "github.com/json-iterator/go"
	"github.com/opentracing/opentracing-go"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/tidwall/gjson"
	"go.uber.org/zap"
	"golang.org/x/sync/semaphore"
	"golang.org/x/time/rate"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/server"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies"
	livedependencies "github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies/live"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/database"
	connections "github.com/sourcegraph/sourcegraph/internal/database/connections/live"
	"github.com/sourcegraph/sourcegraph/internal/debugserver"
	"github.com/sourcegraph/sourcegraph/internal/encryption/keyring"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/auth"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/crates"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gomodproxy"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/npm"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/pypi"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/hostname"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	"github.com/sourcegraph/sourcegraph/internal/logging"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/profiler"
	"github.com/sourcegraph/sourcegraph/internal/ratelimit"
	"github.com/sourcegraph/sourcegraph/internal/repos"
	"github.com/sourcegraph/sourcegraph/internal/sentry"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
	"github.com/sourcegraph/sourcegraph/internal/tracer"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/version"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

var (
	reposDir                       = env.Get("SRC_REPOS_DIR", "/data/repos", "Root dir containing repos.")
	wantPctFree                    = env.MustGetInt("SRC_REPOS_DESIRED_PERCENT_FREE", 10, "Target percentage of free space on disk.")
	janitorInterval                = env.MustGetDuration("SRC_REPOS_JANITOR_INTERVAL", 1*time.Minute, "Interval between cleanup runs")
	syncRepoStateInterval          = env.MustGetDuration("SRC_REPOS_SYNC_STATE_INTERVAL", 10*time.Minute, "Interval between state syncs")
	syncRepoStateBatchSize         = env.MustGetInt("SRC_REPOS_SYNC_STATE_BATCH_SIZE", 500, "Number of upserts to perform per batch")
	syncRepoStateUpsertPerSecond   = env.MustGetInt("SRC_REPOS_SYNC_STATE_UPSERT_PER_SEC", 500, "The number of upserted rows allowed per second across all gitserver instances")
	batchLogGlobalConcurrencyLimit = env.MustGetInt("SRC_BATCH_LOG_GLOBAL_CONCURRENCY_LIMIT", 256, "The maximum number of in-flight Git commands from all /batch-log requests combined")

	// 80 per second (4800 per minute) is well below our alert threshold of 30k per minute.
	rateLimitSyncerLimitPerSecond = env.MustGetInt("SRC_REPOS_SYNC_RATE_LIMIT_RATE_PER_SECOND", 80, "Rate limit applied to rate limit syncing")
)

func main() {
	ctx := context.Background()

	env.Lock()
	env.HandleHelpFlag()

	conf.Init()
	logging.Init()

	liblog := log.Init(log.Resource{
		Name:       env.MyName,
		Version:    version.Version(),
		InstanceID: hostname.Get(),
	}, log.NewSentrySink())
	defer liblog.Sync()
	go conf.Watch(liblog.Update(conf.GetLogSinks))

	tracer.Init(conf.DefaultClient())
	sentry.Init(conf.DefaultClient())
	trace.Init()
	profiler.Init()

	logger := log.Scoped("server", "the gitserver service")

	if reposDir == "" {
		logger.Fatal("SRC_REPOS_DIR is required")
	}
	if err := os.MkdirAll(reposDir, os.ModePerm); err != nil {
		logger.Fatal("failed to create SRC_REPOS_DIR", zap.Error(err))
	}

	wantPctFree2, err := getPercent(wantPctFree)
	if err != nil {
		logger.Fatal("SRC_REPOS_DESIRED_PERCENT_FREE is out of range", zap.Error(err))
	}

	sqlDB, err := getDB()
	if err != nil {
		logger.Fatal("failed to initialize database stores", zap.Error(err))
	}
	db := database.NewDB(sqlDB)

	repoStore := db.Repos()
	depsSvc := livedependencies.GetService(db, nil)
	externalServiceStore := db.ExternalServices()

	err = keyring.Init(ctx)
	if err != nil {
		logger.Fatal("failed to initialise keyring", zap.Error(err))
	}

	gitserver := server.Server{
		Logger:             logger,
		ReposDir:           reposDir,
		DesiredPercentFree: wantPctFree2,
		GetRemoteURLFunc: func(ctx context.Context, repo api.RepoName) (string, error) {
			return getRemoteURLFunc(ctx, externalServiceStore, repoStore, nil, repo)
		},
		GetVCSSyncer: func(ctx context.Context, repo api.RepoName) (server.VCSSyncer, error) {
			return getVCSSyncer(ctx, externalServiceStore, repoStore, depsSvc, repo)
		},
		Hostname:                hostname.Get(),
		DB:                      db,
		CloneQueue:              server.NewCloneQueue(list.New()),
		GlobalBatchLogSemaphore: semaphore.NewWeighted(int64(batchLogGlobalConcurrencyLimit)),
	}

	observationContext := &observation.Context{
		Logger:     logger,
		Tracer:     &trace.Tracer{Tracer: opentracing.GlobalTracer()},
		Registerer: prometheus.DefaultRegisterer,
	}
	gitserver.RegisterMetrics(db, observationContext)

	if tmpDir, err := gitserver.SetupAndClearTmp(); err != nil {
		logger.Fatal("failed to setup temporary directory", log.Error(err))
	} else if err := os.Setenv("TMP_DIR", tmpDir); err != nil {
		// Additionally, set TMP_DIR so other temporary files we may accidentally
		// create are on the faster RepoDir mount.
		logger.Fatal("Setting TMP_DIR", log.Error(err))
	}

	// Create Handler now since it also initializes state
	// TODO: Why do we set server state as a side effect of creating our handler?
	handler := gitserver.Handler()
	handler = actor.HTTPMiddleware(handler)
	handler = ot.HTTPMiddleware(trace.HTTPMiddleware(logger, handler, conf.DefaultClient()))

	// Ready immediately
	ready := make(chan struct{})
	close(ready)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	// Best effort attempt to sync rate limiters for site level external services
	// early on. If it fails, we'll try again in the background sync below.
	if err := syncSiteLevelExternalServiceRateLimiters(ctx, externalServiceStore); err != nil {
		logger.Warn("error performing initial site level rate limit sync", log.Error(err))
	}

	go syncRateLimiters(ctx, externalServiceStore, rateLimitSyncerLimitPerSecond)
	go debugserver.NewServerRoutine(ready).Start()
	go gitserver.Janitor(janitorInterval)
	go gitserver.SyncRepoState(syncRepoStateInterval, syncRepoStateBatchSize, syncRepoStateUpsertPerSecond)

	gitserver.StartClonePipeline(ctx)

	addr := os.Getenv("GITSERVER_ADDR")
	if addr == "" {
		port := "3178"
		host := ""
		if env.InsecureDev {
			host = "127.0.0.1"
		}
		addr = net.JoinHostPort(host, port)
	}
	srv := &http.Server{
		Addr:    addr,
		Handler: handler,
	}
	logger.Info("git-server: listening", log.String("addr", srv.Addr))

	go func() {
		err := srv.ListenAndServe()
		if err != http.ErrServerClosed {
			logger.Fatal(err.Error())
		}
	}()

	// Listen for shutdown signals. When we receive one attempt to clean up,
	// but do an insta-shutdown if we receive more than one signal.
	c := make(chan os.Signal, 2)
	signal.Notify(c, syscall.SIGINT, syscall.SIGHUP, syscall.SIGTERM)

	// Once we receive one of the signals from above, continues with the shutdown
	// process.
	<-c
	go func() {
		// If a second signal is received, exit immediately.
		<-c
		os.Exit(0)
	}()

	// Wait for at most for the configured shutdown timeout.
	ctx, cancel = context.WithTimeout(ctx, goroutine.GracefulShutdownTimeout)
	defer cancel()
	// Stop accepting requests.
	if err := srv.Shutdown(ctx); err != nil {
		logger.Error("shutting down http server", log.Error(err))
	}

	// The most important thing this does is kill all our clones. If we just
	// shutdown they will be orphaned and continue running.
	gitserver.Stop()
}

func configureFusionClient(conn schema.PerforceConnection) server.FusionConfig {
	// Set up default settings first
	fc := server.FusionConfig{
		Enabled:             false,
		Client:              conn.P4Client,
		LookAhead:           2000,
		NetworkThreads:      12,
		NetworkThreadsFetch: 12,
		PrintBatch:          10,
		Refresh:             100,
		Retries:             10,
		MaxChanges:          -1,
		IncludeBinaries:     false,
		FsyncEnable:         false,
	}

	if conn.FusionClient == nil {
		return fc
	}

	// Required
	fc.Enabled = conn.FusionClient.Enabled
	fc.LookAhead = conn.FusionClient.LookAhead

	// Optional
	if conn.FusionClient.NetworkThreads > 0 {
		fc.NetworkThreads = conn.FusionClient.NetworkThreads
	}
	if conn.FusionClient.NetworkThreadsFetch > 0 {
		fc.NetworkThreadsFetch = conn.FusionClient.NetworkThreadsFetch
	}
	if conn.FusionClient.PrintBatch > 0 {
		fc.PrintBatch = conn.FusionClient.PrintBatch
	}
	if conn.FusionClient.Refresh > 0 {
		fc.Refresh = conn.FusionClient.Refresh
	}
	if conn.FusionClient.Retries > 0 {
		fc.Retries = conn.FusionClient.Retries
	}
	if conn.FusionClient.MaxChanges > 0 {
		fc.MaxChanges = conn.FusionClient.MaxChanges
	}
	fc.IncludeBinaries = conn.FusionClient.IncludeBinaries
	fc.FsyncEnable = conn.FusionClient.FsyncEnable

	return fc
}

func getPercent(p int) (int, error) {
	if p < 0 {
		return 0, errors.Errorf("negative value given for percentage: %d", p)
	}
	if p > 100 {
		return 0, errors.Errorf("excessively high value given for percentage: %d", p)
	}
	return p, nil
}

// getDB initializes a connection to the database and returns a dbutil.DB
func getDB() (*sql.DB, error) {
	// Gitserver is an internal actor. We rely on the frontend to do authz checks for
	// user requests.
	//
	// This call to SetProviders is here so that calls to GetProviders don't block.
	authz.SetProviders(true, []authz.Provider{})

	dsn := conf.GetServiceConnectionValueAndRestartOnChange(func(serviceConnections conftypes.ServiceConnections) string {
		return serviceConnections.PostgresDSN
	})
	return connections.EnsureNewFrontendDB(dsn, "gitserver", &observation.TestContext)
}

func getRemoteURLFunc(
	ctx context.Context,
	externalServiceStore database.ExternalServiceStore,
	repoStore database.RepoStore,
	cli httpcli.Doer,
	repo api.RepoName,
) (string, error) {
	r, err := repoStore.GetByName(ctx, repo)
	if err != nil {
		return "", err
	}

	for _, info := range r.Sources {
		// build the clone url using the external service config instead of using
		// the source CloneURL field
		svc, err := externalServiceStore.GetByID(ctx, info.ExternalServiceID())
		if err != nil {
			return "", err
		}

		if svc.CloudDefault && r.Private {
			// We won't be able to use this remote URL, so we should skip it. This can happen
			// if a repo moves from being public to private while belonging to both a cloud
			// default external service and another external service with a token that has
			// access to the private repo.
			continue
		}

		dotcomConfig := conf.SiteConfig().Dotcom
		if envvar.SourcegraphDotComMode() &&
			repos.IsGitHubAppCloudEnabled(dotcomConfig) &&
			svc.Kind == extsvc.KindGitHub {
			installationID := gjson.Get(svc.Config, "githubAppInstallationID").Int()
			if installationID > 0 {
				svc.Config, err = editGitHubAppExternalServiceConfigToken(ctx, externalServiceStore, svc, dotcomConfig, installationID, cli)
				if err != nil {
					return "", errors.Wrap(err, "edit GitHub App external service config token")
				}
			}
		}
		return repos.CloneURL(svc.Kind, svc.Config, r)
	}
	return "", errors.Errorf("no sources for %q", repo)
}

// editGitHubAppExternalServiceConfigToken updates the "token" field of the given
// external service config through GitHub App and returns a new copy of the
// config ensuring the token is always valid.
func editGitHubAppExternalServiceConfigToken(
	ctx context.Context,
	externalServiceStore database.ExternalServiceStore,
	svc *types.ExternalService,
	dotcomConfig *schema.Dotcom,
	installationID int64,
	cli httpcli.Doer,
) (string, error) {
	baseURL, err := url.Parse(gjson.Get(svc.Config, "url").String())
	if err != nil {
		return "", errors.Wrap(err, "parse base URL")
	}

	apiURL, githubDotCom := github.APIRoot(baseURL)
	if !githubDotCom {
		return "", errors.Errorf("only GitHub App on GitHub.com is supported, but got %q", baseURL)
	}

	pkey, err := base64.StdEncoding.DecodeString(dotcomConfig.GithubAppCloud.PrivateKey)
	if err != nil {
		return "", errors.Wrap(err, "decode private key")
	}

	auther, err := auth.NewOAuthBearerTokenWithGitHubApp(dotcomConfig.GithubAppCloud.AppID, pkey)
	if err != nil {
		return "", errors.Wrap(err, "new authenticator with GitHub App")
	}

	client := github.NewV3Client(log.Scoped("github.v3", "github v3 client"), svc.URN(), apiURL, auther, cli)

	token, err := repos.GetOrRenewGitHubAppInstallationAccessToken(ctx, externalServiceStore, svc, client, installationID)
	if err != nil {
		return "", errors.Wrap(err, "get or renew GitHub App installation access token")
	}

	// NOTE: Use `json.Marshal` breaks the actual external service config that fails
	// validation with missing "repos" property when no repository has been selected,
	// due to generated JSON tag of ",omitempty".
	config, err := jsonc.Edit(svc.Config, token, "token")
	if err != nil {
		return "", errors.Wrap(err, "edit token")
	}
	return config, nil
}

func getVCSSyncer(
	ctx context.Context,
	externalServiceStore database.ExternalServiceStore,
	repoStore database.RepoStore,
	depsSvc *dependencies.Service,
	repo api.RepoName,
) (server.VCSSyncer, error) {
	// We need an internal actor in case we are trying to access a private repo. We
	// only need access in order to find out the type of code host we're using, so
	// it's safe.
	r, err := repoStore.GetByName(actor.WithInternalActor(ctx), repo)
	if err != nil {
		return nil, errors.Wrap(err, "get repository")
	}

	extractOptions := func(connection any) (string, error) {
		for _, info := range r.Sources {
			extSvc, err := externalServiceStore.GetByID(ctx, info.ExternalServiceID())
			if err != nil {
				return "", errors.Wrap(err, "get external service")
			}
			normalized, err := jsonc.Parse(extSvc.Config)
			if err != nil {
				return "", errors.Wrap(err, "normalize JSON")
			}
			if err = jsoniter.Unmarshal(normalized, connection); err != nil {
				return "", errors.Wrap(err, "unmarshal JSON")
			}
			return extSvc.URN(), nil
		}
		return "", errors.Errorf("unexpected empty Sources map in %v", r)
	}

	switch r.ExternalRepo.ServiceType {
	case extsvc.TypePerforce:
		var c schema.PerforceConnection
		if _, err := extractOptions(&c); err != nil {
			return nil, err
		}
		return &server.PerforceDepotSyncer{
			MaxChanges:   int(c.MaxChanges),
			Client:       c.P4Client,
			FusionConfig: configureFusionClient(c),
		}, nil
	case extsvc.TypeJVMPackages:
		var c schema.JVMPackagesConnection
		if _, err := extractOptions(&c); err != nil {
			return nil, err
		}
		return server.NewJVMPackagesSyncer(&c, depsSvc), nil
	case extsvc.TypeNpmPackages:
		var c schema.NpmPackagesConnection
		urn, err := extractOptions(&c)
		if err != nil {
			return nil, err
		}
		cli := npm.NewHTTPClient(urn, c.Registry, c.Credentials, httpcli.ExternalDoer)
		return server.NewNpmPackagesSyncer(c, depsSvc, cli), nil
	case extsvc.TypeGoModules:
		var c schema.GoModulesConnection
		urn, err := extractOptions(&c)
		if err != nil {
			return nil, err
		}
		cli := gomodproxy.NewClient(urn, c.Urls, httpcli.ExternalDoer)
		return server.NewGoModulesSyncer(&c, depsSvc, cli), nil
	case extsvc.TypePythonPackages:
		var c schema.PythonPackagesConnection
		urn, err := extractOptions(&c)
		if err != nil {
			return nil, err
		}
		cli := pypi.NewClient(urn, c.Urls, httpcli.ExternalDoer)
		return server.NewPythonPackagesSyncer(&c, depsSvc, cli), nil
	case extsvc.TypeRustPackages:
		var c schema.RustPackagesConnection
		urn, err := extractOptions(&c)
		if err != nil {
			return nil, err
		}
		cli := crates.NewClient(urn, httpcli.ExternalDoer)
		return server.NewRustPackagesSyncer(&c, depsSvc, cli), nil
	}
	return &server.GitRepoSyncer{}, nil
}

func syncSiteLevelExternalServiceRateLimiters(ctx context.Context, store database.ExternalServiceStore) error {
	svcs, err := store.List(ctx, database.ExternalServicesListOptions{NoNamespace: true})
	if err != nil {
		return errors.Wrap(err, "listing external services")
	}
	syncer := repos.NewRateLimitSyncer(ratelimit.DefaultRegistry, store, repos.RateLimitSyncerOpts{})
	return syncer.SyncServices(svcs)
}

// Sync rate limiters from config. Since we don't have a trigger that watches for
// changes to rate limits we'll run this periodically in the background.
func syncRateLimiters(ctx context.Context, store database.ExternalServiceStore, perSecond int) {
	backoff := 5 * time.Second
	batchSize := 50
	logger := log.Scoped("sync rate limiter", "Sync rate limiters from config.")

	// perSecond should be spread across all gitserver instances and we want to wait
	// until we know about at least one instance.
	var instanceCount int
	for {
		instanceCount = len(conf.Get().ServiceConnectionConfig.GitServers)
		if instanceCount > 0 {
			break
		}

		logger.Warn("found zero gitserver instance, trying again after backoff", log.Duration("backoff", backoff))
	}

	limiter := rate.NewLimiter(rate.Limit(float64(perSecond)/float64(instanceCount)), batchSize)
	syncer := repos.NewRateLimitSyncer(ratelimit.DefaultRegistry, store, repos.RateLimitSyncerOpts{
		PageSize: batchSize,
		Limiter:  limiter,
	})

	var lastSuccessfulSync time.Time
	ticker := time.NewTicker(1 * time.Minute)
	for {
		start := time.Now()
		if err := syncer.SyncLimitersSince(ctx, lastSuccessfulSync); err != nil {
			logger.Warn("syncRateLimiters: error syncing rate limits", log.Error(err))
		} else {
			lastSuccessfulSync = start
		}

		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
	}
}
