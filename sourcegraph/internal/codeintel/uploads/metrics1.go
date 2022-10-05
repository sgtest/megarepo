package uploads

import (
	"github.com/prometheus/client_golang/prometheus"

	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type janitorMetrics struct {
	// Data retention metrics
	numAuditLogRecordsExpired prometheus.Counter
	numErrors                 prometheus.Counter
	numIndexRecordsRemoved    prometheus.Counter
	numUploadRecordsRemoved   prometheus.Counter
	numUploadsPurged          prometheus.Counter
}

func newJanitorMetrics(observationContext *observation.Context) *janitorMetrics {
	counter := func(name, help string) prometheus.Counter {
		counter := prometheus.NewCounter(prometheus.CounterOpts{
			Name: name,
			Help: help,
		})

		observationContext.Registerer.MustRegister(counter)
		return counter
	}

	numAuditLogRecordsExpired := counter(
		"src_codeintel_background_audit_log_records_expired_total",
		"The number of audit log records removed due to age.",
	)
	numErrors := counter(
		// TODO - make a dashboard pre-merge
		// TODO - rename things here similar to this
		"src_codeintel_uploads_background_cleanup_errors_total",
		"The number of errors that occur during a codeintel expiration job.",
	)
	numIndexRecordsRemoved := counter(
		"src_codeintel_background_index_records_removed_total",
		"The number of codeintel index records removed.",
	)
	numUploadRecordsRemoved := counter(
		"src_codeintel_background_upload_records_removed_total",
		"The number of codeintel upload records removed.",
	)
	numUploadsPurged := counter(
		"src_codeintel_background_uploads_purged_total",
		"The number of uploads for which records in the codeintel database were removed.",
	)

	return &janitorMetrics{
		numAuditLogRecordsExpired: numAuditLogRecordsExpired,
		numErrors:                 numErrors,
		numIndexRecordsRemoved:    numIndexRecordsRemoved,
		numUploadRecordsRemoved:   numUploadRecordsRemoved,
		numUploadsPurged:          numUploadsPurged,
	}
}
