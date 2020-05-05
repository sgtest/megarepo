package metrics

import (
	"fmt"

	"github.com/prometheus/client_golang/prometheus"
)

// OperationMetrics contains three common metrics for any operation.
type OperationMetrics struct {
	Duration *prometheus.HistogramVec // How long did it take?
	Count    *prometheus.CounterVec   // How many things were processed?
	Errors   *prometheus.CounterVec   // How many errors occurred?
}

// Observe registers an observation of a single operation.
func (m *OperationMetrics) Observe(secs, count float64, err *error, lvals ...string) {
	if m == nil {
		return
	}

	m.Duration.WithLabelValues(lvals...).Observe(secs)
	m.Count.WithLabelValues(lvals...).Add(count)
	if err != nil && *err != nil {
		m.Errors.WithLabelValues(lvals...).Add(1)
	}
}

// MustRegister registers all metrics in OperationMetrics in the given
// prometheus.Registerer. It panics in case of failure.
func (m *OperationMetrics) MustRegister(r prometheus.Registerer) {
	r.MustRegister(m.Duration)
	r.MustRegister(m.Count)
	r.MustRegister(m.Errors)
}

type operationMetricOptions struct {
	durationHelp string
	countHelp    string
	errorsHelp   string
	labels       []string
}

// OperationMetricsOption alter the default behavior of NewOperationMetrics.
type OperationMetricsOption func(o *operationMetricOptions)

// WithDurationHelp overrides the default help text for duration metrics.
func WithDurationHelp(text string) OperationMetricsOption {
	return func(o *operationMetricOptions) { o.durationHelp = text }
}

// WithCountHelp overrides the default help text for count metrics.
func WithCountHelp(text string) OperationMetricsOption {
	return func(o *operationMetricOptions) { o.countHelp = text }
}

// WithErrorsHelp overrides the default help text for errors metrics.
func WithErrorsHelp(text string) OperationMetricsOption {
	return func(o *operationMetricOptions) { o.errorsHelp = text }
}

// WithLabels overrides the default labels for all metrics.
func WithLabels(labels ...string) OperationMetricsOption {
	return func(o *operationMetricOptions) { o.labels = labels }
}

// NewOperationMetrics creates an OperationMetrics value. The supplied metricPrefix should
// be underscore_cased as it is used in the metric name.
func NewOperationMetrics(subsystem, metricPrefix string, fns ...OperationMetricsOption) *OperationMetrics {
	options := &operationMetricOptions{
		durationHelp: fmt.Sprintf("Time in seconds spent performing %s operations", metricPrefix),
		countHelp:    fmt.Sprintf("Total number of %s operations", metricPrefix),
		errorsHelp:   fmt.Sprintf("Total number of errors when performing %s operations", metricPrefix),
		labels:       nil,
	}

	for _, fn := range fns {
		fn(options)
	}

	return &OperationMetrics{
		Duration: prometheus.NewHistogramVec(prometheus.HistogramOpts{
			Namespace: "src",
			Subsystem: subsystem,
			Name:      fmt.Sprintf("%s_duration_seconds", metricPrefix),
			Help:      options.durationHelp,
		}, options.labels),
		Count: prometheus.NewCounterVec(prometheus.CounterOpts{
			Namespace: "src",
			Subsystem: subsystem,
			Name:      fmt.Sprintf("%s_total", metricPrefix),
			Help:      options.countHelp,
		}, options.labels),
		Errors: prometheus.NewCounterVec(prometheus.CounterOpts{
			Namespace: "src",
			Subsystem: subsystem,
			Name:      fmt.Sprintf("%s_errors_total", metricPrefix),
			Help:      options.errorsHelp,
		}, options.labels),
	}
}
