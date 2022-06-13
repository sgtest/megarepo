package database

import (
	"context"
	"database/sql"
	"database/sql/driver"
	"encoding/json"
	"fmt"

	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"

	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	dbworkerstore "github.com/sourcegraph/sourcegraph/internal/workerutil/dbworker/store"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// BitbucketProjectPermissionsStore is used by the BitbucketProjectPermissions worker
// to apply permissions asynchronously.
type BitbucketProjectPermissionsStore interface {
	basestore.ShareableStore
	With(other basestore.ShareableStore) BitbucketProjectPermissionsStore
	Enqueue(ctx context.Context, projectKey string, externalServiceID int64, permissions []types.UserPermission, unrestricted bool) (int, error)
	Transact(ctx context.Context) (BitbucketProjectPermissionsStore, error)
	Done(err error) error
	ListJobs(ctx context.Context, opt ListJobsOptions) ([]*types.BitbucketProjectPermissionJob, error)
}

type bitbucketProjectPermissionsStore struct {
	*basestore.Store
}

// BitbucketProjectPermissionsStoreWith instantiates and returns a new BitbucketProjectPermissionsStore using
// the other store handle.
func BitbucketProjectPermissionsStoreWith(other basestore.ShareableStore) BitbucketProjectPermissionsStore {
	return &bitbucketProjectPermissionsStore{Store: basestore.NewWithHandle(other.Handle())}
}

func (s *bitbucketProjectPermissionsStore) With(other basestore.ShareableStore) BitbucketProjectPermissionsStore {
	return &bitbucketProjectPermissionsStore{Store: s.Store.With(other)}
}

func (s *bitbucketProjectPermissionsStore) copy() *bitbucketProjectPermissionsStore {
	return &bitbucketProjectPermissionsStore{
		Store: s.Store,
	}
}

func (s *bitbucketProjectPermissionsStore) Transact(ctx context.Context) (BitbucketProjectPermissionsStore, error) {
	return s.transact(ctx)
}

func (s *bitbucketProjectPermissionsStore) transact(ctx context.Context) (*bitbucketProjectPermissionsStore, error) {
	txBase, err := s.Store.Transact(ctx)
	c := s.copy()
	c.Store = txBase
	return c, err
}

func (s *bitbucketProjectPermissionsStore) Done(err error) error {
	return s.Store.Done(err)
}

// Enqueue a job to apply permissions to a Bitbucket project.
// The job will be enqueued to the BitbucketProjectPermissions worker.
// If a non-empty permissions slice is passed, unrestricted has to be false, and vice versa.
func (s *bitbucketProjectPermissionsStore) Enqueue(ctx context.Context, projectKey string, externalServiceID int64, permissions []types.UserPermission, unrestricted bool) (jobID int, err error) {
	if len(permissions) > 0 && unrestricted {
		return 0, errors.New("cannot specify permissions when unrestricted is true")
	}
	if len(permissions) == 0 && !unrestricted {
		return 0, errors.New("must specify permissions when unrestricted is false")
	}

	var perms []userPermission
	for _, perm := range permissions {
		perms = append(perms, userPermission(perm))
	}

	tx, err := s.transact(ctx)
	if err != nil {
		return 0, err
	}
	defer func() { err = tx.Done(err) }()

	// ensure we don't enqueue a job for the same project twice.
	// if so, cancel the existing jobs and enqueue a new one.
	// this doesn't apply to running jobs.
	err = tx.Exec(ctx, sqlf.Sprintf(`--sql
-- source: internal/database/bitbucket_project_permissions.go:BitbucketProjectPermissionsStore.Enqueue
UPDATE explicit_permissions_bitbucket_projects_jobs SET state = 'canceled' WHERE project_key = %s AND external_service_id = %s AND state = 'queued'
`, projectKey, externalServiceID))
	if err != nil && err != sql.ErrNoRows {
		return 0, err
	}

	err = tx.QueryRow(ctx, sqlf.Sprintf(`--sql
-- source: internal/database/bitbucket_project_permissions.go:BitbucketProjectPermissionsStore.Enqueue
INSERT INTO
	explicit_permissions_bitbucket_projects_jobs (project_key, external_service_id, permissions, unrestricted)
VALUES (%s, %s, %s, %s) RETURNING id
	`, projectKey, externalServiceID, pq.Array(perms), unrestricted)).Scan(&jobID)
	if err != nil {
		return 0, err
	}

	return jobID, nil
}

// ScanFirstBitbucketProjectPermissionsJob scans a single job from the return value of `*Store.query`.
func ScanFirstBitbucketProjectPermissionsJob(rows *sql.Rows, queryErr error) (_ *types.BitbucketProjectPermissionJob, exists bool, err error) {
	if queryErr != nil {
		return nil, false, queryErr
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	if rows.Next() {
		job, err := scanOneJob(rows)
		if err != nil {
			return nil, false, err
		}

		return job, true, nil
	}

	return nil, false, nil
}

type ListJobsOptions struct {
	ProjectKey string
	Status     string
	Count      int
}

// ListJobs returns a list of types.BitbucketProjectPermissionJob for a given set
// of query options: ListJobsOptions
func (s *bitbucketProjectPermissionsStore) ListJobs(
	ctx context.Context,
	opt ListJobsOptions,
) (jobs []*types.BitbucketProjectPermissionJob, err error) {
	query := listWorkerJobsQuery(opt)

	rows, err := s.Query(ctx, query)
	if err != nil {
		return nil, err
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	for rows.Next() {
		var job *types.BitbucketProjectPermissionJob
		job, err = scanOneJob(rows)
		if err != nil {
			return nil, err
		}

		jobs = append(jobs, job)
	}

	return
}

func scanOneJob(rows *sql.Rows) (*types.BitbucketProjectPermissionJob, error) {
	var job types.BitbucketProjectPermissionJob
	var executionLogs []dbworkerstore.ExecutionLogEntry
	var permissions []userPermission

	if err := rows.Scan(
		&job.ID,
		&job.State,
		&job.FailureMessage,
		&job.QueuedAt,
		&job.StartedAt,
		&job.FinishedAt,
		&job.ProcessAfter,
		&job.NumResets,
		&job.NumFailures,
		&dbutil.NullTime{Time: &job.LastHeartbeatAt},
		pq.Array(&executionLogs),
		&job.WorkerHostname,
		&job.ProjectKey,
		&job.ExternalServiceID,
		pq.Array(&permissions),
		&job.Unrestricted,
	); err != nil {
		return nil, err
	}

	for _, entry := range executionLogs {
		job.ExecutionLogs = append(job.ExecutionLogs, workerutil.ExecutionLogEntry(entry))
	}

	for _, perm := range permissions {
		job.Permissions = append(job.Permissions, types.UserPermission(perm))
	}
	return &job, nil
}

const maxJobsCount = 500

func listWorkerJobsQuery(opt ListJobsOptions) *sqlf.Query {
	var where []*sqlf.Query

	q := `
-- source: internal/database/bitbucket_project_permissions.go:BitbucketProjectPermissionsStore.listWorkerJobsQuery
SELECT id, state, failure_message, queued_at, started_at, finished_at, process_after, num_resets, num_failures, last_heartbeat_at, execution_logs, worker_hostname, project_key, external_services_id, permissions, unrestricted
FROM explicit_permissions_bitbucket_project_jobs
%%s
ORDER BY queued_at DESC
LIMIT %d
`

	if opt.ProjectKey != "" {
		where = append(where, sqlf.Sprintf("project_key = %s", opt.ProjectKey))
	}

	if opt.Status != "" {
		where = append(where, sqlf.Sprintf("status = %s", opt.Status))
	}

	whereClause := sqlf.Sprintf("")
	if len(where) != 0 {
		whereClause = sqlf.Sprintf("WHERE %s", sqlf.Join(where, " AND"))
	}

	limitNum := 100

	if opt.Count > 0 && opt.Count < maxJobsCount {
		limitNum = opt.Count
	} else if opt.Count >= maxJobsCount {
		limitNum = maxJobsCount
	}

	return sqlf.Sprintf(fmt.Sprintf(q, limitNum), whereClause)
}

type userPermission types.UserPermission

func (p *userPermission) Scan(value any) error {
	b, ok := value.([]byte)
	if !ok {
		return errors.Errorf("value is not []byte: %T", value)
	}

	return json.Unmarshal(b, &p)
}

func (p userPermission) Value() (driver.Value, error) {
	return json.Marshal(p)
}
