package types

import (
	"errors"
	"fmt"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"time"

	"github.com/grafana/grafana-azure-sdk-go/azcredentials"
	"github.com/grafana/grafana-plugin-sdk-go/backend"
)

const (
	TimeSeries = "time_series"
	Table      = "table"
	Trace      = "trace"
)

var (
	LegendKeyFormat = regexp.MustCompile(`\{\{\s*(.+?)\s*\}\}`)
)

type AzRoute struct {
	URL     string
	Scopes  []string
	Headers map[string]string
}

type AzureSettings struct {
	AzureMonitorSettings
	AzureClientSettings
}

type AzureMonitorSettings struct {
	SubscriptionId               string `json:"subscriptionId"`
	LogAnalyticsDefaultWorkspace string `json:"logAnalyticsDefaultWorkspace"`
	AppInsightsAppId             string `json:"appInsightsAppId"`
}

type AzureClientSettings struct {
	AzureAuthType string
	CloudName     string
	TenantId      string
	ClientId      string
}

// AzureMonitorCustomizedCloudSettings is the extended Azure Monitor settings for customized cloud
type AzureMonitorCustomizedCloudSettings struct {
	CustomizedRoutes map[string]AzRoute `json:"customizedRoutes"`
}

type DatasourceService struct {
	URL        string
	HTTPClient *http.Client
}

type DatasourceInfo struct {
	Cloud       string
	Credentials azcredentials.AzureCredentials
	Settings    AzureMonitorSettings
	Routes      map[string]AzRoute
	Services    map[string]DatasourceService

	JSONData                map[string]interface{}
	DecryptedSecureJSONData map[string]string
	DatasourceID            int64
	OrgID                   int64

	DatasourceName string
	DatasourceUID  string
}

// AzureMonitorQuery is the query for all the services as they have similar queries
// with a url, a querystring and an alias field
type AzureMonitorQuery struct {
	URL          string
	Target       string
	Params       url.Values
	RefID        string
	Alias        string
	TimeRange    backend.TimeRange
	BodyFilter   string
	Dimensions   []AzureMonitorDimensionFilter
	Resources    map[string]AzureMonitorResource
	Subscription string
}

// AzureMonitorResponse is the json response from the Azure Monitor API
type AzureMonitorResponse struct {
	Cost     int    `json:"cost"`
	Timespan string `json:"timespan"`
	Interval string `json:"interval"`
	Value    []struct {
		ID   string `json:"id"`
		Type string `json:"type"`
		Name struct {
			Value          string `json:"value"`
			LocalizedValue string `json:"localizedValue"`
		} `json:"name"`
		Unit       string `json:"unit"`
		Timeseries []struct {
			Metadatavalues []struct {
				Name struct {
					Value          string `json:"value"`
					LocalizedValue string `json:"localizedValue"`
				} `json:"name"`
				Value string `json:"value"`
			} `json:"metadatavalues"`
			Data []struct {
				TimeStamp time.Time `json:"timeStamp"`
				Average   *float64  `json:"average,omitempty"`
				Total     *float64  `json:"total,omitempty"`
				Count     *float64  `json:"count,omitempty"`
				Maximum   *float64  `json:"maximum,omitempty"`
				Minimum   *float64  `json:"minimum,omitempty"`
			} `json:"data"`
		} `json:"timeseries"`
	} `json:"value"`
	Namespace      string `json:"namespace"`
	Resourceregion string `json:"resourceregion"`
}

// AzureResponseTable is the table format for Azure responses
type AzureResponseTable struct {
	Name    string `json:"name"`
	Columns []struct {
		Name string `json:"name"`
		Type string `json:"type"`
	} `json:"columns"`
	Rows [][]interface{} `json:"rows"`
}

type AzureMonitorResource struct {
	ResourceGroup string `json:"resourceGroup"`
	ResourceName  string `json:"resourceName"`
}

// AzureMonitorJSONQuery is the frontend JSON query model for an Azure Monitor query.
type AzureMonitorJSONQuery struct {
	AzureMonitor struct {
		ResourceURI string `json:"resourceUri"`
		// These are used to reconstruct a resource URI
		MetricNamespace string                 `json:"metricNamespace"`
		CustomNamespace string                 `json:"customNamespace"`
		MetricName      string                 `json:"metricName"`
		Region          string                 `json:"region"`
		Resources       []AzureMonitorResource `json:"resources"`

		Aggregation      string                        `json:"aggregation"`
		Alias            string                        `json:"alias"`
		DimensionFilters []AzureMonitorDimensionFilter `json:"dimensionFilters"` // new model
		TimeGrain        string                        `json:"timeGrain"`
		Top              string                        `json:"top"`

		AllowedTimeGrainsMs []int64 `json:"allowedTimeGrainsMs"`
		Dimension           string  `json:"dimension"`       // old model
		DimensionFilter     string  `json:"dimensionFilter"` // old model
		Format              string  `json:"format"`

		// Deprecated, MetricNamespace should be used instead
		MetricDefinition string `json:"metricDefinition"`
		// Deprecated: Use Resources with a single element instead
		AzureMonitorResource
	} `json:"azureMonitor"`
	Subscription string `json:"subscription"`
}

// AzureMonitorDimensionFilter is the model for the frontend sent for azureMonitor metric
// queries like "BlobType", "eq", "*"
type AzureMonitorDimensionFilter struct {
	Dimension string   `json:"dimension"`
	Operator  string   `json:"operator"`
	Filters   []string `json:"filters,omitempty"`
	// Deprecated: To support multiselection, filters are passed in a slice now. Also migrated in frontend.
	Filter *string `json:"filter,omitempty"`
}

type AzureMonitorDimensionFilterBackend struct {
	Key      string   `json:"key"`
	Operator int      `json:"operator"`
	Values   []string `json:"values"`
}

func (a AzureMonitorDimensionFilter) ConstructFiltersString() string {
	var filterStrings []string
	for _, filter := range a.Filters {
		filterStrings = append(filterStrings, fmt.Sprintf("%v %v '%v'", a.Dimension, a.Operator, filter))
	}
	if a.Operator == "eq" {
		return strings.Join(filterStrings, " or ")
	} else {
		return strings.Join(filterStrings, " and ")
	}
}

// LogJSONQuery is the frontend JSON query model for an Azure Log Analytics query.
type LogJSONQuery struct {
	AzureLogAnalytics struct {
		Query        string   `json:"query"`
		ResultFormat string   `json:"resultFormat"`
		Resources    []string `json:"resources"`
		OperationId  string   `json:"operationId"`

		// Deprecated: Queries should be migrated to use Resource instead
		Workspace string `json:"workspace"`
		// Deprecated: Use Resources instead
		Resource string `json:"resource"`
	} `json:"azureLogAnalytics"`
}

type TracesJSONQuery struct {
	AzureTraces struct {
		// Filters for property values.
		Filters []TracesFilters `json:"filters"`

		// Operation ID. Used only for Traces queries.
		OperationId *string `json:"operationId"`

		// KQL query to be executed.
		Query *string `json:"query"`

		// Array of resource URIs to be queried.
		Resources []string `json:"resources"`

		// Specifies the format results should be returned as.
		ResultFormat *string `json:"resultFormat"`

		// Types of events to filter by.
		TraceTypes []string `json:"traceTypes"`
	} `json:"azureTraces"`
}

type TracesFilters struct {
	// Values to filter by.
	Filters []string `json:"filters"`

	// Comparison operator to use. Either equals or not equals.
	Operation string `json:"operation"`

	// Property name, auto-populated based on available traces.
	Property string `json:"property"`
}

// MetricChartDefinition is the JSON model for a metrics chart definition
type MetricChartDefinition struct {
	ResourceMetadata    map[string]string   `json:"resourceMetadata"`
	Name                string              `json:"name"`
	AggregationType     int                 `json:"aggregationType"`
	Namespace           string              `json:"namespace"`
	MetricVisualization MetricVisualization `json:"metricVisualization"`
}

// MetricVisualization is the JSON model for the visualization field of a
// metricChartDefinition
type MetricVisualization struct {
	DisplayName         string `json:"displayName"`
	ResourceDisplayName string `json:"resourceDisplayName"`
}

type ServiceProxy interface {
	Do(rw http.ResponseWriter, req *http.Request, cli *http.Client) http.ResponseWriter
}

type LogAnalyticsWorkspaceFeatures struct {
	EnableLogAccessUsingOnlyResourcePermissions bool `json:"enableLogAccessUsingOnlyResourcePermissions"`
	Legacy                                      int  `json:"legacy"`
	SearchVersion                               int  `json:"searchVersion"`
}

type LogAnalyticsWorkspaceProperties struct {
	CreatedDate string                        `json:"createdDate"`
	CustomerId  string                        `json:"customerId"`
	Features    LogAnalyticsWorkspaceFeatures `json:"features"`
}

type LogAnalyticsWorkspaceResponse struct {
	Id                              string                          `json:"id"`
	Location                        string                          `json:"location"`
	Name                            string                          `json:"name"`
	Properties                      LogAnalyticsWorkspaceProperties `json:"properties"`
	ProvisioningState               string                          `json:"provisioningState"`
	PublicNetworkAccessForIngestion string                          `json:"publicNetworkAccessForIngestion"`
	PublicNetworkAccessForQuery     string                          `json:"publicNetworkAccessForQuery"`
	RetentionInDays                 int                             `json:"retentionInDays"`
}

type SubscriptionsResponse struct {
	ID             string `json:"id"`
	SubscriptionID string `json:"subscriptionId"`
	TenantID       string `json:"tenantId"`
	DisplayName    string `json:"displayName"`
}

var ErrorAzureHealthCheck = errors.New("health check failed")
