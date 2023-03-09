package shared

import "github.com/sourcegraph/sourcegraph/monitoring/monitoring"

func (codeIntelligence) NewAutoindexingSummaryGroup(containerName string) monitoring.Group {
	// queueContainerName is the set of potential sources of executor queue metrics
	const queueContainerName = "(executor|sourcegraph-code-intel-indexers|executor-batches|frontend|sourcegraph-frontend|worker|sourcegraph-executors)"

	return monitoring.Group{
		Title:  "Codeintel: Autoindexing > Summary",
		Hidden: false,
		Rows: append(
			[]monitoring.Row{
				{
					monitoring.Observable(NoAlertsOption("none")(Observable{
						Description: "auto-index jobs inserted over 5m",
						Owner:       monitoring.ObservableOwnerCodeIntel,
						Query:       "sum(increase(src_codeintel_dbstore_indexes_inserted[5m]))",
						NoAlert:     true,
						Panel:       monitoring.Panel().LegendFormat("inserts"),
					})),
					CodeIntelligence.NewIndexSchedulerGroup(containerName).Rows[0][3],
				},
			},
			Executors.NewExecutorQueueGroup("executor", queueContainerName, "codeintel").Rows...),
	}
}

// src_codeintel_autoindexing_total
// src_codeintel_autoindexing_duration_seconds_bucket
// src_codeintel_autoindexing_errors_total
func (codeIntelligence) NewAutoindexingServiceGroup(containerName string) monitoring.Group {
	return Observation.NewGroup(containerName, monitoring.ObservableOwnerCodeIntel, ObservationGroupOptions{
		GroupConstructorOptions: GroupConstructorOptions{
			Namespace:       "codeintel",
			DescriptionRoot: "Autoindexing > Service",
			Hidden:          true,

			ObservableConstructorOptions: ObservableConstructorOptions{
				MetricNameRoot:        "codeintel_autoindexing",
				MetricDescriptionRoot: "service",
				By:                    []string{"op"},
			},
		},

		SharedObservationGroupOptions: SharedObservationGroupOptions{
			Total:     NoAlertsOption("none"),
			Duration:  NoAlertsOption("none"),
			Errors:    NoAlertsOption("none"),
			ErrorRate: NoAlertsOption("none"),
		},
		Aggregate: &SharedObservationGroupOptions{
			Total:     NoAlertsOption("none"),
			Duration:  NoAlertsOption("none"),
			Errors:    NoAlertsOption("none"),
			ErrorRate: NoAlertsOption("none"),
		},
	})
}

// src_codeintel_autoindexing_transport_graphql_total
// src_codeintel_autoindexing_transport_graphql_duration_seconds_bucket
// src_codeintel_autoindexing_transport_graphql_errors_total
func (codeIntelligence) NewAutoindexingGraphQLTransportGroup(containerName string) monitoring.Group {
	return Observation.NewGroup(containerName, monitoring.ObservableOwnerCodeIntel, ObservationGroupOptions{
		GroupConstructorOptions: GroupConstructorOptions{
			Namespace:       "codeintel",
			DescriptionRoot: "Autoindexing > GQL transport",
			Hidden:          true,

			ObservableConstructorOptions: ObservableConstructorOptions{
				MetricNameRoot:        "codeintel_autoindexing_transport_graphql",
				MetricDescriptionRoot: "resolver",
				By:                    []string{"op"},
			},
		},

		SharedObservationGroupOptions: SharedObservationGroupOptions{
			Total:     NoAlertsOption("none"),
			Duration:  NoAlertsOption("none"),
			Errors:    NoAlertsOption("none"),
			ErrorRate: NoAlertsOption("none"),
		},
		Aggregate: &SharedObservationGroupOptions{
			Total:     NoAlertsOption("none"),
			Duration:  NoAlertsOption("none"),
			Errors:    NoAlertsOption("none"),
			ErrorRate: NoAlertsOption("none"),
		},
	})
}

// src_codeintel_autoindexing_store_total
// src_codeintel_autoindexing_store_duration_seconds_bucket
// src_codeintel_autoindexing_store_errors_total
func (codeIntelligence) NewAutoindexingStoreGroup(containerName string) monitoring.Group {
	return Observation.NewGroup(containerName, monitoring.ObservableOwnerCodeIntel, ObservationGroupOptions{
		GroupConstructorOptions: GroupConstructorOptions{
			Namespace:       "codeintel",
			DescriptionRoot: "Autoindexing > Store (internal)",
			Hidden:          true,

			ObservableConstructorOptions: ObservableConstructorOptions{
				MetricNameRoot:        "codeintel_autoindexing_store",
				MetricDescriptionRoot: "store",
				By:                    []string{"op"},
			},
		},

		SharedObservationGroupOptions: SharedObservationGroupOptions{
			Total:     NoAlertsOption("none"),
			Duration:  NoAlertsOption("none"),
			Errors:    NoAlertsOption("none"),
			ErrorRate: NoAlertsOption("none"),
		},
		Aggregate: &SharedObservationGroupOptions{
			Total:     NoAlertsOption("none"),
			Duration:  NoAlertsOption("none"),
			Errors:    NoAlertsOption("none"),
			ErrorRate: NoAlertsOption("none"),
		},
	})
}

// src_codeintel_autoindexing_background_total
// src_codeintel_autoindexing_background_duration_seconds_bucket
// src_codeintel_autoindexing_background_errors_total
func (codeIntelligence) NewAutoindexingBackgroundJobGroup(containerName string) monitoring.Group {
	return Observation.NewGroup(containerName, monitoring.ObservableOwnerCodeIntel, ObservationGroupOptions{
		GroupConstructorOptions: GroupConstructorOptions{
			Namespace:       "codeintel",
			DescriptionRoot: "Autoindexing > Background jobs (internal)",
			Hidden:          true,

			ObservableConstructorOptions: ObservableConstructorOptions{
				MetricNameRoot:        "codeintel_autoindexing_background",
				MetricDescriptionRoot: "background",
				By:                    []string{"op"},
			},
		},

		SharedObservationGroupOptions: SharedObservationGroupOptions{
			Total:     NoAlertsOption("none"),
			Duration:  NoAlertsOption("none"),
			Errors:    NoAlertsOption("none"),
			ErrorRate: NoAlertsOption("none"),
		},
		Aggregate: &SharedObservationGroupOptions{
			Total:     NoAlertsOption("none"),
			Duration:  NoAlertsOption("none"),
			Errors:    NoAlertsOption("none"),
			ErrorRate: NoAlertsOption("none"),
		},
	})
}

// src_codeintel_autoindexing_inference_total
// src_codeintel_autoindexing_inference_duration_seconds_bucket
// src_codeintel_autoindexing_inference_errors_total
func (codeIntelligence) NewAutoindexingInferenceServiceGroup(containerName string) monitoring.Group {
	return Observation.NewGroup(containerName, monitoring.ObservableOwnerCodeIntel, ObservationGroupOptions{
		GroupConstructorOptions: GroupConstructorOptions{
			Namespace:       "codeintel",
			DescriptionRoot: "Autoindexing > Inference service (internal)",
			Hidden:          true,

			ObservableConstructorOptions: ObservableConstructorOptions{
				MetricNameRoot:        "codeintel_autoindexing_inference",
				MetricDescriptionRoot: "service",
				By:                    []string{"op"},
			},
		},

		SharedObservationGroupOptions: SharedObservationGroupOptions{
			Total:     NoAlertsOption("none"),
			Duration:  NoAlertsOption("none"),
			Errors:    NoAlertsOption("none"),
			ErrorRate: NoAlertsOption("none"),
		},
		Aggregate: &SharedObservationGroupOptions{
			Total:     NoAlertsOption("none"),
			Duration:  NoAlertsOption("none"),
			Errors:    NoAlertsOption("none"),
			ErrorRate: NoAlertsOption("none"),
		},
	})
}

// src_luasandbox_store_total
// src_luasandbox_store_duration_seconds_bucket
// src_luasandbox_store_errors_total
func (codeIntelligence) NewLuasandboxServiceGroup(containerName string) monitoring.Group {
	return Observation.NewGroup(containerName, monitoring.ObservableOwnerCodeIntel, ObservationGroupOptions{
		GroupConstructorOptions: GroupConstructorOptions{
			Namespace:       "codeintel",
			DescriptionRoot: "Luasandbox service",
			Hidden:          true,

			ObservableConstructorOptions: ObservableConstructorOptions{
				MetricNameRoot:        "luasandbox",
				MetricDescriptionRoot: "service",
				By:                    []string{"op"},
			},
		},

		SharedObservationGroupOptions: SharedObservationGroupOptions{
			Total:     NoAlertsOption("none"),
			Duration:  NoAlertsOption("none"),
			Errors:    NoAlertsOption("none"),
			ErrorRate: NoAlertsOption("none"),
		},
		Aggregate: &SharedObservationGroupOptions{
			Total:     NoAlertsOption("none"),
			Duration:  NoAlertsOption("none"),
			Errors:    NoAlertsOption("none"),
			ErrorRate: NoAlertsOption("none"),
		},
	})
}

// Tasks:
//   - codeintel_autoindexing_janitor_unknown_repository
//   - codeintel_autoindexing_janitor_unknown_commit
//   - codeintel_autoindexing_janitor_expired
//
// Suffixes:
//   - _total
//   - _duration_seconds_bucket
//   - _errors_total
//   - _records_scanned_total
//   - _records_altered_total
func (codeIntelligence) NewAutoindexingJanitorTaskGroups(containerName string) []monitoring.Group {
	return CodeIntelligence.newJanitorGroups(
		"Autoindexing > Janitor task",
		containerName,
		[]string{
			"codeintel_autoindexing_janitor_unknown_repository",
			"codeintel_autoindexing_janitor_unknown_commit",
			"codeintel_autoindexing_janitor_expired",
		},
	)
}
