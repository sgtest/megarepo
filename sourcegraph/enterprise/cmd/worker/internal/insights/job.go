package insights

import (
	"context"

	"github.com/inconshreveable/log15"
	"github.com/sourcegraph/sourcegraph/cmd/worker/shared"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/background"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
)

type insightsBaseConfig struct {
	env.BaseConfig

	enabled bool
}

func (i *insightsBaseConfig) Load() {
	i.enabled = insights.IsEnabled()
}

type insightsJob struct {
	env.BaseConfig
}

var insightsConfigInst = &insightsBaseConfig{}

func (s *insightsJob) Config() []env.Config {
	return []env.Config{insightsConfigInst}
}

func (s *insightsJob) Routines(ctx context.Context) ([]goroutine.BackgroundRoutine, error) {
	if !insightsConfigInst.enabled {
		log15.Info("Code Insights Disabled.")
		return []goroutine.BackgroundRoutine{}, nil
	}
	log15.Info("Code Insights Enabled.")

	mainAppDb, err := shared.InitDatabase()
	if err != nil {
		return nil, err
	}
	insightsDB, err := insights.InitializeCodeInsightsDB("worker")
	if err != nil {
		return nil, err
	}

	return background.GetBackgroundJobs(context.Background(), mainAppDb, insightsDB), nil
}

func NewInsightsJob() shared.Job {
	return &insightsJob{}
}
