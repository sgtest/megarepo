package notifiers

import (
	"encoding/json"

	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/services/alerting"
	"github.com/grafana/grafana/pkg/services/alerting/models"
	"github.com/grafana/grafana/pkg/services/notifications"
	"github.com/grafana/grafana/pkg/setting"
)

func init() {
	alerting.RegisterNotifier(&alerting.NotifierPlugin{
		Type:        "teams",
		Name:        "Microsoft Teams",
		Description: "Sends notifications using Incoming Webhook connector to Microsoft Teams",
		Heading:     "Teams settings",
		Factory:     NewTeamsNotifier,
		Options: []alerting.NotifierOption{
			{
				Label:        "URL",
				Element:      alerting.ElementTypeInput,
				InputType:    alerting.InputTypeText,
				Placeholder:  "Teams incoming webhook url",
				PropertyName: "url",
				Required:     true,
			},
		},
	})
}

// NewTeamsNotifier is the constructor for Teams notifier.
func NewTeamsNotifier(_ *setting.Cfg, model *models.AlertNotification, _ alerting.GetDecryptedValueFn, ns notifications.Service) (alerting.Notifier, error) {
	url := model.Settings.Get("url").MustString()
	if url == "" {
		return nil, alerting.ValidationError{Reason: "Could not find url property in settings"}
	}

	return &TeamsNotifier{
		NotifierBase: NewNotifierBase(model, ns),
		URL:          url,
		log:          log.New("alerting.notifier.teams"),
	}, nil
}

// TeamsNotifier is responsible for sending
// alert notifications to Microsoft teams.
type TeamsNotifier struct {
	NotifierBase
	URL string
	log log.Logger
}

// Notify send an alert notification to Microsoft teams.
func (tn *TeamsNotifier) Notify(evalContext *alerting.EvalContext) error {
	tn.log.Info("Executing teams notification", "ruleId", evalContext.Rule.ID, "notification", tn.Name)

	ruleURL, err := evalContext.GetRuleURL()
	if err != nil {
		tn.log.Error("Failed get rule link", "error", err)
		return err
	}

	fields := make([]map[string]any, 0)
	fieldLimitCount := 4
	for index, evt := range evalContext.EvalMatches {
		fields = append(fields, map[string]any{
			"name":  evt.Metric,
			"value": evt.Value,
		})
		if index > fieldLimitCount {
			break
		}
	}

	if evalContext.Error != nil {
		fields = append(fields, map[string]any{
			"name":  "Error message",
			"value": evalContext.Error.Error(),
		})
	}

	message := ""
	if evalContext.Rule.State != models.AlertStateOK { // don't add message when going back to alert state ok.
		message = evalContext.Rule.Message
	}

	images := make([]map[string]any, 0)
	if tn.NeedsImage() && evalContext.ImagePublicURL != "" {
		images = append(images, map[string]any{
			"image": evalContext.ImagePublicURL,
		})
	}

	body := map[string]any{
		"@type":    "MessageCard",
		"@context": "http://schema.org/extensions",
		// summary MUST not be empty or the webhook request fails
		// summary SHOULD contain some meaningful information, since it is used for mobile notifications
		"summary":    evalContext.GetNotificationTitle(),
		"title":      evalContext.GetNotificationTitle(),
		"themeColor": evalContext.GetStateModel().Color,
		"sections": []map[string]any{
			{
				"title":  "Details",
				"facts":  fields,
				"images": images,
				"text":   message,
			},
		},
		"potentialAction": []map[string]any{
			{
				"@context": "http://schema.org",
				"@type":    "OpenUri",
				"name":     "View Rule",
				"targets": []map[string]any{
					{
						"os": "default", "uri": ruleURL,
					},
				},
			},
			{
				"@context": "http://schema.org",
				"@type":    "OpenUri",
				"name":     "View Graph",
				"targets": []map[string]any{
					{
						"os": "default", "uri": evalContext.ImagePublicURL,
					},
				},
			},
		},
	}

	data, _ := json.Marshal(&body)
	cmd := &notifications.SendWebhookSync{Url: tn.URL, Body: string(data)}

	if err := tn.NotificationService.SendWebhookSync(evalContext.Ctx, cmd); err != nil {
		tn.log.Error("Failed to send teams notification", "error", err, "webhook", tn.Name)
		return err
	}

	return nil
}
