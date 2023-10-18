package loganalytics

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/grafana/grafana-plugin-sdk-go/backend"
	"github.com/grafana/grafana-plugin-sdk-go/backend/httpclient"
	"github.com/stretchr/testify/require"

	"github.com/grafana/grafana/pkg/tsdb/azuremonitor/kinds/dataquery"
	"github.com/grafana/grafana/pkg/tsdb/azuremonitor/types"
)

func TestBuildingAzureLogAnalyticsQueries(t *testing.T) {
	datasource := &AzureLogAnalyticsDatasource{}
	fromStart := time.Date(2018, 3, 15, 13, 0, 0, 0, time.UTC).In(time.Local)
	timeRange := backend.TimeRange{From: fromStart, To: fromStart.Add(34 * time.Minute)}
	ctx := context.Background()
	svr := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		var correlationRes AzureCorrelationAPIResponse
		if strings.Contains(r.URL.Path, "test-op-id") {
			correlationRes = AzureCorrelationAPIResponse{
				ID:   "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1",
				Name: "guid-1",
				Type: "microsoft.insights/transactions",
				Properties: AzureCorrelationAPIResponseProperties{
					Resources: []string{
						"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1",
					},
					NextLink: nil,
				},
			}
		} else if strings.Contains(r.URL.Path, "op-id-multi") {
			correlationRes = AzureCorrelationAPIResponse{
				ID:   "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1",
				Name: "guid-1",
				Type: "microsoft.insights/transactions",
				Properties: AzureCorrelationAPIResponseProperties{
					Resources: []string{
						"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1",
						"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r2",
					},
					NextLink: nil,
				},
			}
		} else if strings.Contains(r.URL.Path, "op-id-non-overlapping") {
			correlationRes = AzureCorrelationAPIResponse{
				ID:   "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1",
				Name: "guid-1",
				Type: "microsoft.insights/transactions",
				Properties: AzureCorrelationAPIResponseProperties{
					Resources: []string{
						"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1",
						"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r3",
					},
					NextLink: nil,
				},
			}
		}
		err := json.NewEncoder(w).Encode(correlationRes)
		if err != nil {
			t.Errorf("failed to encode correlation API response")
		}
	}))

	provider := httpclient.NewProvider(httpclient.ProviderOptions{Timeout: &httpclient.DefaultTimeoutOptions})
	client, err := provider.New()
	if err != nil {
		t.Errorf("failed to create fake client")
	}

	dsInfo := types.DatasourceInfo{
		Services: map[string]types.DatasourceService{
			"Azure Monitor": {URL: svr.URL, HTTPClient: client},
		},
		JSONData: map[string]any{
			"azureLogAnalyticsSameAs": false,
		},
	}

	tests := []struct {
		name                     string
		queryModel               []backend.DataQuery
		azureLogAnalyticsQueries []*AzureLogAnalyticsQuery
		Err                      require.ErrorAssertionFunc
	}{
		{
			name: "Query with macros should be interpolated",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
						"queryType": "Azure Log Analytics",
						"azureLogAnalytics": {
							"resource":     "/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace",
							"query":        "Perf | where $__timeFilter() | where $__contains(Computer, 'comp1','comp2') | summarize avg(CounterValue) by bin(TimeGenerated, $__interval), Computer",
							"resultFormat": "%s",
							"dashboardTime": false
						}
					}`, types.TimeSeries)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureLogAnalytics),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: types.TimeSeries,
					URL:          "v1/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace/query",
					JSON: []byte(fmt.Sprintf(`{
						"queryType": "Azure Log Analytics",
						"azureLogAnalytics": {
							"resource":     "/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace",
							"query":        "Perf | where $__timeFilter() | where $__contains(Computer, 'comp1','comp2') | summarize avg(CounterValue) by bin(TimeGenerated, $__interval), Computer",
							"resultFormat": "%s",
							"dashboardTime": false
						}
					}`, types.TimeSeries)),
					Query:            "Perf | where ['TimeGenerated'] >= datetime('2018-03-15T13:00:00Z') and ['TimeGenerated'] <= datetime('2018-03-15T13:34:00Z') | where ['Computer'] in ('comp1','comp2') | summarize avg(CounterValue) by bin(TimeGenerated, 34000ms), Computer",
					Resources:        []string{"/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace"},
					TimeRange:        timeRange,
					QueryType:        dataquery.AzureQueryTypeAzureLogAnalytics,
					AppInsightsQuery: false,
					DashboardTime:    false,
				},
			},
			Err: require.NoError,
		},
		{
			name: "Legacy queries with a workspace GUID should use workspace-centric url",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
						"queryType": "Azure Log Analytics",
						"azureLogAnalytics": {
							"workspace":    "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
							"query":        "Perf",
							"resultFormat": "%s"
						}
					}`, types.TimeSeries)),
					RefID:     "A",
					QueryType: string(dataquery.AzureQueryTypeAzureLogAnalytics),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: types.TimeSeries,
					URL:          "v1/workspaces/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/query",
					JSON: []byte(fmt.Sprintf(`{
						"queryType": "Azure Log Analytics",
						"azureLogAnalytics": {
							"workspace":    "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
							"query":        "Perf",
							"resultFormat": "%s"
						}
					}`, types.TimeSeries)),
					Query:            "Perf",
					Resources:        []string{},
					QueryType:        dataquery.AzureQueryTypeAzureLogAnalytics,
					AppInsightsQuery: false,
					DashboardTime:    false,
				},
			},
			Err: require.NoError,
		},
		{
			name: "Legacy workspace queries with a resource URI (from a template variable) should use resource-centric url",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
						"queryType": "Azure Log Analytics",
						"azureLogAnalytics": {
							"workspace":    "/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace",
							"query":        "Perf",
							"resultFormat": "%s"
						}
					}`, types.TimeSeries)),
					RefID:     "A",
					QueryType: string(dataquery.AzureQueryTypeAzureLogAnalytics),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: types.TimeSeries,
					URL:          "v1/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace/query",
					JSON: []byte(fmt.Sprintf(`{
						"queryType": "Azure Log Analytics",
						"azureLogAnalytics": {
							"workspace":    "/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace",
							"query":        "Perf",
							"resultFormat": "%s"
						}
					}`, types.TimeSeries)),
					Query:            "Perf",
					Resources:        []string{},
					QueryType:        dataquery.AzureQueryTypeAzureLogAnalytics,
					AppInsightsQuery: false,
					DashboardTime:    false,
				},
			},
			Err: require.NoError,
		},
		{
			name: "Queries with multiple resources",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
						"queryType": "Azure Log Analytics",
						"azureLogAnalytics": {
							"resource":     "/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace",
							"query":        "Perf",
							"resultFormat": "%s",
							"dashboardTime": false
						}
					}`, types.TimeSeries)),
					RefID:     "A",
					QueryType: string(dataquery.AzureQueryTypeAzureLogAnalytics),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: types.TimeSeries,
					URL:          "v1/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace/query",
					JSON: []byte(fmt.Sprintf(`{
						"queryType": "Azure Log Analytics",
						"azureLogAnalytics": {
							"resource":     "/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace",
							"query":        "Perf",
							"resultFormat": "%s",
							"dashboardTime": false
						}
					}`, types.TimeSeries)),
					Query:            "Perf",
					Resources:        []string{"/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace"},
					QueryType:        dataquery.AzureQueryTypeAzureLogAnalytics,
					AppInsightsQuery: false,
					DashboardTime:    false,
				},
			},
			Err: require.NoError,
		},
		{
			name: "Query with multiple resources",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
						"queryType": "Azure Log Analytics",
						"azureLogAnalytics": {
							"resources":     ["/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace",  "/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace2"],
							"query":        "Perf",
							"resultFormat": "%s",
							"dashboardTime": false
						}
					}`, types.TimeSeries)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureLogAnalytics),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: types.TimeSeries,
					URL:          "v1/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace/query",
					JSON: []byte(fmt.Sprintf(`{
						"queryType": "Azure Log Analytics",
						"azureLogAnalytics": {
							"resources":     ["/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace",  "/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace2"],
							"query":        "Perf",
							"resultFormat": "%s",
							"dashboardTime": false
						}
					}`, types.TimeSeries)),
					Query:            "Perf",
					Resources:        []string{"/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace", "/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace2"},
					TimeRange:        timeRange,
					QueryType:        dataquery.AzureQueryTypeAzureLogAnalytics,
					AppInsightsQuery: false,
					DashboardTime:    false,
				},
			},
			Err: require.NoError,
		},
		{
			name: "Query that uses dashboard time",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
						"queryType": "Azure Log Analytics",
						"azureLogAnalytics": {
							"resources":     ["/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace"],
							"query":        "Perf",
							"resultFormat": "%s",
							"dashboardTime": true,
							"timeColumn":	"TimeGenerated"
						}
					}`, types.TimeSeries)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureLogAnalytics),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: types.TimeSeries,
					URL:          "v1/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace/query",
					JSON: []byte(fmt.Sprintf(`{
						"queryType": "Azure Log Analytics",
						"azureLogAnalytics": {
							"resources":     ["/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace"],
							"query":        "Perf",
							"resultFormat": "%s",
							"dashboardTime": true,
							"timeColumn":	"TimeGenerated"
						}
					}`, types.TimeSeries)),
					Query:            "Perf",
					Resources:        []string{"/subscriptions/aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee/resourceGroups/cloud-datasources/providers/Microsoft.OperationalInsights/workspaces/AppInsightsTestDataWorkspace"},
					TimeRange:        timeRange,
					QueryType:        dataquery.AzureQueryTypeAzureLogAnalytics,
					AppInsightsQuery: false,
					DashboardTime:    true,
					TimeColumn:       "TimeGenerated",
				},
			},
			Err: require.NoError,
		},

		{
			name: "trace query",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"resultFormat": "%s",
								"traceTypes":	["trace"],
								"operationId":	"test-op-id"
							}
						}`, dataquery.ResultFormatTable)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureTraces),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: dataquery.ResultFormatTable,
					URL:          "v1/apps/r1/query",
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"resultFormat": "%s",
								"traceTypes":	["trace"],
								"operationId":	"test-op-id"
							}
						}`, dataquery.ResultFormatTable)),
					Query: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true trace` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"},
					TimeRange: timeRange,
					QueryType: dataquery.AzureQueryTypeAzureTraces,
					TraceExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true trace` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceParentExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true trace` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| where (operation_ParentId != '' and operation_ParentId == '${__data.fields.parentSpanID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceLogsExploreQuery: "union availabilityResults,\n" + "customEvents,\n" + "dependencies,\n" + "exceptions,\n" + "pageViews,\n" + "requests,\n" + "traces\n" +
						"| where operation_Id == \"test-op-id\"",
					AppInsightsQuery: true,
					DashboardTime:    true,
					TimeColumn:       "timestamp",
				},
			},
			Err: require.NoError,
		},
		{
			name: "trace query with no result format set",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"traceTypes":	["trace"],
								"operationId":	"test-op-id"
							}
						}`),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureTraces),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: dataquery.ResultFormatTable,
					URL:          "v1/apps/r1/query",
					JSON: []byte(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"traceTypes":	["trace"],
								"operationId":	"test-op-id"
							}
						}`),
					Query: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true trace` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"},
					TimeRange: timeRange,
					QueryType: dataquery.AzureQueryTypeAzureTraces,
					TraceExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true trace` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceParentExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true trace` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| where (operation_ParentId != '' and operation_ParentId == '${__data.fields.parentSpanID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceLogsExploreQuery: "union availabilityResults,\n" + "customEvents,\n" + "dependencies,\n" + "exceptions,\n" + "pageViews,\n" + "requests,\n" + "traces\n" +
						"| where operation_Id == \"test-op-id\"",
					AppInsightsQuery: true,
					DashboardTime:    true,
					TimeColumn:       "timestamp",
				},
			},
			Err: require.NoError,
		},
		{
			name: "trace query with no operation ID",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"resultFormat": "%s"
							}
						}`, dataquery.ResultFormatTable)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureTraces),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: dataquery.ResultFormatTable,
					URL:          "v1/apps/r1/query",
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"resultFormat": "%s"
							}
						}`, dataquery.ResultFormatTable)),
					Query: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"},
					TimeRange: timeRange,
					QueryType: dataquery.AzureQueryTypeAzureTraces,
					TraceExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| where (operation_Id != '' and operation_Id == '${__data.fields.traceID}') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == '${__data.fields.traceID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceParentExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| where (operation_Id != '' and operation_Id == '${__data.fields.traceID}') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == '${__data.fields.traceID}')` +
						`| where (operation_ParentId != '' and operation_ParentId == '${__data.fields.parentSpanID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceLogsExploreQuery: "union availabilityResults,\n" + "customEvents,\n" + "dependencies,\n" + "exceptions,\n" + "pageViews,\n" + "requests,\n" + "traces\n" +
						"| where operation_Id == \"${__data.fields.traceID}\"",
					AppInsightsQuery: true,
					DashboardTime:    true,
					TimeColumn:       "timestamp",
				},
			},
			Err: require.NoError,
		},
		{
			name: "trace query with no types",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"resultFormat": "%s",
								"operationId":	"test-op-id"
							}
						}`, dataquery.ResultFormatTable)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureTraces),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: dataquery.ResultFormatTable,
					URL:          "v1/apps/r1/query",
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"resultFormat": "%s",
								"operationId":	"test-op-id"
							}
						}`, dataquery.ResultFormatTable)),
					Query: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"},
					TimeRange: timeRange,
					QueryType: dataquery.AzureQueryTypeAzureTraces,
					TraceExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceParentExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| where (operation_ParentId != '' and operation_ParentId == '${__data.fields.parentSpanID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceLogsExploreQuery: "union availabilityResults,\n" + "customEvents,\n" + "dependencies,\n" + "exceptions,\n" + "pageViews,\n" + "requests,\n" + "traces\n" +
						"| where operation_Id == \"test-op-id\"",
					AppInsightsQuery: true,
					DashboardTime:    true,
					TimeColumn:       "timestamp",
				},
			},
			Err: require.NoError,
		},
		{
			name: "trace query with eq filter",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
								"queryType": "Azure Traces",
								"azureTraces": {
									"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
									"resultFormat": "%s",
									"operationId":	"test-op-id",
									"filters":		[{"filters": ["test-app-id"], "property": "appId", "operation": "eq"}]
								}
							}`, dataquery.ResultFormatTable)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureTraces),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: dataquery.ResultFormatTable,
					URL:          "v1/apps/r1/query",
					JSON: []byte(fmt.Sprintf(`{
								"queryType": "Azure Traces",
								"azureTraces": {
									"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
									"resultFormat": "%s",
									"operationId":	"test-op-id",
									"filters":		[{"filters": ["test-app-id"], "property": "appId", "operation": "eq"}]
								}
							}`, dataquery.ResultFormatTable)),
					Query: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| where appId in ("test-app-id")` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"},
					TimeRange: timeRange,
					QueryType: dataquery.AzureQueryTypeAzureTraces,
					TraceExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| where appId in ("test-app-id")` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceParentExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| where (operation_ParentId != '' and operation_ParentId == '${__data.fields.parentSpanID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| where appId in ("test-app-id")` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceLogsExploreQuery: "union availabilityResults,\n" + "customEvents,\n" + "dependencies,\n" + "exceptions,\n" + "pageViews,\n" + "requests,\n" + "traces\n" +
						"| where operation_Id == \"test-op-id\"",
					AppInsightsQuery: true,
					DashboardTime:    true,
					TimeColumn:       "timestamp",
				},
			},
			Err: require.NoError,
		},
		{
			name: "trace query with ne filter",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
								"queryType": "Azure Traces",
								"azureTraces": {
									"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
									"resultFormat": "%s",
									"operationId":	"test-op-id",
									"filters":		[{"filters": ["test-app-id"], "property": "appId", "operation": "ne"}]
								}
							}`, dataquery.ResultFormatTable)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureTraces),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: dataquery.ResultFormatTable,
					URL:          "v1/apps/r1/query",
					JSON: []byte(fmt.Sprintf(`{
								"queryType": "Azure Traces",
								"azureTraces": {
									"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
									"resultFormat": "%s",
									"operationId":	"test-op-id",
									"filters":		[{"filters": ["test-app-id"], "property": "appId", "operation": "ne"}]
								}
							}`, dataquery.ResultFormatTable)),
					Query: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| where appId !in ("test-app-id")` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"},
					TimeRange: timeRange,
					QueryType: dataquery.AzureQueryTypeAzureTraces,
					TraceExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| where appId !in ("test-app-id")` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceParentExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| where (operation_ParentId != '' and operation_ParentId == '${__data.fields.parentSpanID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| where appId !in ("test-app-id")` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceLogsExploreQuery: "union availabilityResults,\n" + "customEvents,\n" + "dependencies,\n" + "exceptions,\n" + "pageViews,\n" + "requests,\n" + "traces\n" +
						"| where operation_Id == \"test-op-id\"",
					AppInsightsQuery: true,
					DashboardTime:    true,
					TimeColumn:       "timestamp",
				},
			},
			Err: require.NoError,
		},
		{
			name: "trace query with multiple filters",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
								"queryType": "Azure Traces",
								"azureTraces": {
									"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
									"resultFormat": "%s",
									"operationId":	"test-op-id",
									"filters":		[{"filters": ["test-app-id"], "property": "appId", "operation": "ne"},{"filters": ["test-client-id"], "property": "clientId", "operation": "eq"}]
							}
						}`, dataquery.ResultFormatTable)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureTraces),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: dataquery.ResultFormatTable,
					URL:          "v1/apps/r1/query",
					JSON: []byte(fmt.Sprintf(`{
								"queryType": "Azure Traces",
								"azureTraces": {
									"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
									"resultFormat": "%s",
									"operationId":	"test-op-id",
									"filters":		[{"filters": ["test-app-id"], "property": "appId", "operation": "ne"},{"filters": ["test-client-id"], "property": "clientId", "operation": "eq"}]
							}
						}`, dataquery.ResultFormatTable)),
					Query: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| where appId !in ("test-app-id")| where clientId in ("test-client-id")` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"},
					TimeRange: timeRange,
					QueryType: dataquery.AzureQueryTypeAzureTraces,
					TraceExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| where appId !in ("test-app-id")| where clientId in ("test-client-id")` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceParentExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,traces` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| where (operation_ParentId != '' and operation_ParentId == '${__data.fields.parentSpanID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| where appId !in ("test-app-id")| where clientId in ("test-client-id")` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceLogsExploreQuery: "union availabilityResults,\n" + "customEvents,\n" + "dependencies,\n" + "exceptions,\n" + "pageViews,\n" + "requests,\n" + "traces\n" +
						"| where operation_Id == \"test-op-id\"",
					AppInsightsQuery: true,
					DashboardTime:    true,
					TimeColumn:       "timestamp",
				},
			},
			Err: require.NoError,
		},
		{
			name: "trace query with trace result format",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"resultFormat": "%s"
							}
						}`, dataquery.ResultFormatTrace)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureTraces),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: dataquery.ResultFormatTrace,
					URL:          "v1/apps/r1/query",
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"resources":     ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"resultFormat": "%s"
							}
						}`, dataquery.ResultFormatTrace)),
					Query: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"},
					TimeRange: timeRange,
					QueryType: dataquery.AzureQueryTypeAzureTraces,
					TraceExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests` +
						`| where (operation_Id != '' and operation_Id == '${__data.fields.traceID}') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == '${__data.fields.traceID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceParentExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests` +
						`| where (operation_Id != '' and operation_Id == '${__data.fields.traceID}') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == '${__data.fields.traceID}')` +
						`| where (operation_ParentId != '' and operation_ParentId == '${__data.fields.parentSpanID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceLogsExploreQuery: "union availabilityResults,\n" + "customEvents,\n" + "dependencies,\n" + "exceptions,\n" + "pageViews,\n" + "requests,\n" + "traces\n" +
						"| where operation_Id == \"${__data.fields.traceID}\"",
					AppInsightsQuery: true,
					DashboardTime:    true,
					TimeColumn:       "timestamp",
				},
			},
			Err: require.NoError,
		},
		{
			name: "trace query with trace result format and operation ID",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"operationId": 	"test-op-id",
								"resources":    ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"resultFormat": "%s"
							}
						}`, dataquery.ResultFormatTrace)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureTraces),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: dataquery.ResultFormatTrace,
					URL:          "v1/apps/r1/query",
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"operationId": 	"test-op-id",
								"resources":    ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"resultFormat": "%s"
							}
						}`, dataquery.ResultFormatTrace)),
					Query: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"},
					TimeRange: timeRange,
					QueryType: dataquery.AzureQueryTypeAzureTraces,
					TraceExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceParentExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests` +
						`| where (operation_Id != '' and operation_Id == 'test-op-id') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'test-op-id')` +
						`| where (operation_ParentId != '' and operation_ParentId == '${__data.fields.parentSpanID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName` +
						`| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceLogsExploreQuery: "union availabilityResults,\n" + "customEvents,\n" + "dependencies,\n" + "exceptions,\n" + "pageViews,\n" + "requests,\n" + "traces\n" +
						"| where operation_Id == \"test-op-id\"",
					AppInsightsQuery: true,
					DashboardTime:    true,
					TimeColumn:       "timestamp",
				},
			},
			Err: require.NoError,
		},
		{
			name: "trace query with trace result format and only trace type",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"operationId": 	"test-op-id",
								"resources":    ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"resultFormat": "%s",
								"traceTypes":		["traces"]
							}
						}`, dataquery.ResultFormatTrace)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureTraces),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: dataquery.ResultFormatTrace,
					URL:          "v1/apps/r1/query",
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"operationId": 	"test-op-id",
								"resources":    ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"resultFormat": "%s",
								"traceTypes":		["traces"]
							}
						}`, dataquery.ResultFormatTrace)),
					Query:                   "",
					Resources:               []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"},
					TimeRange:               timeRange,
					QueryType:               dataquery.AzureQueryTypeAzureTraces,
					TraceExploreQuery:       "",
					TraceParentExploreQuery: "",
					TraceLogsExploreQuery: "union availabilityResults,\n" + "customEvents,\n" + "dependencies,\n" + "exceptions,\n" + "pageViews,\n" + "requests,\n" + "traces\n" +
						"| where operation_Id == \"test-op-id\"",
					AppInsightsQuery: true,
					DashboardTime:    true,
					TimeColumn:       "timestamp",
				},
			},
			Err: require.NoError,
		},
		{
			name: "trace query with operation ID and correlated workspaces",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"operationId": 	"op-id-multi",
								"resources":    ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"resultFormat": "%s"
							}
						}`, dataquery.ResultFormatTrace)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureTraces),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: dataquery.ResultFormatTrace,
					URL:          "v1/apps/r1/query",
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"operationId": 	"op-id-multi",
								"resources":    ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"],
								"resultFormat": "%s"
							}
						}`, dataquery.ResultFormatTrace)),
					Query: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests` +
						`| where (operation_Id != '' and operation_Id == 'op-id-multi') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'op-id-multi')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1"},
					TimeRange: timeRange,
					QueryType: dataquery.AzureQueryTypeAzureTraces,
					TraceExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests` +
						`| where (operation_Id != '' and operation_Id == 'op-id-multi') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'op-id-multi')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceParentExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests` +
						`| where (operation_Id != '' and operation_Id == 'op-id-multi') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'op-id-multi')` +
						`| where (operation_ParentId != '' and operation_ParentId == '${__data.fields.parentSpanID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceLogsExploreQuery: "union *,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').traces\n" +
						"| where operation_Id == \"op-id-multi\"",
					AppInsightsQuery: true,
					DashboardTime:    true,
					TimeColumn:       "timestamp",
				},
			},
			Err: require.NoError,
		},
		{
			name: "trace query with multiple resources",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"resources":    ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1", "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r2"],
								"resultFormat": "%s"
							}
						}`, dataquery.ResultFormatTrace)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureTraces),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: dataquery.ResultFormatTrace,
					URL:          "v1/apps/r1/query",
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"resources":    ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1", "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r2"],
								"resultFormat": "%s"
							}
						}`, dataquery.ResultFormatTrace)),
					Query: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1", "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r2"},
					TimeRange: timeRange,
					QueryType: dataquery.AzureQueryTypeAzureTraces,
					TraceExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests` +
						`| where (operation_Id != '' and operation_Id == '${__data.fields.traceID}') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == '${__data.fields.traceID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceParentExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests` +
						`| where (operation_Id != '' and operation_Id == '${__data.fields.traceID}') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == '${__data.fields.traceID}')` +
						`| where (operation_ParentId != '' and operation_ParentId == '${__data.fields.parentSpanID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceLogsExploreQuery: "union *,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').traces\n" +
						"| where operation_Id == \"${__data.fields.traceID}\"",
					AppInsightsQuery: true,
					DashboardTime:    true,
					TimeColumn:       "timestamp",
				},
			},
			Err: require.NoError,
		},
		{
			name: "trace query with multiple resources and overlapping correlated workspaces",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"operationId": 	"op-id-multi",
								"resources":    ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1", "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r2"],
								"resultFormat": "%s"
							}
						}`, dataquery.ResultFormatTrace)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureTraces),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: dataquery.ResultFormatTrace,
					URL:          "v1/apps/r1/query",
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"operationId": 	"op-id-multi",
								"resources":    ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1", "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r2"],
								"resultFormat": "%s"
							}
						}`, dataquery.ResultFormatTrace)),
					Query: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests` +
						`| where (operation_Id != '' and operation_Id == 'op-id-multi') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'op-id-multi')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1", "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r2"},
					TimeRange: timeRange,
					QueryType: dataquery.AzureQueryTypeAzureTraces,
					TraceExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests` +
						`| where (operation_Id != '' and operation_Id == 'op-id-multi') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'op-id-multi')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceParentExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests` +
						`| where (operation_Id != '' and operation_Id == 'op-id-multi') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'op-id-multi')` +
						`| where (operation_ParentId != '' and operation_ParentId == '${__data.fields.parentSpanID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceLogsExploreQuery: "union *,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').traces\n" +
						"| where operation_Id == \"op-id-multi\"",
					AppInsightsQuery: true,
					DashboardTime:    true,
					TimeColumn:       "timestamp",
				},
			},
			Err: require.NoError,
		},
		{
			name: "trace query with multiple resources and non-overlapping correlated workspaces",
			queryModel: []backend.DataQuery{
				{
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"operationId": 	"op-id-non-overlapping",
								"resources":    ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1", "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r2"],
								"resultFormat": "%s"
							}
						}`, dataquery.ResultFormatTrace)),
					RefID:     "A",
					TimeRange: timeRange,
					QueryType: string(dataquery.AzureQueryTypeAzureTraces),
				},
			},
			azureLogAnalyticsQueries: []*AzureLogAnalyticsQuery{
				{
					RefID:        "A",
					ResultFormat: dataquery.ResultFormatTrace,
					URL:          "v1/apps/r1/query",
					JSON: []byte(fmt.Sprintf(`{
							"queryType": "Azure Traces",
							"azureTraces": {
								"operationId": 	"op-id-non-overlapping",
								"resources":    ["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1", "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r2"],
								"resultFormat": "%s"
							}
						}`, dataquery.ResultFormatTrace)),
					Query: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').requests` +
						`| where (operation_Id != '' and operation_Id == 'op-id-non-overlapping') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'op-id-non-overlapping')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1", "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r2"},
					TimeRange: timeRange,
					QueryType: dataquery.AzureQueryTypeAzureTraces,
					TraceExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').requests` +
						`| where (operation_Id != '' and operation_Id == 'op-id-non-overlapping') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'op-id-non-overlapping')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceParentExploreQuery: `set truncationmaxrecords=10000; set truncationmaxsize=67108864; union isfuzzy=true availabilityResults,customEvents,dependencies,exceptions,pageViews,requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').availabilityResults,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').customEvents,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').dependencies,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').exceptions,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').pageViews,app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').requests` +
						`| where (operation_Id != '' and operation_Id == 'op-id-non-overlapping') or (customDimensions.ai_legacyRootId != '' and customDimensions.ai_legacyRootId == 'op-id-non-overlapping')` +
						`| where (operation_ParentId != '' and operation_ParentId == '${__data.fields.parentSpanID}')` +
						`| extend duration = iff(isnull(column_ifexists("duration", real(null))), toreal(0), column_ifexists("duration", real(null)))` +
						`| extend spanID = iff(itemType == "pageView" or isempty(column_ifexists("id", "")), tostring(new_guid()), column_ifexists("id", ""))` +
						`| extend operationName = iff(isempty(column_ifexists("name", "")), column_ifexists("problemId", ""), column_ifexists("name", ""))` +
						`| extend serviceName = cloud_RoleName| extend serviceTags = bag_pack_columns(cloud_RoleInstance, cloud_RoleName)` +
						`| extend error = todynamic(iff(itemType == "exception", "true", "false"))` +
						`| extend tags = bag_merge(bag_pack_columns(appId,appName,application_Version,assembly,client_Browser,client_City,client_CountryOrRegion,client_IP,client_Model,client_OS,client_StateOrProvince,client_Type,data,details,duration,error,handledAt,iKey,id,innermostAssembly,innermostMessage,innermostMethod,innermostType,itemCount,itemId,itemType,location,message,method,name,operation_Id,operation_Name,operation_ParentId,operation_SyntheticSource,outerAssembly,outerMessage,outerMethod,outerType,performanceBucket,problemId,resultCode,sdkVersion,session_Id,severityLevel,size,source,success,target,timestamp,type,url,user_AccountId,user_AuthenticatedId,user_Id), customDimensions, customMeasurements)` +
						`| project-rename traceID = operation_Id, parentSpanID = operation_ParentId, startTime = timestamp` +
						`| project startTime, itemType, serviceName, duration, traceID, spanID, parentSpanID, operationName, serviceTags, tags, itemId` +
						`| order by startTime asc`,
					TraceLogsExploreQuery: "union *,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').availabilityResults,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').customEvents,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').dependencies,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').exceptions,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').pageViews,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').requests,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r2').traces,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').availabilityResults,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').customEvents,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').dependencies,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').exceptions,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').pageViews,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').requests,\n" +
						"app('/subscriptions/test-sub/resourcegroups/test-rg/providers/microsoft.insights/components/r3').traces\n" +
						"| where operation_Id == \"op-id-non-overlapping\"",
					AppInsightsQuery: true,
					DashboardTime:    true,
					TimeColumn:       "timestamp",
				},
			},
			Err: require.NoError,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			queries, err := datasource.buildQueries(ctx, tt.queryModel, dsInfo)
			tt.Err(t, err)
			if diff := cmp.Diff(tt.azureLogAnalyticsQueries[0], queries[0]); diff != "" {
				t.Errorf("Result mismatch (-want +got): \n%s", diff)
			}
		})
	}
}

func TestLogAnalyticsCreateRequest(t *testing.T) {
	ctx := context.Background()
	url := "http://ds/"

	t.Run("creates a request", func(t *testing.T) {
		ds := AzureLogAnalyticsDatasource{}
		req, err := ds.createRequest(ctx, url, &AzureLogAnalyticsQuery{
			Resources:        []string{"r"},
			Query:            "Perf",
			DashboardTime:    false,
			AppInsightsQuery: false,
		})
		require.NoError(t, err)
		if req.URL.String() != url {
			t.Errorf("Expecting %s, got %s", url, req.URL.String())
		}
		expectedHeaders := http.Header{"Content-Type": []string{"application/json"}}
		if !cmp.Equal(req.Header, expectedHeaders) {
			t.Errorf("Unexpected HTTP headers: %v", cmp.Diff(req.Header, expectedHeaders))
		}
		expectedBody := `{"query":"Perf"}`
		body, err := io.ReadAll(req.Body)
		require.NoError(t, err)
		if !cmp.Equal(string(body), expectedBody) {
			t.Errorf("Unexpected Body: %v", cmp.Diff(string(body), expectedBody))
		}
	})

	t.Run("creates a request with multiple resources", func(t *testing.T) {
		ds := AzureLogAnalyticsDatasource{}
		req, err := ds.createRequest(ctx, url, &AzureLogAnalyticsQuery{
			Resources:        []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.OperationalInsights/workspaces/r1", "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.OperationalInsights/workspaces/r2"},
			Query:            "Perf",
			QueryType:        dataquery.AzureQueryTypeAzureLogAnalytics,
			AppInsightsQuery: false,
			DashboardTime:    false,
		})
		require.NoError(t, err)
		expectedBody := `{"query":"Perf","workspaces":["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.OperationalInsights/workspaces/r1","/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.OperationalInsights/workspaces/r2"]}`
		body, err := io.ReadAll(req.Body)
		require.NoError(t, err)
		if !cmp.Equal(string(body), expectedBody) {
			t.Errorf("Unexpected Body: %v", cmp.Diff(string(body), expectedBody))
		}
	})

	t.Run("creates a request with timerange from dashboard", func(t *testing.T) {
		ds := AzureLogAnalyticsDatasource{}
		from := time.Now()
		to := from.Add(3 * time.Hour)
		req, err := ds.createRequest(ctx, url, &AzureLogAnalyticsQuery{
			Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.OperationalInsights/workspaces/r1", "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.OperationalInsights/workspaces/r2"},
			Query:     "Perf",
			QueryType: dataquery.AzureQueryTypeAzureLogAnalytics,
			TimeRange: backend.TimeRange{
				From: from,
				To:   to,
			},
			AppInsightsQuery: false,
			DashboardTime:    true,
			TimeColumn:       "TimeGenerated",
		})
		require.NoError(t, err)
		expectedBody := fmt.Sprintf(`{"query":"Perf","query_datetimescope_column":"TimeGenerated","query_datetimescope_from":"%s","query_datetimescope_to":"%s","timespan":"%s/%s","workspaces":["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.OperationalInsights/workspaces/r1","/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.OperationalInsights/workspaces/r2"]}`, from.Format(time.RFC3339), to.Format(time.RFC3339), from.Format(time.RFC3339), to.Format(time.RFC3339))
		body, err := io.ReadAll(req.Body)
		require.NoError(t, err)
		if !cmp.Equal(string(body), expectedBody) {
			t.Errorf("Unexpected Body: %v", cmp.Diff(string(body), expectedBody))
		}
	})

	t.Run("correctly passes multiple resources for traces queries", func(t *testing.T) {
		ds := AzureLogAnalyticsDatasource{}
		from := time.Now()
		to := from.Add(3 * time.Hour)
		req, err := ds.createRequest(ctx, url, &AzureLogAnalyticsQuery{
			Resources: []string{"/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1", "/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r2"},
			QueryType: dataquery.AzureQueryTypeAzureTraces,
			TimeRange: backend.TimeRange{
				From: from,
				To:   to,
			},
			AppInsightsQuery: true,
			DashboardTime:    true,
			TimeColumn:       "timestamp",
		})
		require.NoError(t, err)
		expectedBody := fmt.Sprintf(`{"applications":["/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r1","/subscriptions/test-sub/resourceGroups/test-rg/providers/Microsoft.Insights/components/r2"],"query":"","query_datetimescope_column":"timestamp","query_datetimescope_from":"%s","query_datetimescope_to":"%s","timespan":"%s/%s"}`, from.Format(time.RFC3339), to.Format(time.RFC3339), from.Format(time.RFC3339), to.Format(time.RFC3339))
		body, err := io.ReadAll(req.Body)
		require.NoError(t, err)
		if !cmp.Equal(string(body), expectedBody) {
			t.Errorf("Unexpected Body: %v", cmp.Diff(string(body), expectedBody))
		}
	})
}

func Test_executeQueryErrorWithDifferentLogAnalyticsCreds(t *testing.T) {
	ds := AzureLogAnalyticsDatasource{}
	dsInfo := types.DatasourceInfo{
		Services: map[string]types.DatasourceService{
			"Azure Log Analytics": {URL: "http://ds"},
		},
		JSONData: map[string]any{
			"azureLogAnalyticsSameAs": false,
		},
	}
	ctx := context.Background()
	query := &AzureLogAnalyticsQuery{
		TimeRange: backend.TimeRange{},
	}
	_, err := ds.executeQuery(ctx, query, dsInfo, &http.Client{}, dsInfo.Services["Azure Log Analytics"].URL)
	if err == nil {
		t.Fatal("expecting an error")
	}
	if !strings.Contains(err.Error(), "credentials for Log Analytics are no longer supported") {
		t.Error("expecting the error to inform of bad credentials")
	}
}
