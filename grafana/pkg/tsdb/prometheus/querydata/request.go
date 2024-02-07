package querydata

import (
	"context"
	"fmt"
	"net/http"
	"regexp"
	"time"

	"github.com/grafana/grafana-azure-sdk-go/util/maputil"
	"github.com/grafana/grafana-plugin-sdk-go/backend"
	"github.com/grafana/grafana-plugin-sdk-go/data"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/trace"

	"github.com/grafana/grafana-plugin-sdk-go/backend/log"
	"github.com/grafana/grafana-plugin-sdk-go/backend/tracing"

	"github.com/grafana/grafana/pkg/tsdb/prometheus/client"
	"github.com/grafana/grafana/pkg/tsdb/prometheus/intervalv2"
	"github.com/grafana/grafana/pkg/tsdb/prometheus/models"
	"github.com/grafana/grafana/pkg/tsdb/prometheus/querydata/exemplar"
	"github.com/grafana/grafana/pkg/tsdb/prometheus/utils"
)

const legendFormatAuto = "__auto"

var legendFormatRegexp = regexp.MustCompile(`\{\{\s*(.+?)\s*\}\}`)

type ExemplarEvent struct {
	Time   time.Time
	Value  float64
	Labels map[string]string
}

// QueryData handles querying but different from buffered package uses a custom client instead of default Go Prom
// client.
type QueryData struct {
	intervalCalculator intervalv2.Calculator
	tracer             trace.Tracer
	client             *client.Client
	log                log.Logger
	ID                 int64
	URL                string
	TimeInterval       string
	exemplarSampler    func() exemplar.Sampler
}

func New(
	httpClient *http.Client,
	settings backend.DataSourceInstanceSettings,
	plog log.Logger,
) (*QueryData, error) {
	jsonData, err := utils.GetJsonData(settings)
	if err != nil {
		return nil, err
	}
	httpMethod, _ := maputil.GetStringOptional(jsonData, "httpMethod")

	timeInterval, err := maputil.GetStringOptional(jsonData, "timeInterval")
	if err != nil {
		return nil, err
	}

	if httpMethod == "" {
		httpMethod = http.MethodPost
	}

	promClient := client.NewClient(httpClient, httpMethod, settings.URL)

	// standard deviation sampler is the default for backwards compatibility
	exemplarSampler := exemplar.NewStandardDeviationSampler

	return &QueryData{
		intervalCalculator: intervalv2.NewCalculator(),
		tracer:             tracing.DefaultTracer(),
		log:                plog,
		client:             promClient,
		TimeInterval:       timeInterval,
		ID:                 settings.ID,
		URL:                settings.URL,
		exemplarSampler:    exemplarSampler,
	}, nil
}

func (s *QueryData) Execute(ctx context.Context, req *backend.QueryDataRequest) (*backend.QueryDataResponse, error) {
	fromAlert := req.Headers["FromAlert"] == "true"
	result := backend.QueryDataResponse{
		Responses: backend.Responses{},
	}

	cfg := backend.GrafanaConfigFromContext(ctx)
	hasPromQLScopeFeatureFlag := cfg.FeatureToggles().IsEnabled("promQLScope")
	hasPrometheusDataplaneFeatureFlag := cfg.FeatureToggles().IsEnabled("prometheusDataplane")

	for _, q := range req.Queries {
		query, err := models.Parse(q, s.TimeInterval, s.intervalCalculator, fromAlert, hasPromQLScopeFeatureFlag)
		if err != nil {
			return &result, err
		}

		r := s.fetch(ctx, s.client, query, hasPrometheusDataplaneFeatureFlag)
		if r == nil {
			s.log.FromContext(ctx).Debug("Received nil response from runQuery", "query", query.Expr)
			continue
		}
		result.Responses[q.RefID] = *r
	}

	return &result, nil
}

func (s *QueryData) fetch(ctx context.Context, client *client.Client, q *models.Query, enablePrometheusDataplane bool) *backend.DataResponse {
	traceCtx, end := s.trace(ctx, q)
	defer end()

	logger := s.log.FromContext(traceCtx)
	logger.Debug("Sending query", "start", q.Start, "end", q.End, "step", q.Step, "query", q.Expr)

	dr := &backend.DataResponse{
		Frames: data.Frames{},
		Error:  nil,
	}

	if q.InstantQuery {
		res := s.instantQuery(traceCtx, client, q, enablePrometheusDataplane)
		dr.Error = res.Error
		dr.Frames = res.Frames
		dr.Status = res.Status
	}

	if q.RangeQuery {
		res := s.rangeQuery(traceCtx, client, q, enablePrometheusDataplane)
		if res.Error != nil {
			if dr.Error == nil {
				dr.Error = res.Error
			} else {
				dr.Error = fmt.Errorf("%v %w", dr.Error, res.Error)
			}
			// When both instant and range are true, we may overwrite the status code.
			// To fix this (and other things) they should come in separate http requests.
			dr.Status = res.Status
		}
		dr.Frames = append(dr.Frames, res.Frames...)
	}

	if q.ExemplarQuery {
		res := s.exemplarQuery(traceCtx, client, q, enablePrometheusDataplane)
		if res.Error != nil {
			// If exemplar query returns error, we want to only log it and
			// continue with other results processing
			logger.Error("Exemplar query failed", "query", q.Expr, "err", res.Error)
		}
		dr.Frames = append(dr.Frames, res.Frames...)
	}

	return dr
}

func (s *QueryData) rangeQuery(ctx context.Context, c *client.Client, q *models.Query, enablePrometheusDataplaneFlag bool) backend.DataResponse {
	res, err := c.QueryRange(ctx, q)
	if err != nil {
		return backend.DataResponse{
			Error:  err,
			Status: backend.StatusBadGateway,
		}
	}

	defer func() {
		err := res.Body.Close()
		if err != nil {
			s.log.Warn("Failed to close query range response body", "error", err)
		}
	}()

	return s.parseResponse(ctx, q, res, enablePrometheusDataplaneFlag)
}

func (s *QueryData) instantQuery(ctx context.Context, c *client.Client, q *models.Query, enablePrometheusDataplaneFlag bool) backend.DataResponse {
	res, err := c.QueryInstant(ctx, q)
	if err != nil {
		return backend.DataResponse{
			Error:  err,
			Status: backend.StatusBadGateway,
		}
	}

	// This is only for health check fall back scenario
	if res.StatusCode != 200 && q.RefId == "__healthcheck__" {
		return backend.DataResponse{
			Error: fmt.Errorf(res.Status),
		}
	}

	defer func() {
		err := res.Body.Close()
		if err != nil {
			s.log.Warn("Failed to close response body", "error", err)
		}
	}()

	return s.parseResponse(ctx, q, res, enablePrometheusDataplaneFlag)
}

func (s *QueryData) exemplarQuery(ctx context.Context, c *client.Client, q *models.Query, enablePrometheusDataplaneFlag bool) backend.DataResponse {
	res, err := c.QueryExemplars(ctx, q)
	if err != nil {
		return backend.DataResponse{
			Error: err,
		}
	}

	defer func() {
		err := res.Body.Close()
		if err != nil {
			s.log.Warn("Failed to close response body", "error", err)
		}
	}()
	return s.parseResponse(ctx, q, res, enablePrometheusDataplaneFlag)
}

func (s *QueryData) trace(ctx context.Context, q *models.Query) (context.Context, func()) {
	return utils.StartTrace(ctx, s.tracer, "datasource.prometheus",
		attribute.String("expr", q.Expr),
		attribute.Int64("start_unixnano", q.Start.UnixNano()),
		attribute.Int64("stop_unixnano", q.End.UnixNano()),
	)
}
