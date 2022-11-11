package codeintel

import (
	"context"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/worker/job"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/worker/shared/init/codeintel"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/ranking"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type rankingSourcerJob struct{}

func NewRankingSourcerJob() job.Job {
	return &rankingSourcerJob{}
}

func (j *rankingSourcerJob) Description() string {
	return ""
}

func (j *rankingSourcerJob) Config() []env.Config {
	return []env.Config{
		ranking.IndexerConfigInst,
		ranking.LoaderConfigInst,
	}
}

func (j *rankingSourcerJob) Routines(startupCtx context.Context, logger log.Logger) ([]goroutine.BackgroundRoutine, error) {
	services, err := codeintel.InitServices()
	if err != nil {
		return nil, err
	}

	return append(
		ranking.NewIndexer(services.RankingService, observation.ContextWithLogger(logger)),
		ranking.NewPageRankLoader(services.RankingService, observation.ContextWithLogger(logger))...,
	), nil
}
