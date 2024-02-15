package notifier

import (
	"context"
	"errors"
	"fmt"
	"sync"

	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/services/ngalert/api/tooling/definitions"
	"github.com/grafana/grafana/pkg/services/ngalert/models"
	"github.com/grafana/grafana/pkg/services/ngalert/store"
	"github.com/prometheus/alertmanager/config"
)

// NotificationSettingsValidator validates NotificationSettings against the current Alertmanager configuration
type NotificationSettingsValidator interface {
	Validate(s models.NotificationSettings) error
}

// staticValidator is a NotificationSettingsValidator that uses static pre-fetched values for available receivers and mute timings.
type staticValidator struct {
	availableReceivers   map[string]struct{}
	availableMuteTimings map[string]struct{}
}

// apiAlertingConfig contains the methods required to validate NotificationSettings and create autogen routes.
type apiAlertingConfig[R receiver] interface {
	GetReceivers() []R
	GetMuteTimeIntervals() []config.MuteTimeInterval
	GetRoute() *definitions.Route
}

type receiver interface {
	GetName() string
}

// NewNotificationSettingsValidator creates a new NotificationSettingsValidator from the given apiAlertingConfig.
func NewNotificationSettingsValidator[R receiver](am apiAlertingConfig[R]) NotificationSettingsValidator {
	availableReceivers := make(map[string]struct{})
	for _, receiver := range am.GetReceivers() {
		availableReceivers[receiver.GetName()] = struct{}{}
	}

	availableMuteTimings := make(map[string]struct{})
	for _, interval := range am.GetMuteTimeIntervals() {
		availableMuteTimings[interval.Name] = struct{}{}
	}

	return staticValidator{
		availableReceivers:   availableReceivers,
		availableMuteTimings: availableMuteTimings,
	}
}

// Validate checks that models.NotificationSettings is valid and references existing receivers and mute timings.
func (n staticValidator) Validate(settings models.NotificationSettings) error {
	if err := settings.Validate(); err != nil {
		return err
	}
	var errs []error
	if _, ok := n.availableReceivers[settings.Receiver]; !ok {
		errs = append(errs, fmt.Errorf("receiver '%s' does not exist", settings.Receiver))
	}
	for _, interval := range settings.MuteTimeIntervals {
		if _, ok := n.availableMuteTimings[interval]; !ok {
			errs = append(errs, fmt.Errorf("mute time interval '%s' does not exist", interval))
		}
	}
	return errors.Join(errs...)
}

// NotificationSettingsValidatorProvider provides a NotificationSettingsValidator for a given orgID.
type NotificationSettingsValidatorProvider interface {
	Validator(ctx context.Context, orgID int64) (NotificationSettingsValidator, error)
}

// notificationSettingsValidationService provides a new NotificationSettingsValidator for a given orgID by loading the latest Alertmanager configuration.
type notificationSettingsValidationService struct {
	store store.AlertingStore
}

func NewNotificationSettingsValidationService(store store.AlertingStore) NotificationSettingsValidatorProvider {
	return &notificationSettingsValidationService{
		store: store,
	}
}

// Validator returns a NotificationSettingsValidator using the alertmanager configuration from the given orgID.
func (v *notificationSettingsValidationService) Validator(ctx context.Context, orgID int64) (NotificationSettingsValidator, error) {
	rawCfg, err := v.store.GetLatestAlertmanagerConfiguration(ctx, orgID)
	if err != nil {
		return staticValidator{}, err
	}
	cfg, err := Load([]byte(rawCfg.AlertmanagerConfiguration))
	if err != nil {
		return staticValidator{}, err
	}
	log.New("ngalert.notifier.validator").FromContext(ctx).Debug("Create validator from Alertmanager configuration", "hash", rawCfg.ConfigurationHash)
	return NewNotificationSettingsValidator(&cfg.AlertmanagerConfig), nil
}

type cachedNotificationSettingsValidationService struct {
	srv        NotificationSettingsValidatorProvider
	mtx        sync.Mutex
	validators map[int64]NotificationSettingsValidator
}

func NewCachedNotificationSettingsValidationService(store store.AlertingStore) NotificationSettingsValidatorProvider {
	return &cachedNotificationSettingsValidationService{
		srv:        NewNotificationSettingsValidationService(store),
		mtx:        sync.Mutex{},
		validators: map[int64]NotificationSettingsValidator{},
	}
}

// Validator returns a NotificationSettingsValidator using the alertmanager configuration from the given orgID.
func (v *cachedNotificationSettingsValidationService) Validator(ctx context.Context, orgID int64) (NotificationSettingsValidator, error) {
	v.mtx.Lock()
	defer v.mtx.Unlock()

	result, ok := v.validators[orgID]
	if !ok {
		vd, err := v.srv.Validator(ctx, orgID)
		if err != nil {
			return nil, err
		}
		v.validators[orgID] = vd
		result = vd
	}
	return result, nil
}
