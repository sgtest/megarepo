package elasticsearch

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/grafana/grafana-plugin-sdk-go/backend"
	"github.com/stretchr/testify/require"

	"github.com/grafana/grafana/pkg/infra/httpclient"
	es "github.com/grafana/grafana/pkg/tsdb/elasticsearch/client"
)

type datasourceInfo struct {
	TimeField                  any    `json:"timeField"`
	MaxConcurrentShardRequests int64  `json:"maxConcurrentShardRequests"`
	Interval                   string `json:"interval"`
}

func TestNewInstanceSettings(t *testing.T) {
	t.Run("fields exist", func(t *testing.T) {
		dsInfo := datasourceInfo{
			TimeField:                  "@timestamp",
			MaxConcurrentShardRequests: 5,
		}
		settingsJSON, err := json.Marshal(dsInfo)
		require.NoError(t, err)

		dsSettings := backend.DataSourceInstanceSettings{
			JSONData: json.RawMessage(settingsJSON),
		}

		_, err = newInstanceSettings(httpclient.NewProvider())(context.Background(), dsSettings)
		require.NoError(t, err)
	})

	t.Run("timeField", func(t *testing.T) {
		t.Run("is nil", func(t *testing.T) {
			dsInfo := datasourceInfo{
				MaxConcurrentShardRequests: 5,
				Interval:                   "Daily",
			}

			settingsJSON, err := json.Marshal(dsInfo)
			require.NoError(t, err)

			dsSettings := backend.DataSourceInstanceSettings{
				JSONData: json.RawMessage(settingsJSON),
			}

			_, err = newInstanceSettings(httpclient.NewProvider())(context.Background(), dsSettings)
			require.EqualError(t, err, "timeField cannot be cast to string")
		})

		t.Run("is empty", func(t *testing.T) {
			dsInfo := datasourceInfo{
				MaxConcurrentShardRequests: 5,
				Interval:                   "Daily",
				TimeField:                  "",
			}

			settingsJSON, err := json.Marshal(dsInfo)
			require.NoError(t, err)

			dsSettings := backend.DataSourceInstanceSettings{
				JSONData: json.RawMessage(settingsJSON),
			}

			_, err = newInstanceSettings(httpclient.NewProvider())(context.Background(), dsSettings)
			require.EqualError(t, err, "elasticsearch time field name is required")
		})
	})
}

func TestCreateElasticsearchURL(t *testing.T) {
	tt := []struct {
		name     string
		settings es.DatasourceInfo
		req      backend.CallResourceRequest
		expected string
	}{
		{name: "with /_msearch path and valid url", settings: es.DatasourceInfo{URL: "http://localhost:9200"}, req: backend.CallResourceRequest{Path: "_msearch"}, expected: "http://localhost:9200/_msearch"},
		{name: "with _msearch path and valid url", settings: es.DatasourceInfo{URL: "http://localhost:9200"}, req: backend.CallResourceRequest{Path: "_msearch"}, expected: "http://localhost:9200/_msearch"},
		{name: "with _msearch path and valid url with /", settings: es.DatasourceInfo{URL: "http://localhost:9200/"}, req: backend.CallResourceRequest{Path: "_msearch"}, expected: "http://localhost:9200/_msearch"},
		{name: "with _mapping path and valid url", settings: es.DatasourceInfo{URL: "http://localhost:9200"}, req: backend.CallResourceRequest{Path: "/_mapping"}, expected: "http://localhost:9200/_mapping"},
		{name: "with /_mapping path and valid url", settings: es.DatasourceInfo{URL: "http://localhost:9200"}, req: backend.CallResourceRequest{Path: "/_mapping"}, expected: "http://localhost:9200/_mapping"},
		{name: "with /_mapping path and valid url with /", settings: es.DatasourceInfo{URL: "http://localhost:9200/"}, req: backend.CallResourceRequest{Path: "/_mapping"}, expected: "http://localhost:9200/_mapping"},
		{name: "with abc/_mapping path and valid url", settings: es.DatasourceInfo{URL: "http://localhost:9200"}, req: backend.CallResourceRequest{Path: "abc/_mapping"}, expected: "http://localhost:9200/abc/_mapping"},
		{name: "with /abc/_mapping path and valid url", settings: es.DatasourceInfo{URL: "http://localhost:9200"}, req: backend.CallResourceRequest{Path: "abc/_mapping"}, expected: "http://localhost:9200/abc/_mapping"},
		{name: "with /abc/_mapping path and valid url", settings: es.DatasourceInfo{URL: "http://localhost:9200/"}, req: backend.CallResourceRequest{Path: "abc/_mapping"}, expected: "http://localhost:9200/abc/_mapping"},
		// This is to support mappings to cross cluster search that includes ":"
		{name: "with path including :", settings: es.DatasourceInfo{URL: "http://localhost:9200/"}, req: backend.CallResourceRequest{Path: "ab:c/_mapping"}, expected: "http://localhost:9200/ab:c/_mapping"},
	}

	for _, test := range tt {
		t.Run(test.name, func(t *testing.T) {
			url, err := createElasticsearchURL(&test.req, &test.settings)
			require.NoError(t, err)
			require.Equal(t, test.expected, url.String())
		})
	}
}
