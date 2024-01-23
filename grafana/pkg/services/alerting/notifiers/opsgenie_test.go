package notifiers

import (
	"context"
	"reflect"
	"strings"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/grafana/grafana/pkg/components/simplejson"
	"github.com/grafana/grafana/pkg/services/alerting"
	"github.com/grafana/grafana/pkg/services/alerting/models"
	"github.com/grafana/grafana/pkg/services/annotations/annotationstest"
	encryptionservice "github.com/grafana/grafana/pkg/services/encryption/service"
	"github.com/grafana/grafana/pkg/services/notifications"
	"github.com/grafana/grafana/pkg/services/tag"
	"github.com/grafana/grafana/pkg/services/validations"
	"github.com/grafana/grafana/pkg/setting"
)

func TestOpsGenieNotifier(t *testing.T) {
	encryptionService := encryptionservice.SetupTestService(t)

	t.Run("Parsing alert notification from settings", func(t *testing.T) {
		t.Run("empty settings should return error", func(t *testing.T) {
			json := `{ }`

			settingsJSON, _ := simplejson.NewJson([]byte(json))
			model := &models.AlertNotification{
				Name:     "opsgenie_testing",
				Type:     "opsgenie",
				Settings: settingsJSON,
			}

			_, err := NewOpsGenieNotifier(setting.NewCfg(), model, encryptionService.GetDecryptedValue, nil)
			require.Error(t, err)
		})

		t.Run("settings should trigger incident", func(t *testing.T) {
			json := `
				{
          "apiKey": "abcdefgh0123456789"
				}`

			settingsJSON, _ := simplejson.NewJson([]byte(json))
			model := &models.AlertNotification{
				Name:     "opsgenie_testing",
				Type:     "opsgenie",
				Settings: settingsJSON,
			}

			not, err := NewOpsGenieNotifier(setting.NewCfg(), model, encryptionService.GetDecryptedValue, nil)
			opsgenieNotifier := not.(*OpsGenieNotifier)

			require.Nil(t, err)
			require.Equal(t, "opsgenie_testing", opsgenieNotifier.Name)
			require.Equal(t, "opsgenie", opsgenieNotifier.Type)
			require.Equal(t, "abcdefgh0123456789", opsgenieNotifier.APIKey)
		})
	})

	t.Run("Handling notification tags", func(t *testing.T) {
		t.Run("invalid sendTagsAs value should return error", func(t *testing.T) {
			json := `{
          "apiKey": "abcdefgh0123456789",
          "sendTagsAs": "not_a_valid_value"
                                }`

			settingsJSON, _ := simplejson.NewJson([]byte(json))
			model := &models.AlertNotification{
				Name:     "opsgenie_testing",
				Type:     "opsgenie",
				Settings: settingsJSON,
			}

			_, err := NewOpsGenieNotifier(setting.NewCfg(), model, encryptionService.GetDecryptedValue, nil)
			require.Error(t, err)
			require.Equal(t, reflect.TypeOf(err), reflect.TypeOf(alerting.ValidationError{}))
			require.True(t, strings.HasSuffix(err.Error(), "Invalid value for sendTagsAs: \"not_a_valid_value\""))
		})

		t.Run("alert payload should include tag pairs only as an array in the tags key when sendAsTags is not set", func(t *testing.T) {
			json := `{
          "apiKey": "abcdefgh0123456789"
				}`

			tagPairs := []*tag.Tag{
				{Key: "keyOnly"},
				{Key: "aKey", Value: "aValue"},
			}

			settingsJSON, _ := simplejson.NewJson([]byte(json))
			model := &models.AlertNotification{
				Name:     "opsgenie_testing",
				Type:     "opsgenie",
				Settings: settingsJSON,
			}

			notificationService := notifications.MockNotificationService()
			notifier, notifierErr := NewOpsGenieNotifier(setting.NewCfg(), model, encryptionService.GetDecryptedValue, notificationService) // unhandled error

			opsgenieNotifier := notifier.(*OpsGenieNotifier)

			evalContext := alerting.NewEvalContext(context.Background(), &alerting.Rule{
				ID:            0,
				Name:          "someRule",
				Message:       "someMessage",
				State:         models.AlertStateAlerting,
				AlertRuleTags: tagPairs,
			}, &validations.OSSPluginRequestValidator{}, nil, nil, nil, annotationstest.NewFakeAnnotationsRepo())
			evalContext.IsTestRun = true

			tags := make([]string, 0)
			details := make(map[string]any)

			alertErr := opsgenieNotifier.createAlert(evalContext)

			bodyJSON, err := simplejson.NewJson([]byte(notificationService.Webhook.Body))
			if err == nil {
				tags = bodyJSON.Get("tags").MustStringArray([]string{})
				details = bodyJSON.Get("details").MustMap(map[string]any{})
			}

			require.Nil(t, notifierErr)
			require.Nil(t, alertErr)
			require.Equal(t, tags, []string{"keyOnly", "aKey:aValue"})
			require.Equal(t, details, map[string]any{"url": ""})
		})

		t.Run("alert payload should include tag pairs only as a map in the details key when sendAsTags=details", func(t *testing.T) {
			json := `{
          "apiKey": "abcdefgh0123456789",
          "sendTagsAs": "details"
				}`

			tagPairs := []*tag.Tag{
				{Key: "keyOnly"},
				{Key: "aKey", Value: "aValue"},
			}

			settingsJSON, _ := simplejson.NewJson([]byte(json))
			model := &models.AlertNotification{
				Name:     "opsgenie_testing",
				Type:     "opsgenie",
				Settings: settingsJSON,
			}

			notificationService := notifications.MockNotificationService()
			notifier, notifierErr := NewOpsGenieNotifier(setting.NewCfg(), model, encryptionService.GetDecryptedValue, notificationService) // unhandled error

			opsgenieNotifier := notifier.(*OpsGenieNotifier)

			evalContext := alerting.NewEvalContext(context.Background(), &alerting.Rule{
				ID:            0,
				Name:          "someRule",
				Message:       "someMessage",
				State:         models.AlertStateAlerting,
				AlertRuleTags: tagPairs,
			}, nil, nil, nil, nil, annotationstest.NewFakeAnnotationsRepo())
			evalContext.IsTestRun = true

			tags := make([]string, 0)
			details := make(map[string]any)

			alertErr := opsgenieNotifier.createAlert(evalContext)

			bodyJSON, err := simplejson.NewJson([]byte(notificationService.Webhook.Body))
			if err == nil {
				tags = bodyJSON.Get("tags").MustStringArray([]string{})
				details = bodyJSON.Get("details").MustMap(map[string]any{})
			}

			require.Nil(t, notifierErr)
			require.Nil(t, alertErr)
			require.Equal(t, tags, []string{})
			require.Equal(t, details, map[string]any{"keyOnly": "", "aKey": "aValue", "url": ""})
		})

		t.Run("alert payload should include tag pairs as both a map in the details key and an array in the tags key when sendAsTags=both", func(t *testing.T) {
			json := `{
          "apiKey": "abcdefgh0123456789",
          "sendTagsAs": "both"
				}`

			tagPairs := []*tag.Tag{
				{Key: "keyOnly"},
				{Key: "aKey", Value: "aValue"},
			}

			settingsJSON, _ := simplejson.NewJson([]byte(json))
			model := &models.AlertNotification{
				Name:     "opsgenie_testing",
				Type:     "opsgenie",
				Settings: settingsJSON,
			}

			notificationService := notifications.MockNotificationService()
			notifier, notifierErr := NewOpsGenieNotifier(setting.NewCfg(), model, encryptionService.GetDecryptedValue, notificationService) // unhandled error

			opsgenieNotifier := notifier.(*OpsGenieNotifier)

			evalContext := alerting.NewEvalContext(context.Background(), &alerting.Rule{
				ID:            0,
				Name:          "someRule",
				Message:       "someMessage",
				State:         models.AlertStateAlerting,
				AlertRuleTags: tagPairs,
			}, nil, nil, nil, nil, annotationstest.NewFakeAnnotationsRepo())
			evalContext.IsTestRun = true

			tags := make([]string, 0)
			details := make(map[string]any)

			alertErr := opsgenieNotifier.createAlert(evalContext)

			bodyJSON, err := simplejson.NewJson([]byte(notificationService.Webhook.Body))
			if err == nil {
				tags = bodyJSON.Get("tags").MustStringArray([]string{})
				details = bodyJSON.Get("details").MustMap(map[string]any{})
			}

			require.Nil(t, notifierErr)
			require.Nil(t, alertErr)
			require.Equal(t, tags, []string{"keyOnly", "aKey:aValue"})
			require.Equal(t, details, map[string]any{"keyOnly": "", "aKey": "aValue", "url": ""})
		})
	})
}
