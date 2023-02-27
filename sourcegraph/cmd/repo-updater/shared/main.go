package shared

import (
	"context"
	_ "embed"
	"encoding/json"
	"fmt"
	"html/template"
	"net"
	"net/http"
	"strconv"
	"time"

	"github.com/graph-gophers/graphql-go/relay"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/sourcegraph/log"
	"go.opentelemetry.io/otel"
	"golang.org/x/time/rate"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/globals"
	"github.com/sourcegraph/sourcegraph/cmd/repo-updater/repoupdater"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/authz/permssync"
	"github.com/sourcegraph/sourcegraph/internal/batches"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/database"
	connections "github.com/sourcegraph/sourcegraph/internal/database/connections/live"
	"github.com/sourcegraph/sourcegraph/internal/debugserver"
	"github.com/sourcegraph/sourcegraph/internal/encryption/keyring"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/httpserver"
	"github.com/sourcegraph/sourcegraph/internal/instrumentation"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/ratelimit"
	"github.com/sourcegraph/sourcegraph/internal/repos"
	"github.com/sourcegraph/sourcegraph/internal/service"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const port = "3182"

//go:embed state.html.tmpl
var stateHTMLTemplate string

// EnterpriseInit is a function that allows enterprise code to be triggered when dependencies
// created in Main are ready for use.
//
// It returns a debugserver.Dumper and a function with which to enqueue a
// permission sync for a repository.
type EnterpriseInit func(
	observationCtx *observation.Context,
	db database.DB,
	store repos.Store,
	keyring keyring.Ring,
	cf *httpcli.Factory,
	server *repoupdater.Server,
) (map[string]debugserver.Dumper, func(ctx context.Context, repo api.RepoID, syncReason database.PermissionsSyncJobReason) error)

type LazyDebugserverEndpoint struct {
	repoUpdaterStateEndpoint     http.HandlerFunc
	listAuthzProvidersEndpoint   http.HandlerFunc
	gitserverReposStatusEndpoint http.HandlerFunc
	rateLimiterStateEndpoint     http.HandlerFunc
	manualPurgeEndpoint          http.HandlerFunc
}

func Main(ctx context.Context, observationCtx *observation.Context, ready service.ReadyFunc, debugserverEndpoints *LazyDebugserverEndpoint, enterpriseInit EnterpriseInit) error {
	// NOTE: Internal actor is required to have full visibility of the repo table
	// 	(i.e. bypass repository authorization).
	ctx = actor.WithInternalActor(ctx)

	logger := observationCtx.Logger

	clock := func() time.Time { return time.Now().UTC() }
	if err := keyring.Init(ctx); err != nil {
		return errors.Wrap(err, "initializing encryption keyring")
	}

	dsn := conf.GetServiceConnectionValueAndRestartOnChange(func(serviceConnections conftypes.ServiceConnections) string {
		return serviceConnections.PostgresDSN
	})
	sqlDB, err := connections.EnsureNewFrontendDB(observationCtx, dsn, "repo-updater")
	if err != nil {
		return errors.Wrap(err, "initializing database store")
	}
	db := database.NewDB(logger, sqlDB)

	// Generally we'll mark the service as ready sometime after the database has been
	// connected; migrations may take a while and we don't want to start accepting
	// traffic until we've fully constructed the server we'll be exposing. We have a
	// bit more to do in this method, though, and the process will be marked ready
	// further down this function.

	repos.MustRegisterMetrics(log.Scoped("MustRegisterMetrics", ""), db, envvar.SourcegraphDotComMode())

	store := repos.NewStore(logger.Scoped("store", "repo store"), db)
	{
		m := repos.NewStoreMetrics()
		m.MustRegister(prometheus.DefaultRegisterer)
		store.SetMetrics(m)
	}

	sourcerLogger := logger.Scoped("repos.Sourcer", "repositories source")
	cf := httpcli.NewExternalClientFactory(
		httpcli.NewLoggingMiddleware(sourcerLogger),
	)

	var src repos.Sourcer
	{
		m := repos.NewSourceMetrics()
		m.MustRegister(prometheus.DefaultRegisterer)

		src = repos.NewSourcer(sourcerLogger, db, cf, repos.WithDependenciesService(dependencies.NewService(observationCtx, db)), repos.ObservedSource(sourcerLogger, m))
	}

	updateScheduler := repos.NewUpdateScheduler(logger, db)
	server := &repoupdater.Server{
		Logger:                logger,
		ObservationCtx:        observationCtx,
		Store:                 store,
		Scheduler:             updateScheduler,
		SourcegraphDotComMode: envvar.SourcegraphDotComMode(),
		RateLimitSyncer:       repos.NewRateLimitSyncer(ratelimit.DefaultRegistry, store.ExternalServiceStore(), repos.RateLimitSyncerOpts{}),
		DatabaseBackedPermissionSyncerEnabled: func(ctx context.Context) bool {
			return permssync.PermissionSyncWorkerEnabled(ctx, db, logger)
		},
	}

	// Attempt to perform an initial sync with all external services
	if err := server.RateLimitSyncer.SyncRateLimiters(ctx); err != nil {
		// This is not a fatal error since the syncer has been added to the server above
		// and will still be run whenever an external service is added or updated
		logger.Error("Performing initial rate limit sync", log.Error(err))
	}

	syncer := &repos.Syncer{
		Sourcer: src,
		Store:   store,
		// We always want to listen on the Synced channel since external service syncing
		// happens on both Cloud and non Cloud instances.
		Synced:  make(chan repos.Diff),
		Now:     clock,
		ObsvCtx: observation.ContextWithLogger(logger.Scoped("syncer", "repo syncer"), observationCtx),
	}

	server.Syncer = syncer

	// All dependencies ready
	debugDumpers := make(map[string]debugserver.Dumper)
	var enqueueRepoPerms func(context.Context, api.RepoID, database.PermissionsSyncJobReason) error
	if enterpriseInit != nil {
		debugDumpers, enqueueRepoPerms = enterpriseInit(observationCtx, db, store, keyring.Default(), cf, server)
	}

	go watchSyncer(ctx, logger, syncer, updateScheduler, enqueueRepoPerms, server.ChangesetSyncRegistry)
	go func() {
		err := syncer.Run(ctx, store, repos.RunOptions{
			EnqueueInterval: repos.ConfRepoListUpdateInterval,
			IsCloud:         envvar.SourcegraphDotComMode(),
			MinSyncInterval: repos.ConfRepoListUpdateInterval,
		})
		if err != nil {
			logger.Fatal("syncer.Run failure", log.Error(err))
		}
	}()

	go manageUnclonedRepos(ctx, logger, updateScheduler, store)

	if envvar.SourcegraphDotComMode() {
		rateLimiter := ratelimit.NewInstrumentedLimiter("SyncReposWithLastErrors", rate.NewLimiter(.05, 1))
		go syncer.RunSyncReposWithLastErrorsWorker(ctx, rateLimiter)
	}

	go repos.RunPhabricatorRepositorySyncWorker(ctx, db, log.Scoped("PhabricatorRepositorySyncWorker", ""), store)

	// git-server repos purging thread
	go repos.RunRepositoryPurgeWorker(ctx, log.Scoped("repoPurgeWorker", "remove deleted repositories"),
		db, conf.DefaultClient())

	// Git fetches scheduler
	go repos.RunScheduler(ctx, logger, updateScheduler)
	logger.Debug("started scheduler")

	host := ""
	if env.InsecureDev {
		host = "127.0.0.1"
	}

	addr := net.JoinHostPort(host, port)
	logger.Info("listening", log.String("addr", addr))

	m := repoupdater.NewHandlerMetrics()
	m.MustRegister(prometheus.DefaultRegisterer)

	handler := repoupdater.ObservedHandler(
		logger,
		m,
		otel.GetTracerProvider(),
	)(server.Handler())

	globals.WatchExternalURL()

	debugDumpers["repos"] = updateScheduler
	debugserverEndpoints.repoUpdaterStateEndpoint = repoUpdaterStatsHandler(debugDumpers)
	debugserverEndpoints.listAuthzProvidersEndpoint = listAuthzProvidersHandler()
	debugserverEndpoints.gitserverReposStatusEndpoint = gitserverReposStatusHandler(db)
	debugserverEndpoints.rateLimiterStateEndpoint = rateLimiterStateHandler
	debugserverEndpoints.manualPurgeEndpoint = manualPurgeHandler(db)

	// We mark the service as ready now AFTER assigning the additional endpoints in
	// the debugserver constructed at the top of this function. This ensures we don't
	// have a race between becoming ready and a debugserver request failing directly
	// after being unblocked.
	ready()

	// NOTE: Internal actor is required to have full visibility of the repo table
	// 	(i.e. bypass repository authorization).
	authzBypass := func(f http.Handler) http.HandlerFunc {
		return func(w http.ResponseWriter, r *http.Request) {
			r = r.WithContext(actor.WithInternalActor(r.Context()))
			f.ServeHTTP(w, r)
		}
	}
	httpSrv := httpserver.NewFromAddr(addr, &http.Server{
		ReadTimeout:  75 * time.Second,
		WriteTimeout: 10 * time.Minute,
		Handler: instrumentation.HTTPMiddleware("",
			trace.HTTPMiddleware(logger, authzBypass(handler), conf.DefaultClient())),
	})
	goroutine.MonitorBackgroundRoutines(ctx, httpSrv)

	return nil
}

func createDebugServerEndpoints(ready chan struct{}, debugserverEndpoints *LazyDebugserverEndpoint) []debugserver.Endpoint {
	return []debugserver.Endpoint{
		{
			Name: "Repo Updater State",
			Path: "/repo-updater-state",
			Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				// wait until we're healthy to respond
				<-ready
				// repoUpdaterStateEndpoint is guaranteed to be assigned now
				debugserverEndpoints.repoUpdaterStateEndpoint(w, r)
			}),
		},
		{
			Name: "List Authz Providers",
			Path: "/list-authz-providers",
			Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				// wait until we're healthy to respond
				<-ready
				// listAuthzProvidersEndpoint is guaranteed to be assigned now
				debugserverEndpoints.listAuthzProvidersEndpoint(w, r)
			}),
		},
		{
			Name: "Gitserver Repo Status",
			Path: "/gitserver-repo-status",
			Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				<-ready
				debugserverEndpoints.gitserverReposStatusEndpoint(w, r)
			}),
		},
		{
			Name: "Rate Limiter State",
			Path: "/rate-limiter-state",
			Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				<-ready
				debugserverEndpoints.rateLimiterStateEndpoint(w, r)
			}),
		},
		{
			Name: "Manual Repo Purge",
			Path: "/manual-purge",
			Handler: http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				<-ready
				debugserverEndpoints.manualPurgeEndpoint(w, r)
			}),
		},
	}
}

func gitserverReposStatusHandler(db database.DB) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		repo := r.FormValue("repo")
		if repo == "" {
			http.Error(w, "missing 'repo' param", http.StatusBadRequest)
			return
		}

		status, err := db.GitserverRepos().GetByName(r.Context(), api.RepoName(repo))
		if err != nil {
			http.Error(w, fmt.Sprintf("fetching repository status: %q", err), http.StatusInternalServerError)
			return
		}

		resp, err := json.MarshalIndent(status, "", "  ")
		if err != nil {
			http.Error(w, fmt.Sprintf("failed to marshal status: %q", err.Error()), http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write(resp)
	}
}

func manualPurgeHandler(db database.DB) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		limit, err := strconv.Atoi(r.FormValue("limit"))
		if err != nil {
			http.Error(w, fmt.Sprintf("invalid limit: %v", err), http.StatusBadRequest)
			return
		}
		if limit <= 0 {
			http.Error(w, "limit must be greater than 0", http.StatusBadRequest)
			return
		}
		if limit > 10000 {
			http.Error(w, "limit must be less than 10000", http.StatusBadRequest)
			return
		}
		perSecond := 1.0 // Default value
		perSecondParam := r.FormValue("perSecond")
		if perSecondParam != "" {
			perSecond, err = strconv.ParseFloat(perSecondParam, 64)
			if err != nil {
				http.Error(w, fmt.Sprintf("invalid per second rate limit: %v", err), http.StatusBadRequest)
				return
			}
			// Set a sane lower bound
			if perSecond <= 0.1 {
				http.Error(w, fmt.Sprintf("invalid per second rate limit. Must be > 0.1, got %f", perSecond), http.StatusBadRequest)
				return
			}
		}
		err = repos.PurgeOldestRepos(log.Scoped("PurgeOldestRepos", ""), db, limit, perSecond)
		if err != nil {
			http.Error(w, fmt.Sprintf("starting manual purge: %v", err), http.StatusInternalServerError)
			return
		}
		_, _ = w.Write([]byte(fmt.Sprintf("manual purge started with limit of %d and rate of %f", limit, perSecond)))
	}
}

func rateLimiterStateHandler(w http.ResponseWriter, _ *http.Request) {
	info := ratelimit.DefaultRegistry.LimitInfo()
	resp, err := json.MarshalIndent(info, "", "  ")
	if err != nil {
		http.Error(w, fmt.Sprintf("failed to marshal rate limiter state: %q", err.Error()), http.StatusInternalServerError)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_, _ = w.Write(resp)
}

func listAuthzProvidersHandler() http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		type providerInfo struct {
			ServiceType        string `json:"service_type"`
			ServiceID          string `json:"service_id"`
			ExternalServiceURL string `json:"external_service_url"`
		}

		_, providers := authz.GetProviders()
		infos := make([]providerInfo, len(providers))
		for i, p := range providers {
			_, id := extsvc.DecodeURN(p.URN())

			// Note that the ID marshalling below replicates code found in `graphqlbackend`.
			// We cannot import that package's code into this one (see /dev/check/go-dbconn-import.sh).
			infos[i] = providerInfo{
				ServiceType:        p.ServiceType(),
				ServiceID:          p.ServiceID(),
				ExternalServiceURL: fmt.Sprintf("%s/site-admin/external-services/%s", globals.ExternalURL(), relay.MarshalID("ExternalService", id)),
			}
		}

		resp, err := json.MarshalIndent(infos, "", "  ")
		if err != nil {
			http.Error(w, "failed to marshal infos: "+err.Error(), http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write(resp)
	}
}

func repoUpdaterStatsHandler(debugDumpers map[string]debugserver.Dumper) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		wantDumper := r.URL.Query().Get("dumper")
		wantFormat := r.URL.Query().Get("format")

		// Showing the HTML version of repository syncing schedule as the default,
		// also the only dumper that supports rendering the HTML version.
		if (wantDumper == "" || wantDumper == "repos") && wantFormat != "json" {
			reposDumper, ok := debugDumpers["repos"].(*repos.UpdateScheduler)
			if !ok {
				http.Error(w, "No debug dumper for repos found", http.StatusInternalServerError)
				return
			}

			// This case also applies for defaultOffer. Note that this is preferred
			// over e.g. a 406 status code, according to the MDN:
			// https://developer.mozilla.org/en-US/docs/Web/HTTP/Status/406
			tmpl := template.New("state.html").Funcs(template.FuncMap{
				"truncateDuration": func(d time.Duration) time.Duration {
					return d.Truncate(time.Second)
				},
			})
			template.Must(tmpl.Parse(stateHTMLTemplate))
			err := tmpl.Execute(w, reposDumper.DebugDump(r.Context()))
			if err != nil {
				http.Error(w, "Failed to render template: "+err.Error(), http.StatusInternalServerError)
				return
			}
			return
		}

		var dumps []any
		for name, dumper := range debugDumpers {
			if wantDumper != "" && wantDumper != name {
				continue
			}
			dumps = append(dumps, dumper.DebugDump(r.Context()))
		}

		p, err := json.MarshalIndent(dumps, "", "  ")
		if err != nil {
			http.Error(w, "Failed to marshal dumps: "+err.Error(), http.StatusInternalServerError)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write(p)
	}
}

func watchSyncer(
	ctx context.Context,
	logger log.Logger,
	syncer *repos.Syncer,
	sched *repos.UpdateScheduler,
	enqueueRepoPermsJob func(ctx context.Context, repo api.RepoID, syncReason database.PermissionsSyncJobReason) error,
	changesetSyncer batches.UnarchivedChangesetSyncRegistry,
) {
	logger.Debug("started new repo syncer updates scheduler relay thread")

	for {
		select {
		case <-ctx.Done():
			return
		case diff := <-syncer.Synced:
			if !conf.Get().DisableAutoGitUpdates {
				sched.UpdateFromDiff(diff)
			}

			// Schedule a repo permissions sync for all private repos that were added or
			// modified.
			if enqueueRepoPermsJob != nil {
				for _, repo := range getPrivateAddedOrModifiedRepos(diff) {
					err := enqueueRepoPermsJob(ctx, repo, database.ReasonRepoUpdatedFromCodeHost)
					if err != nil {
						logger.Warn("failed to create repo sync job", log.Error(err), log.Int32("repo", int32(repo)))
					}
				}
			}

			// Similarly, changesetSyncer is only available in enterprise mode.
			if changesetSyncer != nil {
				repositories := diff.Modified.ReposModified(types.RepoModifiedArchived)
				if len(repositories) > 0 {
					if err := changesetSyncer.EnqueueChangesetSyncsForRepos(ctx, repositories.IDs()); err != nil {
						logger.Warn("error enqueuing changeset syncs for archived and unarchived repos", log.Error(err))
					}
				}
			}
		}
	}
}

func getPrivateAddedOrModifiedRepos(diff repos.Diff) []api.RepoID {
	repoIDs := make([]api.RepoID, 0, len(diff.Added)+len(diff.Modified))

	for _, r := range diff.Added {
		if r.Private {
			repoIDs = append(repoIDs, r.ID)
		}
	}

	for _, r := range diff.Modified.Repos() {
		if r.Private {
			repoIDs = append(repoIDs, r.ID)
		}
	}

	return repoIDs
}

// manageUnclonedRepos will periodically list the uncloned repositories on gitserver
// and update the scheduler with the list. It also ensures that if any of our
// indexable repos are missing from the cloned list they will be added for
// cloning ASAP.
func manageUnclonedRepos(ctx context.Context, logger log.Logger, sched *repos.UpdateScheduler, store repos.Store) {
	baseRepoStore := database.ReposWith(logger, store)

	doSync := func() {
		// Don't modify the scheduler if we're not performing auto updates
		if conf.Get().DisableAutoGitUpdates {
			return
		}

		if envvar.SourcegraphDotComMode() {
			// Fetch ALL indexable repos that are NOT cloned so that we can add them to the
			// scheduler
			opts := database.ListSourcegraphDotComIndexableReposOptions{
				CloneStatus: types.CloneStatusNotCloned,
			}
			indexable, err := baseRepoStore.ListSourcegraphDotComIndexableRepos(ctx, opts)
			if err != nil {
				logger.Error("listing indexable repos", log.Error(err))
				return
			}
			// Ensure that uncloned indexable repos are known to the scheduler
			sched.EnsureScheduled(indexable)
		}

		// Next, move any repos managed by the scheduler that are uncloned to the front
		// of the queue
		managed := sched.ListRepoIDs()

		uncloned, err := baseRepoStore.ListMinimalRepos(ctx, database.ReposListOptions{IDs: managed, NoCloned: true})
		if err != nil {
			logger.Warn("failed to fetch list of uncloned repositories", log.Error(err))
			return
		}

		sched.PrioritiseUncloned(uncloned)
	}

	for ctx.Err() == nil {
		doSync()
		select {
		case <-ctx.Done():
		case <-time.After(30 * time.Second):
		}
	}
}
