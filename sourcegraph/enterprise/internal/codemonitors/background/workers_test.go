package background

import (
	"context"
	"testing"
	"time"

	"github.com/graph-gophers/graphql-go/relay"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/log/logtest"

	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
)

func TestActionRunner(t *testing.T) {
	logger := logtest.Scoped(t)
	tests := []struct {
		name           string
		results        []*result.CommitMatch
		wantNumResults int
		wantResults    []*DisplayResult
	}{
		{
			name:           "9 results",
			results:        []*result.CommitMatch{&diffResultMock, &commitResultMock, &diffResultMock, &commitResultMock, &diffResultMock, &commitResultMock},
			wantNumResults: 9,
			wantResults:    []*DisplayResult{diffDisplayResultMock, commitDisplayResultMock, diffDisplayResultMock},
		},
		{
			name:           "1 result",
			results:        []*result.CommitMatch{&commitResultMock},
			wantNumResults: 1,
			wantResults:    []*DisplayResult{commitDisplayResultMock},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			db := database.NewDB(logger, dbtest.NewDB(logger, t))
			testQuery := "test patternType:literal"
			externalURL := "https://www.sourcegraph.com"

			// Mocks.
			got := TemplateDataNewSearchResults{}
			MockSendEmailForNewSearchResult = func(ctx context.Context, db database.DB, userID int32, data *TemplateDataNewSearchResults) error {
				got = *data
				return nil
			}

			// Create a TestStore.
			now := time.Now()
			clock := func() time.Time { return now }
			s := edb.CodeMonitorsWithClock(db, clock)
			ctx, ts := edb.NewTestStore(t, db)

			_, _, _, userCtx := edb.NewTestUser(ctx, t, db)

			// Run a complete pipeline from creation of a code monitor to sending of an email.
			_, err := ts.InsertTestMonitor(userCtx, t)
			require.NoError(t, err)

			triggerJobs, err := ts.EnqueueQueryTriggerJobs(ctx)
			require.NoError(t, err)
			require.Len(t, triggerJobs, 1)
			triggerEventID := triggerJobs[0].ID

			err = ts.UpdateTriggerJobWithResults(ctx, triggerEventID, testQuery, tt.results)
			require.NoError(t, err)

			_, err = ts.EnqueueActionJobsForMonitor(ctx, 1, triggerEventID)
			require.NoError(t, err)

			record, err := ts.GetActionJob(ctx, 1)
			require.NoError(t, err)

			a := actionRunner{s}
			err = a.Handle(ctx, logtest.Scoped(t), record)
			require.NoError(t, err)

			wantResultsPluralized := "results"
			if tt.wantNumResults == 1 {
				wantResultsPluralized = "result"
			}
			wantTruncatedCount := 0
			if tt.wantNumResults > 5 {
				wantTruncatedCount = tt.wantNumResults - 5
			}
			wantTruncatedResultsPluralized := "results"
			if wantTruncatedCount == 1 {
				wantTruncatedResultsPluralized = "result"
			}

			want := TemplateDataNewSearchResults{
				Priority:                  "",
				SearchURL:                 externalURL + "/search?q=test+patternType%3Aliteral&utm_source=code-monitoring-email",
				Description:               "test description",
				CodeMonitorURL:            externalURL + "/code-monitoring/" + string(relay.MarshalID("CodeMonitor", 1)) + "?utm_source=code-monitoring-email",
				TotalCount:                tt.wantNumResults,
				ResultPluralized:          wantResultsPluralized,
				TruncatedCount:            wantTruncatedCount,
				TruncatedResultPluralized: wantTruncatedResultsPluralized,
				TruncatedResults:          tt.wantResults,
			}

			want.TotalCount = tt.wantNumResults
			require.Equal(t, want, got)
		})
	}
}
