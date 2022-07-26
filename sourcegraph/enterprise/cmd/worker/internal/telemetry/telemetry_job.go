package telemetry

import (
	"context"
	"encoding/json"
	"time"

	"github.com/sourcegraph/sourcegraph/internal/conf/deploy"

	"cloud.google.com/go/pubsub"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/version"

	"github.com/sourcegraph/sourcegraph/internal/database"

	"github.com/sourcegraph/sourcegraph/lib/errors"

	"github.com/sourcegraph/sourcegraph/internal/conf"

	"github.com/opentracing/opentracing-go"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/sourcegraph/log"

	workerdb "github.com/sourcegraph/sourcegraph/cmd/worker/shared/init/db"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/trace"
)

type telemetryJob struct {
}

func NewTelemetryJob() *telemetryJob {
	return &telemetryJob{}
}

func (t *telemetryJob) Description() string {
	return "A background routine that exports usage telemetry to Sourcegraph"
}

func (t *telemetryJob) Config() []env.Config {
	return nil
}

func (t *telemetryJob) Routines(ctx context.Context, logger log.Logger) ([]goroutine.BackgroundRoutine, error) {
	if !isEnabled() {
		return nil, nil
	}
	logger.Info("Usage telemetry export enabled - initializing background routine")

	sqlDB, err := workerdb.Init()
	if err != nil {
		return nil, err
	}

	db := database.NewDB(logger, sqlDB)

	return []goroutine.BackgroundRoutine{
		newBackgroundTelemetryJob(logger, db),
	}, nil
}

func newBackgroundTelemetryJob(logger log.Logger, db database.DB) goroutine.BackgroundRoutine {
	observationContext := &observation.Context{
		Tracer:     &trace.Tracer{Tracer: opentracing.GlobalTracer()},
		Registerer: prometheus.NewRegistry(),
	}
	operation := observationContext.Operation(observation.Op{})

	return goroutine.NewPeriodicGoroutineWithMetrics(context.Background(), time.Minute*1, newTelemetryHandler(logger, db.EventLogs(), db.UserEmails(), db.GlobalState(), sendEvents), operation)
}

type sendEventsCallbackFunc func(ctx context.Context, event []*types.Event, config topicConfig, metadata instanceMetadata) error

type telemetryHandler struct {
	logger             log.Logger
	eventLogStore      database.EventLogStore
	globalStateStore   database.GlobalStateStore
	userEmailsStore    database.UserEmailsStore
	sendEventsCallback sendEventsCallbackFunc
}

func newTelemetryHandler(logger log.Logger, store database.EventLogStore, userEmailsStore database.UserEmailsStore, globalStateStore database.GlobalStateStore, sendEventsCallback sendEventsCallbackFunc) *telemetryHandler {
	return &telemetryHandler{
		logger:             logger,
		eventLogStore:      store,
		sendEventsCallback: sendEventsCallback,
		globalStateStore:   globalStateStore,
		userEmailsStore:    userEmailsStore,
	}
}

var disabledErr = errors.New("Usage telemetry export is disabled, but the background job is attempting to execute. This means the configuration was disabled without restarting the worker service. This job is aborting, and no telemetry will be exported.")

const MaxEventsCountDefault = 5000

func (t *telemetryHandler) Handle(ctx context.Context) error {
	if !isEnabled() {
		return disabledErr
	}

	topicConfig, err := getTopicConfig()
	if err != nil {
		return errors.Wrap(err, "getTopicConfig")
	}

	instanceMetadata, err := getInstanceMetadata(ctx, t.globalStateStore, t.userEmailsStore)
	if err != nil {
		return errors.Wrap(err, "getInstanceMetadata")
	}

	batchSize := getBatchSize()

	all, err := t.eventLogStore.ListExportableEvents(ctx, database.LimitOffset{
		Limit:  batchSize,
		Offset: 0, // currently static, will become dynamic with https://github.com/sourcegraph/sourcegraph/issues/39089
	})
	if err != nil {
		return errors.Wrap(err, "eventLogStore.ListExportableEvents")
	}
	if len(all) == 0 {
		return nil
	}

	maxId := int(all[len(all)-1].ID)
	t.logger.Info("telemetryHandler executed", log.Int("event count", len(all)), log.Int("maxId", maxId))
	return t.sendEventsCallback(ctx, all, topicConfig, instanceMetadata)
}

// This package level client is to prevent race conditions when mocking this configuration in tests.
var confClient = conf.DefaultClient()

func isEnabled() bool {
	ptr := confClient.Get().ExportUsageTelemetry
	if ptr != nil {
		return ptr.Enabled
	}

	return false
}

func getBatchSize() int {
	val := confClient.Get().ExportUsageTelemetry.BatchSize
	if val <= 0 {
		val = MaxEventsCountDefault
	}
	return val
}

type topicConfig struct {
	projectName string
	topicName   string
}

func getTopicConfig() (topicConfig, error) {
	var config topicConfig

	config.topicName = confClient.Get().ExportUsageTelemetry.TopicName
	if config.topicName == "" {
		return config, errors.New("missing topic name to export usage data")
	}
	config.projectName = confClient.Get().ExportUsageTelemetry.TopicProjectName
	if config.projectName == "" {
		return config, errors.New("missing project name to export usage data")
	}
	return config, nil
}

func buildBigQueryObject(event *types.Event, metadata *instanceMetadata) *bigQueryEvent {
	return &bigQueryEvent{
		EventName:         event.Name,
		UserID:            int(event.UserID),
		AnonymousUserID:   event.AnonymousUserID,
		URL:               "", // omitting URL intentionally
		Source:            event.Source,
		Timestamp:         event.Timestamp.Format(time.RFC3339),
		PublicArgument:    event.Argument,
		Version:           event.Version, // sending event Version since these events could be scraped from the past
		SiteID:            metadata.SiteID,
		LicenseKey:        metadata.LicenseKey,
		DeployType:        metadata.DeployType,
		InitialAdminEmail: metadata.InitialAdminEmail,
	}
}

func sendEvents(ctx context.Context, events []*types.Event, config topicConfig, metadata instanceMetadata) error {
	client, err := pubsub.NewClient(ctx, config.projectName)
	if err != nil {
		return errors.Wrap(err, "pubsub.NewClient")
	}

	var toSend []*bigQueryEvent
	for _, event := range events {
		pubsubEvent := buildBigQueryObject(event, &metadata)
		toSend = append(toSend, pubsubEvent)
	}

	marshal, err := json.Marshal(toSend)
	if err != nil {
		return errors.Wrap(err, "json.Marshal")
	}

	topic := client.Topic(config.topicName)
	defer topic.Stop()
	result := topic.Publish(ctx, &pubsub.Message{
		Data: marshal,
	})
	_, err = result.Get(ctx)
	if err != nil {
		return errors.Wrap(err, "result.Get")
	}

	return nil
}

type bigQueryEvent struct {
	SiteID            string  `json:"site_id"`
	LicenseKey        string  `json:"license_key"`
	InitialAdminEmail string  `json:"initial_admin_email"`
	DeployType        string  `json:"deploy_type"`
	EventName         string  `json:"name"`
	URL               string  `json:"url"`
	AnonymousUserID   string  `json:"anonymous_user_id"`
	FirstSourceURL    string  `json:"first_source_url"`
	LastSourceURL     string  `json:"last_source_url"`
	UserID            int     `json:"user_id"`
	Source            string  `json:"source"`
	Timestamp         string  `json:"timestamp"`
	Version           string  `json:"Version"`
	FeatureFlags      string  `json:"feature_flags"`
	CohortID          *string `json:"cohort_id,omitempty"`
	Referrer          string  `json:"referrer,omitempty"`
	PublicArgument    string  `json:"public_argument"`
	DeviceID          *string `json:"device_id,omitempty"`
	InsertID          *string `json:"insert_id,omitempty"`
}

type instanceMetadata struct {
	DeployType        string
	Version           string
	SiteID            string
	LicenseKey        string
	InitialAdminEmail string
}

func getInstanceMetadata(ctx context.Context, stateStore database.GlobalStateStore, userEmailsStore database.UserEmailsStore) (instanceMetadata, error) {
	siteId, err := getSiteId(ctx, stateStore)
	if err != nil {
		return instanceMetadata{}, errors.Wrap(err, "getInstanceMetadata.getSiteId")
	}

	initialAdminEmail, err := getInitialAdminEmail(ctx, userEmailsStore)
	if err != nil {
		return instanceMetadata{}, errors.Wrap(err, "getInstanceMetadata.getInitialAdminEmail")
	}

	return instanceMetadata{
		DeployType:        deploy.Type(),
		Version:           version.Version(),
		SiteID:            siteId,
		LicenseKey:        confClient.Get().LicenseKey,
		InitialAdminEmail: initialAdminEmail,
	}, nil
}

func getSiteId(ctx context.Context, store database.GlobalStateStore) (string, error) {
	state, err := store.Get(ctx)
	if err != nil {
		return "", err
	}
	return state.SiteID, nil
}

func getInitialAdminEmail(ctx context.Context, store database.UserEmailsStore) (string, error) {
	info, _, err := store.GetInitialSiteAdminInfo(ctx)
	if err != nil {
		return "", err
	}
	return info, nil
}
