package store

import (
	"encoding/json"

	legacymodels "github.com/grafana/grafana/pkg/services/alerting/models"
)

// uidOrID for both uid and ID, primarily used for mapping legacy channel to migrated receiver.
type UidOrID any

type DashAlert struct {
	*legacymodels.Alert
	ParsedSettings *DashAlertSettings
}

// dashAlertSettings is a type for the JSON that is in the settings field of
// the alert table.
type DashAlertSettings struct {
	NoDataState         string               `json:"noDataState"`
	ExecutionErrorState string               `json:"executionErrorState"`
	Conditions          []DashAlertCondition `json:"conditions"`
	AlertRuleTags       any                  `json:"alertRuleTags"`
	Notifications       []DashAlertNot       `json:"notifications"`
}

// dashAlertNot is the object that represents the Notifications array in
// dashAlertSettings
type DashAlertNot struct {
	UID string `json:"uid,omitempty"`
	ID  int64  `json:"id,omitempty"`
}

// dashAlertingConditionJSON is like classic.ClassicConditionJSON except that it
// includes the model property with the query.
type DashAlertCondition struct {
	Evaluator ConditionEvalJSON `json:"evaluator"`

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

type ConditionEvalJSON struct {
	Params []float64 `json:"params"`
	Type   string    `json:"type"` // e.g. "gt"
}
