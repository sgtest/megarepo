package shared

import "github.com/sourcegraph/sourcegraph/monitoring/monitoring"

// src_codeintel_autoindexing_total
// src_codeintel_autoindexing_duration_seconds_bucket
// src_codeintel_autoindexing_errors_total
func (codeIntelligence) NewAutoindexingServiceGroup(containerName string) monitoring.Group {
	return Observation.NewGroup(containerName, monitoring.ObservableOwnerCodeIntel, ObservationGroupOptions{
		GroupConstructorOptions: GroupConstructorOptions{
			Namespace:       "codeintel",
			DescriptionRoot: "Autoindexing > Service",
			Hidden:          false,

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
			Hidden:          false,

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
			Hidden:          false,

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

// src_codeintel_autoindexing_inference_total
// src_codeintel_autoindexing_inference_duration_seconds_bucket
// src_codeintel_autoindexing_inference_errors_total
func (codeIntelligence) NewAutoindexingInferenceServiceGroup(containerName string) monitoring.Group {
	return Observation.NewGroup(containerName, monitoring.ObservableOwnerCodeIntel, ObservationGroupOptions{
		GroupConstructorOptions: GroupConstructorOptions{
			Namespace:       "codeintel",
			DescriptionRoot: "Autoindexing > Inference service (internal)",
			Hidden:          false,

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
			Hidden:          false,

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
