package expr

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"github.com/grafana/grafana-plugin-sdk-go/backend"
	"github.com/grafana/grafana-plugin-sdk-go/data"
	"go.opentelemetry.io/otel/attribute"
	"gonum.org/v1/gonum/graph/simple"

	"github.com/grafana/grafana/pkg/expr/classic"
	"github.com/grafana/grafana/pkg/expr/mathexp"
	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/services/datasources"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
)

var (
	logger = log.New("expr")
)

type QueryError struct {
	RefID string
	Err   error
}

func (e QueryError) Error() string {
	return fmt.Sprintf("failed to execute query %s: %s", e.RefID, e.Err)
}

func (e QueryError) Unwrap() error {
	return e.Err
}

// baseNode includes common properties used across DPNodes.
type baseNode struct {
	id    int64
	refID string
}

type rawNode struct {
	RefID      string `json:"refId"`
	Query      map[string]interface{}
	QueryType  string
	TimeRange  TimeRange
	DataSource *datasources.DataSource
}

func (rn *rawNode) GetCommandType() (c CommandType, err error) {
	rawType, ok := rn.Query["type"]
	if !ok {
		return c, fmt.Errorf("no expression command type in query for refId %v", rn.RefID)
	}
	typeString, ok := rawType.(string)
	if !ok {
		return c, fmt.Errorf("expected expression command type to be a string, got type %T", rawType)
	}
	return ParseCommandType(typeString)
}

// String returns a string representation of the node. In particular for
// %v formatting in error messages.
func (b *baseNode) String() string {
	return b.refID
}

// CMDNode is a DPNode that holds an expression command.
type CMDNode struct {
	baseNode
	CMDType CommandType
	Command Command
}

// ID returns the id of the node so it can fulfill the gonum's graph Node interface.
func (b *baseNode) ID() int64 {
	return b.id
}

// RefID returns the refId of the node.
func (b *baseNode) RefID() string {
	return b.refID
}

// NodeType returns the data pipeline node type.
func (gn *CMDNode) NodeType() NodeType {
	return TypeCMDNode
}

// Execute runs the node and adds the results to vars. If the node requires
// other nodes they must have already been executed and their results must
// already by in vars.
func (gn *CMDNode) Execute(ctx context.Context, now time.Time, vars mathexp.Vars, s *Service) (mathexp.Results, error) {
	return gn.Command.Execute(ctx, now, vars, s.tracer)
}

func buildCMDNode(dp *simple.DirectedGraph, rn *rawNode) (*CMDNode, error) {
	commandType, err := rn.GetCommandType()
	if err != nil {
		return nil, fmt.Errorf("invalid command type in expression '%v': %w", rn.RefID, err)
	}

	node := &CMDNode{
		baseNode: baseNode{
			id:    dp.NewNode().ID(),
			refID: rn.RefID,
		},
		CMDType: commandType,
	}

	switch commandType {
	case TypeMath:
		node.Command, err = UnmarshalMathCommand(rn)
	case TypeReduce:
		node.Command, err = UnmarshalReduceCommand(rn)
	case TypeResample:
		node.Command, err = UnmarshalResampleCommand(rn)
	case TypeClassicConditions:
		node.Command, err = classic.UnmarshalConditionsCmd(rn.Query, rn.RefID)
	case TypeThreshold:
		node.Command, err = UnmarshalThresholdCommand(rn)
	default:
		return nil, fmt.Errorf("expression command type '%v' in expression '%v' not implemented", commandType, rn.RefID)
	}
	if err != nil {
		return nil, fmt.Errorf("failed to parse expression '%v': %w", rn.RefID, err)
	}

	return node, nil
}

const (
	defaultIntervalMS = int64(64)
	defaultMaxDP      = int64(5000)
)

// DSNode is a DPNode that holds a datasource request.
type DSNode struct {
	baseNode
	query      json.RawMessage
	datasource *datasources.DataSource

	orgID      int64
	queryType  string
	timeRange  TimeRange
	intervalMS int64
	maxDP      int64
	request    Request
}

// NodeType returns the data pipeline node type.
func (dn *DSNode) NodeType() NodeType {
	return TypeDatasourceNode
}

func (s *Service) buildDSNode(dp *simple.DirectedGraph, rn *rawNode, req *Request) (*DSNode, error) {
	if rn.TimeRange == nil {
		return nil, fmt.Errorf("time range must be specified for refID %s", rn.RefID)
	}
	encodedQuery, err := json.Marshal(rn.Query)
	if err != nil {
		return nil, err
	}

	dsNode := &DSNode{
		baseNode: baseNode{
			id:    dp.NewNode().ID(),
			refID: rn.RefID,
		},
		orgID:      req.OrgId,
		query:      json.RawMessage(encodedQuery),
		queryType:  rn.QueryType,
		intervalMS: defaultIntervalMS,
		maxDP:      defaultMaxDP,
		timeRange:  rn.TimeRange,
		request:    *req,
		datasource: rn.DataSource,
	}

	var floatIntervalMS float64
	if rawIntervalMS, ok := rn.Query["intervalMs"]; ok {
		if floatIntervalMS, ok = rawIntervalMS.(float64); !ok {
			return nil, fmt.Errorf("expected intervalMs to be an float64, got type %T for refId %v", rawIntervalMS, rn.RefID)
		}
		dsNode.intervalMS = int64(floatIntervalMS)
	}

	var floatMaxDP float64
	if rawMaxDP, ok := rn.Query["maxDataPoints"]; ok {
		if floatMaxDP, ok = rawMaxDP.(float64); !ok {
			return nil, fmt.Errorf("expected maxDataPoints to be an float64, got type %T for refId %v", rawMaxDP, rn.RefID)
		}
		dsNode.maxDP = int64(floatMaxDP)
	}

	return dsNode, nil
}

// Execute runs the node and adds the results to vars. If the node requires
// other nodes they must have already been executed and their results must
// already by in vars.
func (dn *DSNode) Execute(ctx context.Context, now time.Time, _ mathexp.Vars, s *Service) (r mathexp.Results, e error) {
	logger := logger.FromContext(ctx).New("datasourceType", dn.datasource.Type, "queryRefId", dn.refID, "datasourceUid", dn.datasource.UID, "datasourceVersion", dn.datasource.Version)
	ctx, span := s.tracer.Start(ctx, "SSE.ExecuteDatasourceQuery")
	defer span.End()

	pCtx, err := s.pCtxProvider.GetWithDataSource(ctx, dn.datasource.Type, dn.request.User, dn.datasource)
	if err != nil {
		return mathexp.Results{}, err
	}
	span.SetAttributes("datasource.type", dn.datasource.Type, attribute.Key("datasource.type").String(dn.datasource.Type))

	req := &backend.QueryDataRequest{
		PluginContext: pCtx,
		Queries: []backend.DataQuery{
			{
				RefID:         dn.refID,
				MaxDataPoints: dn.maxDP,
				Interval:      time.Duration(int64(time.Millisecond) * dn.intervalMS),
				JSON:          dn.query,
				TimeRange:     dn.timeRange.AbsoluteTime(now),
				QueryType:     dn.queryType,
			},
		},
		Headers: dn.request.Headers,
	}

	responseType := "unknown"
	respStatus := "success"
	defer func() {
		if e != nil {
			responseType = "error"
			respStatus = "failure"
		}
		logger.Debug("Data source queried", "responseType", responseType)
		useDataplane := strings.HasPrefix(responseType, "dataplane-")
		s.metrics.dsRequests.WithLabelValues(respStatus, fmt.Sprintf("%t", useDataplane)).Inc()
	}()

	resp, err := s.dataService.QueryData(ctx, req)
	if err != nil {
		return mathexp.Results{}, err
	}

	var result mathexp.Results
	responseType, result, err = queryDataResponseToResults(ctx, resp, dn.refID, dn.datasource.Type, s)
	return result, err
}

func queryDataResponseToResults(ctx context.Context, resp *backend.QueryDataResponse, refID string, datasourceType string, s *Service) (string, mathexp.Results, error) {
	vals := make([]mathexp.Value, 0)
	response, ok := resp.Responses[refID]
	if !ok {
		if len(resp.Responses) > 0 {
			keys := make([]string, 0, len(resp.Responses))
			for refID := range resp.Responses {
				keys = append(keys, refID)
			}
			logger.Warn("Can't find response by refID. Return nodata", "responseRefIds", keys)
		}
		return "no-data", mathexp.Results{Values: mathexp.Values{mathexp.NoData{}.New()}}, nil
	}

	if response.Error != nil {
		return "", mathexp.Results{}, QueryError{RefID: refID, Err: response.Error}
	}

	var dt data.FrameType
	dt, useDataplane, _ := shouldUseDataplane(response.Frames, logger, s.features.IsEnabled(featuremgmt.FlagDisableSSEDataplane))
	if useDataplane {
		logger.Debug("Handling SSE data source query through dataplane", "datatype", dt)
		result, err := handleDataplaneFrames(ctx, s.tracer, dt, response.Frames)
		return fmt.Sprintf("dataplane-%s", dt), result, err
	}

	if isAllFrameVectors(datasourceType, response.Frames) { // Prometheus Specific Handling
		vals, err := framesToNumbers(response.Frames)
		if err != nil {
			return "", mathexp.Results{}, fmt.Errorf("failed to read frames as numbers: %w", err)
		}
		return "vector", mathexp.Results{Values: vals}, nil
	}

	if len(response.Frames) == 1 {
		frame := response.Frames[0]
		// Handle Untyped NoData
		if len(frame.Fields) == 0 {
			return "no-data", mathexp.Results{Values: mathexp.Values{mathexp.NoData{Frame: frame}}}, nil
		}

		// Handle Numeric Table
		if frame.TimeSeriesSchema().Type == data.TimeSeriesTypeNot && isNumberTable(frame) {
			numberSet, err := extractNumberSet(frame)
			if err != nil {
				return "", mathexp.Results{}, err
			}
			for _, n := range numberSet {
				vals = append(vals, n)
			}
			return "number set", mathexp.Results{
				Values: vals,
			}, nil
		}
	}

	for _, frame := range response.Frames {
		// Check for TimeSeriesTypeNot in InfluxDB queries. A data frame of this type will cause
		// the WideToMany() function to error out, which results in unhealthy alerts.
		// This check should be removed once inconsistencies in data source responses are solved.
		if frame.TimeSeriesSchema().Type == data.TimeSeriesTypeNot && datasourceType == datasources.DS_INFLUXDB {
			logger.Warn("Ignoring InfluxDB data frame due to missing numeric fields")
			continue
		}
		var series []mathexp.Series
		series, err := WideToMany(frame)
		if err != nil {
			return "", mathexp.Results{}, err
		}
		for _, ser := range series {
			vals = append(vals, ser)
		}
	}

	return "series set", mathexp.Results{
		Values: vals, // TODO vals can be empty. Should we replace with no-data?
	}, nil
}

func isAllFrameVectors(datasourceType string, frames data.Frames) bool {
	if datasourceType != "prometheus" {
		return false
	}
	allVector := false
	for i, frame := range frames {
		if frame.Meta != nil && frame.Meta.Custom != nil {
			if sMap, ok := frame.Meta.Custom.(map[string]string); ok {
				if sMap != nil {
					if sMap["resultType"] == "vector" {
						if i != 0 && !allVector {
							break
						}
						allVector = true
					}
				}
			}
		}
	}
	return allVector
}

func framesToNumbers(frames data.Frames) ([]mathexp.Value, error) {
	vals := make([]mathexp.Value, 0, len(frames))
	for _, frame := range frames {
		if frame == nil {
			continue
		}
		if len(frame.Fields) == 2 && frame.Fields[0].Len() == 1 {
			// Can there be zero Len Field results that are being skipped?
			valueField := frame.Fields[1]
			if valueField.Type().Numeric() { // should be []float64
				val, err := valueField.FloatAt(0) // FloatAt should not err if numeric
				if err != nil {
					return nil, fmt.Errorf("failed to read value of frame [%v] (RefID %v) of type [%v] as float: %w", frame.Name, frame.RefID, valueField.Type(), err)
				}
				n := mathexp.NewNumber(frame.Name, valueField.Labels)
				n.SetValue(&val)
				vals = append(vals, n)
			}
		}
	}
	return vals, nil
}

func isNumberTable(frame *data.Frame) bool {
	if frame == nil || frame.Fields == nil {
		return false
	}
	numericCount := 0
	stringCount := 0
	otherCount := 0
	for _, field := range frame.Fields {
		fType := field.Type()
		switch {
		case fType.Numeric():
			numericCount++
		case fType == data.FieldTypeString || fType == data.FieldTypeNullableString:
			stringCount++
		default:
			otherCount++
		}
	}
	return numericCount == 1 && otherCount == 0
}

func extractNumberSet(frame *data.Frame) ([]mathexp.Number, error) {
	numericField := 0
	stringFieldIdxs := []int{}
	stringFieldNames := []string{}
	for i, field := range frame.Fields {
		fType := field.Type()
		switch {
		case fType.Numeric():
			numericField = i
		case fType == data.FieldTypeString || fType == data.FieldTypeNullableString:
			stringFieldIdxs = append(stringFieldIdxs, i)
			stringFieldNames = append(stringFieldNames, field.Name)
		}
	}
	numbers := make([]mathexp.Number, frame.Rows())

	for rowIdx := 0; rowIdx < frame.Rows(); rowIdx++ {
		val, _ := frame.FloatAt(numericField, rowIdx)
		var labels data.Labels
		for i := 0; i < len(stringFieldIdxs); i++ {
			if i == 0 {
				labels = make(data.Labels)
			}
			key := stringFieldNames[i] // TODO check for duplicate string column names
			val, _ := frame.ConcreteAt(stringFieldIdxs[i], rowIdx)
			labels[key] = val.(string) // TODO check assertion / return error
		}

		n := mathexp.NewNumber(frame.Fields[numericField].Name, labels)

		// The new value fields' configs gets pointed to the one in the original frame
		n.Frame.Fields[0].Config = frame.Fields[numericField].Config
		n.SetValue(&val)

		numbers[rowIdx] = n
	}
	return numbers, nil
}

// WideToMany converts a data package wide type Frame to one or multiple Series. A series
// is created for each value type column of wide frame.
//
// This might not be a good idea long term, but works now as an adapter/shim.
func WideToMany(frame *data.Frame) ([]mathexp.Series, error) {
	tsSchema := frame.TimeSeriesSchema()
	if tsSchema.Type != data.TimeSeriesTypeWide {
		return nil, fmt.Errorf("input data must be a wide series but got type %s (input refid)", tsSchema.Type)
	}

	if len(tsSchema.ValueIndices) == 1 {
		s, err := mathexp.SeriesFromFrame(frame)
		if err != nil {
			return nil, err
		}
		return []mathexp.Series{s}, nil
	}

	series := []mathexp.Series{}
	for _, valIdx := range tsSchema.ValueIndices {
		l := frame.Rows()
		f := data.NewFrameOfFieldTypes(frame.Name, l, frame.Fields[tsSchema.TimeIndex].Type(), frame.Fields[valIdx].Type())
		f.Fields[0].Name = frame.Fields[tsSchema.TimeIndex].Name
		f.Fields[1].Name = frame.Fields[valIdx].Name

		// The new value fields' configs gets pointed to the one in the original frame
		f.Fields[1].Config = frame.Fields[valIdx].Config

		if frame.Fields[valIdx].Labels != nil {
			f.Fields[1].Labels = frame.Fields[valIdx].Labels.Copy()
		}
		for i := 0; i < l; i++ {
			f.SetRow(i, frame.Fields[tsSchema.TimeIndex].CopyAt(i), frame.Fields[valIdx].CopyAt(i))
		}
		s, err := mathexp.SeriesFromFrame(f)
		if err != nil {
			return nil, err
		}
		series = append(series, s)
	}

	return series, nil
}
