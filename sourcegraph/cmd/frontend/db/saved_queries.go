package db

import (
	"context"
	"database/sql"
	"time"

	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/pkg/dbconn"
)

type savedQueries struct{}

type SavedQueryInfo struct {
	Query        string
	LastExecuted time.Time
	LatestResult time.Time
	ExecDuration time.Duration
}

// Get gets the saved query information for the given query. nil
// is returned if there is no existing saved query info.
func (s *savedQueries) Get(ctx context.Context, query string) (*SavedQueryInfo, error) {
	info := &SavedQueryInfo{
		Query: query,
	}
	var execDurationNs int64
	err := dbconn.Global.QueryRowContext(
		ctx,
		"SELECT last_executed, latest_result, exec_duration_ns FROM saved_queries WHERE query=$1",
		query,
	).Scan(&info.LastExecuted, &info.LatestResult, &execDurationNs)
	if err != nil {
		if err == sql.ErrNoRows {
			return nil, nil
		}
		return nil, errors.Wrap(err, "QueryRow")
	}
	info.ExecDuration = time.Duration(execDurationNs)
	return info, nil
}

// Set sets the saved query information for the given info.Query.
//
// It is not safe to call concurrently for the same info.Query, as it uses a
// poor man's upsert implementation.
func (s *savedQueries) Set(ctx context.Context, info *SavedQueryInfo) error {
	res, err := dbconn.Global.ExecContext(
		ctx,
		"UPDATE saved_queries SET last_executed=$1, latest_result=$2, exec_duration_ns=$3 WHERE query=$4",
		info.LastExecuted,
		info.LatestResult,
		int64(info.ExecDuration),
		info.Query,
	)
	if err != nil {
		return errors.Wrap(err, "UPDATE")
	}
	updated, err := res.RowsAffected()
	if err != nil {
		return errors.Wrap(err, "RowsAffected")
	}
	if updated == 0 {
		// Didn't update any row, so insert a new one.
		_, err := dbconn.Global.ExecContext(
			ctx,
			"INSERT INTO saved_queries(query, last_executed, latest_result, exec_duration_ns) VALUES($1, $2, $3, $4)",
			info.Query,
			info.LastExecuted,
			info.LatestResult,
			int64(info.ExecDuration),
		)
		if err != nil {
			return errors.Wrap(err, "INSERT")
		}
	}
	return nil
}

func (s *savedQueries) Delete(ctx context.Context, query string) error {
	_, err := dbconn.Global.ExecContext(
		ctx,
		"DELETE FROM saved_queries WHERE query=$1",
		query,
	)
	return err
}
