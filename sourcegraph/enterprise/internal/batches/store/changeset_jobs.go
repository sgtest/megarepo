package store

import (
	"context"
	"encoding/json"

	"github.com/keegancsmith/sqlf"
	"go.opentelemetry.io/otel/attribute"

	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/internal/database/batch"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// changesetJobInsertColumns is the list of changeset_jobs columns that are
// modified in CreateChangesetJob.
var changesetJobInsertColumns = []string{
	"bulk_group",
	"user_id",
	"batch_change_id",
	"changeset_id",
	"job_type",
	"payload",
	"state",
	"failure_message",
	"started_at",
	"finished_at",
	"process_after",
	"num_resets",
	"num_failures",
	"created_at",
	"updated_at",
}

// changesetJobColumns are used by the changeset job related Store methods to query
// and create changeset jobs.
var changesetJobColumns = SQLColumns{
	"changeset_jobs.id",
	"changeset_jobs.bulk_group",
	"changeset_jobs.user_id",
	"changeset_jobs.batch_change_id",
	"changeset_jobs.changeset_id",
	"changeset_jobs.job_type",
	"changeset_jobs.payload",
	"changeset_jobs.state",
	"changeset_jobs.failure_message",
	"changeset_jobs.started_at",
	"changeset_jobs.finished_at",
	"changeset_jobs.process_after",
	"changeset_jobs.num_resets",
	"changeset_jobs.num_failures",
	"changeset_jobs.created_at",
	"changeset_jobs.updated_at",
}

// CreateChangesetJob creates the given changeset jobs.
func (s *Store) CreateChangesetJob(ctx context.Context, cs ...*btypes.ChangesetJob) (err error) {
	ctx, _, endObservation := s.operations.createChangesetJob.With(ctx, &err, observation.Args{Attrs: []attribute.KeyValue{
		attribute.Int("count", len(cs)),
	}})
	defer endObservation(1, observation.Args{})

	inserter := func(inserter *batch.Inserter) error {
		for _, c := range cs {
			payload, err := jsonbColumn(c.Payload)
			if err != nil {
				return err
			}

			if c.CreatedAt.IsZero() {
				c.CreatedAt = s.now()
			}

			if c.UpdatedAt.IsZero() {
				c.UpdatedAt = c.CreatedAt
			}

			if err := inserter.Insert(
				ctx,
				c.BulkGroup,
				c.UserID,
				c.BatchChangeID,
				c.ChangesetID,
				c.JobType,
				payload,
				c.State.ToDB(),
				c.FailureMessage,
				dbutil.NullTimeColumn(c.StartedAt),
				dbutil.NullTimeColumn(c.FinishedAt),
				dbutil.NullTimeColumn(c.ProcessAfter),
				c.NumResets,
				c.NumFailures,
				c.CreatedAt,
				c.UpdatedAt,
			); err != nil {
				return err
			}
		}

		return nil
	}
	i := -1
	return batch.WithInserterWithReturn(
		ctx,
		s.Handle(),
		"changeset_jobs",
		batch.MaxNumPostgresParameters,
		changesetJobInsertColumns,
		"",
		changesetJobColumns,
		func(rows dbutil.Scanner) error {
			i++
			return scanChangesetJob(cs[i], rows)
		},
		inserter,
	)
}

// GetChangesetJobOpts captures the query options needed for getting a ChangesetJob
type GetChangesetJobOpts struct {
	ID int64
}

// GetChangesetJob gets a ChangesetJob matching the given options.
func (s *Store) GetChangesetJob(ctx context.Context, opts GetChangesetJobOpts) (job *btypes.ChangesetJob, err error) {
	ctx, _, endObservation := s.operations.getChangesetJob.With(ctx, &err, observation.Args{Attrs: []attribute.KeyValue{
		attribute.Int("ID", int(opts.ID)),
	}})
	defer endObservation(1, observation.Args{})

	q := getChangesetJobQuery(&opts)
	var c btypes.ChangesetJob
	err = s.query(ctx, q, func(sc dbutil.Scanner) (err error) {
		return scanChangesetJob(&c, sc)
	})
	if err != nil {
		return nil, err
	}

	if c.ID == 0 {
		return nil, ErrNoResults
	}

	return &c, nil
}

var getChangesetJobsQueryFmtstr = `
SELECT %s FROM changeset_jobs
INNER JOIN changesets ON changesets.id = changeset_jobs.changeset_id
INNER JOIN repo ON repo.id = changesets.repo_id
WHERE %s
LIMIT 1
`

func getChangesetJobQuery(opts *GetChangesetJobOpts) *sqlf.Query {
	preds := []*sqlf.Query{
		sqlf.Sprintf("repo.deleted_at IS NULL"),
		sqlf.Sprintf("changeset_jobs.id = %s", opts.ID),
	}

	return sqlf.Sprintf(
		getChangesetJobsQueryFmtstr,
		sqlf.Join(changesetJobColumns.ToSqlf(), ", "),
		sqlf.Join(preds, "\n AND "),
	)
}

func scanChangesetJob(c *btypes.ChangesetJob, s dbutil.Scanner) error {
	var raw json.RawMessage
	if err := s.Scan(
		&c.ID,
		&c.BulkGroup,
		&c.UserID,
		&c.BatchChangeID,
		&c.ChangesetID,
		&c.JobType,
		&raw,
		&c.State,
		&dbutil.NullString{S: c.FailureMessage},
		&dbutil.NullTime{Time: &c.StartedAt},
		&dbutil.NullTime{Time: &c.FinishedAt},
		&dbutil.NullTime{Time: &c.ProcessAfter},
		&c.NumResets,
		&c.NumFailures,
		&c.CreatedAt,
		&c.UpdatedAt,
	); err != nil {
		return err
	}
	switch c.JobType {
	case btypes.ChangesetJobTypeComment:
		c.Payload = new(btypes.ChangesetJobCommentPayload)
	case btypes.ChangesetJobTypeDetach:
		c.Payload = new(btypes.ChangesetJobDetachPayload)
	case btypes.ChangesetJobTypeReenqueue:
		c.Payload = new(btypes.ChangesetJobReenqueuePayload)
	case btypes.ChangesetJobTypeMerge:
		c.Payload = new(btypes.ChangesetJobMergePayload)
	case btypes.ChangesetJobTypeClose:
		c.Payload = new(btypes.ChangesetJobClosePayload)
	case btypes.ChangesetJobTypePublish:
		c.Payload = new(btypes.ChangesetJobPublishPayload)
	default:
		return errors.Errorf("unknown job type %q", c.JobType)
	}
	return json.Unmarshal(raw, &c.Payload)
}
