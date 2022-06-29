package backend

import (
	"context"
	"fmt"
	"strconv"
	"time"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	tracepkg "github.com/sourcegraph/sourcegraph/internal/trace"
)

var metricLabels = []string{"method", "success"}
var requestDuration = promauto.NewHistogramVec(prometheus.HistogramOpts{
	Name:    "src_backend_client_request_duration_seconds",
	Help:    "Total time spent on backend endpoints.",
	Buckets: tracepkg.UserLatencyBuckets,
}, metricLabels)

var requestGauge = promauto.NewGaugeVec(prometheus.GaugeOpts{
	Name: "src_backend_client_requests",
	Help: "Current number of requests running for a method.",
}, []string{"method"})

//nolint:unparam // unparam complains that `server` always has same value across call-sites, but that's OK
func trace(ctx context.Context, server, method string, arg any, err *error) (context.Context, func()) {
	requestGauge.WithLabelValues(server + "." + method).Inc()

	span, ctx := tracepkg.New(ctx, server+"."+method, "")
	span.SetTag("Server", server)
	span.SetTag("Method", method)
	span.SetTag("Argument", fmt.Sprintf("%#v", arg))
	start := time.Now()

	done := func() {
		elapsed := time.Since(start)
		span.SetTag("UserID", actor.FromContext(ctx).UID)

		if err != nil && *err != nil {
			span.SetTag("Error", (*err).Error())
		}
		span.Finish()

		name := server + "." + method
		labels := prometheus.Labels{
			"method":  name,
			"success": strconv.FormatBool(err == nil),
		}
		requestDuration.With(labels).Observe(elapsed.Seconds())
		requestGauge.WithLabelValues(name).Dec()
	}

	return ctx, done
}
