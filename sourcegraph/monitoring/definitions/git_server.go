package definitions

import (
	"time"

	"github.com/sourcegraph/sourcegraph/monitoring/definitions/shared"
	"github.com/sourcegraph/sourcegraph/monitoring/monitoring"
)

func GitServer() *monitoring.Container {
	const containerName = "gitserver"

	gitserverHighMemoryNoAlertTransformer := func(observable shared.Observable) shared.Observable {
		return observable.WithNoAlerts(`Git Server is expected to use up all the memory it is provided.`)
	}

	provisioningIndicatorsOptions := &shared.ContainerProvisioningIndicatorsGroupOptions{
		LongTermMemoryUsage:  gitserverHighMemoryNoAlertTransformer,
		ShortTermMemoryUsage: gitserverHighMemoryNoAlertTransformer,
	}

	return &monitoring.Container{
		Name:        "gitserver",
		Title:       "Git Server",
		Description: "Stores, manages, and operates Git repositories.",
		Variables: []monitoring.ContainerVariable{
			{
				Label:        "Shard",
				Name:         "shard",
				OptionsQuery: "label_values(src_gitserver_exec_running, instance)",
				Multi:        true,
			},
		},
		Groups: []monitoring.Group{
			{
				Title: "General",
				Rows: []monitoring.Row{
					{
						{
							Name:        "memory_working_set",
							Description: "memory working set",
							Query:       "sum by (container_label_io_kubernetes_pod_name) (container_memory_working_set_bytes{container_label_io_kubernetes_container_name=\"gitserver\", container_label_io_kubernetes_pod_name=~`${shard:regex}`})",
							NoAlert:     true,
							Panel: monitoring.Panel().LegendFormat("{{container_label_io_kubernetes_pod_name}}").
								Unit(monitoring.Bytes).
								With(monitoring.PanelOptions.LegendOnRight()),
							Owner: monitoring.ObservableOwnerRepoManagement,
							Interpretation: `
						`,
						},
						{
							Name:        "go_routines",
							Description: "go routines",
							Query:       "go_goroutines{app=\"gitserver\", instance=~`${shard:regex}`}",
							NoAlert:     true,
							Panel: monitoring.Panel().LegendFormat("{{instance}}").
								With(monitoring.PanelOptions.LegendOnRight()),
							Owner: monitoring.ObservableOwnerRepoManagement,
							Interpretation: `
						`,
						},
					},
					{
						{
							Name:        "cpu_throttling_time",
							Description: "container CPU throttling time %",
							Query:       "sum by (container_label_io_kubernetes_pod_name) ((rate(container_cpu_cfs_throttled_periods_total{container_label_io_kubernetes_container_name=\"gitserver\", container_label_io_kubernetes_pod_name=~`${shard:regex}`}[5m]) / rate(container_cpu_cfs_periods_total{container_label_io_kubernetes_container_name=\"gitserver\", container_label_io_kubernetes_pod_name=~`${shard:regex}`}[5m])) * 100)",
							NoAlert:     true,
							Panel: monitoring.Panel().LegendFormat("{{container_label_io_kubernetes_pod_name}}").
								Unit(monitoring.Percentage).
								With(monitoring.PanelOptions.LegendOnRight()),
							Owner: monitoring.ObservableOwnerRepoManagement,
							Interpretation: `
						`,
						},
						{
							Name:        "cpu_usage_seconds",
							Description: "cpu usage seconds",
							Query:       "sum by (container_label_io_kubernetes_pod_name) (rate(container_cpu_usage_seconds_total{container_label_io_kubernetes_container_name=\"gitserver\", container_label_io_kubernetes_pod_name=~`${shard:regex}`}[5m]))",
							NoAlert:     true,
							Panel: monitoring.Panel().LegendFormat("{{container_label_io_kubernetes_pod_name}}").
								With(monitoring.PanelOptions.LegendOnRight()),
							Owner: monitoring.ObservableOwnerRepoManagement,
							Interpretation: `
						`,
						},
					},
					{
						{
							Name:        "disk_space_remaining",
							Description: "disk space remaining by instance",
							Query:       `(src_gitserver_disk_space_available / src_gitserver_disk_space_total) * 100`,
							Warning:     monitoring.Alert().LessOrEqual(25),
							Critical:    monitoring.Alert().LessOrEqual(15),
							Panel: monitoring.Panel().LegendFormat("{{instance}}").
								Unit(monitoring.Percentage).
								With(monitoring.PanelOptions.LegendOnRight()),
							Owner: monitoring.ObservableOwnerRepoManagement,
							PossibleSolutions: `
								- **Provision more disk space:** Sourcegraph will begin deleting least-used repository clones at 10% disk space remaining which may result in decreased performance, users having to wait for repositories to clone, etc.
							`,
						},
					},
					{
						{
							Name:        "io_reads_total",
							Description: "i/o reads total",
							Query:       "sum by (container_label_io_kubernetes_container_name) (rate(container_fs_reads_total{container_label_io_kubernetes_container_name=\"gitserver\"}[5m]))",
							NoAlert:     true,
							Panel: monitoring.Panel().LegendFormat("{{container_label_io_kubernetes_pod_name}}").
								Unit(monitoring.ReadsPerSecond).
								With(monitoring.PanelOptions.LegendOnRight()),
							Owner: monitoring.ObservableOwnerRepoManagement,
							Interpretation: `
						`,
						},
						{
							Name:        "io_writes_total",
							Description: "i/o writes total",
							Query:       "sum by (container_label_io_kubernetes_container_name) (rate(container_fs_writes_total{container_label_io_kubernetes_container_name=\"gitserver\"}[5m]))",
							NoAlert:     true,
							Panel: monitoring.Panel().LegendFormat("{{container_label_io_kubernetes_pod_name}}").
								Unit(monitoring.WritesPerSecond).
								With(monitoring.PanelOptions.LegendOnRight()),
							Owner: monitoring.ObservableOwnerRepoManagement,
							Interpretation: `
						`,
						},
					},
					{
						{
							Name:        "io_reads",
							Description: "i/o reads",
							Query:       "sum by (container_label_io_kubernetes_pod_name) (rate(container_fs_reads_total{container_label_io_kubernetes_container_name=\"gitserver\", container_label_io_kubernetes_pod_name=~`${shard:regex}`}[5m]))",
							NoAlert:     true,
							Panel: monitoring.Panel().LegendFormat("{{container_label_io_kubernetes_pod_name}}").
								Unit(monitoring.ReadsPerSecond).
								With(monitoring.PanelOptions.LegendOnRight()),
							Owner: monitoring.ObservableOwnerRepoManagement,
							Interpretation: `
						`,
						},
						{
							Name:        "io_writes",
							Description: "i/o writes",
							Query:       "sum by (container_label_io_kubernetes_pod_name) (rate(container_fs_writes_total{container_label_io_kubernetes_container_name=\"gitserver\", container_label_io_kubernetes_pod_name=~`${shard:regex}`}[5m]))",
							NoAlert:     true,
							Panel: monitoring.Panel().LegendFormat("{{container_label_io_kubernetes_pod_name}}").
								Unit(monitoring.WritesPerSecond).
								With(monitoring.PanelOptions.LegendOnRight()),
							Owner: monitoring.ObservableOwnerRepoManagement,
							Interpretation: `
						`,
						},
					},
					{
						{
							Name:        "io_read_througput",
							Description: "i/o read throughput",
							Query:       "sum by (container_label_io_kubernetes_pod_name) (rate(container_fs_reads_bytes_total{container_label_io_kubernetes_container_name=\"gitserver\", container_label_io_kubernetes_pod_name=~`${shard:regex}`}[5m]))",
							NoAlert:     true,
							Panel: monitoring.Panel().LegendFormat("{{container_label_io_kubernetes_pod_name}}").
								Unit(monitoring.ReadsPerSecond).
								With(monitoring.PanelOptions.LegendOnRight()),
							Owner: monitoring.ObservableOwnerRepoManagement,
							Interpretation: `
						`,
						},
						{
							Name:        "io_write_throughput",
							Description: "i/o write throughput",
							Query:       "sum by (container_label_io_kubernetes_pod_name) (rate(container_fs_writes_bytes_total{container_label_io_kubernetes_container_name=\"gitserver\", container_label_io_kubernetes_pod_name=~`${shard:regex}`}[5m]))",
							NoAlert:     true,
							Panel: monitoring.Panel().LegendFormat("{{container_label_io_kubernetes_pod_name}}").
								Unit(monitoring.WritesPerSecond).
								With(monitoring.PanelOptions.LegendOnRight()),
							Owner: monitoring.ObservableOwnerRepoManagement,
							Interpretation: `
						`,
						},
					},
					{
						{
							Name:        "running_git_commands",
							Description: "git commands running on each gitserver instance",
							Query:       "sum by (instance, cmd) (src_gitserver_exec_running{instance=~`${shard:regex}`})",
							Warning:     monitoring.Alert().GreaterOrEqual(50).For(2 * time.Minute),
							Critical:    monitoring.Alert().GreaterOrEqual(100).For(5 * time.Minute),
							Panel: monitoring.Panel().LegendFormat("{{instance}} {{cmd}}").
								With(monitoring.PanelOptions.LegendOnRight()),
							Owner: monitoring.ObservableOwnerRepoManagement,
							Interpretation: `
								A high value signals load.
							`,
							PossibleSolutions: `
								- **Check if the problem may be an intermittent and temporary peak** using the "Container monitoring" section at the bottom of the Git Server dashboard.
								- **Single container deployments:** Consider upgrading to a [Docker Compose deployment](../deploy/docker-compose/migrate.md) which offers better scalability and resource isolation.
								- **Kubernetes and Docker Compose:** Check that you are running a similar number of git server replicas and that their CPU/memory limits are allocated according to what is shown in the [Sourcegraph resource estimator](../deploy/resource_estimator.md).
							`,
						},
						{
							Name:           "git_commands_received",
							Description:    "rate of git commands received across all instances",
							Query:          "sum by (cmd) (rate(src_gitserver_exec_duration_seconds_count[5m]))",
							NoAlert:        true,
							Interpretation: "per second rate per command across all instances",
							Panel: monitoring.Panel().LegendFormat("{{cmd}}").
								With(monitoring.PanelOptions.LegendOnRight()),
							Owner: monitoring.ObservableOwnerRepoManagement,
						},
					},
					{
						{
							Name:        "repository_clone_queue_size",
							Description: "repository clone queue size",
							Query:       "sum(src_gitserver_clone_queue)",
							Warning:     monitoring.Alert().GreaterOrEqual(25),
							Panel:       monitoring.Panel().LegendFormat("queue size"),
							Owner:       monitoring.ObservableOwnerRepoManagement,
							PossibleSolutions: `
								- **If you just added several repositories**, the warning may be expected.
								- **Check which repositories need cloning**, by visiting e.g. https://sourcegraph.example.com/site-admin/repositories?filter=not-cloned
							`,
						},
						{
							Name:        "repository_existence_check_queue_size",
							Description: "repository existence check queue size",
							Query:       "sum(src_gitserver_lsremote_queue)",
							Warning:     monitoring.Alert().GreaterOrEqual(25),
							Panel:       monitoring.Panel().LegendFormat("queue size"),
							Owner:       monitoring.ObservableOwnerRepoManagement,
							PossibleSolutions: `
								- **Check the code host status indicator for errors:** on the Sourcegraph app homepage, when signed in as an admin click the cloud icon in the top right corner of the page.
								- **Check if the issue continues to happen after 30 minutes**, it may be temporary.
								- **Check the gitserver logs for more information.**
							`,
						},
					},
					{
						{
							Name:        "echo_command_duration_test",
							Description: "echo test command duration",
							Query:       "max(src_gitserver_echo_duration_seconds)",
							NoAlert:     true,
							Panel:       monitoring.Panel().LegendFormat("running commands").Unit(monitoring.Seconds),
							Owner:       monitoring.ObservableOwnerRepoManagement,
							Interpretation: `
								A high value here likely indicates a problem, especially if consistently high.
								You can query for individual commands using 'sum by (cmd)(src_gitserver_exec_running)' in Grafana ('/-/debug/grafana') to see if a specific Git Server command might be spiking in frequency.

								If this value is consistently high, consider the following:

								- **Single container deployments:** Upgrade to a [Docker Compose deployment](../deploy/docker-compose/migrate.md) which offers better scalability and resource isolation.
								- **Kubernetes and Docker Compose:** Check that you are running a similar number of git server replicas and that their CPU/memory limits are allocated according to what is shown in the [Sourcegraph resource estimator](../deploy/resource_estimator.md).
							`,
						},
						shared.FrontendInternalAPIErrorResponses("gitserver", monitoring.ObservableOwnerRepoManagement).Observable(),
					},
				},
			},
			shared.GitServer.NewAPIGroup(containerName),
			shared.GitServer.NewBatchLogSemaphoreWait(containerName),
			{
				Title:  "Gitservice for internal cloning",
				Hidden: true,
				Rows: []monitoring.Row{
					{
						{
							Name:           "aggregate_gitservice_request_duration",
							Description:    "95th percentile gitservice request duration aggregate",
							Query:          "histogram_quantile(0.95, sum(rate(src_gitserver_gitservice_duration_seconds_bucket{type=`gitserver`, error=`false`}[5m])) by (le))",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("{{le}}").Unit(monitoring.Seconds),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: `A high value means any internal service trying to clone a repo from gitserver is slowed down.`,
						},
						{
							Name:           "gitservice_request_duration",
							Description:    "95th percentile gitservice request duration per shard",
							Query:          "histogram_quantile(0.95, sum(rate(src_gitserver_gitservice_duration_seconds_bucket{type=`gitserver`, error=`false`, instance=~`${shard:regex}`}[5m])) by (le, instance))",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("{{instance}}").Unit(monitoring.Seconds),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: `A high value means any internal service trying to clone a repo from gitserver is slowed down.`,
						},
					},
					{
						{
							Name:           "aggregate_gitservice_error_request_duration",
							Description:    "95th percentile gitservice error request duration aggregate",
							Query:          "histogram_quantile(0.95, sum(rate(src_gitserver_gitservice_duration_seconds_bucket{type=`gitserver`, error=`true`}[5m])) by (le))",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("{{le}}").Unit(monitoring.Seconds),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: `95th percentile gitservice error request duration aggregate`,
						},
						{
							Name:           "gitservice_request_duration",
							Description:    "95th percentile gitservice error request duration per shard",
							Query:          "histogram_quantile(0.95, sum(rate(src_gitserver_gitservice_duration_seconds_bucket{type=`gitserver`, error=`true`, instance=~`${shard:regex}`}[5m])) by (le, instance))",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("{{instance}}").Unit(monitoring.Seconds),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: `95th percentile gitservice error request duration per shard`,
						},
					},
					{
						{
							Name:           "aggregate_gitservice_request_rate",
							Description:    "aggregate gitservice request rate",
							Query:          "sum(rate(src_gitserver_gitservice_duration_seconds_count{type=`gitserver`, error=`false`}[5m]))",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("gitservers").Unit(monitoring.RequestsPerSecond),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: `Aggregate gitservice request rate`,
						},
						{
							Name:           "gitservice_request_rate",
							Description:    "gitservice request rate per shard",
							Query:          "sum(rate(src_gitserver_gitservice_duration_seconds_count{type=`gitserver`, error=`false`, instance=~`${shard:regex}`}[5m]))",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("{{instance}}").Unit(monitoring.RequestsPerSecond),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: `Per shard gitservice request rate`,
						},
					},
					{
						{
							Name:           "aggregate_gitservice_request_error_rate",
							Description:    "aggregate gitservice request error rate",
							Query:          "sum(rate(src_gitserver_gitservice_duration_seconds_count{type=`gitserver`, error=`true`}[5m]))",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("gitservers").Unit(monitoring.RequestsPerSecond),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: `Aggregate gitservice request error rate`,
						},
						{
							Name:           "gitservice_request_error_rate",
							Description:    "gitservice request error rate per shard",
							Query:          "sum(rate(src_gitserver_gitservice_duration_seconds_count{type=`gitserver`, error=`true`, instance=~`${shard:regex}`}[5m]))",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("{{instance}}").Unit(monitoring.RequestsPerSecond),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: `Per shard gitservice request error rate`,
						},
					},
					{
						{
							Name:           "aggregate_gitservice_requests_running",
							Description:    "aggregate gitservice requests running",
							Query:          "sum(src_gitserver_gitservice_running{type=`gitserver`})",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("gitservers").Unit(monitoring.Number),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: `Aggregate gitservice requests running`,
						},
						{
							Name:           "gitservice_requests_running",
							Description:    "gitservice requests running per shard",
							Query:          "sum(src_gitserver_gitservice_running{type=`gitserver`, instance=~`${shard:regex}`}) by (instance)",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("{{instance}}").Unit(monitoring.RequestsPerSecond),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: `Per shard gitservice requests running`,
						},
					},
				},
			},
			{
				Title:  "Gitserver cleanup jobs",
				Hidden: true,
				Rows: []monitoring.Row{
					{
						{
							Name:           "janitor_running",
							Description:    "if the janitor process is running",
							Query:          "max by (instance) (src_gitserver_janitor_running)",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("janitor process running").Unit(monitoring.Number),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: "1, if the janitor process is currently running",
						},
					},
					{
						{
							Name:           "janitor_job_duration",
							Description:    "95th percentile job run duration",
							Query:          "histogram_quantile(0.95, sum(rate(src_gitserver_janitor_job_duration_seconds_bucket[5m])) by (le, job_name))",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("{{job_name}}").Unit(monitoring.Seconds),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: "95th percentile job run duration",
						},
					},
					{
						{
							Name:           "janitor_job_failures",
							Description:    "failures over 5m (by job)",
							Query:          `sum by (job_name) (rate(src_gitserver_janitor_job_duration_seconds_count{success="false"}[5m]))`,
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("{{job_name}}").Unit(monitoring.Number),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: "the rate of failures over 5m (by job)",
						},
					},
					{
						{
							Name:           "repos_removed",
							Description:    "repositories removed due to disk pressure",
							Query:          "sum by (instance) (rate(src_gitserver_repos_removed_disk_pressure[5m]))",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("{{instance}}").Unit(monitoring.Number),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: "Repositories removed due to disk pressure",
						},
					},
					{
						{
							Name:           "sg_maintenance_reason",
							Description:    "successful sg maintenance jobs over 1h (by reason)",
							Query:          `sum by (reason) (rate(src_gitserver_maintenance_status{success="true"}[1h]))`,
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("{{reason}}").Unit(monitoring.Number),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: "the rate of successful sg maintenance jobs and the reason why they were triggered",
						},
					},
					{
						{
							Name:           "git_prune_skipped",
							Description:    "successful git prune jobs over 1h",
							Query:          `sum by (skipped) (rate(src_gitserver_prune_status{success="true"}[1h]))`,
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("skipped={{skipped}}").Unit(monitoring.Number),
							Owner:          monitoring.ObservableOwnerRepoManagement,
							Interpretation: "the rate of successful git prune jobs over 1h and whether they were skipped",
						},
					},
				},
			},
			{
				Title:  "Search",
				Hidden: true,
				Rows: []monitoring.Row{
					{
						{
							Name:           "search_latency",
							Description:    "mean time until first result is sent",
							Query:          "rate(src_gitserver_search_latency_seconds_sum[5m]) / rate(src_gitserver_search_latency_seconds_count[5m])",
							NoAlert:        true,
							Panel:          monitoring.Panel().Unit(monitoring.Seconds),
							Owner:          monitoring.ObservableOwnerSearch,
							Interpretation: "Mean latency (time to first result) of gitserver search requests",
						},
						{
							Name:           "search_duration",
							Description:    "mean search duration",
							Query:          "rate(src_gitserver_search_duration_seconds_sum[5m]) / rate(src_gitserver_search_duration_seconds_count[5m])",
							NoAlert:        true,
							Panel:          monitoring.Panel().Unit(monitoring.Seconds),
							Owner:          monitoring.ObservableOwnerSearch,
							Interpretation: "Mean duration of gitserver search requests",
						},
					},
					{
						{
							Name:           "search_rate",
							Description:    "rate of searches run by pod",
							Query:          "rate(src_gitserver_search_latency_seconds_count{instance=~`${shard:regex}`}[5m])",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("{{instance}}").Unit(monitoring.RequestsPerSecond),
							Owner:          monitoring.ObservableOwnerSearch,
							Interpretation: "The rate of searches executed on gitserver by pod",
						},
						{
							Name:           "running_searches",
							Description:    "number of searches currently running by pod",
							Query:          "sum by (instance) (src_gitserver_search_running{instance=~`${shard:regex}`})",
							NoAlert:        true,
							Panel:          monitoring.Panel().LegendFormat("{{instance}}").Unit(monitoring.Number),
							Owner:          monitoring.ObservableOwnerSearch,
							Interpretation: "The number of searches currently executing on gitserver by pod",
						},
					},
				},
			},

			shared.CodeIntelligence.NewCoursierGroup(containerName),
			shared.CodeIntelligence.NewNpmGroup(containerName),

			shared.NewDatabaseConnectionsMonitoringGroup(containerName),
			shared.NewContainerMonitoringGroup(containerName, monitoring.ObservableOwnerRepoManagement, nil),
			shared.NewProvisioningIndicatorsGroup(containerName, monitoring.ObservableOwnerRepoManagement, provisioningIndicatorsOptions),
			shared.NewGolangMonitoringGroup(containerName, monitoring.ObservableOwnerRepoManagement, nil),
			shared.NewKubernetesMonitoringGroup(containerName, monitoring.ObservableOwnerRepoManagement, nil),
		},
	}
}
