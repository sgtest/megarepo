package database

import (
	"context"
	"strconv"
	"strings"
	"time"

	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"

	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/executor"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/lib/errors"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
)

const CancellationReasonHigherPriority = "A job with higher priority was added."

type PermissionsSyncJobState string

// PermissionsSyncJobState constants.
const (
	PermissionsSyncJobStateQueued     PermissionsSyncJobState = "queued"
	PermissionsSyncJobStateProcessing PermissionsSyncJobState = "processing"
	PermissionsSyncJobStateErrored    PermissionsSyncJobState = "errored"
	PermissionsSyncJobStateFailed     PermissionsSyncJobState = "failed"
	PermissionsSyncJobStateCompleted  PermissionsSyncJobState = "completed"
	PermissionsSyncJobStateCanceled   PermissionsSyncJobState = "canceled"
)

// ToGraphQL returns the GraphQL representation of the worker state.
func (s PermissionsSyncJobState) ToGraphQL() string { return strings.ToUpper(string(s)) }

type PermissionsSyncJobPriority int

const (
	LowPriorityPermissionsSync    PermissionsSyncJobPriority = 0
	MediumPriorityPermissionsSync PermissionsSyncJobPriority = 5
	HighPriorityPermissionsSync   PermissionsSyncJobPriority = 10
)

func (p PermissionsSyncJobPriority) ToString() string {
	switch p {
	case HighPriorityPermissionsSync:
		return "HIGH"
	case MediumPriorityPermissionsSync:
		return "MEDIUM"
	case LowPriorityPermissionsSync:
		fallthrough
	default:
		return "LOW"
	}
}

// PermissionsSyncJobReasonGroup combines multiple permission sync job trigger
// reasons into groups with similar grounds.
type PermissionsSyncJobReasonGroup string

// PermissionsSyncJobReasonGroup constants.
const (
	PermissionsSyncJobReasonGroupManual      PermissionsSyncJobReasonGroup = "MANUAL"
	PermissionsSyncJobReasonGroupWebhook     PermissionsSyncJobReasonGroup = "WEBHOOK"
	PermissionsSyncJobReasonGroupSchedule    PermissionsSyncJobReasonGroup = "SCHEDULE"
	PermissionsSyncJobReasonGroupSourcegraph PermissionsSyncJobReasonGroup = "SOURCEGRAPH"
	PermissionsSyncJobReasonGroupUnknown     PermissionsSyncJobReasonGroup = "UNKNOWN"
)

var ReasonGroupToReasons = map[PermissionsSyncJobReasonGroup][]PermissionsSyncJobReason{
	PermissionsSyncJobReasonGroupManual: {
		ReasonManualRepoSync,
		ReasonManualUserSync,
	},
	PermissionsSyncJobReasonGroupWebhook: {
		ReasonGitHubUserEvent,
		ReasonGitHubUserAddedEvent,
		ReasonGitHubUserRemovedEvent,
		ReasonGitHubUserMembershipAddedEvent,
		ReasonGitHubUserMembershipRemovedEvent,
		ReasonGitHubTeamAddedToRepoEvent,
		ReasonGitHubTeamRemovedFromRepoEvent,
		ReasonGitHubOrgMemberAddedEvent,
		ReasonGitHubOrgMemberRemovedEvent,
		ReasonGitHubRepoEvent,
		ReasonGitHubRepoMadePrivateEvent,
	},
	PermissionsSyncJobReasonGroupSchedule: {
		ReasonUserOutdatedPermissions,
		ReasonUserNoPermissions,
		ReasonRepoOutdatedPermissions,
		ReasonRepoNoPermissions,
		ReasonRepoUpdatedFromCodeHost,
	},
	PermissionsSyncJobReasonGroupSourcegraph: {
		ReasonUserEmailRemoved,
		ReasonUserEmailVerified,
		ReasonUserAddedToOrg,
		ReasonUserRemovedFromOrg,
		ReasonUserAcceptedOrgInvite,
	},
}

// sqlConds returns SQL query conditions to filter by reasons which are included
// into given PermissionsSyncJobReasonGroup.
//
// If provided PermissionsSyncJobReasonGroup doesn't contain any reasons
// (currently it is only PermissionsSyncJobReasonGroupUnknown), then nil is
// returned.
func (g PermissionsSyncJobReasonGroup) sqlConds() (conditions *sqlf.Query) {
	if reasons, ok := ReasonGroupToReasons[g]; ok {
		reasonQueries := make([]*sqlf.Query, 0, len(reasons))
		for _, reason := range reasons {
			reasonQueries = append(reasonQueries, sqlf.Sprintf("%s", reason))
		}
		conditions = sqlf.Sprintf("reason IN (%s)", sqlf.Join(reasonQueries, ", "))
	}
	return
}

type PermissionsSyncJobReason string

// ResolveGroup returns a PermissionsSyncJobReasonGroup for a given
// PermissionsSyncJobReason or PermissionsSyncJobReasonGroupUnknown if the reason
// doesn't belong to any of groups.
func (r PermissionsSyncJobReason) ResolveGroup() PermissionsSyncJobReasonGroup {
	switch r {
	case ReasonManualRepoSync,
		ReasonManualUserSync:
		return PermissionsSyncJobReasonGroupManual
	case ReasonGitHubUserEvent,
		ReasonGitHubUserAddedEvent,
		ReasonGitHubUserRemovedEvent,
		ReasonGitHubUserMembershipAddedEvent,
		ReasonGitHubUserMembershipRemovedEvent,
		ReasonGitHubTeamAddedToRepoEvent,
		ReasonGitHubTeamRemovedFromRepoEvent,
		ReasonGitHubOrgMemberAddedEvent,
		ReasonGitHubOrgMemberRemovedEvent,
		ReasonGitHubRepoEvent,
		ReasonGitHubRepoMadePrivateEvent:
		return PermissionsSyncJobReasonGroupWebhook
	case ReasonUserOutdatedPermissions,
		ReasonUserNoPermissions,
		ReasonRepoOutdatedPermissions,
		ReasonRepoNoPermissions,
		ReasonRepoUpdatedFromCodeHost:
		return PermissionsSyncJobReasonGroupSchedule
	case ReasonUserEmailRemoved,
		ReasonUserEmailVerified,
		ReasonUserAddedToOrg,
		ReasonUserRemovedFromOrg,
		ReasonUserAcceptedOrgInvite:
		return PermissionsSyncJobReasonGroupSourcegraph
	default:
		return PermissionsSyncJobReasonGroupUnknown
	}
}

const (
	// ReasonUserOutdatedPermissions and below are reasons of scheduled permission
	// syncs.
	ReasonUserOutdatedPermissions PermissionsSyncJobReason = "REASON_USER_OUTDATED_PERMS"
	ReasonUserNoPermissions       PermissionsSyncJobReason = "REASON_USER_NO_PERMS"
	ReasonRepoOutdatedPermissions PermissionsSyncJobReason = "REASON_REPO_OUTDATED_PERMS"
	ReasonRepoNoPermissions       PermissionsSyncJobReason = "REASON_REPO_NO_PERMS"
	ReasonRepoUpdatedFromCodeHost PermissionsSyncJobReason = "REASON_REPO_UPDATED_FROM_CODE_HOST"

	// ReasonUserEmailRemoved and below are reasons of permission syncs scheduled due
	// to Sourcegraph internal events.
	ReasonUserEmailRemoved      PermissionsSyncJobReason = "REASON_USER_EMAIL_REMOVED"
	ReasonUserEmailVerified     PermissionsSyncJobReason = "REASON_USER_EMAIL_VERIFIED"
	ReasonUserAddedToOrg        PermissionsSyncJobReason = "REASON_USER_ADDED_TO_ORG"
	ReasonUserRemovedFromOrg    PermissionsSyncJobReason = "REASON_USER_REMOVED_FROM_ORG"
	ReasonUserAcceptedOrgInvite PermissionsSyncJobReason = "REASON_USER_ACCEPTED_ORG_INVITE"

	// ReasonGitHubUserEvent and below are reasons of permission syncs triggered by
	// webhook events.
	ReasonGitHubUserEvent                  PermissionsSyncJobReason = "REASON_GITHUB_USER_EVENT"
	ReasonGitHubUserAddedEvent             PermissionsSyncJobReason = "REASON_GITHUB_USER_ADDED_EVENT"
	ReasonGitHubUserRemovedEvent           PermissionsSyncJobReason = "REASON_GITHUB_USER_REMOVED_EVENT"
	ReasonGitHubUserMembershipAddedEvent   PermissionsSyncJobReason = "REASON_GITHUB_USER_MEMBERSHIP_ADDED_EVENT"
	ReasonGitHubUserMembershipRemovedEvent PermissionsSyncJobReason = "REASON_GITHUB_USER_MEMBERSHIP_REMOVED_EVENT"
	ReasonGitHubTeamAddedToRepoEvent       PermissionsSyncJobReason = "REASON_GITHUB_TEAM_ADDED_TO_REPO_EVENT"
	ReasonGitHubTeamRemovedFromRepoEvent   PermissionsSyncJobReason = "REASON_GITHUB_TEAM_REMOVED_FROM_REPO_EVENT"
	ReasonGitHubOrgMemberAddedEvent        PermissionsSyncJobReason = "REASON_GITHUB_ORG_MEMBER_ADDED_EVENT"
	ReasonGitHubOrgMemberRemovedEvent      PermissionsSyncJobReason = "REASON_GITHUB_ORG_MEMBER_REMOVED_EVENT"
	ReasonGitHubRepoEvent                  PermissionsSyncJobReason = "REASON_GITHUB_REPO_EVENT"
	ReasonGitHubRepoMadePrivateEvent       PermissionsSyncJobReason = "REASON_GITHUB_REPO_MADE_PRIVATE_EVENT"

	// ReasonManualRepoSync and below are reasons of permission syncs triggered
	// manually.
	ReasonManualRepoSync PermissionsSyncJobReason = "REASON_MANUAL_REPO_SYNC"
	ReasonManualUserSync PermissionsSyncJobReason = "REASON_MANUAL_USER_SYNC"
)

type PermissionSyncJobOpts struct {
	Priority          PermissionsSyncJobPriority
	InvalidateCaches  bool
	ProcessAfter      time.Time
	Reason            PermissionsSyncJobReason
	TriggeredByUserID int32
	NoPerms           bool
}

type PermissionSyncJobStore interface {
	basestore.ShareableStore
	With(other basestore.ShareableStore) PermissionSyncJobStore
	// Transact begins a new transaction and make a new PermissionSyncJobStore over it.
	Transact(ctx context.Context) (PermissionSyncJobStore, error)
	Done(err error) error

	CreateUserSyncJob(ctx context.Context, user int32, opts PermissionSyncJobOpts) error
	CreateRepoSyncJob(ctx context.Context, repo api.RepoID, opts PermissionSyncJobOpts) error

	List(ctx context.Context, opts ListPermissionSyncJobOpts) ([]*PermissionSyncJob, error)
	Count(ctx context.Context, opts ListPermissionSyncJobOpts) (int, error)
	CancelQueuedJob(ctx context.Context, reason string, id int) error
	SaveSyncResult(ctx context.Context, id int, result *SetPermissionsResult, codeHostStatuses CodeHostStatusesSet) error
}

type permissionSyncJobStore struct {
	logger log.Logger
	*basestore.Store
}

var _ PermissionSyncJobStore = (*permissionSyncJobStore)(nil)

func PermissionSyncJobsWith(logger log.Logger, other basestore.ShareableStore) PermissionSyncJobStore {
	return &permissionSyncJobStore{logger: logger, Store: basestore.NewWithHandle(other.Handle())}
}

func (s *permissionSyncJobStore) With(other basestore.ShareableStore) PermissionSyncJobStore {
	return &permissionSyncJobStore{logger: s.logger, Store: s.Store.With(other)}
}

func (s *permissionSyncJobStore) Transact(ctx context.Context) (PermissionSyncJobStore, error) {
	return s.transact(ctx)
}

func (s *permissionSyncJobStore) transact(ctx context.Context) (*permissionSyncJobStore, error) {
	txBase, err := s.Store.Transact(ctx)
	return &permissionSyncJobStore{Store: txBase}, err
}

func (s *permissionSyncJobStore) Done(err error) error {
	return s.Store.Done(err)
}

func (s *permissionSyncJobStore) CreateUserSyncJob(ctx context.Context, user int32, opts PermissionSyncJobOpts) error {
	job := &PermissionSyncJob{
		UserID:            int(user),
		Priority:          opts.Priority,
		InvalidateCaches:  opts.InvalidateCaches,
		Reason:            opts.Reason,
		TriggeredByUserID: opts.TriggeredByUserID,
		NoPerms:           opts.NoPerms,
	}
	if !opts.ProcessAfter.IsZero() {
		job.ProcessAfter = opts.ProcessAfter
	}
	return s.createSyncJob(ctx, job)
}

func (s *permissionSyncJobStore) CreateRepoSyncJob(ctx context.Context, repo api.RepoID, opts PermissionSyncJobOpts) error {
	job := &PermissionSyncJob{
		RepositoryID:      int(repo),
		Priority:          opts.Priority,
		InvalidateCaches:  opts.InvalidateCaches,
		Reason:            opts.Reason,
		TriggeredByUserID: opts.TriggeredByUserID,
		NoPerms:           opts.NoPerms,
	}
	if !opts.ProcessAfter.IsZero() {
		job.ProcessAfter = opts.ProcessAfter
	}
	return s.createSyncJob(ctx, job)
}

const permissionSyncJobCreateQueryFmtstr = `
INSERT INTO permission_sync_jobs (
	reason,
	triggered_by_user_id,
	process_after,
	repository_id,
	user_id,
	priority,
	invalidate_caches,
	no_perms
)
VALUES (
	%s,
	%s,
	%s,
	%s,
	%s,
	%s,
	%s,
	%s
)
ON CONFLICT DO NOTHING
RETURNING %s
`

// createSyncJob inserts a postponed (`process_after IS NOT NULL`) sync job right
// away and checks new sync jobs without provided delay for duplicates.
func (s *permissionSyncJobStore) createSyncJob(ctx context.Context, job *PermissionSyncJob) error {
	if job.ProcessAfter.IsZero() {
		// sync jobs without delay are checked for duplicates
		return s.checkDuplicateAndCreateSyncJob(ctx, job)
	}
	return s.create(ctx, job)
}

func (s *permissionSyncJobStore) create(ctx context.Context, job *PermissionSyncJob) error {
	q := sqlf.Sprintf(
		permissionSyncJobCreateQueryFmtstr,
		job.Reason,
		dbutil.NewNullInt32(job.TriggeredByUserID),
		dbutil.NullTimeColumn(job.ProcessAfter),
		dbutil.NewNullInt(job.RepositoryID),
		dbutil.NewNullInt(job.UserID),
		job.Priority,
		job.InvalidateCaches,
		job.NoPerms,
		sqlf.Join(PermissionSyncJobColumns, ", "),
	)

	return scanPermissionSyncJob(job, s.QueryRow(ctx, q))
}

// checkDuplicateAndCreateSyncJob adds a new perms sync job with `process_after
// IS NULL` if there is no present duplicate of it.
//
// Duplicates are handled in this way:
//
// 1) If there is no existing job for given user/repo ID in a queued state, we
// insert right away.
//
// 2) If there is an existing job with lower priority, we cancel it and insert a
// new one with higher priority.
//
// 3) If there is an existing job with higher priority, we don't insert new job.
func (s *permissionSyncJobStore) checkDuplicateAndCreateSyncJob(ctx context.Context, job *PermissionSyncJob) (err error) {
	tx, err := s.transact(ctx)
	if err != nil {
		return err
	}
	defer func() {
		err = tx.Done(err)
	}()
	opts := ListPermissionSyncJobOpts{UserID: job.UserID, RepoID: job.RepositoryID, State: PermissionsSyncJobStateQueued, NotCanceled: true, NullProcessAfter: true}
	syncJobs, err := tx.List(ctx, opts)
	if err != nil {
		return err
	}
	// Job doesn't exist -- create it
	if len(syncJobs) == 0 {
		return tx.create(ctx, job)
	}
	// Database constraint guarantees that we have at most 1 job with NULL
	// `process_after` value.
	existingJob := syncJobs[0]

	// Existing job with higher priority should not be overridden. Existing
	// priority job shouldn't be overridden by another same priority job.
	if existingJob.Priority >= job.Priority {
		logField := "repositoryID"
		id := strconv.Itoa(job.RepositoryID)
		if job.RepositoryID == 0 {
			logField = "userID"
			id = strconv.Itoa(job.UserID)
		}
		s.logger.Debug(
			"Permissions sync job is not added because a job with similar or higher priority already exists",
			log.String(logField, id),
		)
		return nil
	}

	err = tx.CancelQueuedJob(ctx, CancellationReasonHigherPriority, existingJob.ID)
	if err != nil && !errcode.IsNotFound(err) {
		return err
	}
	return tx.create(ctx, job)
}

type notFoundError struct{ error }

func (e notFoundError) NotFound() bool { return true }

func (s *permissionSyncJobStore) CancelQueuedJob(ctx context.Context, reason string, id int) error {
	now := timeutil.Now()
	q := sqlf.Sprintf(`
UPDATE permission_sync_jobs
SET cancel = TRUE, state = 'canceled', finished_at = %s, cancellation_reason = %s
WHERE id = %s AND state = 'queued' AND cancel IS FALSE
`, now, reason, id)

	res, err := s.ExecResult(ctx, q)
	if err != nil {
		return err
	}
	af, err := res.RowsAffected()
	if err != nil {
		return err
	}
	if af != 1 {
		return notFoundError{errors.Newf("sync job with id %d not found", id)}
	}
	return nil
}

type SetPermissionsResult struct {
	Added   int
	Removed int
	Found   int
}

func (s *permissionSyncJobStore) SaveSyncResult(ctx context.Context, id int, result *SetPermissionsResult, statuses CodeHostStatusesSet) error {
	q := sqlf.Sprintf(`
		UPDATE permission_sync_jobs
		SET
			permissions_added = %d,
			permissions_removed = %d,
			permissions_found = %d,
			code_host_states = %s
		WHERE id = %d
		`, result.Added, result.Removed, result.Found, pq.Array(statuses), id)

	_, err := s.ExecResult(ctx, q)
	return err
}

type ListPermissionSyncJobOpts struct {
	ID                  int
	UserID              int
	RepoID              int
	Reason              PermissionsSyncJobReason
	ReasonGroup         PermissionsSyncJobReasonGroup
	State               PermissionsSyncJobState
	NullProcessAfter    bool
	NotNullProcessAfter bool
	NotCanceled         bool

	// Cursor-based pagination arguments.
	PaginationArgs *PaginationArgs
}

func (opts ListPermissionSyncJobOpts) sqlConds() []*sqlf.Query {
	conds := []*sqlf.Query{}

	if opts.ID != 0 {
		conds = append(conds, sqlf.Sprintf("id = %s", opts.ID))
	}
	if opts.UserID != 0 {
		conds = append(conds, sqlf.Sprintf("user_id = %s", opts.UserID))
	}
	if opts.RepoID != 0 {
		conds = append(conds, sqlf.Sprintf("repository_id = %s", opts.RepoID))
	}
	// If both reason group and reason are provided, we narrow down the filtering to
	// just a reason.
	if opts.ReasonGroup != "" && opts.Reason == "" {
		if reasonConds := opts.ReasonGroup.sqlConds(); reasonConds != nil {
			conds = append(conds, reasonConds)
		}
	}
	if opts.Reason != "" {
		conds = append(conds, sqlf.Sprintf("reason = %s", opts.Reason))
	}
	if opts.State != "" {
		conds = append(conds, sqlf.Sprintf("state = %s", opts.State))
	}
	if opts.NullProcessAfter {
		conds = append(conds, sqlf.Sprintf("process_after IS NULL"))
	}
	if opts.NotNullProcessAfter {
		conds = append(conds, sqlf.Sprintf("process_after IS NOT NULL"))
	}
	if opts.NotCanceled {
		conds = append(conds, sqlf.Sprintf("cancel = false"))
	}
	return conds
}

const listPermissionSyncJobQueryFmtstr = `
SELECT %s
FROM permission_sync_jobs
%s -- whereClause
`

func (s *permissionSyncJobStore) List(ctx context.Context, opts ListPermissionSyncJobOpts) ([]*PermissionSyncJob, error) {
	conds := opts.sqlConds()

	paginationArgs := PaginationArgs{OrderBy: []OrderByOption{{Field: "id"}}, Ascending: true}
	if opts.PaginationArgs != nil {
		paginationArgs = *opts.PaginationArgs
	}
	pagination := paginationArgs.SQL()

	if pagination.Where != nil {
		conds = append(conds, pagination.Where)
	}

	whereClause := sqlf.Sprintf("")
	if len(conds) > 0 {
		whereClause = sqlf.Sprintf("WHERE %s", sqlf.Join(conds, "\n AND "))
	}

	q := sqlf.Sprintf(
		listPermissionSyncJobQueryFmtstr,
		sqlf.Join(PermissionSyncJobColumns, ", "),
		whereClause,
	)
	q = pagination.AppendOrderToQuery(q)
	q = pagination.AppendLimitToQuery(q)

	rows, err := s.Query(ctx, q)
	if err != nil {
		return nil, err
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	var syncJobs []*PermissionSyncJob
	for rows.Next() {
		job, err := ScanPermissionSyncJob(rows)
		if err != nil {
			return nil, err
		}
		syncJobs = append(syncJobs, job)
	}

	return syncJobs, nil
}

const countPermissionSyncJobsQuery = `
SELECT COUNT(*)
FROM permission_sync_jobs
%s -- whereClause
`

func (s *permissionSyncJobStore) Count(ctx context.Context, opts ListPermissionSyncJobOpts) (int, error) {
	conds := opts.sqlConds()

	whereClause := sqlf.Sprintf("")
	if len(conds) > 0 {
		whereClause = sqlf.Sprintf("WHERE %s", sqlf.Join(conds, "\n AND "))
	}

	q := sqlf.Sprintf(countPermissionSyncJobsQuery, whereClause)
	var count int
	if err := s.QueryRow(ctx, q).Scan(&count); err != nil {
		return 0, err
	}
	return count, nil
}

type PermissionSyncJob struct {
	ID                 int
	State              PermissionsSyncJobState
	FailureMessage     *string
	Reason             PermissionsSyncJobReason
	CancellationReason *string
	TriggeredByUserID  int32
	QueuedAt           time.Time
	StartedAt          time.Time
	FinishedAt         time.Time
	ProcessAfter       time.Time
	NumResets          int
	NumFailures        int
	LastHeartbeatAt    time.Time
	ExecutionLogs      []executor.ExecutionLogEntry
	WorkerHostname     string
	Cancel             bool

	RepositoryID int
	UserID       int

	Priority         PermissionsSyncJobPriority
	NoPerms          bool
	InvalidateCaches bool

	PermissionsAdded   int
	PermissionsRemoved int
	PermissionsFound   int
	CodeHostStates     []PermissionSyncCodeHostState
}

func (j *PermissionSyncJob) RecordID() int { return j.ID }

var PermissionSyncJobColumns = []*sqlf.Query{
	sqlf.Sprintf("permission_sync_jobs.id"),
	sqlf.Sprintf("permission_sync_jobs.state"),
	sqlf.Sprintf("permission_sync_jobs.reason"),
	sqlf.Sprintf("permission_sync_jobs.cancellation_reason"),
	sqlf.Sprintf("permission_sync_jobs.triggered_by_user_id"),
	sqlf.Sprintf("permission_sync_jobs.failure_message"),
	sqlf.Sprintf("permission_sync_jobs.queued_at"),
	sqlf.Sprintf("permission_sync_jobs.started_at"),
	sqlf.Sprintf("permission_sync_jobs.finished_at"),
	sqlf.Sprintf("permission_sync_jobs.process_after"),
	sqlf.Sprintf("permission_sync_jobs.num_resets"),
	sqlf.Sprintf("permission_sync_jobs.num_failures"),
	sqlf.Sprintf("permission_sync_jobs.last_heartbeat_at"),
	sqlf.Sprintf("permission_sync_jobs.execution_logs"),
	sqlf.Sprintf("permission_sync_jobs.worker_hostname"),
	sqlf.Sprintf("permission_sync_jobs.cancel"),

	sqlf.Sprintf("permission_sync_jobs.repository_id"),
	sqlf.Sprintf("permission_sync_jobs.user_id"),

	sqlf.Sprintf("permission_sync_jobs.priority"),
	sqlf.Sprintf("permission_sync_jobs.no_perms"),
	sqlf.Sprintf("permission_sync_jobs.invalidate_caches"),

	sqlf.Sprintf("permission_sync_jobs.permissions_added"),
	sqlf.Sprintf("permission_sync_jobs.permissions_removed"),
	sqlf.Sprintf("permission_sync_jobs.permissions_found"),
	sqlf.Sprintf("permission_sync_jobs.code_host_states"),
}

func ScanPermissionSyncJob(s dbutil.Scanner) (*PermissionSyncJob, error) {
	var job PermissionSyncJob
	if err := scanPermissionSyncJob(&job, s); err != nil {
		return nil, err
	}
	return &job, nil
}

func scanPermissionSyncJob(job *PermissionSyncJob, s dbutil.Scanner) error {
	var executionLogs []executor.ExecutionLogEntry
	var codeHostStates []PermissionSyncCodeHostState

	if err := s.Scan(
		&job.ID,
		&job.State,
		&job.Reason,
		&job.CancellationReason,
		&dbutil.NullInt32{N: &job.TriggeredByUserID},
		&job.FailureMessage,
		&job.QueuedAt,
		&dbutil.NullTime{Time: &job.StartedAt},
		&dbutil.NullTime{Time: &job.FinishedAt},
		&dbutil.NullTime{Time: &job.ProcessAfter},
		&job.NumResets,
		&job.NumFailures,
		&dbutil.NullTime{Time: &job.LastHeartbeatAt},
		pq.Array(&executionLogs),
		&job.WorkerHostname,
		&job.Cancel,

		&dbutil.NullInt{N: &job.RepositoryID},
		&dbutil.NullInt{N: &job.UserID},

		&job.Priority,
		&job.NoPerms,
		&job.InvalidateCaches,

		&job.PermissionsAdded,
		&job.PermissionsRemoved,
		&job.PermissionsFound,
		pq.Array(&codeHostStates),
	); err != nil {
		return err
	}

	job.ExecutionLogs = append(job.ExecutionLogs, executionLogs...)
	job.CodeHostStates = append(job.CodeHostStates, codeHostStates...)

	return nil
}
