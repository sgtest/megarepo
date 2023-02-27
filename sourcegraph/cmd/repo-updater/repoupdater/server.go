// Package repoupdater implements the repo-updater service HTTP handler.
package repoupdater

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"

	"github.com/sourcegraph/log"
	"go.opentelemetry.io/otel/attribute"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/batches"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/repos"
	"github.com/sourcegraph/sourcegraph/internal/repoupdater/protocol"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// Server is a repoupdater server.
type Server struct {
	repos.Store
	*repos.Syncer
	Logger                log.Logger
	ObservationCtx        *observation.Context
	SourcegraphDotComMode bool
	Scheduler             interface {
		UpdateOnce(id api.RepoID, name api.RepoName)
		ScheduleInfo(id api.RepoID) *protocol.RepoUpdateSchedulerInfoResult
	}
	ChangesetSyncRegistry batches.ChangesetSyncRegistry
	RateLimitSyncer       interface {
		// SyncRateLimiters should be called when an external service changes so that
		// our internal rate limiters are kept in sync
		SyncRateLimiters(ctx context.Context, ids ...int64) error
	}
	PermsSyncer interface {
		// ScheduleUsers schedules new permissions syncing requests for given users.
		ScheduleUsers(ctx context.Context, opts authz.FetchPermsOptions, userIDs ...int32)
		// ScheduleRepos schedules new permissions syncing requests for given repositories.
		ScheduleRepos(ctx context.Context, repoIDs ...api.RepoID)
	}
	DatabaseBackedPermissionSyncerEnabled func(ctx context.Context) bool
}

// Handler returns the http.Handler that should be used to serve requests.
func (s *Server) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/healthz", trace.WithRouteName("healthz", func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
	}))
	mux.HandleFunc("/repo-update-scheduler-info", trace.WithRouteName("repo-update-scheduler-info", s.handleRepoUpdateSchedulerInfo))
	mux.HandleFunc("/repo-lookup", trace.WithRouteName("repo-lookup", s.handleRepoLookup))
	mux.HandleFunc("/enqueue-repo-update", trace.WithRouteName("enqueue-repo-update", s.handleEnqueueRepoUpdate))
	mux.HandleFunc("/sync-external-service", trace.WithRouteName("sync-external-service", s.handleExternalServiceSync))
	mux.HandleFunc("/enqueue-changeset-sync", trace.WithRouteName("enqueue-changeset-sync", s.handleEnqueueChangesetSync))
	mux.HandleFunc("/schedule-perms-sync", trace.WithRouteName("schedule-perms-sync", s.handleSchedulePermsSync))
	mux.HandleFunc("/external-service-namespaces", trace.WithRouteName("external-service-namespaces", s.handleExternalServiceNamespaces))
	mux.HandleFunc("/external-service-repositories", trace.WithRouteName("external-service-repositories", s.handleExternalServiceRepositories))
	return mux
}

func (s *Server) handleRepoUpdateSchedulerInfo(w http.ResponseWriter, r *http.Request) {
	var args protocol.RepoUpdateSchedulerInfoArgs
	if err := json.NewDecoder(r.Body).Decode(&args); err != nil {
		s.respond(w, http.StatusBadRequest, err)
		return
	}

	result := s.Scheduler.ScheduleInfo(args.ID)
	s.respond(w, http.StatusOK, result)
}

func (s *Server) handleRepoLookup(w http.ResponseWriter, r *http.Request) {
	var args protocol.RepoLookupArgs
	if err := json.NewDecoder(r.Body).Decode(&args); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	result, err := s.repoLookup(r.Context(), args)
	if err != nil {
		if r.Context().Err() != nil {
			http.Error(w, "request canceled", http.StatusGatewayTimeout)
			return
		}
		s.Logger.Error("repoLookup failed",
			log.Object("repo",
				log.String("name", string(args.Repo)),
				log.Bool("update", args.Update),
			),
			log.Error(err))
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	s.respond(w, http.StatusOK, result)
}

func (s *Server) handleEnqueueRepoUpdate(w http.ResponseWriter, r *http.Request) {
	var req protocol.RepoUpdateRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		s.respond(w, http.StatusBadRequest, err)
		return
	}
	result, status, err := s.enqueueRepoUpdate(r.Context(), &req)
	if err != nil {
		s.Logger.Warn("enqueueRepoUpdate failed", log.String("req", fmt.Sprint(req)), log.Error(err))
		s.respond(w, status, err)
		return
	}
	s.respond(w, status, result)
}

func (s *Server) enqueueRepoUpdate(ctx context.Context, req *protocol.RepoUpdateRequest) (resp *protocol.RepoUpdateResponse, httpStatus int, err error) {
	tr, ctx := trace.New(ctx, "enqueueRepoUpdate", req.String())
	defer func() {
		s.Logger.Debug("enqueueRepoUpdate", log.Object("http", log.Int("status", httpStatus), log.String("resp", fmt.Sprint(resp)), log.Error(err)))
		if resp != nil {
			tr.SetAttributes(
				attribute.Int("resp.id", int(resp.ID)),
				attribute.String("resp.name", resp.Name),
			)
		}
		tr.SetError(err)
		tr.Finish()
	}()

	rs, err := s.Store.RepoStore().List(ctx, database.ReposListOptions{Names: []string{string(req.Repo)}})
	if err != nil {
		return nil, http.StatusInternalServerError, errors.Wrap(err, "store.list-repos")
	}

	if len(rs) != 1 {
		return nil, http.StatusNotFound, errors.Errorf("repo %q not found in store", req.Repo)
	}

	repo := rs[0]

	s.Scheduler.UpdateOnce(repo.ID, repo.Name)

	return &protocol.RepoUpdateResponse{
		ID:   repo.ID,
		Name: string(repo.Name),
	}, http.StatusOK, nil
}

func (s *Server) handleExternalServiceSync(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithCancel(r.Context())
	defer cancel()

	var req protocol.ExternalServiceSyncRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	logger := s.Logger.With(log.Int64("ExternalServiceID", req.ExternalServiceID))

	externalServiceID := req.ExternalServiceID

	es, err := s.ExternalServiceStore().GetByID(ctx, externalServiceID)
	if err != nil {
		if errcode.IsNotFound(err) {
			s.respond(w, http.StatusNotFound, err)
		} else {
			s.respond(w, http.StatusInternalServerError, err)
		}
		return
	}

	genericSourcer := s.NewGenericSourcer(logger)
	genericSrc, err := genericSourcer(ctx, es)
	if err != nil {
		logger.Error("server.external-service-sync", log.Error(err))
		return
	}

	statusCode, resp := handleExternalServiceValidate(ctx, logger, es, genericSrc)
	if statusCode > 0 {
		s.respond(w, statusCode, resp)
		return
	}
	if statusCode == 0 {
		// client is gone
		return
	}

	if s.RateLimitSyncer != nil {
		err = s.RateLimitSyncer.SyncRateLimiters(ctx, req.ExternalServiceID)
		if err != nil {
			logger.Warn("Handling rate limiter sync", log.Error(err))
		}
	}

	if err := s.Syncer.TriggerExternalServiceSync(ctx, req.ExternalServiceID); err != nil {
		logger.Warn("Enqueueing external service sync job", log.Error(err))
	}

	logger.Info("server.external-service-sync", log.Bool("synced", true))
	s.respond(w, http.StatusOK, &protocol.ExternalServiceSyncResult{})
}

func (s *Server) respond(w http.ResponseWriter, code int, v any) {
	switch val := v.(type) {
	case error:
		if val != nil {
			s.Logger.Error("response value error", log.Error(val))
			w.Header().Set("Content-Type", "text/plain; charset=utf-8")
			w.WriteHeader(code)
			fmt.Fprintf(w, "%v", val)
		}
	default:
		w.Header().Set("Content-Type", "application/json")
		bs, err := json.Marshal(v)
		if err != nil {
			s.respond(w, http.StatusInternalServerError, err)
			return
		}

		w.WriteHeader(code)
		if _, err = w.Write(bs); err != nil {
			s.Logger.Error("failed to write response", log.Error(err))
		}
	}
}

func handleExternalServiceValidate(ctx context.Context, logger log.Logger, es *types.ExternalService, src repos.Source) (int, any) {
	err := externalServiceValidate(ctx, es, src)
	if err == github.ErrIncompleteResults {
		logger.Info("server.external-service-sync", log.Error(err))
		syncResult := &protocol.ExternalServiceSyncResult{
			Error: err.Error(),
		}
		return http.StatusOK, syncResult
	}
	if ctx.Err() != nil {
		// client is gone
		return 0, nil
	}
	if err != nil {
		logger.Error("server.external-service-sync", log.Error(err))
		if errcode.IsUnauthorized(err) {
			return http.StatusUnauthorized, err
		}
		if errcode.IsForbidden(err) {
			return http.StatusForbidden, err
		}
		return http.StatusInternalServerError, err
	}
	return -1, nil
}

func externalServiceValidate(ctx context.Context, es *types.ExternalService, src repos.Source) error {
	if !es.DeletedAt.IsZero() {
		// We don't need to check deleted services.
		return nil
	}

	if v, ok := src.(repos.UserSource); ok {
		return v.ValidateAuthenticator(ctx)
	}

	ctx, cancel := context.WithCancel(ctx)
	results := make(chan repos.SourceResult)

	defer func() {
		cancel()

		// We need to drain the rest of the results to not leak a blocked goroutine.
		for range results {
		}
	}()

	go func() {
		src.ListRepos(ctx, results)
		close(results)
	}()

	select {
	case res := <-results:
		// As soon as we get the first result back, we've got what we need to validate the external service.
		return res.Err
	case <-ctx.Done():
		return ctx.Err()
	}
}

var mockRepoLookup func(protocol.RepoLookupArgs) (*protocol.RepoLookupResult, error)

func (s *Server) repoLookup(ctx context.Context, args protocol.RepoLookupArgs) (result *protocol.RepoLookupResult, err error) {
	// Sourcegraph.com: this is on the user path, do not block forever if codehost is
	// being bad. Ideally block before cloudflare 504s the request (1min). Other: we
	// only speak to our database, so response should be in a few ms.
	ctx, cancel := context.WithTimeout(ctx, 30*time.Second)
	defer cancel()

	tr, ctx := trace.New(ctx, "repoLookup", args.String())
	defer func() {
		s.Logger.Debug("repoLookup", log.String("result", fmt.Sprint(result)), log.Error(err))
		if result != nil {
			tr.LazyPrintf("result: %s", result)
		}
		tr.SetError(err)
		tr.Finish()
	}()

	if args.Repo == "" {
		return nil, errors.New("Repo must be set (is blank)")
	}

	if mockRepoLookup != nil {
		return mockRepoLookup(args)
	}

	repo, err := s.Syncer.SyncRepo(ctx, args.Repo, true)

	switch {
	case err == nil:
		break
	case errcode.IsNotFound(err):
		return &protocol.RepoLookupResult{ErrorNotFound: true}, nil
	case errcode.IsUnauthorized(err) || errcode.IsForbidden(err):
		return &protocol.RepoLookupResult{ErrorUnauthorized: true}, nil
	case errcode.IsTemporary(err):
		return &protocol.RepoLookupResult{ErrorTemporarilyUnavailable: true}, nil
	default:
		return nil, err
	}

	if s.Scheduler != nil && args.Update {
		// Enqueue a high priority update for this repo.
		s.Scheduler.UpdateOnce(repo.ID, repo.Name)
	}

	repoInfo := protocol.NewRepoInfo(repo)

	return &protocol.RepoLookupResult{Repo: repoInfo}, nil
}

func (s *Server) handleEnqueueChangesetSync(w http.ResponseWriter, r *http.Request) {
	if s.ChangesetSyncRegistry == nil {
		s.Logger.Warn("ChangesetSyncer is nil")
		s.respond(w, http.StatusForbidden, nil)
		return
	}

	var req protocol.ChangesetSyncRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		s.respond(w, http.StatusBadRequest, err)
		return
	}
	if len(req.IDs) == 0 {
		s.respond(w, http.StatusBadRequest, errors.New("no ids provided"))
		return
	}
	err := s.ChangesetSyncRegistry.EnqueueChangesetSyncs(r.Context(), req.IDs)
	if err != nil {
		resp := protocol.ChangesetSyncResponse{Error: err.Error()}
		s.respond(w, http.StatusInternalServerError, resp)
		return
	}
	s.respond(w, http.StatusOK, nil)
}

// TODO(naman): remove this while removing old perms syncer
func (s *Server) handleSchedulePermsSync(w http.ResponseWriter, r *http.Request) {
	if s.DatabaseBackedPermissionSyncerEnabled != nil && s.DatabaseBackedPermissionSyncerEnabled(r.Context()) {
		s.Logger.Warn("Dropping schedule-perms-sync request because PermissionSyncWorker is enabled. This should not happen.")
		s.respond(w, http.StatusOK, nil)
		return
	}

	if s.PermsSyncer == nil {
		s.respond(w, http.StatusForbidden, nil)
		return
	}

	var req protocol.PermsSyncRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		s.respond(w, http.StatusBadRequest, err)
		return
	}
	if len(req.UserIDs) == 0 && len(req.RepoIDs) == 0 {
		s.respond(w, http.StatusBadRequest, errors.New("neither user IDs nor repo IDs was provided in request (must provide at least one)"))
		return
	}

	s.PermsSyncer.ScheduleUsers(r.Context(), req.Options, req.UserIDs...)
	s.PermsSyncer.ScheduleRepos(r.Context(), req.RepoIDs...)

	s.respond(w, http.StatusOK, nil)
}

func (s *Server) handleExternalServiceNamespaces(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithCancel(r.Context())
	defer cancel()

	var req protocol.ExternalServiceNamespacesArgs
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	externalSvc := &types.ExternalService{
		Kind:   req.Kind,
		Config: extsvc.NewUnencryptedConfig(req.Config),
	}

	logger := s.Logger.With(log.String("ExternalServiceKind", req.Kind))

	var result *protocol.ExternalServiceNamespacesResult

	genericSourcer := s.NewGenericSourcer(logger)
	genericSrc, err := genericSourcer(ctx, externalSvc)
	if err != nil {
		logger.Error("server.query-external-service-namespaces", log.Error(err))
		result = &protocol.ExternalServiceNamespacesResult{Error: err.Error()}
		s.respond(w, http.StatusBadRequest, result)
		return
	}

	if err = genericSrc.CheckConnection(ctx); err != nil {
		result = &protocol.ExternalServiceNamespacesResult{Error: err.Error()}
		s.respond(w, http.StatusServiceUnavailable, result)
		return
	}

	discoverableSrc, ok := genericSrc.(repos.DiscoverableSource)

	if !ok {
		result = &protocol.ExternalServiceNamespacesResult{Error: repos.UnimplementedDiscoverySource}
		s.respond(w, http.StatusNotImplemented, result)
		return
	}

	results := make(chan repos.SourceNamespaceResult)

	go func() {
		discoverableSrc.ListNamespaces(ctx, results)
		close(results)
	}()

	var sourceErrs error
	namespaces := make([]*types.ExternalServiceNamespace, 0)

	for res := range results {
		if res.Err != nil {
			sourceErrs = errors.Append(sourceErrs, &repos.SourceError{Err: res.Err, ExtSvc: externalSvc})
			continue
		}
		namespaces = append(namespaces, res.Namespace)
	}

	if sourceErrs != nil {
		result = &protocol.ExternalServiceNamespacesResult{Namespaces: namespaces, Error: sourceErrs.Error()}
	} else {
		result = &protocol.ExternalServiceNamespacesResult{Namespaces: namespaces}
	}
	s.respond(w, http.StatusOK, result)
}

func (s *Server) handleExternalServiceRepositories(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithCancel(r.Context())
	defer cancel()

	var req protocol.ExternalServiceRepositoriesArgs
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	externalSvc := &types.ExternalService{
		Kind:   req.Kind,
		Config: extsvc.NewUnencryptedConfig(req.Config),
	}

	logger := s.Logger.With(log.String("ExternalServiceKind", req.Kind))

	var result *protocol.ExternalServiceRepositoriesResult

	genericSourcer := s.NewGenericSourcer(logger)
	genericSrc, err := genericSourcer(ctx, externalSvc)
	if err != nil {
		logger.Error("server.query-external-service-repositories", log.Error(err))
		result = &protocol.ExternalServiceRepositoriesResult{Error: err.Error()}
		s.respond(w, http.StatusBadRequest, result)
		return
	}

	if err = genericSrc.CheckConnection(ctx); err != nil {
		result = &protocol.ExternalServiceRepositoriesResult{Error: err.Error()}
		s.respond(w, http.StatusServiceUnavailable, result)
		return
	}

	discoverableSrc, ok := genericSrc.(repos.DiscoverableSource)
	if !ok {
		result = &protocol.ExternalServiceRepositoriesResult{Error: repos.UnimplementedDiscoverySource}
		s.respond(w, http.StatusNotImplemented, result)
		return
	}

	results := make(chan repos.SourceResult)

	first := int(req.First)
	if first > 100 {
		first = 100
	}

	go func() {
		discoverableSrc.SearchRepositories(ctx, req.Query, first, req.ExcludeRepos, results)
		close(results)
	}()

	var sourceErrs error
	repositories := make([]*types.Repo, 0)

	for res := range results {
		if res.Err != nil {
			sourceErrs = errors.Append(sourceErrs, &repos.SourceError{Err: res.Err, ExtSvc: externalSvc})
			continue
		}
		repositories = append(repositories, res.Repo)
	}

	if sourceErrs != nil {
		result = &protocol.ExternalServiceRepositoriesResult{Repos: repositories, Error: sourceErrs.Error()}
	} else {
		result = &protocol.ExternalServiceRepositoriesResult{Repos: repositories}
	}
	s.respond(w, http.StatusOK, result)
}

var mockNewGenericSourcer func() repos.Sourcer

func (s *Server) NewGenericSourcer(logger log.Logger) repos.Sourcer {
	if mockNewGenericSourcer != nil {
		return mockNewGenericSourcer()
	}

	// We use the generic sourcer that doesn't have observability attached to it here because the way externalServiceValidate is set up,
	// using the regular sourcer will cause a large dump of errors to be logged when it exits ListRepos prematurely.
	sourcerLogger := logger.Scoped("repos.Sourcer", "repositories source")
	db := database.NewDBWith(sourcerLogger.Scoped("db", "sourcer database"), s)
	dependenciesService := dependencies.NewService(s.ObservationCtx, db)
	cf := httpcli.NewExternalClientFactory(httpcli.NewLoggingMiddleware(sourcerLogger))
	return repos.NewSourcer(sourcerLogger, db, cf, repos.WithDependenciesService(dependenciesService))
}
