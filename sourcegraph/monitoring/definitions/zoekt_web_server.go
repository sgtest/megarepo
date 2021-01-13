package definitions

import (
	"fmt"
	"time"

	"github.com/sourcegraph/sourcegraph/monitoring/definitions/shared"
	"github.com/sourcegraph/sourcegraph/monitoring/monitoring"
)

func ZoektWebServer() *monitoring.Container {
	return &monitoring.Container{
		Name:        "zoekt-webserver",
		Title:       "Zoekt Web Server",
		Description: "Serves indexed search requests using the search index.",
		Groups: []monitoring.Group{
			{
				Title: "General",
				Rows: []monitoring.Row{
					{
						{
							Name:              "indexed_search_request_errors",
							Description:       "indexed search request errors every 5m by code",
							Query:             `sum by (code)(increase(src_zoekt_request_duration_seconds_count{code!~"2.."}[5m])) / ignoring(code) group_left sum(increase(src_zoekt_request_duration_seconds_count[5m])) * 100`,
							Warning:           monitoring.Alert().GreaterOrEqual(5).For(5 * time.Minute),
							Panel:             monitoring.Panel().LegendFormat("{{code}}").Unit(monitoring.Percentage),
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
						shared.ContainerCPUUsage("zoekt-webserver", monitoring.ObservableOwnerSearch).Observable(),
						shared.ContainerMemoryUsage("zoekt-webserver", monitoring.ObservableOwnerSearch).Observable(),
					},
					{
						shared.ContainerRestarts("zoekt-webserver", monitoring.ObservableOwnerSearch).Observable(),
						shared.ContainerFsInodes("zoekt-webserver", monitoring.ObservableOwnerSearch).Observable(),
					},
					{
						{
							Name:              "fs_io_operations",
							Description:       "filesystem reads and writes by instance rate over 1h",
							Query:             fmt.Sprintf(`sum by(name) (rate(container_fs_reads_total{%[1]s}[1h]) + rate(container_fs_writes_total{%[1]s}[1h]))`, shared.CadvisorNameMatcher("zoekt-webserver")),
							Warning:           monitoring.Alert().GreaterOrEqual(5000),
							Panel:             monitoring.Panel().LegendFormat("{{name}}"),
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
						shared.ProvisioningCPUUsageLongTerm("zoekt-webserver", monitoring.ObservableOwnerSearch).Observable(),
						shared.ProvisioningMemoryUsageLongTerm("zoekt-webserver", monitoring.ObservableOwnerSearch).Observable(),
					},
					{
						shared.ProvisioningCPUUsageShortTerm("zoekt-webserver", monitoring.ObservableOwnerSearch).Observable(),
						shared.ProvisioningMemoryUsageShortTerm("zoekt-webserver", monitoring.ObservableOwnerSearch).Observable(),
					},
				},
			},
			// kubernetes monitoring for zoekt-web-server is provided by zoekt-index-server,
			// since both services are deployed together
		},
	}
}
