package service

import (
	"bufio"
	"context"
	"encoding/csv"
	"fmt"
	"io"
	"sync"
	"time"

	"github.com/sourcegraph/log"
	"go.opentelemetry.io/otel/attribute"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/search/exhaustive/store"
	"github.com/sourcegraph/sourcegraph/internal/search/exhaustive/types"
	"github.com/sourcegraph/sourcegraph/internal/uploadstore"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/iterator"
)

// New returns a Service.
func New(observationCtx *observation.Context, store *store.Store, uploadStore uploadstore.Store) *Service {
	logger := log.Scoped("searchjobs.Service", "search job service")
	svc := &Service{
		logger:      logger,
		store:       store,
		uploadStore: uploadStore,
		operations:  newOperations(observationCtx),
	}

	return svc
}

type Service struct {
	logger      log.Logger
	store       *store.Store
	uploadStore uploadstore.Store
	operations  *operations
}

func opAttrs(attrs ...attribute.KeyValue) observation.Args {
	return observation.Args{Attrs: attrs}
}

type operations struct {
	createSearchJob          *observation.Operation
	getSearchJob             *observation.Operation
	deleteSearchJob          *observation.Operation
	listSearchJobs           *observation.Operation
	cancelSearchJob          *observation.Operation
	writeSearchJobCSV        *observation.Operation
	getAggregateRepoRevState *observation.Operation
}

var (
	singletonOperations *operations
	operationsOnce      sync.Once
)

// newOperations generates a singleton of the operations struct.
//
// TODO: We should create one per observationCtx. This is a copy-pasta from
// the batches service, we should validate if we need to do this protection.
func newOperations(observationCtx *observation.Context) *operations {
	operationsOnce.Do(func() {
		m := metrics.NewREDMetrics(
			observationCtx.Registerer,
			"searchjobs_service",
			metrics.WithLabels("op"),
			metrics.WithCountHelp("Total number of method invocations."),
		)

		op := func(name string) *observation.Operation {
			return observationCtx.Operation(observation.Op{
				Name:              fmt.Sprintf("searchjobs.service.%s", name),
				MetricLabelValues: []string{name},
				Metrics:           m,
			})
		}

		singletonOperations = &operations{
			createSearchJob:          op("CreateSearchJob"),
			getSearchJob:             op("GetSearchJob"),
			deleteSearchJob:          op("DeleteSearchJob"),
			listSearchJobs:           op("ListSearchJobs"),
			cancelSearchJob:          op("CancelSearchJob"),
			writeSearchJobCSV:        op("WriteSearchJobCSV"),
			getAggregateRepoRevState: op("GetAggregateRepoRevState"),
		}
	})
	return singletonOperations
}

func (s *Service) CreateSearchJob(ctx context.Context, query string) (_ *types.ExhaustiveSearchJob, err error) {
	ctx, _, endObservation := s.operations.createSearchJob.With(ctx, &err, opAttrs(
		attribute.String("query", query),
	))
	defer endObservation(1, observation.Args{})

	actor := actor.FromContext(ctx)
	if !actor.IsAuthenticated() {
		return nil, errors.New("search jobs can only be created by an authenticated user")
	}

	tx, err := s.store.Transact(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { err = tx.Done(err) }()

	// XXX(keegancsmith) this API for creating seems easy to mess up since the
	// ExhaustiveSearchJob type has lots of fields, but reading the store
	// implementation only two fields are read.
	jobID, err := tx.CreateExhaustiveSearchJob(ctx, types.ExhaustiveSearchJob{
		InitiatorID: actor.UID,
		Query:       query,
	})
	if err != nil {
		return nil, err
	}

	return tx.GetExhaustiveSearchJob(ctx, jobID)
}

func (s *Service) CancelSearchJob(ctx context.Context, id int64) (err error) {
	ctx, _, endObservation := s.operations.cancelSearchJob.With(ctx, &err, opAttrs(
		attribute.Int64("id", id),
	))
	defer endObservation(1, observation.Args{})

	tx, err := s.store.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Done(err) }()

	_, err = tx.CancelSearchJob(ctx, id)
	return err
}

func (s *Service) GetSearchJob(ctx context.Context, id int64) (_ *types.ExhaustiveSearchJob, err error) {
	ctx, _, endObservation := s.operations.getSearchJob.With(ctx, &err, opAttrs(
		attribute.Int64("id", id),
	))
	defer endObservation(1, observation.Args{})

	return s.store.GetExhaustiveSearchJob(ctx, id)
}

func (s *Service) ListSearchJobs(ctx context.Context, args store.ListArgs) (jobs []*types.ExhaustiveSearchJob, err error) {
	ctx, _, endObservation := s.operations.listSearchJobs.With(ctx, &err, observation.Args{})
	defer func() {
		endObservation(1, opAttrs(
			attribute.Int("len", len(jobs)),
		))
	}()

	return s.store.ListExhaustiveSearchJobs(ctx, args)
}

func (s *Service) WriteSearchJobLogs(ctx context.Context, w io.Writer, id int64) (err error) {
	// 🚨 SECURITY: only someone with access to the job may copy the blobs
	if _, err := s.GetSearchJob(ctx, id); err != nil {
		return err
	}

	iter := s.getJobLogsIter(ctx, id)

	cw := csv.NewWriter(w)

	header := []string{
		"repository",
		"revision",
		"started_at",
		"finished_at",
		"status",
		"failure_message",
	}
	err = cw.Write(header)
	if err != nil {
		return err
	}

	for iter.Next() {
		job := iter.Current()
		err = cw.Write([]string{
			string(job.RepoName),
			job.Revision,
			formatOrNULL(job.StartedAt),
			formatOrNULL(job.FinishedAt),
			string(job.State),
			job.FailureMessage,
		})
		if err != nil {
			return err
		}
	}

	if err := iter.Err(); err != nil {
		return err
	}

	// Flush data before checking for any final write errors.
	cw.Flush()
	return cw.Error()
}

// JobLogsIterLimit is the number of lines the iterator will read from the
// database per page. Assuming 100 bytes per line, this will be ~1MB of memory
// per 10k repo-rev jobs.
var JobLogsIterLimit = 10_000

func (s *Service) getJobLogsIter(ctx context.Context, id int64) *iterator.Iterator[types.SearchJobLog] {
	var cursor int64
	limit := JobLogsIterLimit

	return iterator.New(func() ([]types.SearchJobLog, error) {
		if cursor == -1 {
			return nil, nil
		}

		opts := &store.GetJobLogsOpts{
			From:  cursor,
			Limit: limit + 1,
		}

		logs, err := s.store.GetJobLogs(ctx, id, opts)
		if err != nil {
			return nil, err
		}

		if len(logs) > limit {
			cursor = logs[len(logs)-1].ID
			logs = logs[:len(logs)-1]
		} else {
			cursor = -1
		}

		return logs, nil
	})
}

func formatOrNULL(t time.Time) string {
	if t.IsZero() {
		return "NULL"
	}

	return t.Format(time.RFC3339)
}

func getPrefix(id int64) string {
	return fmt.Sprintf("%d-", id)
}

func (s *Service) DeleteSearchJob(ctx context.Context, id int64) (err error) {
	ctx, _, endObservation := s.operations.deleteSearchJob.With(ctx, &err, opAttrs(
		attribute.Int64("id", id)))
	defer func() {
		endObservation(1, observation.Args{})
	}()

	// 🚨 SECURITY: only someone with access to the job may delete data and the db entries
	_, err = s.GetSearchJob(ctx, id)
	if err != nil {
		return err
	}

	iter, err := s.uploadStore.List(ctx, getPrefix(id))
	if err != nil {
		return err
	}
	for iter.Next() {
		key := iter.Current()
		err := s.uploadStore.Delete(ctx, key)
		// If we continued, we might end up with data in the upload store without
		// entries in the db to reference it.
		if err != nil {
			return errors.Wrapf(err, "deleting key %q", key)
		}
	}

	if err := iter.Err(); err != nil {
		return err
	}

	return s.store.DeleteExhaustiveSearchJob(ctx, id)
}

// WriteSearchJobCSV copies all CSVs associated with a search job to the given
// writer. It returns the number of bytes written and any error encountered.
func (s *Service) WriteSearchJobCSV(ctx context.Context, w io.Writer, id int64) (n int64, err error) {
	ctx, _, endObservation := s.operations.writeSearchJobCSV.With(ctx, &err, opAttrs(
		attribute.Int64("id", id)))
	defer endObservation(1, observation.Args{})

	// 🚨 SECURITY: only someone with access to the job may copy the blobs
	_, err = s.GetSearchJob(ctx, id)
	if err != nil {
		return
	}

	iter, err := s.uploadStore.List(ctx, getPrefix(id))
	if err != nil {
		return
	}

	n, err = writeSearchJobCSV(ctx, iter, s.uploadStore, w)
	if err != nil {
		return n, errors.Wrapf(err, "writing csv for job %d", id)
	}

	return n, nil
}

// GetAggregateRepoRevState returns the map of state -> count for all repo
// revision jobs for the given job.
func (s *Service) GetAggregateRepoRevState(ctx context.Context, id int64) (_ *types.RepoRevJobStats, err error) {
	ctx, _, endObservation := s.operations.getAggregateRepoRevState.With(ctx, &err, opAttrs(
		attribute.Int64("id", id)))
	defer endObservation(1, observation.Args{})

	m, err := s.store.GetAggregateRepoRevState(ctx, id)
	if err != nil {
		return nil, err
	}

	stats := types.RepoRevJobStats{}
	for state, count := range m {
		switch types.JobState(state) {
		case types.JobStateCompleted:
			stats.Completed += int32(count)
		case types.JobStateFailed:
			stats.Failed += int32(count)
		case types.JobStateProcessing, types.JobStateErrored, types.JobStateQueued:
			stats.InProgress += int32(count)
		case types.JobStateCanceled:
			stats.InProgress = 0
		default:
			return nil, errors.Newf("unknown job state %q", state)
		}
	}

	stats.Total = stats.Completed + stats.Failed + stats.InProgress

	return &stats, nil
}

// discards output from br up until delim is read. If an error is encountered
// it is returned. Note: often the error is io.EOF
func discardUntil(br *bufio.Reader, delim byte) error {
	// This function just wraps ReadSlice which will read until delim. If we
	// get the error ErrBufferFull we didn't find delim since we need to read
	// more, so we just try again. For every other error (or nil) we can
	// return it.
	for {
		_, err := br.ReadSlice(delim)
		if err != bufio.ErrBufferFull {
			return err
		}
	}
}

func writeSearchJobCSV(ctx context.Context, iter *iterator.Iterator[string], uploadStore uploadstore.Store, w io.Writer) (int64, error) {
	// keep a single bufio.Reader so we can reuse its buffer.
	var br bufio.Reader
	writeKey := func(key string, skipHeader bool) (int64, error) {
		rc, err := uploadStore.Get(ctx, key)
		if err != nil {
			_ = rc.Close()
			return 0, err
		}
		defer rc.Close()

		br.Reset(rc)

		// skip header line
		if skipHeader {
			err := discardUntil(&br, '\n')
			if err == io.EOF {
				// reached end of file before finding the newline. Write
				// nothing
				return 0, nil
			} else if err != nil {
				return 0, err
			}
		}

		return br.WriteTo(w)
	}

	// For the first blob we want the header, for the rest we don't
	skipHeader := false
	var n int64
	for iter.Next() {
		key := iter.Current()
		m, err := writeKey(key, skipHeader)
		n += m
		if err != nil {
			return n, errors.Wrapf(err, "writing csv for key %q", key)
		}
		skipHeader = true
	}

	return n, iter.Err()
}
