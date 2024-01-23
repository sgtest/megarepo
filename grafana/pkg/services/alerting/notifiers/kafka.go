package notifiers

import (
	"fmt"
	"strconv"

	"github.com/grafana/grafana/pkg/components/simplejson"
	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/services/alerting"
	"github.com/grafana/grafana/pkg/services/alerting/models"
	"github.com/grafana/grafana/pkg/services/notifications"
	"github.com/grafana/grafana/pkg/setting"
)

func init() {
	alerting.RegisterNotifier(&alerting.NotifierPlugin{
		Type:        "kafka",
		Name:        "Kafka REST Proxy",
		Description: "Sends notifications to Kafka Rest Proxy",
		Heading:     "Kafka settings",
		Factory:     NewKafkaNotifier,
		Options: []alerting.NotifierOption{
			{
				Label:        "Kafka REST Proxy",
				Element:      alerting.ElementTypeInput,
				InputType:    alerting.InputTypeText,
				Placeholder:  "http://localhost:8082",
				PropertyName: "kafkaRestProxy",
				Required:     true,
			},
			{
				Label:        "Topic",
				Element:      alerting.ElementTypeInput,
				InputType:    alerting.InputTypeText,
				Placeholder:  "topic1",
				PropertyName: "kafkaTopic",
				Required:     true,
			},
		},
	})
}

// NewKafkaNotifier is the constructor function for the Kafka notifier.
func NewKafkaNotifier(_ *setting.Cfg, model *models.AlertNotification, _ alerting.GetDecryptedValueFn, ns notifications.Service) (alerting.Notifier, error) {
	endpoint := model.Settings.Get("kafkaRestProxy").MustString()
	if endpoint == "" {
		return nil, alerting.ValidationError{Reason: "Could not find kafka rest proxy endpoint property in settings"}
	}
	topic := model.Settings.Get("kafkaTopic").MustString()
	if topic == "" {
		return nil, alerting.ValidationError{Reason: "Could not find kafka topic property in settings"}
	}

	return &KafkaNotifier{
		NotifierBase: NewNotifierBase(model, ns),
		Endpoint:     endpoint,
		Topic:        topic,
		log:          log.New("alerting.notifier.kafka"),
	}, nil
}

// KafkaNotifier is responsible for sending
// alert notifications to Kafka.
type KafkaNotifier struct {
	NotifierBase
	Endpoint string
	Topic    string
	log      log.Logger
}

// Notify sends the alert notification.
func (kn *KafkaNotifier) Notify(evalContext *alerting.EvalContext) error {
	state := evalContext.Rule.State

	customData := triggMetrString
	for _, evt := range evalContext.EvalMatches {
		customData += fmt.Sprintf("%s: %v\n", evt.Metric, evt.Value)
	}

	kn.log.Info("Notifying Kafka", "alert_state", state)

	recordJSON := simplejson.New()
	records := make([]any, 1)

	bodyJSON := simplejson.New()
	// get alert state in the kafka output issue #11401
	bodyJSON.Set("alert_state", state)
	bodyJSON.Set("description", evalContext.Rule.Name+" - "+evalContext.Rule.Message)
	bodyJSON.Set("client", "Grafana")
	bodyJSON.Set("details", customData)
	bodyJSON.Set("incident_key", "alertId-"+strconv.FormatInt(evalContext.Rule.ID, 10))

	ruleURL, err := evalContext.GetRuleURL()
	if err != nil {
		kn.log.Error("Failed get rule link", "error", err)
		return err
	}
	bodyJSON.Set("client_url", ruleURL)

	if kn.NeedsImage() && evalContext.ImagePublicURL != "" {
		contexts := make([]any, 1)
		imageJSON := simplejson.New()
		imageJSON.Set("type", "image")
		imageJSON.Set("src", evalContext.ImagePublicURL)
		contexts[0] = imageJSON
		bodyJSON.Set("contexts", contexts)
	}

	valueJSON := simplejson.New()
	valueJSON.Set("value", bodyJSON)
	records[0] = valueJSON
	recordJSON.Set("records", records)
	body, _ := recordJSON.MarshalJSON()

	topicURL := kn.Endpoint + "/topics/" + kn.Topic

	cmd := &notifications.SendWebhookSync{
		Url:        topicURL,
		Body:       string(body),
		HttpMethod: "POST",
		HttpHeader: map[string]string{
			"Content-Type": "application/vnd.kafka.json.v2+json",
			"Accept":       "application/vnd.kafka.v2+json",
		},
	}

	if err := kn.NotificationService.SendWebhookSync(evalContext.Ctx, cmd); err != nil {
		kn.log.Error("Failed to send notification to Kafka", "error", err, "body", string(body))
		return err
	}

	return nil
}
