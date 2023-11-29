package migration

import (
	"context"
	"fmt"
	"strings"
	"testing"

	"github.com/google/uuid"
	"github.com/stretchr/testify/require"

	"github.com/grafana/grafana/pkg/components/simplejson"
	"github.com/grafana/grafana/pkg/infra/db"
	"github.com/grafana/grafana/pkg/infra/log/logtest"
	legacymodels "github.com/grafana/grafana/pkg/services/alerting/models"
	migmodels "github.com/grafana/grafana/pkg/services/ngalert/migration/models"
	migrationStore "github.com/grafana/grafana/pkg/services/ngalert/migration/store"
	"github.com/grafana/grafana/pkg/services/ngalert/models"
	"github.com/grafana/grafana/pkg/services/ngalert/store"
)

func TestMigrateAlertRuleQueries(t *testing.T) {
	tc := []struct {
		name     string
		input    *simplejson.Json
		expected string
		err      error
	}{
		{
			name:     "when a query has a sub query - it is extracted",
			input:    simplejson.NewFromAny(map[string]any{"targetFull": "thisisafullquery", "target": "ahalfquery"}),
			expected: `{"target":"thisisafullquery"}`,
		},
		{
			name:     "when a query does not have a sub query - it no-ops",
			input:    simplejson.NewFromAny(map[string]any{"target": "ahalfquery"}),
			expected: `{"target":"ahalfquery"}`,
		},
		{
			name:     "when query was hidden, it removes the flag",
			input:    simplejson.NewFromAny(map[string]any{"hide": true}),
			expected: `{}`,
		},
		{
			name: "when prometheus both type query, convert to range",
			input: simplejson.NewFromAny(map[string]any{
				"datasource": map[string]string{
					"type": "prometheus",
				},
				"instant": true,
				"range":   true,
			}),
			expected: `{"datasource":{"type":"prometheus"},"instant":false,"range":true}`,
		},
		{
			name: "when prometheus instant type query, do nothing",
			input: simplejson.NewFromAny(map[string]any{
				"datasource": map[string]string{
					"type": "prometheus",
				},
				"instant": true,
			}),
			expected: `{"datasource":{"type":"prometheus"},"instant":true}`,
		},
		{
			name: "when non-prometheus with instant and range, do nothing",
			input: simplejson.NewFromAny(map[string]any{
				"datasource": map[string]string{
					"type": "something",
				},
				"instant": true,
				"range":   true,
			}),
			expected: `{"datasource":{"type":"something"},"instant":true,"range":true}`,
		},
	}

	for _, tt := range tc {
		t.Run(tt.name, func(t *testing.T) {
			model, err := tt.input.Encode()
			require.NoError(t, err)
			queries, err := migrateAlertRuleQueries(&logtest.Fake{}, []models.AlertQuery{{Model: model}})
			if tt.err != nil {
				require.Error(t, err)
				require.EqualError(t, err, tt.err.Error())
				return
			}

			require.NoError(t, err)
			r, err := queries[0].Model.MarshalJSON()
			require.NoError(t, err)
			require.JSONEq(t, tt.expected, string(r))
		})
	}
}

func TestAddMigrationInfo(t *testing.T) {
	tt := []struct {
		name                string
		alert               *migrationStore.DashAlert
		dashboard           string
		expectedLabels      map[string]string
		expectedAnnotations map[string]string
	}{
		{
			name:                "when alert rule tags are a JSON array, they're ignored.",
			alert:               &migrationStore.DashAlert{Alert: &legacymodels.Alert{ID: 43, PanelID: 42}, ParsedSettings: &migrationStore.DashAlertSettings{AlertRuleTags: []string{"one", "two", "three", "four"}}},
			dashboard:           "dashboard",
			expectedLabels:      map[string]string{},
			expectedAnnotations: map[string]string{"__alertId__": "43", "__dashboardUid__": "dashboard", "__panelId__": "42"},
		},
		{
			name:                "when alert rule tags are a JSON object",
			alert:               &migrationStore.DashAlert{Alert: &legacymodels.Alert{ID: 43, PanelID: 42}, ParsedSettings: &migrationStore.DashAlertSettings{AlertRuleTags: map[string]any{"key": "value", "key2": "value2"}}},
			dashboard:           "dashboard",
			expectedLabels:      map[string]string{"key": "value", "key2": "value2"},
			expectedAnnotations: map[string]string{"__alertId__": "43", "__dashboardUid__": "dashboard", "__panelId__": "42"},
		},
	}

	for _, tc := range tt {
		t.Run(tc.name, func(t *testing.T) {
			labels, annotations := addMigrationInfo(tc.alert, tc.dashboard)
			require.Equal(t, tc.expectedLabels, labels)
			require.Equal(t, tc.expectedAnnotations, annotations)
		})
	}
}

func TestMakeAlertRule(t *testing.T) {
	sqlStore := db.InitTestDB(t)
	info := migmodels.DashboardUpgradeInfo{
		DashboardUID:  "dashboarduid",
		DashboardName: "dashboardname",
		NewFolderUID:  "ewfolderuid",
		NewFolderName: "newfoldername",
	}
	t.Run("when mapping rule names", func(t *testing.T) {
		t.Run("leaves basic names untouched", func(t *testing.T) {
			service := NewTestMigrationService(t, sqlStore, nil)
			m := service.newOrgMigration(1)
			da := createTestDashAlert()

			ar, err := m.migrateAlert(context.Background(), &logtest.Fake{}, &da, info)

			require.NoError(t, err)
			require.Equal(t, da.Name, ar.Title)
		})

		t.Run("truncates very long names to max length", func(t *testing.T) {
			service := NewTestMigrationService(t, sqlStore, nil)
			m := service.newOrgMigration(1)
			da := createTestDashAlert()
			da.Name = strings.Repeat("a", store.AlertDefinitionMaxTitleLength+1)

			ar, err := m.migrateAlert(context.Background(), &logtest.Fake{}, &da, info)

			require.NoError(t, err)
			require.Len(t, ar.Title, store.AlertDefinitionMaxTitleLength)
		})

		t.Run("deduplicate names in same org and folder", func(t *testing.T) {
			service := NewTestMigrationService(t, sqlStore, nil)
			m := service.newOrgMigration(1)
			da := createTestDashAlert()
			da.Name = strings.Repeat("a", store.AlertDefinitionMaxTitleLength+1)

			ar, err := m.migrateAlert(context.Background(), &logtest.Fake{}, &da, info)

			require.NoError(t, err)
			require.Len(t, ar.Title, store.AlertDefinitionMaxTitleLength)

			da = createTestDashAlert()
			da.Name = strings.Repeat("a", store.AlertDefinitionMaxTitleLength+1)

			ar, err = m.migrateAlert(context.Background(), &logtest.Fake{}, &da, info)

			require.NoError(t, err)
			require.Len(t, ar.Title, store.AlertDefinitionMaxTitleLength)
			require.Equal(t, ar.Title, fmt.Sprintf("%s #2", strings.Repeat("a", store.AlertDefinitionMaxTitleLength-3)))
		})
	})

	t.Run("alert is not paused", func(t *testing.T) {
		service := NewTestMigrationService(t, sqlStore, nil)
		m := service.newOrgMigration(1)
		da := createTestDashAlert()

		ar, err := m.migrateAlert(context.Background(), &logtest.Fake{}, &da, info)
		require.NoError(t, err)
		require.False(t, ar.IsPaused)
	})

	t.Run("paused dash alert is paused", func(t *testing.T) {
		service := NewTestMigrationService(t, sqlStore, nil)
		m := service.newOrgMigration(1)
		da := createTestDashAlert()
		da.State = "paused"

		ar, err := m.migrateAlert(context.Background(), &logtest.Fake{}, &da, info)
		require.NoError(t, err)
		require.True(t, ar.IsPaused)
	})

	t.Run("use default if execution of NoData is not known", func(t *testing.T) {
		service := NewTestMigrationService(t, sqlStore, nil)
		m := service.newOrgMigration(1)
		da := createTestDashAlert()
		da.ParsedSettings.NoDataState = uuid.NewString()

		ar, err := m.migrateAlert(context.Background(), &logtest.Fake{}, &da, info)
		require.Nil(t, err)
		require.Equal(t, models.NoData, ar.NoDataState)
	})

	t.Run("use default if execution of Error is not known", func(t *testing.T) {
		service := NewTestMigrationService(t, sqlStore, nil)
		m := service.newOrgMigration(1)
		da := createTestDashAlert()
		da.ParsedSettings.ExecutionErrorState = uuid.NewString()

		ar, err := m.migrateAlert(context.Background(), &logtest.Fake{}, &da, info)
		require.Nil(t, err)
		require.Equal(t, models.ErrorErrState, ar.ExecErrState)
	})

	t.Run("migrate message template", func(t *testing.T) {
		service := NewTestMigrationService(t, sqlStore, nil)
		m := service.newOrgMigration(1)
		da := createTestDashAlert()
		da.Message = "Instance ${instance} is down"

		ar, err := m.migrateAlert(context.Background(), &logtest.Fake{}, &da, info)
		require.Nil(t, err)
		expected :=
			"{{- $mergedLabels := mergeLabelValues $values -}}\n" +
				"Instance {{$mergedLabels.instance}} is down"
		require.Equal(t, expected, ar.Annotations["message"])
	})

	t.Run("create unique group from dashboard title and humanized interval", func(t *testing.T) {
		service := NewTestMigrationService(t, sqlStore, nil)
		m := service.newOrgMigration(1)
		da := createTestDashAlert()
		da.PanelID = 42

		intervalTests := []struct {
			interval int64
			expected string
		}{
			{interval: 10, expected: "10s"},
			{interval: 30, expected: "30s"},
			{interval: 60, expected: "1m"},
			{interval: 120, expected: "2m"},
			{interval: 3600, expected: "1h"},
			{interval: 7200, expected: "2h"},
			{interval: 86400, expected: "1d"},
			{interval: 172800, expected: "2d"},
			{interval: 604800, expected: "1w"},
			{interval: 1209600, expected: "2w"},
			{interval: 31536000, expected: "1y"},
			{interval: 63072000, expected: "2y"},
			{interval: 60 + 30, expected: "1m30s"},
			{interval: 3600 + 10, expected: "1h10s"},
			{interval: 3600 + 60, expected: "1h1m"},
			{interval: 3600 + 60 + 10, expected: "1h1m10s"},
			{interval: 86400 + 10, expected: "1d10s"},
			{interval: 86400 + 60, expected: "1d1m"},
			{interval: 86400 + 3600, expected: "1d1h"},
			{interval: 86400 + 3600 + 60, expected: "1d1h1m"},
			{interval: 86400 + 3600 + 10, expected: "1d1h10s"},
			{interval: 86400 + 60 + 10, expected: "1d1m10s"},
			{interval: 86400 + 3600 + 60 + 10, expected: "1d1h1m10s"},
			{interval: 604800 + 86400 + 3600 + 60 + 10, expected: "8d1h1m10s"},
			{interval: 31536000 + 604800 + 86400 + 3600 + 60 + 10, expected: "373d1h1m10s"},
		}

		for _, test := range intervalTests {
			t.Run(fmt.Sprintf("interval %ds should be %s", test.interval, test.expected), func(t *testing.T) {
				da.Frequency = test.interval

				ar, err := m.migrateAlert(context.Background(), &logtest.Fake{}, &da, info)

				require.NoError(t, err)
				require.Equal(t, fmt.Sprintf("%s - %s", info.DashboardName, test.expected), ar.RuleGroup)
			})
		}
	})

	t.Run("truncate dashboard name part of rule group if too long", func(t *testing.T) {
		service := NewTestMigrationService(t, sqlStore, nil)
		m := service.newOrgMigration(1)
		da := createTestDashAlert()
		info := migmodels.DashboardUpgradeInfo{
			DashboardUID:  "dashboarduid",
			DashboardName: strings.Repeat("a", store.AlertRuleMaxRuleGroupNameLength-1),
			NewFolderUID:  "newfolderuid",
			NewFolderName: "newfoldername",
		}

		ar, err := m.migrateAlert(context.Background(), &logtest.Fake{}, &da, info)

		require.NoError(t, err)
		require.Len(t, ar.RuleGroup, store.AlertRuleMaxRuleGroupNameLength)
		suffix := fmt.Sprintf(" - %ds", ar.IntervalSeconds)
		require.Equal(t, fmt.Sprintf("%s%s", strings.Repeat("a", store.AlertRuleMaxRuleGroupNameLength-len(suffix)), suffix), ar.RuleGroup)
	})
}

func createTestDashAlert() migrationStore.DashAlert {
	return migrationStore.DashAlert{
		Alert: &legacymodels.Alert{
			ID:   1,
			Name: "test",
		},
		ParsedSettings: &migrationStore.DashAlertSettings{},
	}
}
