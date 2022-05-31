package server

import (
	"context"
	"os/exec"
	"syscall"
	"time"

	"github.com/prometheus/client_golang/prometheus"

	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/log"
)

func (s *Server) RegisterMetrics(db dbutil.DB, observationContext *observation.Context) {
	// test the latency of exec, which may increase under certain memory
	// conditions
	echoDuration := prometheus.NewGauge(prometheus.GaugeOpts{
		Name: "src_gitserver_echo_duration_seconds",
		Help: "Duration of executing the echo command.",
	})
	prometheus.MustRegister(echoDuration)
	go func(server *Server) {
		for {
			time.Sleep(10 * time.Second)
			s := time.Now()
			if err := exec.Command("echo").Run(); err != nil {
				server.Logger.Warn("exec measurement failed", log.Error(err))
				continue
			}
			echoDuration.Set(time.Since(s).Seconds())
		}
	}(s)

	// report the size of the repos dir
	if s.ReposDir == "" {
		s.Logger.Error("ReposDir is not set, cannot export disk_space_available metric.")
		return
	}

	metrics.MustRegisterDiskMonitor(s.ReposDir)

	// TODO(keegan) these are older names for the above disk metric. Keeping
	// them to prevent breaking dashboards. Can remove once no
	// alert/dashboards use them.
	c := prometheus.NewGaugeFunc(prometheus.GaugeOpts{
		Name: "src_gitserver_disk_space_available",
		Help: "Amount of free space disk space on the repos mount.",
	}, func() float64 {
		var stat syscall.Statfs_t
		_ = syscall.Statfs(s.ReposDir, &stat)
		return float64(stat.Bavail * uint64(stat.Bsize))
	})
	prometheus.MustRegister(c)

	c = prometheus.NewGaugeFunc(prometheus.GaugeOpts{
		Name: "src_gitserver_disk_space_total",
		Help: "Amount of total disk space in the repos directory.",
	}, func() float64 {
		var stat syscall.Statfs_t
		_ = syscall.Statfs(s.ReposDir, &stat)
		return float64(stat.Blocks * uint64(stat.Bsize))
	})
	prometheus.MustRegister(c)

	c = prometheus.NewGaugeFunc(prometheus.GaugeOpts{
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
			s.Logger.Error("failed to count repository errors", log.Error(err))
			return 0
		}
		return float64(count)
	})
	prometheus.MustRegister(c)

	// Register uniform observability via internal/observation
	s.operations = newOperations(observationContext)
}
