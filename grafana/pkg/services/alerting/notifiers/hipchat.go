package notifiers

import (
	"encoding/json"
	"fmt"
	"strconv"
	"strings"

	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/services/alerting"
	"github.com/grafana/grafana/pkg/services/alerting/models"
	"github.com/grafana/grafana/pkg/services/notifications"
	"github.com/grafana/grafana/pkg/setting"
)

func init() {
	alerting.RegisterNotifier(&alerting.NotifierPlugin{
		Type:        "hipchat",
		Name:        "HipChat",
		Description: "Sends notifications uto a HipChat Room",
		Heading:     "HipChat settings",
		Factory:     NewHipChatNotifier,
		Options: []alerting.NotifierOption{
			{
				Label:        "Hip Chat Url",
				Element:      alerting.ElementTypeInput,
				InputType:    alerting.InputTypeText,
				Placeholder:  "HipChat URL (ex https://grafana.hipchat.com)",
				PropertyName: "url",
				Required:     true,
			},
			{
				Label:        "API Key",
				Element:      alerting.ElementTypeInput,
				InputType:    alerting.InputTypeText,
				Placeholder:  "HipChat API Key",
				PropertyName: "apiKey",
				Required:     true,
			},
			{
				Label:        "Room ID",
				Element:      alerting.ElementTypeInput,
				InputType:    alerting.InputTypeText,
				PropertyName: "roomid",
			},
		},
	})
}

const (
	maxFieldCount int = 4
)

// NewHipChatNotifier is the constructor functions
// for the HipChatNotifier
func NewHipChatNotifier(_ *setting.Cfg, model *models.AlertNotification, _ alerting.GetDecryptedValueFn, ns notifications.Service) (alerting.Notifier, error) {
	url := model.Settings.Get("url").MustString()
	url = strings.TrimSuffix(url, "/")
	if url == "" {
		return nil, alerting.ValidationError{Reason: "Could not find url property in settings"}
	}

	apikey := model.Settings.Get("apikey").MustString()
	roomID := model.Settings.Get("roomid").MustString()

	return &HipChatNotifier{
		NotifierBase: NewNotifierBase(model, ns),
		URL:          url,
		APIKey:       apikey,
		RoomID:       roomID,
		log:          log.New("alerting.notifier.hipchat"),
	}, nil
}

// HipChatNotifier is responsible for sending
// alert notifications to Hipchat.
type HipChatNotifier struct {
	NotifierBase
	URL    string
	APIKey string
	RoomID string
	log    log.Logger
}

// Notify sends an alert notification to HipChat
func (hc *HipChatNotifier) Notify(evalContext *alerting.EvalContext) error {
	hc.log.Info("Executing hipchat notification", "ruleId", evalContext.Rule.ID, "notification", hc.Name)

	ruleURL, err := evalContext.GetRuleURL()
	if err != nil {
		hc.log.Error("Failed get rule link", "error", err)
		return err
	}

	attributes := make([]map[string]any, 0)
	for index, evt := range evalContext.EvalMatches {
		metricName := evt.Metric
		if len(metricName) > 50 {
			metricName = metricName[:50]
		}
		attributes = append(attributes, map[string]any{
			"label": metricName,
			"value": map[string]any{
				"label": strconv.FormatFloat(evt.Value.Float64, 'f', -1, 64),
			},
		})
		if index > maxFieldCount {
			break
		}
	}

	if evalContext.Error != nil {
		attributes = append(attributes, map[string]any{
			"label": "Error message",
			"value": map[string]any{
				"label": evalContext.Error.Error(),
			},
		})
	}

	message := ""
	if evalContext.Rule.State != models.AlertStateOK { // don't add message when going back to alert state ok.
		message += " " + evalContext.Rule.Message
	}

	if message == "" {
		message = evalContext.GetNotificationTitle() + " in state " + evalContext.GetStateModel().Text
	}

	// HipChat has a set list of colors
	var color string
	switch evalContext.Rule.State {
	case models.AlertStateOK:
		color = "green"
	case models.AlertStateNoData:
		color = "gray"
	case models.AlertStateAlerting:
		color = "red"
	default:
		// Handle other cases?
	}

	// Add a card with link to the dashboard
	card := map[string]any{
		"style":       "application",
		"url":         ruleURL,
		"id":          "1",
		"title":       evalContext.GetNotificationTitle(),
		"description": message,
		"icon": map[string]any{
			"url": "https://grafana.com/static/assets/img/fav32.png",
		},
		"date":       evalContext.EndTime.Unix(),
		"attributes": attributes,
	}
	if hc.NeedsImage() && evalContext.ImagePublicURL != "" {
		card["thumbnail"] = map[string]any{
			"url":    evalContext.ImagePublicURL,
			"url@2x": evalContext.ImagePublicURL,
			"width":  1193,
			"height": 564,
		}
	}

	body := map[string]any{
		"message":        message,
		"notify":         "true",
		"message_format": "html",
		"color":          color,
		"card":           card,
	}

	hipURL := fmt.Sprintf("%s/v2/room/%s/notification?auth_token=%s", hc.URL, hc.RoomID, hc.APIKey)
	data, _ := json.Marshal(&body)
	hc.log.Info("Request payload", "json", string(data))
	cmd := &notifications.SendWebhookSync{Url: hipURL, Body: string(data)}

	if err := hc.NotificationService.SendWebhookSync(evalContext.Ctx, cmd); err != nil {
		hc.log.Error("Failed to send hipchat notification", "error", err, "webhook", hc.Name)
		return err
	}

	return nil
}
