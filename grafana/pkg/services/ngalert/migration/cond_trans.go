package migration

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strings"
	"time"

	"github.com/grafana/grafana/pkg/components/simplejson"
	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/services/datasources"
	migrationStore "github.com/grafana/grafana/pkg/services/ngalert/migration/store"
	ngmodels "github.com/grafana/grafana/pkg/services/ngalert/models"
	"github.com/grafana/grafana/pkg/tsdb/legacydata"
	"github.com/grafana/grafana/pkg/tsdb/legacydata/interval"
	"github.com/grafana/grafana/pkg/util"
)

// It is defined in pkg/expr/service.go as "DatasourceType"
const expressionDatasourceUID = "__expr__"

// dashAlertSettings is a type for the JSON that is in the settings field of
// the alert table.
type dashAlertSettings struct {
	NoDataState         string               `json:"noDataState"`
	ExecutionErrorState string               `json:"executionErrorState"`
	Conditions          []dashAlertCondition `json:"conditions"`
	AlertRuleTags       any                  `json:"alertRuleTags"`
	Notifications       []notificationKey    `json:"notifications"`
}

// notificationKey is the object that represents the Notifications array in legacymodels.Alert.Settings.
// At least one of ID or UID should always be present, otherwise the legacy channel was invalid.
type notificationKey struct {
	UID string `json:"uid,omitempty"`
	ID  int64  `json:"id,omitempty"`
}

// dashAlertingConditionJSON is like classic.ClassicConditionJSON except that it
// includes the model property with the query.
type dashAlertCondition struct {
	Evaluator evaluator `json:"evaluator"`

	Operator struct {
		Type string `json:"type"`
	} `json:"operator"`

	Query struct {
		Params       []string `json:"params"`
		DatasourceID int64    `json:"datasourceId"`
		Model        json.RawMessage
	} `json:"query"`

	Reducer struct {
		// Params []any `json:"params"` (Unused)
		Type string `json:"type"`
	}
}

type evaluator struct {
	Params []float64 `json:"params"`
	Type   string    `json:"type"` // e.g. "gt"
}

//nolint:gocyclo
func transConditions(ctx context.Context, l log.Logger, set dashAlertSettings, orgID int64, store migrationStore.ReadStore) (*condition, error) {
	// TODO: needs a significant refactor to reduce complexity.
	usr := getMigrationUser(orgID)

	refIDtoCondIdx := make(map[string][]int) // a map of original refIds to their corresponding condition index
	for i, cond := range set.Conditions {
		if len(cond.Query.Params) != 3 {
			return nil, fmt.Errorf("unexpected number of query parameters in cond %v, want 3 got %v", i+1, len(cond.Query.Params))
		}
		refID := cond.Query.Params[0]
		refIDtoCondIdx[refID] = append(refIDtoCondIdx[refID], i)
	}

	newRefIDstoCondIdx := make(map[string][]int) // a map of the new refIds to their coresponding condition index

	refIDs := make([]string, 0, len(refIDtoCondIdx)) // a unique sorted list of the original refIDs
	for refID := range refIDtoCondIdx {
		refIDs = append(refIDs, refID)
	}
	sort.Strings(refIDs)

	newRefIDsToTimeRanges := make(map[string][2]string) // a map of new RefIDs to their time range string tuple representation
	for _, refID := range refIDs {
		condIdxes := refIDtoCondIdx[refID]

		if len(condIdxes) == 1 {
			// If the refID does not exist yet and the condition only has one reference, we can add it directly.
			if _, exists := newRefIDstoCondIdx[refID]; !exists {
				// If the refID is used in only condition, keep the letter a new refID
				newRefIDstoCondIdx[refID] = append(newRefIDstoCondIdx[refID], condIdxes[0])
				newRefIDsToTimeRanges[refID] = [2]string{set.Conditions[condIdxes[0]].Query.Params[1], set.Conditions[condIdxes[0]].Query.Params[2]}
				continue
			}
		}

		// track unique time ranges within the same refID
		timeRangesToCondIdx := make(map[[2]string][]int) // a map of the time range tuple to the condition index
		for _, idx := range condIdxes {
			timeParamFrom := set.Conditions[idx].Query.Params[1]
			timeParamTo := set.Conditions[idx].Query.Params[2]
			key := [2]string{timeParamFrom, timeParamTo}
			timeRangesToCondIdx[key] = append(timeRangesToCondIdx[key], idx)
		}

		if len(timeRangesToCondIdx) == 1 {
			// If the refID does not exist yet and the condition only has one reference, we can add it directly.
			if _, exists := newRefIDstoCondIdx[refID]; !exists {
				// if all shared time range, no need to create a new query with a new RefID
				for i := range condIdxes {
					newRefIDstoCondIdx[refID] = append(newRefIDstoCondIdx[refID], condIdxes[i])
					newRefIDsToTimeRanges[refID] = [2]string{set.Conditions[condIdxes[i]].Query.Params[1], set.Conditions[condIdxes[i]].Query.Params[2]}
				}
				continue
			}
		}

		// This referenced query/refID has different time ranges, so new queries are needed for each unique time range.
		timeRanges := make([][2]string, 0, len(timeRangesToCondIdx)) // a sorted list of unique time ranges for the query
		for tr := range timeRangesToCondIdx {
			timeRanges = append(timeRanges, tr)
		}

		sort.Slice(timeRanges, func(i, j int) bool {
			switch {
			case timeRanges[i][0] < timeRanges[j][0]:
				return true
			case timeRanges[i][0] > timeRanges[j][0]:
				return false
			default:
				return timeRanges[i][1] < timeRanges[j][1]
			}
		})

		for _, tr := range timeRanges {
			idxes := timeRangesToCondIdx[tr]
			for i := 0; i < len(idxes); i++ {
				newLetter, err := getNewRefID(newRefIDstoCondIdx)
				if err != nil {
					return nil, err
				}
				newRefIDstoCondIdx[newLetter] = append(newRefIDstoCondIdx[newLetter], idxes[i])
				newRefIDsToTimeRanges[newLetter] = [2]string{set.Conditions[idxes[i]].Query.Params[1], set.Conditions[idxes[i]].Query.Params[2]}
			}
		}
	}

	newRefIDs := make([]string, 0, len(newRefIDstoCondIdx)) // newRefIds is a sorted list of the unique refIds of new queries
	for refID := range newRefIDstoCondIdx {
		newRefIDs = append(newRefIDs, refID)
	}
	sort.Strings(newRefIDs)

	newCond := &condition{}
	condIdxToNewRefID := make(map[int]string) // a map of condition indices to the RefIDs of new queries

	// build the new data source queries
	for _, refID := range newRefIDs {
		condIdxes := newRefIDstoCondIdx[refID]
		for i, condIdx := range condIdxes {
			condIdxToNewRefID[condIdx] = refID
			if i > 0 {
				// only create each unique query once
				continue
			}

			var queryObj map[string]any // copy the model
			err := json.Unmarshal(set.Conditions[condIdx].Query.Model, &queryObj)
			if err != nil {
				return nil, err
			}

			var queryType string
			if v, ok := queryObj["queryType"]; ok {
				if s, ok := v.(string); ok {
					queryType = s
				}
			}

			// Could have an alert saved but datasource deleted, so can not require match.
			ds, err := store.GetDatasource(ctx, set.Conditions[condIdx].Query.DatasourceID, usr)
			if err != nil && !errors.Is(err, datasources.ErrDataSourceNotFound) {
				return nil, err
			}

			queryObj["refId"] = refID

			// See services/alerting/conditions/query.go's newQueryCondition
			queryObj["maxDataPoints"] = interval.DefaultRes

			simpleJson, err := simplejson.NewJson(set.Conditions[condIdx].Query.Model)
			if err != nil {
				return nil, err
			}

			rawFrom := newRefIDsToTimeRanges[refID][0]
			rawTo := newRefIDsToTimeRanges[refID][1]

			// We check if the minInterval stored in the model is parseable. If it's not, we use "1s" instead.
			// The reason for this is because of a bug in legacy alerting which allows arbitrary variables to be used
			// as the min interval, even though those variables do not work and will cause the legacy alert
			// to fail with `interval calculation failed: time: invalid duration`.
			if _, err := interval.GetIntervalFrom(ds, simpleJson, time.Millisecond*1); err != nil {
				l.Warn("failed to parse min interval from query model, using '1s' instead", "interval", simpleJson.Get("interval").MustString(), "err", err)
				simpleJson.Set("interval", "1s")
			}

			calculatedInterval, err := calculateInterval(legacydata.NewDataTimeRange(rawFrom, rawTo), simpleJson, ds)
			if err != nil {
				return nil, err
			}
			queryObj["intervalMs"] = calculatedInterval.Milliseconds()

			encodedObj, err := json.Marshal(queryObj)
			if err != nil {
				return nil, err
			}

			rTR, err := getRelativeDuration(rawFrom, rawTo)
			if err != nil {
				return nil, err
			}

			alertQuery := ngmodels.AlertQuery{
				RefID:             refID,
				Model:             encodedObj,
				RelativeTimeRange: *rTR,
				QueryType:         queryType,
			}

			if ds != nil {
				alertQuery.DatasourceUID = ds.UID
			}

			newCond.Data = append(newCond.Data, alertQuery)
		}
	}

	// build the new classic condition pointing our new equivalent queries
	conditions := make([]classicCondition, len(set.Conditions))
	for i, cond := range set.Conditions {
		newCond := classicCondition{}
		newCond.Evaluator = evaluator{
			Type:   cond.Evaluator.Type,
			Params: cond.Evaluator.Params,
		}
		newCond.Operator.Type = cond.Operator.Type
		newCond.Query.Params = append(newCond.Query.Params, condIdxToNewRefID[i])
		newCond.Reducer.Type = cond.Reducer.Type

		conditions[i] = newCond
	}

	ccRefID, err := getNewRefID(newRefIDstoCondIdx) // get refID for the classic condition
	if err != nil {
		return nil, err
	}
	newCond.Condition = ccRefID // set the alert condition to point to the classic condition
	newCond.OrgID = orgID

	exprModel := struct {
		Type       string             `json:"type"`
		RefID      string             `json:"refId"`
		Conditions []classicCondition `json:"conditions"`
	}{
		"classic_conditions",
		ccRefID,
		conditions,
	}

	exprModelJSON, err := json.Marshal(&exprModel)
	if err != nil {
		return nil, err
	}

	ccAlertQuery := ngmodels.AlertQuery{
		RefID:         ccRefID,
		Model:         exprModelJSON,
		DatasourceUID: expressionDatasourceUID,
	}

	newCond.Data = append(newCond.Data, ccAlertQuery)

	sort.Slice(newCond.Data, func(i, j int) bool {
		return newCond.Data[i].RefID < newCond.Data[j].RefID
	})

	return newCond, nil
}

type condition struct {
	// Condition is the RefID of the query or expression from
	// the Data property to get the results for.
	Condition string `json:"condition"`
	OrgID     int64  `json:"-"`

	// Data is an array of data source queries and/or server side expressions.
	Data []ngmodels.AlertQuery `json:"data"`
}

const alpha = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"

// getNewRefID finds first capital letter in the alphabet not in use
// to use for a new RefID. It errors if it runs out of letters.
func getNewRefID(refIDs map[string][]int) (string, error) {
	for _, r := range alpha {
		sR := string(r)
		if _, ok := refIDs[sR]; ok {
			continue
		}
		return sR, nil
	}
	for i := 0; i < 20; i++ {
		sR := util.GenerateShortUID()
		if _, ok := refIDs[sR]; ok {
			continue
		}
		return sR, nil
	}
	return "", errors.New("failed to generate unique RefID")
}

// getRelativeDuration turns the alerting durations for dashboard conditions
// into a relative time range.
func getRelativeDuration(rawFrom, rawTo string) (*ngmodels.RelativeTimeRange, error) {
	fromD, err := getFrom(rawFrom)
	if err != nil {
		return nil, err
	}

	toD, err := getTo(rawTo)
	if err != nil {
		return nil, err
	}
	return &ngmodels.RelativeTimeRange{
		From: ngmodels.Duration(fromD),
		To:   ngmodels.Duration(toD),
	}, nil
}

func getFrom(from string) (time.Duration, error) {
	fromRaw := strings.Replace(from, "now-", "", 1)

	d, err := time.ParseDuration("-" + fromRaw)
	if err != nil {
		return 0, err
	}
	return -d, err
}

func getTo(to string) (time.Duration, error) {
	if to == "now" {
		return 0, nil
	} else if strings.HasPrefix(to, "now-") {
		withoutNow := strings.Replace(to, "now-", "", 1)

		d, err := time.ParseDuration("-" + withoutNow)
		if err != nil {
			return 0, err
		}
		return -d, nil
	}

	d, err := time.ParseDuration(to)
	if err != nil {
		return 0, err
	}
	return -d, nil
}

type classicCondition struct {
	Evaluator evaluator `json:"evaluator"`

	Operator struct {
		Type string `json:"type"`
	} `json:"operator"`

	Query struct {
		Params []string `json:"params"`
	} `json:"query"`

	Reducer struct {
		// Params []any `json:"params"` (Unused)
		Type string `json:"type"`
	} `json:"reducer"`
}

// Copied from services/alerting/conditions/query.go's calculateInterval
func calculateInterval(timeRange legacydata.DataTimeRange, model *simplejson.Json, dsInfo *datasources.DataSource) (time.Duration, error) {
	// if there is no min-interval specified in the datasource or in the dashboard-panel,
	// the value of 1ms is used (this is how it is done in the dashboard-interval-calculation too,
	// see https://github.com/grafana/grafana/blob/9a0040c0aeaae8357c650cec2ee644a571dddf3d/packages/grafana-data/src/datetime/rangeutil.ts#L264)
	defaultMinInterval := time.Millisecond * 1

	// interval.GetIntervalFrom has two problems (but they do not affect us here):
	// - it returns the min-interval, so it should be called interval.GetMinIntervalFrom
	// - it falls back to model.intervalMs. it should not, because that one is the real final
	//   interval-value calculated by the browser. but, in this specific case (old-alert),
	//   that value is not set, so the fallback never happens.
	minInterval, err := interval.GetIntervalFrom(dsInfo, model, defaultMinInterval)

	if err != nil {
		return time.Duration(0), err
	}

	calc := interval.NewCalculator()

	intvl := calc.Calculate(timeRange, minInterval)

	return intvl.Value, nil
}
