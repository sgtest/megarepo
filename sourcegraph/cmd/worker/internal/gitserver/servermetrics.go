package gitserver

import (
	"context"
	"time"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/worker/job"
	workerdb "github.com/sourcegraph/sourcegraph/cmd/worker/shared/init/db"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
)

type metricsJob struct {
	Logger log.Logger
}

func NewMetricsJob() job.Job {
	return &metricsJob{
		Logger: log.Scoped("gitserver-metrics", ""),
	}
}

func (j *metricsJob) Description() string {
	return ""
}

func (j *metricsJob) Config() []env.Config {
	return nil
}

func (j *metricsJob) Routines(ctx context.Context, logger log.Logger) ([]goroutine.BackgroundRoutine, error) {
	db, err := workerdb.Init()
	if err != nil {
		return nil, err
	}

	c := prometheus.NewGaugeFunc(prometheus.GaugeOpts{
		Name: "src_gitserver_repo_last_error_total",
		Help: "Number of repositories whose last_error column is not empty.",
	}, func() float64 {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()

		var count int64
		err := db.QueryRowContext(ctx, `
			SELECT COUNT(*) FROM gitserver_repos AS g
			INNER JOIN repo AS r ON g.repo_id = r.id
			WHERE g.last_error IS NOT NULL AND r.deleted_at IS NULL
		`).Scan(&count)
		if err != nil {
			j.Logger.Error("failed to count repository errors", log.Error(err))
			return 0
		}
		return float64(count)
	})
	prometheus.MustRegister(c)

	c = prometheus.NewGaugeFunc(prometheus.GaugeOpts{
		Name: "src_gitserver_repo_count",
		Help: "Number of repos.",
	}, func() float64 {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()

		var count int64
		err := db.QueryRowContext(ctx, `
			SELECT COUNT(*) FROM repo AS r
			WHERE r.deleted_at IS NULL
		`).Scan(&count)
		if err != nil {
			j.Logger.Error("failed to count repositories", log.Error(err))
			return 0
		}
		return float64(count)
	})
	prometheus.MustRegister(c)

	return nil, nil
}
