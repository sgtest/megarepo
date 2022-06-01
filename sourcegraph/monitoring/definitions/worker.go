package definitions

import (
	"fmt"
	"time"

	"github.com/sourcegraph/sourcegraph/monitoring/definitions/shared"
	"github.com/sourcegraph/sourcegraph/monitoring/monitoring"
)

func Worker() *monitoring.Dashboard {
	const containerName = "worker"

	workerJobs := []struct {
		Name  string
		Owner monitoring.ObservableOwner
	}{
		{Name: "codeintel-upload-janitor", Owner: monitoring.ObservableOwnerCodeIntel},
		{Name: "codeintel-commitgraph-updater", Owner: monitoring.ObservableOwnerCodeIntel},
		{Name: "codeintel-autoindexing-scheduler", Owner: monitoring.ObservableOwnerCodeIntel},
	}

	var activeJobObservables []monitoring.Observable
	for _, job := range workerJobs {
		activeJobObservables = append(activeJobObservables, monitoring.Observable{
			Name:          fmt.Sprintf("worker_job_%s_count", job.Name),
			Description:   fmt.Sprintf("number of worker instances running the %s job", job.Name),
			Query:         fmt.Sprintf(`sum (src_worker_jobs{job="worker", job_name="%s"})`, job.Name),
			Panel:         monitoring.Panel().LegendFormat(fmt.Sprintf("instances running %s", job.Name)),
			DataMustExist: true,
			Warning:       monitoring.Alert().Less(1).For(1 * time.Minute),
			Critical:      monitoring.Alert().Less(1).For(5 * time.Minute),
			Owner:         job.Owner,
			PossibleSolutions: fmt.Sprintf(`
				- Ensure your instance defines a worker container such that:
					- `+"`"+`WORKER_JOB_ALLOWLIST`+"`"+` contains "%[1]s" (or "all"), and
					- `+"`"+`WORKER_JOB_BLOCKLIST`+"`"+` does not contain "%[1]s"
				- Ensure that such a container is not failing to start or stay active
			`, job.Name),
		})
	}

	panelsPerRow := 4
	if rem := len(activeJobObservables) % panelsPerRow; rem == 1 || rem == 2 {
		// If we'd leave one or two panels on the only/last row, then reduce
		// the number of panels in previous rows so that we have less of a width
		// difference at the end
		panelsPerRow = 3
	}

	var activeJobRows []monitoring.Row
	for _, observable := range activeJobObservables {
		if n := len(activeJobRows); n == 0 || len(activeJobRows[n-1]) >= panelsPerRow {
			activeJobRows = append(activeJobRows, nil)
		}

		n := len(activeJobRows)
		activeJobRows[n-1] = append(activeJobRows[n-1], observable)
	}

	activeJobsGroup := monitoring.Group{
		Title: "Active jobs",
		Rows: append(
			[]monitoring.Row{
				{
					{
						Name:        "worker_job_count",
						Description: "number of worker instances running each job",
						Query:       `sum by (job_name) (src_worker_jobs{job="worker"})`,
						Panel:       monitoring.Panel().LegendFormat("instances running {{job_name}}"),
						NoAlert:     true,
						Interpretation: `
							The number of worker instances running each job type.
							It is necessary for each job type to be managed by at least one worker instance.
						`,
					},
				},
			},
			activeJobRows...,
		),
	}

	return &monitoring.Dashboard{
		Name:        "worker",
		Title:       "Worker",
		Description: "Manages background processes.",
		Groups: []monitoring.Group{
			// src_worker_jobs
			activeJobsGroup,

			shared.CodeIntelligence.NewCommitGraphQueueGroup(containerName),
			shared.CodeIntelligence.NewCommitGraphProcessorGroup(containerName),
			shared.CodeIntelligence.NewDependencyIndexQueueGroup(containerName),
			shared.CodeIntelligence.NewDependencyIndexProcessorGroup(containerName),
			shared.CodeIntelligence.NewJanitorGroup(containerName),
			shared.CodeIntelligence.NewIndexSchedulerGroup(containerName),
			shared.CodeIntelligence.NewAutoIndexEnqueuerGroup(containerName),
			shared.CodeIntelligence.NewDBStoreGroup(containerName),
			shared.CodeIntelligence.NewLSIFStoreGroup(containerName),
			shared.CodeIntelligence.NewDependencyIndexDBWorkerStoreGroup(containerName),
			shared.CodeIntelligence.NewGitserverClientGroup(containerName),
			shared.CodeIntelligence.NewRepoUpdaterClientGroup(containerName),
			shared.CodeIntelligence.NewDependencyReposStoreGroup(containerName),

			shared.Batches.NewDBStoreGroup(containerName),
			shared.Batches.NewServiceGroup(containerName),

			// src_codeintel_background_upload_resets_total
			// src_codeintel_background_upload_reset_failures_total
			// src_codeintel_background_upload_reset_errors_total
			shared.WorkerutilResetter.NewGroup(containerName, monitoring.ObservableOwnerCodeIntel, shared.ResetterGroupOptions{
				GroupConstructorOptions: shared.GroupConstructorOptions{
					Namespace:       "codeintel",
					DescriptionRoot: "lsif_upload record resetter",
					Hidden:          true,

					ObservableConstructorOptions: shared.ObservableConstructorOptions{
						MetricNameRoot:        "codeintel_background_upload",
						MetricDescriptionRoot: "lsif upload",
					},
				},

				RecordResets:        shared.NoAlertsOption("none"),
				RecordResetFailures: shared.NoAlertsOption("none"),
				Errors:              shared.NoAlertsOption("none"),
			}),

			// src_codeintel_background_index_resets_total
			// src_codeintel_background_index_reset_failures_total
			// src_codeintel_background_index_reset_errors_total
			shared.WorkerutilResetter.NewGroup(containerName, monitoring.ObservableOwnerCodeIntel, shared.ResetterGroupOptions{
				GroupConstructorOptions: shared.GroupConstructorOptions{
					Namespace:       "codeintel",
					DescriptionRoot: "lsif_index record resetter",
					Hidden:          true,

					ObservableConstructorOptions: shared.ObservableConstructorOptions{
						MetricNameRoot:        "codeintel_background_index",
						MetricDescriptionRoot: "lsif index",
					},
				},

				RecordResets:        shared.NoAlertsOption("none"),
				RecordResetFailures: shared.NoAlertsOption("none"),
				Errors:              shared.NoAlertsOption("none"),
			}),

			// src_codeintel_background_dependency_index_resets_total
			// src_codeintel_background_dependency_index_reset_failures_total
			// src_codeintel_background_dependency_index_reset_errors_total
			shared.WorkerutilResetter.NewGroup(containerName, monitoring.ObservableOwnerCodeIntel, shared.ResetterGroupOptions{
				GroupConstructorOptions: shared.GroupConstructorOptions{
					Namespace:       "codeintel",
					DescriptionRoot: "lsif_dependency_index record resetter",
					Hidden:          true,

					ObservableConstructorOptions: shared.ObservableConstructorOptions{
						MetricNameRoot:        "codeintel_background_dependency_index",
						MetricDescriptionRoot: "lsif dependency index",
					},
				},

				RecordResets:        shared.NoAlertsOption("none"),
				RecordResetFailures: shared.NoAlertsOption("none"),
				Errors:              shared.NoAlertsOption("none"),
			}),
			shared.CodeInsights.NewInsightsQueryRunnerQueueGroup(containerName),
			shared.CodeInsights.NewInsightsQueryRunnerWorkerGroup(containerName),
			shared.CodeInsights.NewInsightsQueryRunnerResetterGroup(containerName),
			shared.CodeInsights.NewInsightsQueryRunnerStoreGroup(containerName),
			{
				Title:  "Code Insights queue utilization",
				Hidden: true,
				Rows: []monitoring.Row{{monitoring.Observable{
					Name:              "insights_queue_unutilized_size",
					Description:       "insights queue size that is not utilized (not processing)",
					Owner:             monitoring.ObservableOwnerCodeInsights,
					Query:             "max(src_insights_search_queue_total{job=~\"^worker.*\"}) > 0 and on(job) sum by (op)(increase(src_workerutil_dbworker_store_insights_query_runner_jobs_store_total{job=~\"^worker.*\",op=\"Dequeue\"}[5m])) < 1",
					DataMustExist:     false,
					Warning:           monitoring.Alert().Greater(0.0).For(time.Minute * 30),
					PossibleSolutions: "Verify code insights worker job has successfully started. Restart worker service and monitoring startup logs, looking for worker panics.",
					Interpretation:    "Any value on this panel indicates code insights is not processing queries from its queue. This observable and alert only fire if there are records in the queue and there have been no dequeue attempts for 30 minutes.",
					Panel:             monitoring.Panel().LegendFormat("count"),
				}}},
			},

			// Resource monitoring
			shared.NewFrontendInternalAPIErrorResponseMonitoringGroup(containerName, monitoring.ObservableOwnerCodeIntel, nil),
			shared.NewDatabaseConnectionsMonitoringGroup(containerName),
			shared.NewContainerMonitoringGroup(containerName, monitoring.ObservableOwnerCodeIntel, nil),
			shared.NewProvisioningIndicatorsGroup(containerName, monitoring.ObservableOwnerCodeIntel, nil),
			shared.NewGolangMonitoringGroup(containerName, monitoring.ObservableOwnerCodeIntel, nil),
			shared.NewKubernetesMonitoringGroup(containerName, monitoring.ObservableOwnerCodeIntel, nil),
		},
	}
}
