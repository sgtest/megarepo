package permissions

import (
	"context"
	"database/sql"
	"fmt"
	"net/url"
	"sort"
	"time"

	"github.com/keegancsmith/sqlf"
	"github.com/opentracing/opentracing-go"
	"github.com/prometheus/client_golang/prometheus"

	"github.com/sourcegraph/log"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/globals"
	"github.com/sourcegraph/sourcegraph/cmd/worker/job"
	workerdb "github.com/sourcegraph/sourcegraph/cmd/worker/shared/init/db"
	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/conf/reposource"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/bitbucketserver"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	"github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker"
	dbworkerstore "github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

// bitbucketProjectPermissionsJob implements the job.Job interface. It is used by the worker service
// to spawn a new background worker.
type bitbucketProjectPermissionsJob struct{}

// NewBitbucketProjectPermissionsJob creates a new job for applying explicit permissions
// to all the repositories of a Bitbucket Project.
func NewBitbucketProjectPermissionsJob() job.Job {
	return &bitbucketProjectPermissionsJob{}
}

func (j *bitbucketProjectPermissionsJob) Description() string {
	return "Applies explicit permissions to all repositories of a Bitbucket Project."
}

func (j *bitbucketProjectPermissionsJob) Config() []env.Config {
	return []env.Config{ConfigInst}
}

// Routines is called by the worker service to start the worker.
// It returns a list of goroutines that the worker service should start and manage.
func (j *bitbucketProjectPermissionsJob) Routines(_ context.Context, logger log.Logger) ([]goroutine.BackgroundRoutine, error) {
	wdb, err := workerdb.Init()
	if err != nil {
		return nil, err
	}
	db := edb.NewEnterpriseDB(database.NewDB(logger, wdb))

	bbProjectMetrics := newMetricsForBitbucketProjectPermissionsQueries(logger)

	return []goroutine.BackgroundRoutine{
		newBitbucketProjectPermissionsWorker(db, ConfigInst, bbProjectMetrics),
		newBitbucketProjectPermissionsResetter(db, ConfigInst, bbProjectMetrics),
	}, nil
}

// bitbucketProjectPermissionsHandler handles the execution of a single explicit_permissions_bitbucket_projects_jobs record.
type bitbucketProjectPermissionsHandler struct {
	db     edb.EnterpriseDB
	client *bitbucketserver.Client
}

// Handle implements the workerutil.Handler interface.
func (h *bitbucketProjectPermissionsHandler) Handle(ctx context.Context, logger log.Logger, record workerutil.Record) (err error) {
	logger = logger.Scoped("bitbucketProjectPermissionsHandler", "handles jobs to apply explicit permissions to all repositories of a Bitbucket Project")
	defer func() {
		if err != nil {
			logger.Error("bitbucketProjectPermissionsHandler.Handle", log.Error(err))
		}
	}()

	workerJob := record.(*types.BitbucketProjectPermissionJob)

	// get the external service
	svc, err := h.db.ExternalServices().GetByID(ctx, workerJob.ExternalServiceID)
	if err != nil {
		return errcode.MakeNonRetryable(errors.Wrapf(err, "failed to get external service %d", workerJob.ExternalServiceID))
	}

	if svc.Kind != extsvc.KindBitbucketServer {
		return errcode.MakeNonRetryable(errors.Newf("expected Bitbucket Server external service, got: %s", svc.Kind))
	}

	// get repos from the Bitbucket project
	client, err := h.getBitbucketClient(svc)
	if err != nil {
		return errors.Wrapf(err, "failed to build Bitbucket client for external service %d", svc.ID)
	}

	projectKey := workerJob.ProjectKey

	// These repos are fetched from Bitbucket, therefore their IDs are Bitbucket IDs
	// and we need to search for these repos in frontend DB using the name
	bitbucketRepos, err := client.ProjectRepos(ctx, projectKey)
	if err != nil {
		return errors.Wrapf(err, "failed to list repositories of Bitbucket Project %q", projectKey)
	}

	repoIDs, err := h.getRepoIDsByNames(ctx, svc, projectKey, bitbucketRepos)
	if err != nil {
		return errors.Wrap(err, "failed to get gitserver repos from the database")
	}

	if workerJob.Unrestricted {
		return h.setReposUnrestricted(ctx, logger, repoIDs, projectKey)
	}

	err = h.setPermissionsForUsers(ctx, svc, logger, workerJob.Permissions, repoIDs, projectKey)
	if err != nil {
		return errors.Wrapf(err, "failed to set permissions for Bitbucket Project %q", projectKey)
	}

	return nil
}

// getBitbucketClient creates a Bitbucket client for the given external service.
func (h *bitbucketProjectPermissionsHandler) getBitbucketClient(svc *types.ExternalService) (*bitbucketserver.Client, error) {
	// for testing purpose
	if h.client != nil {
		return h.client, nil
	}

	var c schema.BitbucketServerConnection
	if err := jsonc.Unmarshal(svc.Config, &c); err != nil {
		return nil, errors.Errorf("external service id=%d config error: %s", svc.ID, err)
	}

	var opts []httpcli.Opt
	if c.Certificate != "" {
		opts = append(opts, httpcli.NewCertPoolOpt(c.Certificate))
	}

	cli, err := httpcli.ExternalClientFactory.Doer(opts...)
	if err != nil {
		return nil, err
	}

	return bitbucketserver.NewClient(svc.URN(), &c, cli)
}

func (h *bitbucketProjectPermissionsHandler) setReposUnrestricted(ctx context.Context, logger log.Logger, repoIDs []api.RepoID, projectKey string) error {
	sort.Slice(repoIDs, func(i, j int) bool {
		return repoIDs[i] < repoIDs[j]
	})

	// converting api.RepoID to int32
	repoIntIDs := make([]int32, len(repoIDs))
	for i, id := range repoIDs {
		repoIntIDs[i] = int32(id)
	}

	logger.Info("Setting bitbucket repositories to unrestricted", log.String("project_key", projectKey))

	err := h.db.Perms().SetRepoPermissionsUnrestricted(ctx, repoIntIDs, true)
	if err != nil {
		return errors.Wrapf(err, "failed to set permissions to unrestricted for Bitbucket Project %q", projectKey)
	}

	return nil
}

// getRepoIDsByNames queries repo IDs from frontend database using repo names fetched from
// Bitbucket code host.
func (h *bitbucketProjectPermissionsHandler) getRepoIDsByNames(ctx context.Context, svc *types.ExternalService, projectKey string, repos []*bitbucketserver.Repo) ([]api.RepoID, error) {
	count := len(repos)
	IDs := make([]api.RepoID, 0, count)
	if count == 0 {
		return IDs, nil
	}

	// unmarshalling external service config
	var cfg schema.BitbucketServerConnection
	if err := jsonc.Unmarshal(svc.Config, &cfg); err != nil {
		return nil, errors.Errorf("external service id=%d config error: %s", svc.ID, err)
	}

	// parsing the hostname from the URL
	parsedURL, err := url.Parse(cfg.Url)
	if err != nil {
		return nil, errors.Errorf("error during parsing external service URL", err)
	}
	hostname := parsedURL.Hostname()

	names := make([]api.RepoName, 0, count)
	for _, repo := range repos {
		// this is how repo names are composed before creating repos in `repo` table
		// we are reconstructing this name for successful pattern matching
		name := reposource.BitbucketServerRepoName(
			cfg.RepositoryPathPattern,
			hostname,
			projectKey,
			repo.Slug,
		)

		names = append(names, name)
	}

	// searching for repos by names
	gitserverRepos, err := h.db.GitserverRepos().GetByNames(ctx, names...)
	if err != nil {
		return nil, err
	}

	// mapping repos to repo IDs
	for _, gitserverRepo := range gitserverRepos {
		IDs = append(IDs, gitserverRepo.RepoID)
	}

	return IDs, nil
}

// setPermissionsForUsers applies user permissions to a list of repos.
// It updates the repo_permissions, user_permissions, repo_pending_permissions and user_pending_permissions table.
// Each repo is processed atomically. In case of error, the task fails but doesn't rollback the committed changes
// done on previous repos. This is fine because when the task is retried, previous repos won't incur any
// additional writes.
func (h *bitbucketProjectPermissionsHandler) setPermissionsForUsers(ctx context.Context, svc *types.ExternalService, logger log.Logger, perms []types.UserPermission, repoIDs []api.RepoID, projectKey string) error {
	sort.Slice(perms, func(i, j int) bool {
		return perms[i].BindID < perms[j].BindID
	})
	sort.Slice(repoIDs, func(i, j int) bool {
		return repoIDs[i] < repoIDs[j]
	})

	bindIDs := make([]string, 0, len(perms))
	for _, up := range perms {
		bindIDs = append(bindIDs, up.BindID)
	}

	// bind the bindIDs to actual user IDs
	mapping, err := h.db.Perms().MapUsers(ctx, bindIDs, globals.PermissionsUserMapping())
	if err != nil {
		return errors.Wrap(err, "failed to map bind IDs to user IDs")
	}

	userIDs := make(map[int32]struct{}, len(mapping))
	for _, id := range mapping {
		userIDs[id] = struct{}{}
	}

	// determine which users don't exist yet
	pendingBindIDs := make([]string, 0, len(bindIDs))
	for _, bindID := range bindIDs {
		if _, ok := mapping[bindID]; !ok {
			pendingBindIDs = append(pendingBindIDs, bindID)
		}
	}

	logger.Info("Applying permissions to Bitbucket project repositories",
		log.String("project_key", projectKey),
		log.Int("repo_ids_len", len(repoIDs)),
		log.Int("user_ids_len", len(userIDs)),
		log.Int("pending_bind_ids_len", len(pendingBindIDs)),
	)

	// apply the permissions for each repo
	for _, repoID := range repoIDs {
		err = h.setRepoPermissions(ctx, svc, repoID, perms, userIDs, pendingBindIDs)
		if err != nil {
			return errors.Wrapf(err, "failed to set permissions for repo %d", repoID)
		}
	}

	return nil
}

func (h *bitbucketProjectPermissionsHandler) setRepoPermissions(ctx context.Context, svc *types.ExternalService, repoID api.RepoID, _ []types.UserPermission, userIDs map[int32]struct{}, pendingBindIDs []string) (err error) {
	// Make sure the repo ID is valid.
	if err := h.repoExists(ctx, repoID); err != nil {
		return errcode.MakeNonRetryable(errors.Wrapf(err, "failed to query repo %d", repoID))
	}

	p := authz.RepoPermissions{
		RepoID:  int32(repoID),
		Perm:    authz.Read, // Note: We currently only support read for repository permissions.
		UserIDs: userIDs,
	}

	txs, err := h.db.Perms().Transact(ctx)
	if err != nil {
		return errors.Wrap(err, "failed to start transaction")
	}
	defer func() { err = txs.Done(err) }()

	accounts := &extsvc.Accounts{
		ServiceType: authz.SourcegraphServiceType,
		ServiceID:   authz.SourcegraphServiceID,
		AccountIDs:  pendingBindIDs,
	}

	// make sure the repo is not unrestricted
	err = txs.SetRepoPermissionsUnrestricted(ctx, []int32{int32(repoID)}, false)
	if err != nil {
		return errors.Wrapf(err, "failed to set repo %d to restricted", repoID)
	}

	// set repo permissions (and user permissions)
	err = txs.SetRepoPermissions(ctx, &p)
	if err != nil {
		return errors.Wrapf(err, "failed to set repo permissions for repo %d", repoID)
	}

	// set pending permissions
	err = txs.SetRepoPendingPermissions(ctx, accounts, &p)
	if err != nil {
		return errors.Wrapf(err, "failed to set pending permissions for repo %d", repoID)
	}

	return nil
}

func (h *bitbucketProjectPermissionsHandler) repoExists(ctx context.Context, repoID api.RepoID) (err error) {
	var id int
	if err := h.db.QueryRowContext(ctx, fmt.Sprintf("SELECT id FROM repo WHERE id = %d", repoID)).Scan(&id); err != nil {
		if err == sql.ErrNoRows {
			return errors.New("repo not found")
		}
		return err
	}
	return nil
}

// newBitbucketProjectPermissionsWorker creates a worker that reads the explicit_permissions_bitbucket_projects_jobs table and
// executes the jobs.
func newBitbucketProjectPermissionsWorker(db edb.EnterpriseDB, cfg *config, metrics bitbucketProjectPermissionsMetrics) *workerutil.Worker {
	options := workerutil.WorkerOptions{
		Name:              "explicit_permissions_bitbucket_projects_jobs_worker",
		NumHandlers:       cfg.WorkerConcurrency,
		Interval:          cfg.WorkerPollInterval,
		HeartbeatInterval: 15 * time.Second,
		Metrics:           metrics.workerMetrics,
	}

	return dbworker.NewWorker(context.Background(), createBitbucketProjectPermissionsStore(db, cfg), &bitbucketProjectPermissionsHandler{db: db}, options)
}

// newBitbucketProjectPermissionsResetter implements resetter for the explicit_permissions_bitbucket_projects_jobs table.
// See resetter documentation for more details. https://docs.sourcegraph.com/dev/background-information/workers#dequeueing-and-resetting-jobs
func newBitbucketProjectPermissionsResetter(db edb.EnterpriseDB, cfg *config, metrics bitbucketProjectPermissionsMetrics) *dbworker.Resetter {
	workerStore := createBitbucketProjectPermissionsStore(db, cfg)

	options := dbworker.ResetterOptions{
		Name:     "explicit_permissions_bitbucket_projects_jobs_worker_resetter",
		Interval: 1 * time.Minute,
		Metrics: dbworker.ResetterMetrics{
			Errors:              metrics.errors,
			RecordResetFailures: metrics.resetFailures,
			RecordResets:        metrics.resets,
		},
	}
	return dbworker.NewResetter(workerStore, options)
}

// createBitbucketProjectPermissionsStore creates a store that reads and writes to the explicit_permissions_bitbucket_projects_jobs table.
// It is used by the worker and resetter.
func createBitbucketProjectPermissionsStore(s basestore.ShareableStore, cfg *config) dbworkerstore.Store {
	return dbworkerstore.New(s.Handle(), dbworkerstore.Options{
		Name:              "explicit_permissions_bitbucket_projects_jobs_store",
		TableName:         "explicit_permissions_bitbucket_projects_jobs",
		ColumnExpressions: database.BitbucketProjectPermissionsColumnExpressions,
		Scan: func(rows *sql.Rows, err error) (workerutil.Record, bool, error) {
			j, ok, err := database.ScanFirstBitbucketProjectPermissionsJob(rows, err)
			return j, ok, err
		},
		StalledMaxAge:     60 * time.Second,
		RetryAfter:        cfg.WorkerRetryInterval,
		MaxNumRetries:     5,
		OrderByExpression: sqlf.Sprintf("explicit_permissions_bitbucket_projects_jobs.id"),
	})
}

// These are the metrics that are used by the worker and resetter.
// They are required by the workerutil package for automatic metrics collection.
type bitbucketProjectPermissionsMetrics struct {
	workerMetrics workerutil.WorkerMetrics
	resets        prometheus.Counter
	resetFailures prometheus.Counter
	errors        prometheus.Counter
}

func newMetricsForBitbucketProjectPermissionsQueries(logger log.Logger) bitbucketProjectPermissionsMetrics {
	observationContext := &observation.Context{
		Logger:     logger.Scoped("routines", "bitbucket projects explicit permissions job routines"),
		Tracer:     &trace.Tracer{Tracer: opentracing.GlobalTracer()},
		Registerer: prometheus.DefaultRegisterer,
	}

	resetFailures := prometheus.NewCounter(prometheus.CounterOpts{
		Name: "src_explicit_permissions_bitbucket_project_query_reset_failures_total",
		Help: "The number of reset failures.",
	})
	observationContext.Registerer.MustRegister(resetFailures)

	resets := prometheus.NewCounter(prometheus.CounterOpts{
		Name: "src_explicit_permissions_bitbucket_project_query_resets_total",
		Help: "The number of records reset.",
	})
	observationContext.Registerer.MustRegister(resets)

	errors := prometheus.NewCounter(prometheus.CounterOpts{
		Name: "src_explicit_permissions_bitbucket_project_query_errors_total",
		Help: "The number of errors that occur during job.",
	})
	observationContext.Registerer.MustRegister(errors)

	return bitbucketProjectPermissionsMetrics{
		workerMetrics: workerutil.NewMetrics(observationContext, "explicit_permissions_bitbucket_project_queries"),
		resets:        resets,
		resetFailures: resetFailures,
		errors:        errors,
	}
}
