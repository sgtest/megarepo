package insights

import (
	"context"

	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/cmd/worker/job"
	"github.com/sourcegraph/sourcegraph/cmd/worker/workerdb"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/background"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type insightsJob struct{}

func (s *insightsJob) Config() []env.Config {
	return nil
}

func (s *insightsJob) Routines(ctx context.Context) ([]goroutine.BackgroundRoutine, error) {
	if !insights.IsEnabled() {
		log15.Info("Code Insights Disabled.")
		return []goroutine.BackgroundRoutine{}, nil
	}
	log15.Info("Code Insights Enabled.")

	mainAppDb, err := workerdb.Init()
	if err != nil {
		return nil, err
	}

	authz.DefaultSubRepoPermsChecker, err = authz.NewSubRepoPermsClient(database.SubRepoPerms(mainAppDb))
	if err != nil {
		return nil, errors.Errorf("Failed to create sub-repo client: %v", err)
	}

	insightsDB, err := insights.InitializeCodeInsightsDB("worker")
	if err != nil {
		return nil, err
	}

	return background.GetBackgroundJobs(context.Background(), mainAppDb, insightsDB), nil
}

func NewInsightsJob() job.Job {
	return &insightsJob{}
}
