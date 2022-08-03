package shared

import (
	"fmt"
	"time"

	"github.com/sourcegraph/sourcegraph/monitoring/monitoring"
)

// Golang monitoring overviews.
//
// Uses metrics exported by the Prometheus Golang library, so is available on all
// deployment types.
const TitleGolangMonitoring = "Golang runtime monitoring"

var (
	GoGoroutines = func(jobLabel, instanceLabel string) sharedObservable {
		return func(containerName string, owner monitoring.ObservableOwner) Observable {
			return Observable{
				Name:           "go_goroutines",
				Description:    "maximum active goroutines",
				Query:          fmt.Sprintf(`max by(%s) (go_goroutines{%s=~".*%s"})`, instanceLabel, jobLabel, containerName),
				Warning:        monitoring.Alert().GreaterOrEqual(10000).For(10 * time.Minute),
				Panel:          monitoring.Panel().LegendFormat("{{name}}"),
				Owner:          owner,
				Interpretation: "A high value here indicates a possible goroutine leak.",
				NextSteps:      "none",
			}
		}
	}

	GoGcDuration = func(jobLabel, instanceLabel string) sharedObservable {
		return func(containerName string, owner monitoring.ObservableOwner) Observable {
			return Observable{
				Name:        "go_gc_duration_seconds",
				Description: "maximum go garbage collection duration",
				Query:       fmt.Sprintf(`max by(%s) (go_gc_duration_seconds{%s=~".*%s"})`, instanceLabel, jobLabel, containerName),
				Warning:     monitoring.Alert().GreaterOrEqual(2),
				Panel:       monitoring.Panel().LegendFormat("{{name}}").Unit(monitoring.Seconds),
				Owner:       owner,
				NextSteps:   "none",
			}
		}
	}
)

type GolangMonitoringOptions struct {
	// Goroutines transforms the default observable used to construct the Go goroutines count panel.
	Goroutines ObservableOption

	// GCDuration transforms the default observable used to construct the Go GC duration panel.
	GCDuration ObservableOption

	JobLabelName string

	InstanceLabelName string
}

// NewGolangMonitoringGroup creates a group containing panels displaying Go monitoring
// metrics for the given container.
func NewGolangMonitoringGroup(containerName string, owner monitoring.ObservableOwner, options *GolangMonitoringOptions) monitoring.Group {
	if options == nil {
		options = &GolangMonitoringOptions{}
	}

	if options.InstanceLabelName == "" {
		options.InstanceLabelName = "instance"
	}
	if options.JobLabelName == "" {
		options.JobLabelName = "job"
	}

	return monitoring.Group{
		Title:  TitleGolangMonitoring,
		Hidden: true,
		Rows: []monitoring.Row{
			{
				options.Goroutines.safeApply(GoGoroutines(options.JobLabelName, options.InstanceLabelName)(containerName, owner)).Observable(),
				options.GCDuration.safeApply(GoGcDuration(options.JobLabelName, options.InstanceLabelName)(containerName, owner)).Observable(),
			},
		},
	}
}
