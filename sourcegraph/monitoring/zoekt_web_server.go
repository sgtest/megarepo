package main

func ZoektWebServer() *Container {
	return &Container{
		Name:        "zoekt-webserver",
		Title:       "Zoekt Web Server",
		Description: "Serves indexed search requests using the search index.",
		Groups: []Group{
			{
				Title: "General",
				Rows: []Row{
					{
						{
							Name:              "indexed_search_request_errors",
							Description:       "indexed search request errors every 5m by code",
							Query:             `sum by (code)(increase(src_zoekt_request_duration_seconds_count{code!~"2.."}[5m]))`,
							DataMayNotExist:   true,
							Warning:           Alert{GreaterOrEqual: 50},
							PanelOptions:      PanelOptions().LegendFormat("{{code}}").Unit(Seconds),
							PossibleSolutions: "none",
						},
					},
				},
			},
			{
				Title:  "Container monitoring (not available on server)",
				Hidden: true,
				Rows: []Row{
					{
						sharedContainerRestarts("zoekt-webserver"),
						sharedContainerMemoryUsage("zoekt-webserver"),
						sharedContainerCPUUsage("zoekt-webserver"),
					},
				},
			},
			{
				Title:  "Provisioning indicators (not available on server)",
				Hidden: true,
				Rows: []Row{
					{
						sharedProvisioningCPUUsage1d("zoekt-webserver"),
						sharedProvisioningMemoryUsage1d("zoekt-webserver"),
					},
					{
						sharedProvisioningCPUUsage5m("zoekt-webserver"),
						sharedProvisioningMemoryUsage5m("zoekt-webserver"),
					},
				},
			},
		},
	}
}
