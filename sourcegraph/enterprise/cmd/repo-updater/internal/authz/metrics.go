package authz

import (
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
)

// The metrics that are exposed to Prometheus.
var (
	metricsOutdatedPerms = promauto.NewGaugeVec(prometheus.GaugeOpts{
		Name: "src_repoupdater_perms_syncer_outdated_perms",
		Help: "The number of records that have outdated permissions",
	}, []string{"type"})
	metricsNoPerms = promauto.NewGaugeVec(prometheus.GaugeOpts{
		Name: "src_repoupdater_perms_syncer_no_perms",
		Help: "The number of records that do not have any permissions",
	}, []string{"type"})
	metricsStalePerms = promauto.NewGaugeVec(prometheus.GaugeOpts{
		Name: "src_repoupdater_perms_syncer_stale_perms",
		Help: "The number of records that have stale permissions",
	}, []string{"type"})
	metricsStrictStalePerms = promauto.NewGaugeVec(prometheus.GaugeOpts{
		Name: "src_repoupdater_perms_syncer_strict_stale_perms",
		Help: "The number of records that have permissions older than 1h",
	}, []string{"type"})
	metricsPermsGap = promauto.NewGaugeVec(prometheus.GaugeOpts{
		Name: "src_repoupdater_perms_syncer_perms_gap_seconds",
		Help: "The time gap between least and most up to date permissions",
	}, []string{"type"})
	metricsSyncDuration = promauto.NewHistogramVec(prometheus.HistogramOpts{
		Name:    "src_repoupdater_perms_syncer_sync_duration_seconds",
		Help:    "Time spent on syncing permissions",
		Buckets: []float64{1, 2, 5, 10, 30, 60, 120},
	}, []string{"type", "success"})
	metricsSyncErrors = promauto.NewCounterVec(prometheus.CounterOpts{
		Name: "src_repoupdater_perms_syncer_sync_errors_total",
		Help: "Total number of permissions sync errors",
	}, []string{"type"})
	metricsQueueSize = promauto.NewGauge(prometheus.GaugeOpts{
		Name: "src_repoupdater_perms_syncer_queue_size",
		Help: "The size of the sync request queue",
	})
	metricsRateLimiterWaitDuration = promauto.NewHistogramVec(prometheus.HistogramOpts{
		Name:    "src_repoupdater_perms_syncer_sync_wait_duration_seconds",
		Help:    "Time spent waiting on rate-limiter to sync permissions",
		Buckets: []float64{0.1, 0.2, 0.5, 1, 2, 5, 10, 30, 60, 120},
	}, []string{"type", "success"})

	metricsConcurrentSyncs = promauto.NewGaugeVec(prometheus.GaugeOpts{
		Name: "src_repoupdater_perms_syncer_concurrent_syncs",
		Help: "The number of concurrent permissions syncs",
	}, []string{"type"})
)
