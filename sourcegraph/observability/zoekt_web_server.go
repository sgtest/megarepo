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
							Name:            "indexed_search_request_errors",
							Description:     "indexed search request errors every 5m by code",
							Query:           `sum by (code)(increase(src_zoekt_request_duration_seconds_count{code!~"2.."}[5m]))`,
							DataMayNotExist: true,
							Warning:         Alert{GreaterOrEqual: 50},
							PanelOptions:    PanelOptions().LegendFormat("{{code}}").Unit(Seconds),
						},
					},
				},
			},
		},
	}
}
