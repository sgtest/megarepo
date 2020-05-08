package worker

import (
	"github.com/prometheus/client_golang/prometheus"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
)

type WorkerMetrics struct {
	Jobs *metrics.OperationMetrics
}

func NewWorkerMetrics(r prometheus.Registerer) WorkerMetrics {
	jobs := metrics.NewOperationMetrics(r, "precise_code_intel_worker", "jobs")

	return WorkerMetrics{
		Jobs: jobs,
	}
}
