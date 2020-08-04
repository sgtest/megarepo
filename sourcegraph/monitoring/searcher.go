package main

func Searcher() *Container {
	return &Container{
		Name:        "searcher",
		Title:       "Searcher",
		Description: "Performs unindexed searches (diff and commit search, text search for unindexed branches).",
		Groups: []Group{
			{
				Title: "General",
				Rows: []Row{
					{
						{
							Name:              "unindexed_search_request_errors",
							Description:       "unindexed search request errors every 5m by code",
							Query:             `sum by (code)(increase(searcher_service_request_total{code!="200",code!="canceled"}[5m]))`,
							DataMayNotExist:   true,
							Warning:           Alert{GreaterOrEqual: 5},
							PanelOptions:      PanelOptions().LegendFormat("{{code}}"),
							Owner:             ObservableOwnerSearch,
							PossibleSolutions: "none",
						},
						{
							Name:              "replica_traffic",
							Description:       "requests per second over 10m",
							Query:             "sum by(instance) (rate(searcher_service_request_total[10m]))",
							Warning:           Alert{GreaterOrEqual: 5},
							PanelOptions:      PanelOptions().LegendFormat("{{instance}}"),
							Owner:             ObservableOwnerSearch,
							PossibleSolutions: "none",
						},
						sharedFrontendInternalAPIErrorResponses("searcher"),
					},
				},
			},
			{
				Title:  "Container monitoring (not available on server)",
				Hidden: true,
				Rows: []Row{
					{
						sharedContainerCPUUsage("searcher"),
						sharedContainerMemoryUsage("searcher"),
					},
					{
						sharedContainerRestarts("searcher"),
						sharedContainerFsInodes("searcher"),
					},
				},
			},
			{
				Title:  "Provisioning indicators (not available on server)",
				Hidden: true,
				Rows: []Row{
					{
						sharedProvisioningCPUUsage7d("searcher"),
						sharedProvisioningMemoryUsage7d("searcher"),
					},
					{
						sharedProvisioningCPUUsage5m("searcher"),
						sharedProvisioningMemoryUsage5m("searcher"),
					},
				},
			},
			{
				Title:  "Golang runtime monitoring",
				Hidden: true,
				Rows: []Row{
					{
						sharedGoGoroutines("searcher"),
						sharedGoGcDuration("searcher"),
					},
				},
			},
			{
				Title:  "Kubernetes monitoring (ignore if using Docker Compose or server)",
				Hidden: true,
				Rows: []Row{
					{
						sharedKubernetesPodsAvailable("searcher"),
					},
				},
			},
		},
	}
}
