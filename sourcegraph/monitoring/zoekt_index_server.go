package main

import (
	"fmt"

	"github.com/sourcegraph/sourcegraph/monitoring/monitoring"
)

func ZoektIndexServer() *monitoring.Container {
	return &monitoring.Container{
		Name:        "zoekt-indexserver",
		Title:       "Zoekt Index Server",
		Description: "Indexes repositories and populates the search index.",
		Groups: []monitoring.Group{
			{
				Title: "General",
				Rows: []monitoring.Row{
					{
						{
							Name:              "average_resolve_revision_duration",
							Description:       "average resolve revision duration over 5m",
							Query:             `sum(rate(resolve_revision_seconds_sum[5m])) / sum(rate(resolve_revision_seconds_count[5m]))`,
							DataMayNotExist:   true,
							Warning:           monitoring.Alert().GreaterOrEqual(15),
							Critical:          monitoring.Alert().GreaterOrEqual(30),
							PanelOptions:      monitoring.PanelOptions().LegendFormat("{{duration}}").Unit(monitoring.Seconds),
							Owner:             monitoring.ObservableOwnerSearch,
							PossibleSolutions: "none",
						},
					},
				},
			},
			{
				Title:  "Container monitoring (not available on server)",
				Hidden: true,
				Rows: []monitoring.Row{
					{
						sharedContainerCPUUsage("zoekt-indexserver", monitoring.ObservableOwnerSearch),
						sharedContainerMemoryUsage("zoekt-indexserver", monitoring.ObservableOwnerSearch),
					},
					{
						sharedContainerRestarts("zoekt-indexserver", monitoring.ObservableOwnerSearch),
						sharedContainerFsInodes("zoekt-indexserver", monitoring.ObservableOwnerSearch),
					},
					{
						{
							Name:              "fs_io_operations",
							Description:       "filesystem reads and writes rate by instance over 1h",
							Query:             fmt.Sprintf(`sum by(name) (rate(container_fs_reads_total{%[1]s}[1h]) + rate(container_fs_writes_total{%[1]s}[1h]))`, promCadvisorContainerMatchers("zoekt-indexserver")),
							DataMayNotExist:   true,
							Warning:           monitoring.Alert().GreaterOrEqual(5000),
							PanelOptions:      monitoring.PanelOptions().LegendFormat("{{name}}"),
							Owner:             monitoring.ObservableOwnerSearch,
							PossibleSolutions: "none",
						},
					},
				},
			},
			{
				Title:  "Provisioning indicators (not available on server)",
				Hidden: true,
				Rows: []monitoring.Row{
					{
						sharedProvisioningCPUUsageLongTerm("zoekt-indexserver", monitoring.ObservableOwnerSearch),
						sharedProvisioningMemoryUsageLongTerm("zoekt-indexserver", monitoring.ObservableOwnerSearch),
					},
					{
						sharedProvisioningCPUUsageShortTerm("zoekt-indexserver", monitoring.ObservableOwnerSearch),
						sharedProvisioningMemoryUsageShortTerm("zoekt-indexserver", monitoring.ObservableOwnerSearch),
					},
				},
			},
			{
				Title:  "Kubernetes monitoring (ignore if using Docker Compose or server)",
				Hidden: true,
				Rows: []monitoring.Row{
					{
						// zoekt_index_server, zoekt_web_server are deployed together
						// as part of the indexed-search service, so only show pod
						// availability here.
						sharedKubernetesPodsAvailable("indexed-search", monitoring.ObservableOwnerSearch),
					},
				},
			},
		},
	}
}
