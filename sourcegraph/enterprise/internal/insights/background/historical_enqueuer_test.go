package background

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/hexops/autogold"
	"golang.org/x/time/rate"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/background/queryrunner"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/compression"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/discovery"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/store"
	itypes "github.com/sourcegraph/sourcegraph/enterprise/internal/insights/types"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/featureflag"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/ratelimit"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

type testParams struct {
	settings              *api.Settings
	numRepos              int
	frames                int
	recordSleepOperations bool
	haveData              bool
	deprecateJITFeature   bool
}

type testResults struct {
	allReposIteratorCalls int
	reposGetByName        int
	operations            []string
}

func testHistoricalEnqueuer(t *testing.T, p *testParams) *testResults {
	r := &testResults{}
	ctx := context.Background()
	clock := func() time.Time {
		baseNow, err := time.Parse(time.RFC3339, "2021-01-01T00:00:01Z")
		if err != nil {
			panic(err)
		}
		return baseNow
	}

	mockedFeatureFlag := featureflag.FeatureFlag{
		Name:      "code_insights_deprecate_jit",
		Bool:      &featureflag.FeatureFlagBool{Value: p.deprecateJITFeature},
		Rollout:   nil,
		CreatedAt: time.Now(),
		UpdatedAt: time.Now(),
		DeletedAt: nil,
	}
	featureFlagStore := database.NewMockFeatureFlagStore()

	featureFlagStore.GetFeatureFlagFunc.SetDefaultReturn(&mockedFeatureFlag, nil)

	settingStore := discovery.NewMockSettingStore()
	if p.settings != nil {
		settingStore.GetLatestFunc.SetDefaultReturn(p.settings, nil)
	}

	dataSeriesStore := store.NewMockDataSeriesStore()
	dataSeriesStore.GetDataSeriesFunc.SetDefaultReturn([]itypes.InsightSeries{
		{
			ID:                  1,
			SeriesID:            "series1",
			Query:               "query1",
			NextRecordingAfter:  clock().Add(-1 * time.Hour),
			CreatedAt:           clock(),
			OldestHistoricalAt:  clock().Add(-time.Hour * 24 * 365),
			SampleIntervalUnit:  string(itypes.Month),
			SampleIntervalValue: 1,
		},
		{
			ID:                  2,
			SeriesID:            "series2",
			Query:               "query2",
			NextRecordingAfter:  clock().Add(1 * time.Hour),
			CreatedAt:           clock(),
			OldestHistoricalAt:  clock().Add(-time.Hour * 24 * 365),
			SampleIntervalUnit:  string(itypes.Month),
			SampleIntervalValue: 1,
		},
	}, nil)

	dataSeriesStore.IncrementBackfillAttemptsFunc.SetDefaultHook(func(ctx context.Context, is itypes.InsightSeries) error {
		r.operations = append(r.operations, fmt.Sprintf("IncrementBackfillAttemptsFunc(seriesId=%v)", is.SeriesID))
		return nil
	})

	dataSeriesStore.StampBackfillFunc.SetDefaultHook(func(ctx context.Context, is itypes.InsightSeries) (itypes.InsightSeries, error) {
		r.operations = append(r.operations, fmt.Sprintf("StampBackfillFunc(seriesId=%v)", is.SeriesID))
		is.BackfillQueuedAt = time.Now()
		return is, nil
	})

	dataFrameFilter := compression.NoopFilter{}

	insightsStore := store.NewMockInterface()
	insightsStore.CountDataFunc.SetDefaultHook(func(ctx context.Context, opts store.CountDataOpts) (int, error) {
		if p.haveData {
			return 100, nil
		}
		return 0, nil
	})
	insightsStore.RecordSeriesPointFunc.SetDefaultHook(func(ctx context.Context, args store.RecordSeriesPointArgs) error {
		r.operations = append(r.operations, fmt.Sprintf("recordSeriesPoint(point=%v, repoName=%v)", args.Point.String(), *args.RepoName))
		return nil
	})

	repoStore := NewMockRepoStore()
	repos := map[api.RepoName]*types.Repo{}
	for i := 0; i < p.numRepos; i++ {
		name := api.RepoName(fmt.Sprintf("repo/%d", i))
		repos[name] = &types.Repo{
			ID:   api.RepoID(i),
			Name: name,
		}
	}
	repoStore.GetByNameFunc.SetDefaultHook(func(ctx context.Context, name api.RepoName) (*types.Repo, error) {
		r.reposGetByName++
		return repos[name], nil
	})

	enqueueQueryRunnerJob := func(ctx context.Context, job *queryrunner.Job) error {
		r.operations = append(r.operations, fmt.Sprintf(`enqueueQueryRunnerJob("%s", "%s")`, job.RecordTime.Format(time.RFC3339), job.SearchQuery))
		return nil
	}

	allReposIterator := func(ctx context.Context, each func(repoName string, id api.RepoID) error) error {
		r.allReposIteratorCalls++
		for i := 0; i < p.numRepos; i++ {
			if err := each(fmt.Sprintf("repo/%d", i), api.RepoID(i)); err != nil {
				return err
			}
		}
		return nil
	}

	gitFirstEverCommit := func(ctx context.Context, db database.DB, repoName api.RepoName) (*gitdomain.Commit, error) {
		if repoName == "repo/1" {
			daysAgo := clock().Add(-3 * 24 * time.Hour)
			return &gitdomain.Commit{Committer: &gitdomain.Signature{Date: daysAgo}}, nil
		}
		yearsAgo := clock().Add(-2 * 365 * 24 * time.Hour)
		return &gitdomain.Commit{Committer: &gitdomain.Signature{Date: yearsAgo}}, nil
	}

	gitFindRecentCommit := func(ctx context.Context, repoName api.RepoName, target time.Time) ([]*gitdomain.Commit, error) {
		nearby := target.Add(-2 * 24 * time.Hour)
		return []*gitdomain.Commit{{Committer: &gitdomain.Signature{Date: nearby}}}, nil
	}

	limiter := ratelimit.NewInstrumentedLimiter("TestHistoricalEnqueuer", rate.NewLimiter(10, 1))

	stats := make(statistics)
	analyzer := backfillAnalyzer{
		statistics:  stats,
		frameFilter: &dataFrameFilter,

		limiter:             limiter,
		gitFirstEverCommit:  gitFirstEverCommit,
		gitFindRecentCommit: gitFindRecentCommit,
	}

	historicalEnqueuer := &historicalEnqueuer{
		now:                   clock,
		insightsStore:         insightsStore,
		enqueueQueryRunnerJob: enqueueQueryRunnerJob,
		repoIterator:          allReposIterator,
		analyzer:              analyzer,
		dataSeriesStore:       dataSeriesStore,
		statistics:            stats,
		featureFlagStore:      featureFlagStore,
	}

	// If we do an iteration without any insights or repos, we should expect no sleep calls to be made.
	if err := historicalEnqueuer.Handler(ctx); err != nil {
		t.Fatal(err)
	}
	return r
}

func Test_historicalEnqueuer(t *testing.T) {
	// Test that when no insights are defined, no work or sleeping is performed.
	t.Run("no_insights_no_repos", func(t *testing.T) {
		want := autogold.Want("no_insights_no_repos", &testResults{allReposIteratorCalls: 1, operations: []string{
			"IncrementBackfillAttemptsFunc(seriesId=series1)",
			"IncrementBackfillAttemptsFunc(seriesId=series2)",
			"StampBackfillFunc(seriesId=series1)",
			"StampBackfillFunc(seriesId=series2)",
		}})
		want.Equal(t, testHistoricalEnqueuer(t, &testParams{}))
	})

	// Test that when insights are defined, but no repos exist, no work or sleeping is performed.
	t.Run("some_insights_no_repos", func(t *testing.T) {
		want := autogold.Want("some_insights_no_repos", &testResults{allReposIteratorCalls: 1, operations: []string{
			"IncrementBackfillAttemptsFunc(seriesId=series1)",
			"IncrementBackfillAttemptsFunc(seriesId=series2)",
			"StampBackfillFunc(seriesId=series1)",
			"StampBackfillFunc(seriesId=series2)",
		}})
		want.Equal(t, testHistoricalEnqueuer(t, &testParams{
			settings: testRealGlobalSettings,
		}))
	})

	// Test that when insights AND repos exist:
	//
	// * We enqueue a job for every timeframe*repo*series
	// * repo/1 is only enqueued once, because its oldest commit is 3 days ago.
	// * repo/1 has zero data points directly recorded for points in time before its oldest commit.
	// * We enqueue jobs to build out historical data in most-recent to oldest order.
	//
	t.Run("no_data", func(t *testing.T) {
		want := autogold.Want("no_data", &testResults{allReposIteratorCalls: 1, operations: []string{
			"IncrementBackfillAttemptsFunc(seriesId=series1)",
			"IncrementBackfillAttemptsFunc(seriesId=series2)",
			`enqueueQueryRunnerJob("2021-01-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-12-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-11-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-10-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-09-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-08-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-07-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-06-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-05-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-04-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-03-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-02-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2021-01-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-12-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-11-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-10-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-09-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-08-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-07-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-06-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-05-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-04-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-03-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2020-02-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/0$@")`,
			`enqueueQueryRunnerJob("2021-01-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-12-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-11-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-10-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-09-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-08-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-07-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-06-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-05-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-04-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-03-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-02-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query1 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2021-01-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-12-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-11-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-10-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-09-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-08-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-07-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-06-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-05-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-04-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-03-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/1$@")`,
			`enqueueQueryRunnerJob("2020-02-01T00:00:00Z", "fork:no archived:no patterntype:literal count:99999999 query2 repo:^repo/1$@")`,
			"StampBackfillFunc(seriesId=series1)",
			"StampBackfillFunc(seriesId=series2)",
		}})
		want.Equal(t, testHistoricalEnqueuer(t, &testParams{
			settings:              testRealGlobalSettings,
			numRepos:              2,
			frames:                2,
			recordSleepOperations: true,
		}))
	})

}
