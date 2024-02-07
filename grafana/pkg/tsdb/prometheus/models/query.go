package models

import (
	"encoding/json"
	"fmt"
	"math"
	"strconv"
	"strings"
	"time"

	"github.com/grafana/grafana-plugin-sdk-go/backend"
	"github.com/grafana/grafana-plugin-sdk-go/backend/gtime"
	"github.com/prometheus/prometheus/model/labels"
	"github.com/prometheus/prometheus/promql/parser"

	"github.com/grafana/grafana/pkg/tsdb/prometheus/intervalv2"
	"github.com/grafana/grafana/pkg/tsdb/prometheus/kinds/dataquery"
)

// Internal interval and range variables
const (
	varInterval       = "$__interval"
	varIntervalMs     = "$__interval_ms"
	varRange          = "$__range"
	varRangeS         = "$__range_s"
	varRangeMs        = "$__range_ms"
	varRateInterval   = "$__rate_interval"
	varRateIntervalMs = "$__rate_interval_ms"
)

// Internal interval and range variables with {} syntax
// Repetitive code, we should have functionality to unify these
const (
	varIntervalAlt       = "${__interval}"
	varIntervalMsAlt     = "${__interval_ms}"
	varRangeAlt          = "${__range}"
	varRangeSAlt         = "${__range_s}"
	varRangeMsAlt        = "${__range_ms}"
	varRateIntervalAlt   = "${__rate_interval}"
	varRateIntervalMsAlt = "${__rate_interval_ms}"
)

type TimeSeriesQueryType string

const (
	RangeQueryType    TimeSeriesQueryType = "range"
	InstantQueryType  TimeSeriesQueryType = "instant"
	ExemplarQueryType TimeSeriesQueryType = "exemplar"
	UnknownQueryType  TimeSeriesQueryType = "unknown"
)

var safeResolution = 11000

type QueryModel struct {
	dataquery.PrometheusDataQuery
	// The following properties may be part of the request payload, however they are not saved in panel JSON
	// Timezone offset to align start & end time on backend
	UtcOffsetSec   int64  `json:"utcOffsetSec,omitempty"`
	LegendFormat   string `json:"legendFormat,omitempty"`
	Interval       string `json:"interval,omitempty"`
	IntervalMs     int64  `json:"intervalMs,omitempty"`
	IntervalFactor int64  `json:"intervalFactor,omitempty"`
}

type TimeRange struct {
	Start time.Time
	End   time.Time
	Step  time.Duration
}

type Query struct {
	Expr          string
	Step          time.Duration
	LegendFormat  string
	Start         time.Time
	End           time.Time
	RefId         string
	InstantQuery  bool
	RangeQuery    bool
	ExemplarQuery bool
	UtcOffsetSec  int64
	Scope         Scope
}

type Scope struct {
	Matchers []*labels.Matcher
}

func Parse(query backend.DataQuery, dsScrapeInterval string, intervalCalculator intervalv2.Calculator, fromAlert bool, enableScope bool) (*Query, error) {
	model := &QueryModel{}
	if err := json.Unmarshal(query.JSON, model); err != nil {
		return nil, err
	}

	// Final step value for prometheus
	calculatedStep, err := calculatePrometheusInterval(model.Interval, dsScrapeInterval, model.IntervalMs, model.IntervalFactor, query, intervalCalculator)
	if err != nil {
		return nil, err
	}

	// Interpolate variables in expr
	timeRange := query.TimeRange.To.Sub(query.TimeRange.From)
	expr := interpolateVariables(
		model.Expr,
		query.Interval,
		calculatedStep,
		model.Interval,
		dsScrapeInterval,
		timeRange,
	)
	var matchers []*labels.Matcher
	if enableScope && model.Scope != nil && model.Scope.Matchers != "" {
		matchers, err = parser.ParseMetricSelector(model.Scope.Matchers)
		if err != nil {
			return nil, fmt.Errorf("failed to parse metric selector %v in scope", model.Scope.Matchers)
		}
		expr, err = ApplyQueryScope(expr, matchers)
		if err != nil {
			return nil, err
		}
	}
	var rangeQuery, instantQuery bool
	if model.Instant == nil {
		instantQuery = false
	} else {
		instantQuery = *model.Instant
	}
	if model.Range == nil {
		rangeQuery = false
	} else {
		rangeQuery = *model.Range
	}
	if !instantQuery && !rangeQuery {
		// In older dashboards, we were not setting range query param and !range && !instant was run as range query
		rangeQuery = true
	}

	// We never want to run exemplar query for alerting
	exemplarQuery := false
	if model.Exemplar != nil {
		exemplarQuery = *model.Exemplar
	}
	if fromAlert {
		exemplarQuery = false
	}

	return &Query{
		Expr:          expr,
		Step:          calculatedStep,
		LegendFormat:  model.LegendFormat,
		Start:         query.TimeRange.From,
		End:           query.TimeRange.To,
		RefId:         query.RefID,
		InstantQuery:  instantQuery,
		RangeQuery:    rangeQuery,
		ExemplarQuery: exemplarQuery,
		UtcOffsetSec:  model.UtcOffsetSec,
	}, nil
}

func (query *Query) Type() TimeSeriesQueryType {
	if query.InstantQuery {
		return InstantQueryType
	}
	if query.RangeQuery {
		return RangeQueryType
	}
	if query.ExemplarQuery {
		return ExemplarQueryType
	}
	return UnknownQueryType
}

func (query *Query) TimeRange() TimeRange {
	return TimeRange{
		Step: query.Step,
		// Align query range to step. It rounds start and end down to a multiple of step.
		Start: AlignTimeRange(query.Start, query.Step, query.UtcOffsetSec),
		End:   AlignTimeRange(query.End, query.Step, query.UtcOffsetSec),
	}
}

func calculatePrometheusInterval(
	queryInterval, dsScrapeInterval string,
	intervalMs, intervalFactor int64,
	query backend.DataQuery,
	intervalCalculator intervalv2.Calculator,
) (time.Duration, error) {
	// we need to compare the original query model after it is overwritten below to variables so that we can
	// calculate the rateInterval if it is equal to $__rate_interval or ${__rate_interval}
	originalQueryInterval := queryInterval

	// If we are using variable for interval/step, we will replace it with calculated interval
	if isVariableInterval(queryInterval) {
		queryInterval = ""
	}

	minInterval, err := gtime.GetIntervalFrom(dsScrapeInterval, queryInterval, intervalMs, 15*time.Second)
	if err != nil {
		return time.Duration(0), err
	}
	calculatedInterval := intervalCalculator.Calculate(query.TimeRange, minInterval, query.MaxDataPoints)
	safeInterval := intervalCalculator.CalculateSafeInterval(query.TimeRange, int64(safeResolution))

	adjustedInterval := safeInterval.Value
	if calculatedInterval.Value > safeInterval.Value {
		adjustedInterval = calculatedInterval.Value
	}

	// here is where we compare for $__rate_interval or ${__rate_interval}
	if originalQueryInterval == varRateInterval || originalQueryInterval == varRateIntervalAlt {
		// Rate interval is final and is not affected by resolution
		return calculateRateInterval(adjustedInterval, dsScrapeInterval), nil
	} else {
		queryIntervalFactor := intervalFactor
		if queryIntervalFactor == 0 {
			queryIntervalFactor = 1
		}
		return time.Duration(int64(adjustedInterval) * queryIntervalFactor), nil
	}
}

// calculateRateInterval calculates the $__rate_interval value
// queryInterval is the value calculated range / maxDataPoints on the frontend
// queryInterval is shown on the Query Options Panel above the query editor
// requestedMinStep is the data source scrape interval (default 15s)
// requestedMinStep can be changed by setting "Min Step" value in Options panel below the code editor
func calculateRateInterval(
	queryInterval time.Duration,
	requestedMinStep string,
) time.Duration {
	scrape := requestedMinStep
	if scrape == "" {
		scrape = "15s"
	}

	scrapeIntervalDuration, err := gtime.ParseIntervalStringToTimeDuration(scrape)
	if err != nil {
		return time.Duration(0)
	}

	rateInterval := time.Duration(int64(math.Max(float64(queryInterval+scrapeIntervalDuration), float64(4)*float64(scrapeIntervalDuration))))
	return rateInterval
}

// interpolateVariables interpolates built-in variables
// expr                         PromQL query
// queryInterval                Requested interval in milliseconds. This value may be overridden by MinStep in query options
// calculatedStep               Calculated final step value. It was calculated in calculatePrometheusInterval
// requestedMinStep             Requested minimum step value. QueryModel.interval
// dsScrapeInterval             Data source scrape interval in the config
// timeRange                    Requested time range for query
func interpolateVariables(
	expr string,
	queryInterval time.Duration,
	calculatedStep time.Duration,
	requestedMinStep string,
	dsScrapeInterval string,
	timeRange time.Duration,
) string {
	rangeMs := timeRange.Milliseconds()
	rangeSRounded := int64(math.Round(float64(rangeMs) / 1000.0))

	var rateInterval time.Duration
	if requestedMinStep == varRateInterval || requestedMinStep == varRateIntervalAlt {
		rateInterval = calculatedStep
	} else {
		if requestedMinStep == varInterval || requestedMinStep == varIntervalAlt {
			requestedMinStep = calculatedStep.String()
		}
		if requestedMinStep == "" {
			requestedMinStep = dsScrapeInterval
		}
		rateInterval = calculateRateInterval(queryInterval, requestedMinStep)
	}

	expr = strings.ReplaceAll(expr, varIntervalMs, strconv.FormatInt(int64(calculatedStep/time.Millisecond), 10))
	expr = strings.ReplaceAll(expr, varInterval, gtime.FormatInterval(calculatedStep))
	expr = strings.ReplaceAll(expr, varRangeMs, strconv.FormatInt(rangeMs, 10))
	expr = strings.ReplaceAll(expr, varRangeS, strconv.FormatInt(rangeSRounded, 10))
	expr = strings.ReplaceAll(expr, varRange, strconv.FormatInt(rangeSRounded, 10)+"s")
	expr = strings.ReplaceAll(expr, varRateIntervalMs, strconv.FormatInt(int64(rateInterval/time.Millisecond), 10))
	expr = strings.ReplaceAll(expr, varRateInterval, rateInterval.String())

	// Repetitive code, we should have functionality to unify these
	expr = strings.ReplaceAll(expr, varIntervalMsAlt, strconv.FormatInt(int64(calculatedStep/time.Millisecond), 10))
	expr = strings.ReplaceAll(expr, varIntervalAlt, gtime.FormatInterval(calculatedStep))
	expr = strings.ReplaceAll(expr, varRangeMsAlt, strconv.FormatInt(rangeMs, 10))
	expr = strings.ReplaceAll(expr, varRangeSAlt, strconv.FormatInt(rangeSRounded, 10))
	expr = strings.ReplaceAll(expr, varRangeAlt, strconv.FormatInt(rangeSRounded, 10)+"s")
	expr = strings.ReplaceAll(expr, varRateIntervalMsAlt, strconv.FormatInt(int64(rateInterval/time.Millisecond), 10))
	expr = strings.ReplaceAll(expr, varRateIntervalAlt, rateInterval.String())
	return expr
}

func isVariableInterval(interval string) bool {
	if interval == varInterval || interval == varIntervalMs || interval == varRateInterval || interval == varRateIntervalMs {
		return true
	}
	// Repetitive code, we should have functionality to unify these
	if interval == varIntervalAlt || interval == varIntervalMsAlt || interval == varRateIntervalAlt || interval == varRateIntervalMsAlt {
		return true
	}
	return false
}

// AlignTimeRange aligns query range to step and handles the time offset.
// It rounds start and end down to a multiple of step.
// Prometheus caching is dependent on the range being aligned with the step.
// Rounding to the step can significantly change the start and end of the range for larger steps, i.e. a week.
// In rounding the range to a 1w step the range will always start on a Thursday.
func AlignTimeRange(t time.Time, step time.Duration, offset int64) time.Time {
	offsetNano := float64(offset * 1e9)
	stepNano := float64(step.Nanoseconds())
	return time.Unix(0, int64(math.Floor((float64(t.UnixNano())+offsetNano)/stepNano)*stepNano-offsetNano)).UTC()
}
