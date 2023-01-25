package insights

import (
	"context"

	"github.com/sourcegraph/sourcegraph/cmd/worker/job"
	workerdb "github.com/sourcegraph/sourcegraph/cmd/worker/shared/init/db"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/background"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type insightsQueryRunnerJob struct {
	env.BaseConfig
}

func (s *insightsQueryRunnerJob) Description() string {
	return ""
}

func (s *insightsQueryRunnerJob) Config() []env.Config {
	return nil
}

func (s *insightsQueryRunnerJob) Routines(_ context.Context, observationCtx *observation.Context) ([]goroutine.BackgroundRoutine, error) {
	if !insights.IsEnabled() {
		observationCtx.Logger.Debug("Code Insights disabled. Disabling query runner.")
		return []goroutine.BackgroundRoutine{}, nil
	}
	observationCtx.Logger.Debug("Code Insights enabled. Enabling query runner.")

	db, err := workerdb.InitDB(observationCtx)
	if err != nil {
		return nil, err
	}

	insightsDB, err := insights.InitializeCodeInsightsDB(observationCtx, "query-runner-worker")
	if err != nil {
		return nil, err
	}

	return background.GetBackgroundQueryRunnerJob(context.Background(), observationCtx.Logger, db, insightsDB), nil
}

func NewInsightsQueryRunnerJob() job.Job {
	return &insightsQueryRunnerJob{}
}
