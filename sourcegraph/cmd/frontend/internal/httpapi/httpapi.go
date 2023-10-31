package httpapi

import (
	"context"
	"log"
	"net/http"
	"os"
	"reflect"
	"strconv"
	"time"

	"github.com/gorilla/mux"
	"github.com/gorilla/schema"
	"github.com/graph-gophers/graphql-go"
	sglog "github.com/sourcegraph/log"
	"google.golang.org/grpc"

	zoektProto "github.com/sourcegraph/zoekt/cmd/zoekt-sourcegraph-indexserver/protos/sourcegraph/zoekt/configuration/v1"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/codyapp"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/enterprise"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/handlerutil"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/httpapi/releasecache"
	apirouter "github.com/sourcegraph/sourcegraph/cmd/frontend/internal/httpapi/router"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/httpapi/webhookhandlers"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/routevar"
	frontendsearch "github.com/sourcegraph/sourcegraph/cmd/frontend/internal/search"
	frontendregistry "github.com/sourcegraph/sourcegraph/cmd/frontend/registry/api"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/webhooks"
	"github.com/sourcegraph/sourcegraph/internal/api"
	confProto "github.com/sourcegraph/sourcegraph/internal/api/internalapi/v1"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/encryption/keyring"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/searchcontexts"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/updatecheck"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type Handlers struct {
	// Repo sync
	GitHubSyncWebhook          webhooks.Registerer
	GitLabSyncWebhook          webhooks.Registerer
	BitbucketServerSyncWebhook webhooks.Registerer
	BitbucketCloudSyncWebhook  webhooks.Registerer

	// Permissions
	PermissionsGitHubWebhook webhooks.Registerer

	// Batch changes
	BatchesGitHubWebhook            webhooks.Registerer
	BatchesGitLabWebhook            webhooks.RegistererHandler
	BatchesBitbucketServerWebhook   webhooks.RegistererHandler
	BatchesBitbucketCloudWebhook    webhooks.RegistererHandler
	BatchesAzureDevOpsWebhook       webhooks.Registerer
	BatchesChangesFileGetHandler    http.Handler
	BatchesChangesFileExistsHandler http.Handler
	BatchesChangesFileUploadHandler http.Handler

	// SCIM
	SCIMHandler http.Handler

	// Code intel
	NewCodeIntelUploadHandler enterprise.NewCodeIntelUploadHandler

	// Compute
	NewComputeStreamHandler enterprise.NewComputeStreamHandler

	// Code Insights
	CodeInsightsDataExportHandler http.Handler

	// Search jobs
	SearchJobsDataExportHandler http.Handler
	SearchJobsLogsHandler       http.Handler

	// Dotcom license check
	NewDotcomLicenseCheckHandler enterprise.NewDotcomLicenseCheckHandler

	// Completions stream
	NewChatCompletionsStreamHandler enterprise.NewChatCompletionsStreamHandler
	NewCodeCompletionsHandler       enterprise.NewCodeCompletionsHandler
}

// NewHandler returns a new API handler.
//
// 🚨 SECURITY: The caller MUST wrap the returned handler in middleware that checks authentication
// and sets the actor in the request context.
func NewHandler(
	db database.DB,
	schema *graphql.Schema,
	rateLimiter graphqlbackend.LimitWatcher,
	handlers *Handlers,
) (http.Handler, error) {
	logger := sglog.Scoped("Handler")

	m := mux.NewRouter().PathPrefix("/.api/").Subrouter()
	m.StrictSlash(true)

	jsonHandler := JsonMiddleware(&ErrorHandler{
		Logger: logger,
		// Only display error message to admins when in debug mode, since it
		// may contain sensitive info (like API keys in net/http error
		// messages).
		WriteErrBody: env.InsecureDev,
	})

	m.PathPrefix("/registry").Methods("GET").Name(apirouter.Registry).Handler(trace.Route(jsonHandler(frontendregistry.HandleRegistry)))
	m.PathPrefix("/scim/v2").Methods("GET", "POST", "PUT", "PATCH", "DELETE").Name(apirouter.SCIM).Handler(trace.Route(handlers.SCIMHandler))
	m.Path("/graphql").Methods("POST").Name(apirouter.GraphQL).Handler(trace.Route(jsonHandler(serveGraphQL(logger, schema, rateLimiter, false))))

	// Webhooks
	//
	// First: register handlers
	wh := webhooks.Router{
		Logger: logger.Scoped("webhooks.Router"),
		DB:     db,
	}
	webhookhandlers.Init(&wh)
	handlers.BatchesGitHubWebhook.Register(&wh)
	handlers.BatchesGitLabWebhook.Register(&wh)
	handlers.BitbucketServerSyncWebhook.Register(&wh)
	handlers.BitbucketCloudSyncWebhook.Register(&wh)
	handlers.BatchesBitbucketServerWebhook.Register(&wh)
	handlers.BatchesBitbucketCloudWebhook.Register(&wh)
	handlers.GitHubSyncWebhook.Register(&wh)
	handlers.GitLabSyncWebhook.Register(&wh)
	handlers.PermissionsGitHubWebhook.Register(&wh)
	handlers.BatchesAzureDevOpsWebhook.Register(&wh)
	// Second: register handler on main router
	// 🚨 SECURITY: This handler implements its own secret-based auth
	webhookMiddleware := webhooks.NewLogMiddleware(db.WebhookLogs(keyring.Default().WebhookLogKey))
	webhookHandler := webhooks.NewHandler(logger, db, &wh)
	m.Path("/webhooks/{webhook_uuid}").Methods("POST").Handler(trace.Route(webhookMiddleware.Logger(webhookHandler)))

	// Old, soon to be deprecated, webhook handlers
	gitHubWebhook := webhooks.GitHubWebhook{Router: &wh}
	m.Path("/github-webhooks").Methods("POST").Handler(trace.Route(webhookMiddleware.Logger(&gitHubWebhook)))
	m.Path("/gitlab-webhooks").Methods("POST").Handler(trace.Route(webhookMiddleware.Logger(handlers.BatchesGitLabWebhook)))
	m.Path("/bitbucket-server-webhooks").Methods("POST").Handler(trace.Route(webhookMiddleware.Logger(handlers.BatchesBitbucketServerWebhook)))
	m.Path("/bitbucket-cloud-webhooks").Methods("POST").Handler(trace.Route(webhookMiddleware.Logger(handlers.BatchesBitbucketCloudWebhook)))

	// Other routes
	m.Path("/files/batch-changes/{spec}/{file}").Methods("GET").Handler(trace.Route(handlers.BatchesChangesFileGetHandler))
	m.Path("/files/batch-changes/{spec}/{file}").Methods("HEAD").Handler(trace.Route(handlers.BatchesChangesFileExistsHandler))
	m.Path("/files/batch-changes/{spec}").Methods("POST").Handler(trace.Route(handlers.BatchesChangesFileUploadHandler))
	m.Path("/lsif/upload").Methods("POST").Name(apirouter.LSIFUpload).Handler(trace.Route(lsifDeprecationHandler))
	m.Path("/scip/upload").Methods("POST").Name(apirouter.SCIPUpload).Handler(trace.Route(handlers.NewCodeIntelUploadHandler(true)))
	m.Path("/scip/upload").Methods("HEAD").Name(apirouter.SCIPUploadExists).Handler(trace.Route(noopHandler))
	m.Path("/compute/stream").Methods("GET", "POST").Name(apirouter.ComputeStream).Handler(trace.Route(handlers.NewComputeStreamHandler()))
	m.Path("/blame/" + routevar.Repo + routevar.RepoRevSuffix + "/stream/{Path:.*}").Methods("GET").Handler(trace.Route(handleStreamBlame(logger, db, gitserver.NewClient("http.blamestream"))))
	// Set up the src-cli version cache handler (this will effectively be a
	// no-op anywhere other than dot-com).
	m.Path("/src-cli/versions/{rest:.*}").Methods("GET", "POST").Handler(trace.Route(releasecache.NewHandler(logger)))
	// Return the minimum src-cli version that's compatible with this instance
	m.Path("/src-cli/{rest:.*}").Methods("GET").Handler(trace.Route(newSrcCliVersionHandler(logger)))
	m.Path("/insights/export/{id}").Methods("GET").Handler(trace.Route(handlers.CodeInsightsDataExportHandler))
	m.Path("/search/stream").Methods("GET").Handler(trace.Route(frontendsearch.StreamHandler(db)))
	m.Path("/search/export/{id}.csv").Methods("GET").Handler(trace.Route(handlers.SearchJobsDataExportHandler))
	m.Path("/search/export/{id}.log").Methods("GET").Handler(trace.Route(handlers.SearchJobsLogsHandler))

	m.Path("/completions/stream").Methods("POST").Handler(trace.Route(handlers.NewChatCompletionsStreamHandler()))
	m.Path("/completions/code").Methods("POST").Handler(trace.Route(handlers.NewCodeCompletionsHandler()))

	if envvar.SourcegraphDotComMode() {
		m.Path("/app/check/update").Name(codyapp.RouteAppUpdateCheck).Handler(trace.Route(codyapp.AppUpdateHandler(logger)))
		m.Path("/app/latest").Name(codyapp.RouteCodyAppLatestVersion).Handler(trace.Route(codyapp.LatestVersionHandler(logger)))
		m.Path("/license/check").Methods("POST").Name("dotcom.license.check").Handler(trace.Route(handlers.NewDotcomLicenseCheckHandler()))

		updatecheckHandler, err := updatecheck.ForwardHandler()
		if err != nil {
			return nil, errors.Errorf("create updatecheck handler: %v", err)
		}
		m.Path("/updates").
			Methods(http.MethodGet, http.MethodPost).
			Name("updatecheck").
			Handler(trace.Route(updatecheckHandler))
	}

	// repo contains routes that are NOT specific to a revision. In these routes, the URL may not contain a revspec after the repo (that is, no "github.com/foo/bar@myrevspec").
	// repo contains routes that are NOT specific to a revision. In these routes, the URL may not contain a revspec after the repo (that is, no "github.com/foo/bar@myrevspec").
	repoPath := `/repos/` + routevar.Repo
	// Additional paths added will be treated as a repo. To add a new path that should not be treated as a repo
	// add above repo paths.
	repo := m.PathPrefix(repoPath + "/" + routevar.RepoPathDelim + "/").Subrouter()
	repo.Path("/shield").Methods("GET").Handler(trace.Route(jsonHandler(serveRepoShield())))

	m.NotFoundHandler = http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		log.Printf("API no route: %s %s from %s", r.Method, r.URL, r.Referer())
		http.Error(w, "no route", http.StatusNotFound)
	})

	return m, nil
}

// RegisterInternalServices registers REST and gRPC handlers for Sourcegraph's internal API on the
// provided mux.Router and gRPC server.
//
// 🚨 SECURITY: This handler should not be served on a publicly exposed port. 🚨
// This handler is not guaranteed to provide the same authorization checks as
// public API handlers.
func RegisterInternalServices(
	m *mux.Router,
	s *grpc.Server,

	db database.DB,
	schema *graphql.Schema,
	newCodeIntelUploadHandler enterprise.NewCodeIntelUploadHandler,
	rankingService enterprise.RankingService,
	newComputeStreamHandler enterprise.NewComputeStreamHandler,
	rateLimitWatcher graphqlbackend.LimitWatcher,
) {
	logger := sglog.Scoped("InternalHandler")
	m.StrictSlash(true)

	handler := JsonMiddleware(&ErrorHandler{
		Logger: logger,
		// Internal endpoints can expose sensitive errors
		WriteErrBody: true,
	})

	// zoekt-indexserver endpoints
	gsClient := gitserver.NewClient("http.zoektindexerserver")
	indexer := &searchIndexerServer{
		db:              db,
		logger:          logger.Scoped("searchIndexerServer"),
		gitserverClient: gsClient,
		ListIndexable:   backend.NewRepos(logger, db, gsClient).ListIndexable,
		RepoStore:       db.Repos(),
		SearchContextsRepoRevs: func(ctx context.Context, repoIDs []api.RepoID) (map[api.RepoID][]string, error) {
			return searchcontexts.RepoRevs(ctx, db, repoIDs)
		},
		Indexers:               search.Indexers(),
		Ranking:                rankingService,
		MinLastChangedDisabled: os.Getenv("SRC_SEARCH_INDEXER_EFFICIENT_POLLING_DISABLED") != "",
	}
	m.Get(apirouter.SearchConfiguration).Handler(trace.Route(handler(indexer.serveConfiguration)))
	m.Get(apirouter.ReposIndex).Handler(trace.Route(handler(indexer.serveList)))
	m.Get(apirouter.DocumentRanks).Handler(trace.Route(handler(indexer.serveDocumentRanks)))
	m.Get(apirouter.UpdateIndexStatus).Handler(trace.Route(handler(indexer.handleIndexStatusUpdate)))

	zoektProto.RegisterZoektConfigurationServiceServer(s, &searchIndexerGRPCServer{server: indexer})
	confProto.RegisterConfigServiceServer(s, &configServer{})

	gitService := &gitServiceHandler{
		Gitserver: gsClient,
	}
	m.Get(apirouter.GitInfoRefs).Handler(trace.Route(handler(gitService.serveInfoRefs())))
	m.Get(apirouter.GitUploadPack).Handler(trace.Route(handler(gitService.serveGitUploadPack())))
	m.Get(apirouter.GraphQL).Handler(trace.Route(handler(serveGraphQL(logger, schema, rateLimitWatcher, true))))
	m.Get(apirouter.Configuration).Handler(trace.Route(handler(serveConfiguration)))
	m.Path("/ping").Methods("GET").Name("ping").HandlerFunc(handlePing)
	m.Get(apirouter.StreamingSearch).Handler(trace.Route(frontendsearch.StreamHandler(db)))
	m.Get(apirouter.ComputeStream).Handler(trace.Route(newComputeStreamHandler()))

	m.Get(apirouter.LSIFUpload).Handler(trace.Route(newCodeIntelUploadHandler(false)))
	m.Get(apirouter.SCIPUpload).Handler(trace.Route(newCodeIntelUploadHandler(false)))
	m.Get(apirouter.SCIPUploadExists).Handler(trace.Route(noopHandler))

	m.NotFoundHandler = http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		log.Printf("API no route: %s %s from %s", r.Method, r.URL, r.Referer())
		http.Error(w, "no route", http.StatusNotFound)
	})
}

var schemaDecoder = schema.NewDecoder()

func init() {
	schemaDecoder.IgnoreUnknownKeys(true)

	// Register a converter for unix timestamp strings -> time.Time values
	// (needed for Appdash PageLoadEvent type).
	schemaDecoder.RegisterConverter(time.Time{}, func(s string) reflect.Value {
		ms, err := strconv.ParseInt(s, 10, 64)
		if err != nil {
			return reflect.ValueOf(err)
		}
		return reflect.ValueOf(time.Unix(0, ms*int64(time.Millisecond)))
	})
}

type ErrorHandler struct {
	// Logger is required
	Logger sglog.Logger

	WriteErrBody bool
}

func (h *ErrorHandler) Handle(w http.ResponseWriter, r *http.Request, status int, err error) {
	logger := trace.Logger(r.Context(), h.Logger)

	trace.SetRequestErrorCause(r.Context(), err)

	// Handle custom errors
	var e *handlerutil.URLMovedError
	if errors.As(err, &e) {
		err := handlerutil.RedirectToNewRepoName(w, r, e.NewRepo)
		if err != nil {
			logger.Error("error redirecting to new URI",
				sglog.Error(err),
				sglog.String("new_url", string(e.NewRepo)))
		}
		return
	}

	// Never cache error responses.
	w.Header().Set("cache-control", "no-cache, max-age=0")

	errBody := err.Error()

	var displayErrBody string
	if h.WriteErrBody {
		displayErrBody = errBody
	}
	http.Error(w, displayErrBody, status)

	// No need to log, as SetRequestErrorCause is consumed and logged.
}

func JsonMiddleware(errorHandler *ErrorHandler) func(func(http.ResponseWriter, *http.Request) error) http.Handler {
	return func(h func(http.ResponseWriter, *http.Request) error) http.Handler {
		return handlerutil.HandlerWithErrorReturn{
			Handler: func(w http.ResponseWriter, r *http.Request) error {
				w.Header().Set("Content-Type", "application/json")
				return h(w, r)
			},
			Error: errorHandler.Handle,
		}
	}
}

var noopHandler = http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
	w.WriteHeader(http.StatusOK)
})

var lsifDeprecationHandler = http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
	w.WriteHeader(http.StatusNotImplemented)
	w.Write([]byte("Sourcegraph v4.5+ no longer accepts LSIF uploads. The Sourcegraph CLI v4.4.2+ will translate LSIF to SCIP prior to uploading. Please check the version of the CLI utility used to upload this artifact."))
})
