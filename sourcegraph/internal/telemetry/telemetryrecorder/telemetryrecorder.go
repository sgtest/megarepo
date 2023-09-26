// Package telemetryrecorder provides default constructors for telemetry
// recorders.
//
// This package partly exists to avoid dependency cycles with the database
// package and the telemetry package.
package telemetryrecorder

import (
	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/telemetry"
	"github.com/sourcegraph/sourcegraph/internal/telemetry/teestore"
)

// New creates a default EventRecorder for Telemetry V2, which exports recorded
// events to Sourcegraph's Telemetry Gateway service.
//
// The current defaults tee events to both the legacy event_logs table, as well
// as the new Telemetry Gateway export queue.
func New(db database.DB) *telemetry.EventRecorder {
	return telemetry.NewEventRecorder(teestore.NewStore(db.TelemetryEventsExportQueue(), db.EventLogs()))
}

// New creates a default BestEffortEventRecorder for Telemetry V2, which exports
// recorded events to Sourcegraph's Telemetry Gateway service while logging any
// recording errors and swallowing them.
//
// The current defaults tee events to both the legacy event_logs table, as well
// as the new Telemetry Gateway export queue.
func NewBestEffort(logger log.Logger, db database.DB) *telemetry.BestEffortEventRecorder {
	return telemetry.NewBestEffortEventRecorder(
		logger.Scoped("telemetry", "telemetry event recorder"),
		New(db))
}
