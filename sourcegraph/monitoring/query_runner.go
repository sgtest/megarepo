package main

func QueryRunner() *Container {
	return &Container{
		Name:        "query-runner",
		Title:       "Query Runner",
		Description: "Periodically runs saved searches and instructs the frontend to send out notifications.",
		Groups: []Group{
			{
				Title: "General",
				Rows: []Row{
					{
						sharedFrontendInternalAPIErrorResponses("query-runner"),
					},
				},
			},
			{
				Title:  "Container monitoring (not available on server)",
				Hidden: true,
				Rows: []Row{
					{
						sharedContainerRestarts("query-runner"),
						sharedContainerMemoryUsage("query-runner"),
						sharedContainerCPUUsage("query-runner"),
					},
				},
			},
			{
				Title:  "Provisioning indicators (not available on server)",
				Hidden: true,
				Rows: []Row{
					{
						sharedProvisioningCPUUsage1d("query-runner"),
						sharedProvisioningMemoryUsage1d("query-runner"),
					},
					{
						sharedProvisioningCPUUsage5m("query-runner"),
						sharedProvisioningMemoryUsage5m("query-runner"),
					},
				},
			},
		},
	}
}
