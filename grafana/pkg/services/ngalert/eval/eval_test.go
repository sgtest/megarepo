package eval

import (
	"context"
	"errors"
	"fmt"
	"math/rand"
	"testing"
	"time"

	"github.com/grafana/grafana-plugin-sdk-go/backend"
	"github.com/grafana/grafana-plugin-sdk-go/data"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/grafana/grafana/pkg/expr"
	"github.com/grafana/grafana/pkg/infra/tracing"
	"github.com/grafana/grafana/pkg/plugins"
	"github.com/grafana/grafana/pkg/services/datasources"
	fakes "github.com/grafana/grafana/pkg/services/datasources/fakes"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/services/ngalert/models"
	"github.com/grafana/grafana/pkg/services/pluginsintegration/pluginstore"
	"github.com/grafana/grafana/pkg/services/user"
	"github.com/grafana/grafana/pkg/setting"
	"github.com/grafana/grafana/pkg/util"
)

func TestEvaluateExecutionResult(t *testing.T) {
	cases := []struct {
		desc               string
		execResults        ExecutionResults
		expectResultLength int
		expectResults      Results
	}{
		{
			desc: "zero valued single instance is single Normal state result",
			execResults: ExecutionResults{
				Condition: []*data.Frame{
					data.NewFrame("", data.NewField("", nil, []*float64{util.Pointer(0.0)})),
				},
			},
			expectResultLength: 1,
			expectResults: Results{
				{
					State: Normal,
				},
			},
		},
		{
			desc: "non-zero valued single instance is single Alerting state result",
			execResults: ExecutionResults{
				Condition: []*data.Frame{
					data.NewFrame("", data.NewField("", nil, []*float64{util.Pointer(1.0)})),
				},
			},
			expectResultLength: 1,
			expectResults: Results{
				{
					State: Alerting,
				},
			},
		},
		{
			desc: "nil value single instance is single a NoData state result",
			execResults: ExecutionResults{
				Condition: []*data.Frame{
					data.NewFrame("", data.NewField("", nil, []*float64{nil})),
				},
			},
			expectResultLength: 1,
			expectResults: Results{
				{
					State: NoData,
				},
			},
		},
		{
			desc: "an execution error produces a single Error state result",
			execResults: ExecutionResults{
				Error: fmt.Errorf("an execution error"),
			},
			expectResultLength: 1,
			expectResults: Results{
				{
					State: Error,
					Error: fmt.Errorf("an execution error"),
				},
			},
		},
		{
			desc:               "empty results produces a single NoData state result",
			execResults:        ExecutionResults{},
			expectResultLength: 1,
			expectResults: Results{
				{
					State: NoData,
				},
			},
		},
		{
			desc: "frame with no fields produces a NoData state result",
			execResults: ExecutionResults{
				Condition: []*data.Frame{
					data.NewFrame(""),
				},
			},
			expectResultLength: 1,
			expectResults: Results{
				{
					State: NoData,
				},
			},
		},
		{
			desc: "empty field produces a NoData state result",
			execResults: ExecutionResults{
				Condition: []*data.Frame{
					data.NewFrame("", data.NewField("", nil, []*float64{})),
				},
			},
			expectResultLength: 1,
			expectResults: Results{
				{
					State: NoData,
				},
			},
		},
		{
			desc: "empty field with labels produces a NoData state result with labels",
			execResults: ExecutionResults{
				Condition: []*data.Frame{
					data.NewFrame("", data.NewField("", data.Labels{"a": "b"}, []*float64{})),
				},
			},
			expectResultLength: 1,
			expectResults: Results{
				{
					State:    NoData,
					Instance: data.Labels{"a": "b"},
				},
			},
		},
		{
			desc: "malformed frame (unequal lengths) produces Error state result",
			execResults: ExecutionResults{
				Condition: []*data.Frame{
					data.NewFrame("",
						data.NewField("", nil, []*float64{util.Pointer(23.0)}),
						data.NewField("", nil, []*float64{}),
					),
				},
			},
			expectResultLength: 1,
			expectResults: Results{
				{
					State: Error,
					Error: fmt.Errorf("invalid format of evaluation results for the alert definition : unable to get frame row length: frame has different field lengths, field 0 is len 1 but field 1 is len 0"),
				},
			},
		},
		{
			desc: "too many fields produces Error state result",
			execResults: ExecutionResults{
				Condition: []*data.Frame{
					data.NewFrame("",
						data.NewField("", nil, []*float64{}),
						data.NewField("", nil, []*float64{}),
					),
				},
			},
			expectResultLength: 1,
			expectResults: Results{
				{
					State: Error,
					Error: fmt.Errorf("invalid format of evaluation results for the alert definition : unexpected field length: 2 instead of 1"),
				},
			},
		},
		{
			desc: "more than one row produces Error state result",
			execResults: ExecutionResults{
				Condition: []*data.Frame{
					data.NewFrame("",
						data.NewField("", nil, []*float64{util.Pointer(2.0), util.Pointer(3.0)}),
					),
				},
			},
			expectResultLength: 1,
			expectResults: Results{
				{
					State: Error,
					Error: fmt.Errorf("invalid format of evaluation results for the alert definition : unexpected row length: 2 instead of 0 or 1"),
				},
			},
		},
		{
			desc: "time fields (looks like time series) returns error",
			execResults: ExecutionResults{
				Condition: []*data.Frame{
					data.NewFrame("",
						data.NewField("", nil, []time.Time{}),
					),
				},
			},
			expectResultLength: 1,
			expectResults: Results{
				{
					State: Error,
					Error: fmt.Errorf("invalid format of evaluation results for the alert definition : looks like time series data, only reduced data can be alerted on."),
				},
			},
		},
		{
			desc: "non []*float64 field will produce Error state result",
			execResults: ExecutionResults{
				Condition: []*data.Frame{
					data.NewFrame("",
						data.NewField("", nil, []float64{2}),
					),
				},
			},
			expectResultLength: 1,
			expectResults: Results{
				{
					State: Error,
					Error: fmt.Errorf("invalid format of evaluation results for the alert definition : invalid field type: []float64"),
				},
			},
		},
		{
			desc: "duplicate labels produce a single Error state result",
			execResults: ExecutionResults{
				Condition: []*data.Frame{
					data.NewFrame("",
						data.NewField("", nil, []*float64{util.Pointer(1.0)}),
					),
					data.NewFrame("",
						data.NewField("", nil, []*float64{util.Pointer(2.0)}),
					),
				},
			},
			expectResultLength: 1,
			expectResults: Results{
				{
					State: Error,
					Error: fmt.Errorf("invalid format of evaluation results for the alert definition : frame cannot uniquely be identified by its labels: has duplicate results with labels {}"),
				},
			},
		},
		{
			desc: "error that produce duplicate empty labels produce a single Error state result",
			execResults: ExecutionResults{
				Condition: []*data.Frame{
					data.NewFrame("",
						data.NewField("", data.Labels{"a": "b"}, []float64{2}),
					),
					data.NewFrame("",
						data.NewField("", nil, []float64{2}),
					),
				},
			},
			expectResultLength: 1,
			expectResults: Results{
				{
					State: Error,
					Error: fmt.Errorf("invalid format of evaluation results for the alert definition : frame cannot uniquely be identified by its labels: has duplicate results with labels {}"),
				},
			},
		},
		{
			desc: "certain errors will produce multiple mixed Error and other state results",
			execResults: ExecutionResults{
				Condition: []*data.Frame{
					data.NewFrame("",
						data.NewField("", nil, []float64{3}),
					),
					data.NewFrame("",
						data.NewField("", data.Labels{"a": "b"}, []*float64{util.Pointer(2.0)}),
					),
				},
			},
			expectResultLength: 2,
			expectResults: Results{
				{
					State: Error,
					Error: fmt.Errorf("invalid format of evaluation results for the alert definition : invalid field type: []float64"),
				},
				{
					State:    Alerting,
					Instance: data.Labels{"a": "b"},
				},
			},
		},
	}

	for _, tc := range cases {
		t.Run(tc.desc, func(t *testing.T) {
			res := evaluateExecutionResult(tc.execResults, time.Time{})

			require.Equal(t, tc.expectResultLength, len(res))

			for i, r := range res {
				require.Equal(t, tc.expectResults[i].State, r.State)
				require.Equal(t, tc.expectResults[i].Instance, r.Instance)
				if tc.expectResults[i].State == Error {
					require.EqualError(t, tc.expectResults[i].Error, r.Error.Error())
				}
			}
		})
	}
}

func TestEvaluateExecutionResultsNoData(t *testing.T) {
	t.Run("no data for Ref ID will produce NoData result", func(t *testing.T) {
		results := ExecutionResults{
			NoData: map[string]string{
				"A": "1",
			},
		}
		v := evaluateExecutionResult(results, time.Time{})
		require.Len(t, v, 1)
		require.Equal(t, data.Labels{"datasource_uid": "1", "ref_id": "A"}, v[0].Instance)
		require.Equal(t, NoData, v[0].State)
	})

	t.Run("no data for Ref IDs will produce NoData result for each Ref ID", func(t *testing.T) {
		results := ExecutionResults{
			NoData: map[string]string{
				"A": "1",
				"B": "1",
				"C": "2",
			},
		}
		v := evaluateExecutionResult(results, time.Time{})
		require.Len(t, v, 2)

		datasourceUIDs := make([]string, 0, len(v))
		refIDs := make([]string, 0, len(v))

		for _, next := range v {
			require.Equal(t, NoData, next.State)

			datasourceUID, ok := next.Instance["datasource_uid"]
			require.True(t, ok)
			require.NotEqual(t, "", datasourceUID)
			datasourceUIDs = append(datasourceUIDs, datasourceUID)

			refID, ok := next.Instance["ref_id"]
			require.True(t, ok)
			require.NotEqual(t, "", refID)
			refIDs = append(refIDs, refID)
		}

		require.ElementsMatch(t, []string{"1", "2"}, datasourceUIDs)
		require.ElementsMatch(t, []string{"A,B", "C"}, refIDs)
	})
}

func TestValidate(t *testing.T) {
	type services struct {
		cache        *fakes.FakeCacheService
		pluginsStore *pluginstore.FakePluginStore
	}

	testCases := []struct {
		name      string
		condition func(services services) models.Condition
		error     bool
	}{
		{
			name:  "fail if no expressions",
			error: true,
			condition: func(_ services) models.Condition {
				return models.Condition{
					Condition: "A",
					Data:      []models.AlertQuery{},
				}
			},
		},
		{
			name:  "fail if condition RefID does not exist",
			error: true,
			condition: func(services services) models.Condition {
				dsQuery := models.GenerateAlertQuery()
				ds := &datasources.DataSource{
					UID:  dsQuery.DatasourceUID,
					Type: util.GenerateShortUID(),
				}
				services.cache.DataSources = append(services.cache.DataSources, ds)
				services.pluginsStore.PluginList = append(services.pluginsStore.PluginList, pluginstore.Plugin{
					JSONData: plugins.JSONData{
						ID:      ds.Type,
						Backend: true,
					},
				})

				return models.Condition{
					Condition: "C",
					Data: []models.AlertQuery{
						dsQuery,
						models.CreateClassicConditionExpression("B", dsQuery.RefID, "last", "gt", rand.Int()),
					},
				}
			},
		},
		{
			name:  "fail if condition RefID is empty",
			error: true,
			condition: func(services services) models.Condition {
				dsQuery := models.GenerateAlertQuery()
				ds := &datasources.DataSource{
					UID:  dsQuery.DatasourceUID,
					Type: util.GenerateShortUID(),
				}
				services.cache.DataSources = append(services.cache.DataSources, ds)
				services.pluginsStore.PluginList = append(services.pluginsStore.PluginList, pluginstore.Plugin{
					JSONData: plugins.JSONData{
						ID:      ds.Type,
						Backend: true,
					},
				})
				return models.Condition{
					Condition: "",
					Data: []models.AlertQuery{
						dsQuery,
						models.CreateClassicConditionExpression("B", dsQuery.RefID, "last", "gt", rand.Int()),
					},
				}
			},
		},
		{
			name:  "fail if datasource with UID does not exists",
			error: true,
			condition: func(services services) models.Condition {
				dsQuery := models.GenerateAlertQuery()
				// do not update the cache service
				return models.Condition{
					Condition: dsQuery.RefID,
					Data: []models.AlertQuery{
						dsQuery,
					},
				}
			},
		},
		{
			name:  "fail if datasource cannot be found in plugin store",
			error: true,
			condition: func(services services) models.Condition {
				dsQuery := models.GenerateAlertQuery()
				ds := &datasources.DataSource{
					UID:  dsQuery.DatasourceUID,
					Type: util.GenerateShortUID(),
				}
				services.cache.DataSources = append(services.cache.DataSources, ds)
				// do not update the plugin store
				return models.Condition{
					Condition: dsQuery.RefID,
					Data: []models.AlertQuery{
						dsQuery,
					},
				}
			},
		},
		{
			name:  "fail if datasource is not backend one",
			error: true,
			condition: func(services services) models.Condition {
				dsQuery1 := models.GenerateAlertQuery()
				dsQuery2 := models.GenerateAlertQuery()
				ds1 := &datasources.DataSource{
					UID:  dsQuery1.DatasourceUID,
					Type: util.GenerateShortUID(),
				}
				ds2 := &datasources.DataSource{
					UID:  dsQuery2.DatasourceUID,
					Type: util.GenerateShortUID(),
				}
				services.cache.DataSources = append(services.cache.DataSources, ds1, ds2)
				services.pluginsStore.PluginList = append(services.pluginsStore.PluginList, pluginstore.Plugin{
					JSONData: plugins.JSONData{
						ID:      ds1.Type,
						Backend: false,
					},
				}, pluginstore.Plugin{
					JSONData: plugins.JSONData{
						ID:      ds2.Type,
						Backend: true,
					},
				})
				// do not update the plugin store
				return models.Condition{
					Condition: dsQuery1.RefID,
					Data: []models.AlertQuery{
						dsQuery1,
						dsQuery2,
					},
				}
			},
		},
		{
			name:  "pass if datasource exists and condition is correct",
			error: false,
			condition: func(services services) models.Condition {
				dsQuery := models.GenerateAlertQuery()
				ds := &datasources.DataSource{
					UID:  dsQuery.DatasourceUID,
					Type: util.GenerateShortUID(),
				}
				services.cache.DataSources = append(services.cache.DataSources, ds)
				services.pluginsStore.PluginList = append(services.pluginsStore.PluginList, pluginstore.Plugin{
					JSONData: plugins.JSONData{
						ID:      ds.Type,
						Backend: true,
					},
				})

				return models.Condition{
					Condition: "B",
					Data: []models.AlertQuery{
						dsQuery,
						models.CreateClassicConditionExpression("B", dsQuery.RefID, "last", "gt", rand.Int()),
					},
				}
			},
		},
		{
			name:  "fail if hysteresis command is not the condition",
			error: true,
			condition: func(services services) models.Condition {
				dsQuery := models.GenerateAlertQuery()
				ds := &datasources.DataSource{
					UID:  dsQuery.DatasourceUID,
					Type: util.GenerateShortUID(),
				}
				services.cache.DataSources = append(services.cache.DataSources, ds)
				services.pluginsStore.PluginList = append(services.pluginsStore.PluginList, pluginstore.Plugin{
					JSONData: plugins.JSONData{
						ID:      ds.Type,
						Backend: true,
					},
				})

				return models.Condition{
					Condition: "C",
					Data: []models.AlertQuery{
						dsQuery,
						models.CreateHysteresisExpression(t, "B", dsQuery.RefID, 4, 1),
						models.CreateClassicConditionExpression("C", "B", "last", "gt", rand.Int()),
					},
				}
			},
		},
		{
			name:  "pass if hysteresis command and it is the condition",
			error: false,
			condition: func(services services) models.Condition {
				dsQuery := models.GenerateAlertQuery()
				ds := &datasources.DataSource{
					UID:  dsQuery.DatasourceUID,
					Type: util.GenerateShortUID(),
				}
				services.cache.DataSources = append(services.cache.DataSources, ds)
				services.pluginsStore.PluginList = append(services.pluginsStore.PluginList, pluginstore.Plugin{
					JSONData: plugins.JSONData{
						ID:      ds.Type,
						Backend: true,
					},
				})

				return models.Condition{
					Condition: "B",
					Data: []models.AlertQuery{
						dsQuery,
						models.CreateHysteresisExpression(t, "B", dsQuery.RefID, 4, 1),
					},
				}
			},
		},
	}

	for _, testCase := range testCases {
		u := &user.SignedInUser{}

		t.Run(testCase.name, func(t *testing.T) {
			cacheService := &fakes.FakeCacheService{}
			store := &pluginstore.FakePluginStore{}
			condition := testCase.condition(services{
				cache:        cacheService,
				pluginsStore: store,
			})

			evaluator := NewEvaluatorFactory(setting.UnifiedAlertingSettings{}, cacheService, expr.ProvideService(&setting.Cfg{ExpressionsEnabled: true}, nil, nil, &featuremgmt.FeatureManager{}, nil, tracing.InitializeTracerForTest()), store)
			evalCtx := NewContext(context.Background(), u)

			err := evaluator.Validate(evalCtx, condition)
			if testCase.error {
				require.Error(t, err)
			} else {
				require.NoError(t, err)
			}
		})
	}
}

func TestCreate_HysteresisCommand(t *testing.T) {
	type services struct {
		cache        *fakes.FakeCacheService
		pluginsStore *pluginstore.FakePluginStore
	}

	testCases := []struct {
		name      string
		reader    AlertingResultsReader
		condition func(services services) models.Condition
		error     bool
	}{
		{
			name:  "fail if hysteresis command is not the condition",
			error: true,
			condition: func(services services) models.Condition {
				dsQuery := models.GenerateAlertQuery()
				ds := &datasources.DataSource{
					UID:  dsQuery.DatasourceUID,
					Type: util.GenerateShortUID(),
				}
				services.cache.DataSources = append(services.cache.DataSources, ds)
				services.pluginsStore.PluginList = append(services.pluginsStore.PluginList, pluginstore.Plugin{
					JSONData: plugins.JSONData{
						ID:      ds.Type,
						Backend: true,
					},
				})

				return models.Condition{
					Condition: "C",
					Data: []models.AlertQuery{
						dsQuery,
						models.CreateHysteresisExpression(t, "B", dsQuery.RefID, 4, 1),
						models.CreateClassicConditionExpression("C", "B", "last", "gt", rand.Int()),
					},
				}
			},
		},
		{
			name:   "populate with loaded metrics",
			error:  false,
			reader: FakeLoadedMetricsReader{fingerprints: map[data.Fingerprint]struct{}{1: {}, 2: {}, 3: {}}},
			condition: func(services services) models.Condition {
				dsQuery := models.GenerateAlertQuery()
				ds := &datasources.DataSource{
					UID:  dsQuery.DatasourceUID,
					Type: util.GenerateShortUID(),
				}
				services.cache.DataSources = append(services.cache.DataSources, ds)
				services.pluginsStore.PluginList = append(services.pluginsStore.PluginList, pluginstore.Plugin{
					JSONData: plugins.JSONData{
						ID:      ds.Type,
						Backend: true,
					},
				})

				return models.Condition{
					Condition: "B",
					Data: []models.AlertQuery{
						dsQuery,
						models.CreateHysteresisExpression(t, "B", dsQuery.RefID, 4, 1),
					},
				}
			},
		},
		{
			name:   "do nothing if reader is not specified",
			error:  false,
			reader: nil,
			condition: func(services services) models.Condition {
				dsQuery := models.GenerateAlertQuery()
				ds := &datasources.DataSource{
					UID:  dsQuery.DatasourceUID,
					Type: util.GenerateShortUID(),
				}
				services.cache.DataSources = append(services.cache.DataSources, ds)
				services.pluginsStore.PluginList = append(services.pluginsStore.PluginList, pluginstore.Plugin{
					JSONData: plugins.JSONData{
						ID:      ds.Type,
						Backend: true,
					},
				})

				return models.Condition{
					Condition: "B",
					Data: []models.AlertQuery{
						dsQuery,
						models.CreateHysteresisExpression(t, "B", dsQuery.RefID, 4, 1),
					},
				}
			},
		},
	}

	for _, testCase := range testCases {
		u := &user.SignedInUser{}

		t.Run(testCase.name, func(t *testing.T) {
			cacheService := &fakes.FakeCacheService{}
			store := &pluginstore.FakePluginStore{}
			condition := testCase.condition(services{
				cache:        cacheService,
				pluginsStore: store,
			})
			evaluator := NewEvaluatorFactory(setting.UnifiedAlertingSettings{}, cacheService, expr.ProvideService(&setting.Cfg{ExpressionsEnabled: true}, nil, nil, featuremgmt.WithFeatures(featuremgmt.FlagRecoveryThreshold), nil, tracing.InitializeTracerForTest()), store)
			evalCtx := NewContextWithPreviousResults(context.Background(), u, testCase.reader)

			eval, err := evaluator.Create(evalCtx, condition)
			if testCase.error {
				require.Error(t, err)
				return
			}
			require.IsType(t, &conditionEvaluator{}, eval)
			ce := eval.(*conditionEvaluator)

			cmds := expr.GetCommandsFromPipeline[*expr.HysteresisCommand](ce.pipeline)
			require.Len(t, cmds, 1)
			if testCase.reader == nil {
				require.Empty(t, cmds[0].LoadedDimensions)
			} else {
				require.EqualValues(t, testCase.reader.Read(), cmds[0].LoadedDimensions)
			}
		})
	}
}

func TestEvaluate(t *testing.T) {
	cases := []struct {
		name     string
		cond     models.Condition
		resp     backend.QueryDataResponse
		expected Results
		error    string
	}{{
		name: "is no data with no frames",
		cond: models.Condition{
			Data: []models.AlertQuery{{
				RefID:         "A",
				DatasourceUID: "test",
			}, {
				RefID:         "B",
				DatasourceUID: expr.DatasourceUID,
			}, {
				RefID:         "C",
				DatasourceUID: expr.OldDatasourceUID,
			}, {
				RefID:         "D",
				DatasourceUID: expr.MLDatasourceUID,
			}},
		},
		resp: backend.QueryDataResponse{
			Responses: backend.Responses{
				"A": {Frames: nil},
				"B": {Frames: []*data.Frame{{Fields: nil}}},
				"C": {Frames: nil},
				"D": {Frames: []*data.Frame{{Fields: nil}}},
			},
		},
		expected: Results{{
			State: NoData,
			Instance: data.Labels{
				"datasource_uid": "test",
				"ref_id":         "A",
			},
		}},
	}, {
		name: "is no data for one frame with no fields",
		cond: models.Condition{
			Data: []models.AlertQuery{{
				RefID:         "A",
				DatasourceUID: "test",
			}},
		},
		resp: backend.QueryDataResponse{
			Responses: backend.Responses{
				"A": {Frames: []*data.Frame{{Fields: nil}}},
			},
		},
		expected: Results{{
			State: NoData,
			Instance: data.Labels{
				"datasource_uid": "test",
				"ref_id":         "A",
			},
		}},
	}, {
		name: "results contains captured values for exact label matches",
		cond: models.Condition{
			Condition: "B",
		},
		resp: backend.QueryDataResponse{
			Responses: backend.Responses{
				"A": {
					Frames: []*data.Frame{{
						RefID: "A",
						Fields: []*data.Field{
							data.NewField(
								"Value",
								data.Labels{"foo": "bar"},
								[]*float64{util.Pointer(10.0)},
							),
						},
					}},
				},
				"B": {
					Frames: []*data.Frame{{
						RefID: "B",
						Fields: []*data.Field{
							data.NewField(
								"Value",
								data.Labels{"foo": "bar"},
								[]*float64{util.Pointer(1.0)},
							),
						},
					}},
				},
			},
		},
		expected: Results{{
			State: Alerting,
			Instance: data.Labels{
				"foo": "bar",
			},
			Values: map[string]NumberValueCapture{
				"A": {
					Var:    "A",
					Labels: data.Labels{"foo": "bar"},
					Value:  util.Pointer(10.0),
				},
				"B": {
					Var:    "B",
					Labels: data.Labels{"foo": "bar"},
					Value:  util.Pointer(1.0),
				},
			},
			EvaluationString: "[ var='A' labels={foo=bar} value=10 ], [ var='B' labels={foo=bar} value=1 ]",
		}},
	}, {
		name: "results contains captured values for subset of labels",
		cond: models.Condition{
			Condition: "B",
		},
		resp: backend.QueryDataResponse{
			Responses: backend.Responses{
				"A": {
					Frames: []*data.Frame{{
						RefID: "A",
						Fields: []*data.Field{
							data.NewField(
								"Value",
								data.Labels{"foo": "bar"},
								[]*float64{util.Pointer(10.0)},
							),
						},
					}},
				},
				"B": {
					Frames: []*data.Frame{{
						RefID: "B",
						Fields: []*data.Field{
							data.NewField(
								"Value",
								data.Labels{"foo": "bar", "bar": "baz"},
								[]*float64{util.Pointer(1.0)},
							),
						},
					}},
				},
			},
		},
		expected: Results{{
			State: Alerting,
			Instance: data.Labels{
				"foo": "bar",
				"bar": "baz",
			},
			Values: map[string]NumberValueCapture{
				"A": {
					Var:    "A",
					Labels: data.Labels{"foo": "bar"},
					Value:  util.Pointer(10.0),
				},
				"B": {
					Var:    "B",
					Labels: data.Labels{"foo": "bar", "bar": "baz"},
					Value:  util.Pointer(1.0),
				},
			},
			EvaluationString: "[ var='A' labels={foo=bar} value=10 ], [ var='B' labels={bar=baz, foo=bar} value=1 ]",
		}},
	}}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			ev := conditionEvaluator{
				pipeline: nil,
				expressionService: &fakeExpressionService{
					hook: func(ctx context.Context, now time.Time, pipeline expr.DataPipeline) (*backend.QueryDataResponse, error) {
						return &tc.resp, nil
					},
				},
				condition: tc.cond,
			}
			results, err := ev.Evaluate(context.Background(), time.Now())
			if tc.error != "" {
				require.EqualError(t, err, tc.error)
			} else {
				require.NoError(t, err)
				require.Len(t, results, len(tc.expected))
				for i := range results {
					tc.expected[i].EvaluatedAt = results[i].EvaluatedAt
					tc.expected[i].EvaluationDuration = results[i].EvaluationDuration
					assert.Equal(t, tc.expected[i], results[i])
				}
			}
		})
	}
}

func TestEvaluateRaw(t *testing.T) {
	t.Run("should timeout if request takes too long", func(t *testing.T) {
		unexpectedResponse := &backend.QueryDataResponse{}

		e := conditionEvaluator{
			pipeline: nil,
			expressionService: &fakeExpressionService{
				hook: func(ctx context.Context, now time.Time, pipeline expr.DataPipeline) (*backend.QueryDataResponse, error) {
					ts := time.Now()
					for time.Since(ts) <= 10*time.Second {
						if ctx.Err() != nil {
							return nil, ctx.Err()
						}
						time.Sleep(10 * time.Millisecond)
					}
					return unexpectedResponse, nil
				},
			},
			condition:   models.Condition{},
			evalTimeout: 10 * time.Millisecond,
		}

		_, err := e.EvaluateRaw(context.Background(), time.Now())
		require.ErrorIs(t, err, context.DeadlineExceeded)
	})
}

func TestResults_HasNonRetryableErrors(t *testing.T) {
	tc := []struct {
		name     string
		eval     Results
		expected bool
	}{
		{
			name: "with non-retryable errors",
			eval: Results{
				{
					State: Error,
					Error: &invalidEvalResultFormatError{refID: "A", reason: "unable to get frame row length", err: errors.New("weird error")},
				},
			},
			expected: true,
		},
		{
			name: "with retryable errors",
			eval: Results{
				{
					State: Error,
					Error: errors.New("some weird error"),
				},
			},
			expected: false,
		},
	}

	for _, tt := range tc {
		t.Run(tt.name, func(t *testing.T) {
			require.Equal(t, tt.expected, tt.eval.HasNonRetryableErrors())
		})
	}
}

func TestResults_Error(t *testing.T) {
	tc := []struct {
		name     string
		eval     Results
		expected string
	}{
		{
			name: "with non-retryable errors",
			eval: Results{
				{
					State: Error,
					Error: &invalidEvalResultFormatError{refID: "A", reason: "unable to get frame row length", err: errors.New("weird error")},
				},
				{
					State: Error,
					Error: errors.New("unable to get a data frame"),
				},
			},
			expected: "invalid format of evaluation results for the alert definition A: unable to get frame row length: weird error\nunable to get a data frame",
		},
	}

	for _, tt := range tc {
		t.Run(tt.name, func(t *testing.T) {
			require.Equal(t, tt.expected, tt.eval.Error().Error())
		})
	}
}

type fakeExpressionService struct {
	hook func(ctx context.Context, now time.Time, pipeline expr.DataPipeline) (*backend.QueryDataResponse, error)
}

func (f fakeExpressionService) ExecutePipeline(ctx context.Context, now time.Time, pipeline expr.DataPipeline) (*backend.QueryDataResponse, error) {
	return f.hook(ctx, now, pipeline)
}
