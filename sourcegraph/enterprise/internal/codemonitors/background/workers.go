package background

import (
	"context"
	"fmt"
	"time"

	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codemonitors"
	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/featureflag"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/search/job/jobutil"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/settings"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	"github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker"
	dbworkerstore "github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const (
	eventRetentionInDays int = 30
)

func newTriggerQueryRunner(ctx context.Context, observationCtx *observation.Context, db edb.EnterpriseDB, enterpriseJobs jobutil.EnterpriseJobs, metrics codeMonitorsMetrics) *workerutil.Worker[*edb.TriggerJob] {
	options := workerutil.WorkerOptions{
		Name:                 "code_monitors_trigger_jobs_worker",
		Description:          "runs trigger queries for code monitors",
		NumHandlers:          4,
		Interval:             5 * time.Second,
		HeartbeatInterval:    15 * time.Second,
		Metrics:              metrics.workerMetrics,
		MaximumRuntimePerJob: time.Minute,
	}

	store := createDBWorkerStoreForTriggerJobs(observationCtx, db)

	worker := dbworker.NewWorker[*edb.TriggerJob](ctx, store, &queryRunner{db: db, enterpriseJobs: enterpriseJobs}, options)
	return worker
}

func newTriggerQueryEnqueuer(ctx context.Context, store edb.CodeMonitorStore) goroutine.BackgroundRoutine {
	enqueueActive := goroutine.HandlerFunc(

		func(ctx context.Context) error {
			_, err := store.EnqueueQueryTriggerJobs(ctx)
			return err
		})
	return goroutine.NewPeriodicGoroutine(
		ctx,
		enqueueActive,
		goroutine.WithName("code_monitors.trigger_query_enqueuer"),
		goroutine.WithDescription("enqueues code monitor trigger query jobs"),
		goroutine.WithInterval(1*time.Minute),
	)
}

func newTriggerQueryResetter(_ context.Context, observationCtx *observation.Context, s edb.CodeMonitorStore, metrics codeMonitorsMetrics) *dbworker.Resetter[*edb.TriggerJob] {
	workerStore := createDBWorkerStoreForTriggerJobs(observationCtx, s)

	options := dbworker.ResetterOptions{
		Name:     "code_monitors_trigger_jobs_worker_resetter",
		Interval: 1 * time.Minute,
		Metrics: dbworker.ResetterMetrics{
			Errors:              metrics.errors,
			RecordResetFailures: metrics.resetFailures,
			RecordResets:        metrics.resets,
		},
	}
	return dbworker.NewResetter(observationCtx.Logger, workerStore, options)
}

func newTriggerJobsLogDeleter(ctx context.Context, store edb.CodeMonitorStore) goroutine.BackgroundRoutine {
	deleteLogs := goroutine.HandlerFunc(
		func(ctx context.Context) error {
			return store.DeleteOldTriggerJobs(ctx, eventRetentionInDays)
		})
	return goroutine.NewPeriodicGoroutine(
		ctx,
		deleteLogs,
		goroutine.WithName("code_monitors.trigger_jobs_log_deleter"),
		goroutine.WithDescription("deletes code job logs from code monitor triggers"),
		goroutine.WithInterval(60*time.Minute),
	)
}

func newActionRunner(ctx context.Context, observationCtx *observation.Context, s edb.CodeMonitorStore, metrics codeMonitorsMetrics) *workerutil.Worker[*edb.ActionJob] {
	options := workerutil.WorkerOptions{
		Name:              "code_monitors_action_jobs_worker",
		Description:       "runs actions for code monitors",
		NumHandlers:       1,
		Interval:          5 * time.Second,
		HeartbeatInterval: 15 * time.Second,
		Metrics:           metrics.workerMetrics,
	}

	store := createDBWorkerStoreForActionJobs(observationCtx, s)

	worker := dbworker.NewWorker[*edb.ActionJob](ctx, store, &actionRunner{s}, options)
	return worker
}

func newActionJobResetter(_ context.Context, observationCtx *observation.Context, s edb.CodeMonitorStore, metrics codeMonitorsMetrics) *dbworker.Resetter[*edb.ActionJob] {
	workerStore := createDBWorkerStoreForActionJobs(observationCtx, s)

	options := dbworker.ResetterOptions{
		Name:     "code_monitors_action_jobs_worker_resetter",
		Interval: 1 * time.Minute,
		Metrics: dbworker.ResetterMetrics{
			Errors:              metrics.errors,
			RecordResetFailures: metrics.resetFailures,
			RecordResets:        metrics.resets,
		},
	}
	return dbworker.NewResetter(observationCtx.Logger, workerStore, options)
}

func createDBWorkerStoreForTriggerJobs(observationCtx *observation.Context, s basestore.ShareableStore) dbworkerstore.Store[*edb.TriggerJob] {
	observationCtx = observation.ContextWithLogger(observationCtx.Logger.Scoped("triggerJobs.dbworker.Store", ""), observationCtx)

	return dbworkerstore.New(observationCtx, s.Handle(), dbworkerstore.Options[*edb.TriggerJob]{
		Name:              "code_monitors_trigger_jobs_worker_store",
		TableName:         "cm_trigger_jobs",
		ColumnExpressions: edb.TriggerJobsColumns,
		Scan:              dbworkerstore.BuildWorkerScan(edb.ScanTriggerJob),
		StalledMaxAge:     60 * time.Second,
		RetryAfter:        10 * time.Second,
		MaxNumRetries:     3,
		OrderByExpression: sqlf.Sprintf("id"),
	})
}

func createDBWorkerStoreForActionJobs(observationCtx *observation.Context, s edb.CodeMonitorStore) dbworkerstore.Store[*edb.ActionJob] {
	observationCtx = observation.ContextWithLogger(observationCtx.Logger.Scoped("actionJobs.dbworker.Store", ""), observationCtx)

	return dbworkerstore.New(observationCtx, s.Handle(), dbworkerstore.Options[*edb.ActionJob]{
		Name:              "code_monitors_action_jobs_worker_store",
		TableName:         "cm_action_jobs",
		ColumnExpressions: edb.ActionJobColumns,
		Scan:              dbworkerstore.BuildWorkerScan(edb.ScanActionJob),
		StalledMaxAge:     60 * time.Second,
		RetryAfter:        10 * time.Second,
		MaxNumRetries:     3,
		OrderByExpression: sqlf.Sprintf("id"),
	})
}

type queryRunner struct {
	db             edb.EnterpriseDB
	enterpriseJobs jobutil.EnterpriseJobs
}

func (r *queryRunner) Handle(ctx context.Context, logger log.Logger, triggerJob *edb.TriggerJob) (err error) {
	defer func() {
		if err != nil {
			logger.Error("queryRunner.Handle", log.Error(err))
		}
	}()

	cm := r.db.CodeMonitors()

	q, err := cm.GetQueryTriggerForJob(ctx, triggerJob.ID)
	if err != nil {
		return err
	}

	m, err := cm.GetMonitor(ctx, q.Monitor)
	if err != nil {
		return err
	}

	// SECURITY: set the actor to the user that owns the code monitor.
	// For all downstream actions (specifically executing searches),
	// we should run as the user who owns the code monitor.
	ctx = actor.WithActor(ctx, actor.FromUser(m.UserID))
	ctx = featureflag.WithFlags(ctx, r.db.FeatureFlags())

	settings, err := settings.CurrentUserFinal(ctx, r.db)
	if err != nil {
		return errors.Wrap(err, "query settings")
	}

	results, searchErr := codemonitors.Search(ctx, logger, r.db, r.enterpriseJobs, q.QueryString, m.ID, settings)

	// Log next_run and latest_result to table cm_queries.
	newLatestResult := latestResultTime(q.LatestResult, results, searchErr)
	err = cm.SetQueryTriggerNextRun(ctx, q.ID, cm.Clock()().Add(5*time.Minute), newLatestResult.UTC())
	if err != nil {
		return err
	}

	// After setting the next run, check the error value
	if searchErr != nil {
		return errors.Wrap(searchErr, "execute search")
	}

	// Log the actual query we ran and whether we got any new results.
	err = cm.UpdateTriggerJobWithResults(ctx, triggerJob.ID, q.QueryString, results)
	if err != nil {
		return errors.Wrap(err, "UpdateTriggerJobWithResults")
	}

	if len(results) > 0 {
		_, err := cm.EnqueueActionJobsForMonitor(ctx, m.ID, triggerJob.ID)
		if err != nil {
			return errors.Wrap(err, "store.EnqueueActionJobsForQuery")
		}
	}
	return nil
}

type actionRunner struct {
	edb.CodeMonitorStore
}

func (r *actionRunner) Handle(ctx context.Context, logger log.Logger, j *edb.ActionJob) (err error) {
	logger.Info("actionRunner.Handle starting")
	defer func() {
		if err != nil {
			logger.Error("actionRunner.Handle", log.Error(err))
		}
	}()

	switch {
	case j.Email != nil:
		return r.handleEmail(ctx, j)
	case j.Webhook != nil:
		return r.handleWebhook(ctx, j)
	case j.SlackWebhook != nil:
		return r.handleSlackWebhook(ctx, j)
	default:
		return errors.New("job must be one of type email, webhook, or slack webhook")
	}
}

func (r *actionRunner) handleEmail(ctx context.Context, j *edb.ActionJob) error {
	s, err := r.CodeMonitorStore.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = s.Done(err) }()

	m, err := s.GetActionJobMetadata(ctx, j.ID)
	if err != nil {
		return errors.Wrap(err, "GetActionJobMetadata")
	}

	e, err := s.GetEmailAction(ctx, *j.Email)
	if err != nil {
		return errors.Wrap(err, "GetEmailAction")
	}

	recs, err := s.ListRecipients(ctx, edb.ListRecipientsOpts{EmailID: j.Email})
	if err != nil {
		return errors.Wrap(err, "ListRecipients")
	}

	externalURL, err := getExternalURL(ctx)
	if err != nil {
		return err
	}

	args := actionArgs{
		MonitorDescription: m.Description,
		MonitorID:          m.MonitorID,
		ExternalURL:        externalURL,
		UTMSource:          utmSourceEmail,
		Query:              m.Query,
		MonitorOwnerName:   m.OwnerName,
		Results:            m.Results,
		IncludeResults:     e.IncludeResults,
	}

	data, err := NewTemplateDataForNewSearchResults(args, e)
	if err != nil {
		return errors.Wrap(err, "NewTemplateDataForNewSearchResults")
	}
	for _, rec := range recs {
		if rec.NamespaceOrgID != nil {
			// TODO (stefan): Send emails to org members.
			continue
		}
		if rec.NamespaceUserID == nil {
			return errors.New("nil recipient")
		}
		err = SendEmailForNewSearchResult(ctx, database.NewDBWith(log.Scoped("handleEmail", ""), r.CodeMonitorStore), *rec.NamespaceUserID, data)
		if err != nil {
			return err
		}
	}
	return nil
}

func (r *actionRunner) handleWebhook(ctx context.Context, j *edb.ActionJob) error {
	s, err := r.CodeMonitorStore.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = s.Done(err) }()

	m, err := s.GetActionJobMetadata(ctx, j.ID)
	if err != nil {
		return errors.Wrap(err, "GetActionJobMetadata")
	}

	w, err := s.GetWebhookAction(ctx, *j.Webhook)
	if err != nil {
		return errors.Wrap(err, "GetWebhookAction")
	}

	externalURL, err := getExternalURL(ctx)
	if err != nil {
		return err
	}

	args := actionArgs{
		MonitorDescription: m.Description,
		MonitorID:          w.Monitor,
		ExternalURL:        externalURL,
		UTMSource:          "code-monitor-webhook",
		Query:              m.Query,
		MonitorOwnerName:   m.OwnerName,
		Results:            m.Results,
		IncludeResults:     w.IncludeResults,
	}

	return sendWebhookNotification(ctx, w.URL, args)
}

func (r *actionRunner) handleSlackWebhook(ctx context.Context, j *edb.ActionJob) error {
	s, err := r.CodeMonitorStore.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = s.Done(err) }()

	m, err := s.GetActionJobMetadata(ctx, j.ID)
	if err != nil {
		return errors.Wrap(err, "GetActionJobMetadata")
	}

	w, err := s.GetSlackWebhookAction(ctx, *j.SlackWebhook)
	if err != nil {
		return errors.Wrap(err, "GetSlackWebhookAction")
	}

	externalURL, err := getExternalURL(ctx)
	if err != nil {
		return err
	}

	args := actionArgs{
		MonitorDescription: m.Description,
		MonitorID:          w.Monitor,
		ExternalURL:        externalURL,
		UTMSource:          "code-monitor-slack-webhook",
		Query:              m.Query,
		MonitorOwnerName:   m.OwnerName,
		Results:            m.Results,
		IncludeResults:     w.IncludeResults,
	}

	return sendSlackNotification(ctx, w.URL, args)
}

type StatusCodeError struct {
	Code   int
	Status string
	Body   string
}

func (s StatusCodeError) Error() string {
	return fmt.Sprintf("non-200 response %d %s with body %q", s.Code, s.Status, s.Body)
}

func latestResultTime(previousLastResult *time.Time, results []*result.CommitMatch, searchErr error) time.Time {
	if searchErr != nil || len(results) == 0 {
		// Error performing the search, or there were no results. Assume the
		// previous info's result time.
		if previousLastResult != nil {
			return *previousLastResult
		}
		return time.Now()
	}

	if results[0].Commit.Committer != nil {
		return results[0].Commit.Committer.Date
	}
	return time.Now()
}
