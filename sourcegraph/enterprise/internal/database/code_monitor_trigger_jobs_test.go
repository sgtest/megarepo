package database

import (
	"context"
	"testing"

	"github.com/keegancsmith/sqlf"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
)

const setToCompletedFmtStr = `
UPDATE cm_trigger_jobs
SET state = 'completed',
    started_at = %s,
    finished_at = %s
WHERE id = %s;
`

const getJobIDs = `
SELECT id
FROM cm_trigger_jobs;
`

func TestDeleteOldJobLogs(t *testing.T) {
	retentionInDays := 7
	ctx, db, s := newTestStore(t)
	_, _, userCTX := newTestUser(ctx, t, db)
	_ = s.insertTestMonitor(userCTX, t)

	// Add 1 job and date it back to a long time ago.
	triggerJobs, err := s.EnqueueQueryTriggerJobs(ctx)
	require.NoError(t, err)
	require.Len(t, triggerJobs, 1)
	firstTriggerJobID := triggerJobs[0].ID

	longTimeAgo := s.Now().AddDate(0, 0, -(retentionInDays + 1))
	err = s.Exec(ctx, sqlf.Sprintf(setToCompletedFmtStr, longTimeAgo, longTimeAgo, firstTriggerJobID))
	require.NoError(t, err)

	// Add second job.
	triggerJobs, err = s.EnqueueQueryTriggerJobs(ctx)
	require.NoError(t, err)
	require.Len(t, triggerJobs, 1)
	secondTriggerJobID := triggerJobs[0].ID

	err = s.Exec(ctx, sqlf.Sprintf(setToCompletedFmtStr, s.Now(), s.Now(), secondTriggerJobID))
	require.NoError(t, err)

	err = s.DeleteOldTriggerJobs(ctx, retentionInDays)
	require.NoError(t, err)

	rows, err := s.Query(ctx, sqlf.Sprintf(getJobIDs))
	require.NoError(t, err)
	defer rows.Close()

	rowCount := 0
	var id int32
	for rows.Next() {
		rowCount++
		if rowCount > 1 {
			t.Fatalf("got more than 1 row, expected exactly 1 row")
		}
		err = rows.Scan(&id)
		require.NoError(t, err)
	}
	require.Equal(t, secondTriggerJobID, id)
}

func TestUpdateTriggerJob(t *testing.T) {
	logger := logtest.Scoped(t)
	t.Run("handles null results", func(t *testing.T) {
		ctx := context.Background()
		db := NewEnterpriseDB(database.NewDB(logger, dbtest.NewDB(logger, t)))
		_ = populateCodeMonitorFixtures(t, db)
		jobs, err := db.CodeMonitors().EnqueueQueryTriggerJobs(ctx)
		require.NoError(t, err)
		require.Len(t, jobs, 1)

		err = db.CodeMonitors().UpdateTriggerJobWithResults(ctx, jobs[0].ID, "", nil)
		require.NoError(t, err)
	})
}

func TestListTriggerJobs(t *testing.T) {
	logger := logtest.Scoped(t)
	t.Run("handles null results", func(t *testing.T) {
		ctx := context.Background()
		db := NewEnterpriseDB(database.NewDB(logger, dbtest.NewDB(logger, t)))
		f := populateCodeMonitorFixtures(t, db)
		jobs, err := db.CodeMonitors().EnqueueQueryTriggerJobs(ctx)
		require.NoError(t, err)
		require.Len(t, jobs, 1)

		js, err := db.CodeMonitors().ListQueryTriggerJobs(ctx, ListTriggerJobsOpts{QueryID: &f.Query.ID})
		require.NoError(t, err)
		require.Len(t, js, 1)
	})
}
