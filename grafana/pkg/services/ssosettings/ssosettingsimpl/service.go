package ssosettingsimpl

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/grafana/grafana/pkg/api/routing"
	"github.com/grafana/grafana/pkg/infra/db"
	"github.com/grafana/grafana/pkg/infra/log"
	ac "github.com/grafana/grafana/pkg/services/accesscontrol"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/services/secrets"
	"github.com/grafana/grafana/pkg/services/ssosettings"
	"github.com/grafana/grafana/pkg/services/ssosettings/api"
	"github.com/grafana/grafana/pkg/services/ssosettings/database"
	"github.com/grafana/grafana/pkg/services/ssosettings/models"
	"github.com/grafana/grafana/pkg/services/ssosettings/strategies"
	"github.com/grafana/grafana/pkg/setting"
)

var _ ssosettings.Service = (*SSOSettingsService)(nil)

type SSOSettingsService struct {
	log     log.Logger
	cfg     *setting.Cfg
	store   ssosettings.Store
	ac      ac.AccessControl
	secrets secrets.Service

	fbStrategies []ssosettings.FallbackStrategy
	reloadables  map[string]ssosettings.Reloadable
}

func ProvideService(cfg *setting.Cfg, sqlStore db.DB, ac ac.AccessControl,
	routeRegister routing.RouteRegister, features *featuremgmt.FeatureManager,
	secrets secrets.Service) *SSOSettingsService {
	strategies := []ssosettings.FallbackStrategy{
		strategies.NewOAuthStrategy(cfg),
		// register other strategies here, for example SAML
	}

	store := database.ProvideStore(sqlStore)

	svc := &SSOSettingsService{
		log:          log.New("ssosettings.service"),
		cfg:          cfg,
		store:        store,
		ac:           ac,
		fbStrategies: strategies,
		secrets:      secrets,
		reloadables:  make(map[string]ssosettings.Reloadable),
	}

	if features.IsEnabledGlobally(featuremgmt.FlagSsoSettingsApi) {
		ssoSettingsApi := api.ProvideApi(svc, routeRegister, ac)
		ssoSettingsApi.RegisterAPIEndpoints()
	}

	return svc
}

var _ ssosettings.Service = (*SSOSettingsService)(nil)

func (s *SSOSettingsService) GetForProvider(ctx context.Context, provider string) (*models.SSOSettings, error) {
	storeSettings, err := s.store.Get(ctx, provider)

	if errors.Is(err, ssosettings.ErrNotFound) {
		settings, err := s.loadSettingsUsingFallbackStrategy(ctx, provider)
		if err != nil {
			return nil, err
		}

		return settings, nil
	}

	if err != nil {
		return nil, err
	}

	storeSettings.Source = models.DB

	return storeSettings, nil
}

func (s *SSOSettingsService) List(ctx context.Context) ([]*models.SSOSettings, error) {
	result := make([]*models.SSOSettings, 0, len(ssosettings.AllOAuthProviders))
	storedSettings, err := s.store.List(ctx)

	if err != nil {
		return nil, err
	}

	for _, provider := range ssosettings.AllOAuthProviders {
		settings := getSettingsByProvider(provider, storedSettings)
		if len(settings) == 0 {
			// If there is no data in the DB then we need to load the settings using the fallback strategy
			fallbackSettings, err := s.loadSettingsUsingFallbackStrategy(ctx, provider)
			if err != nil {
				return nil, err
			}

			settings = append(settings, fallbackSettings)
		}
		result = append(result, settings...)
	}

	return result, nil
}

func (s *SSOSettingsService) Upsert(ctx context.Context, settings models.SSOSettings) error {
	if !isProviderConfigurable(settings.Provider) {
		return ssosettings.ErrInvalidProvider.Errorf("provider %s is not configurable", settings.Provider)
	}

	social, ok := s.reloadables[settings.Provider]
	if !ok {
		return ssosettings.ErrInvalidProvider.Errorf("provider %s not found in reloadables", settings.Provider)
	}

	err := social.Validate(ctx, settings)
	if err != nil {
		return err
	}

	systemSettings, err := s.loadSettingsUsingFallbackStrategy(ctx, settings.Provider)
	if err != nil {
		return err
	}

	// add the SSO settings from system that are not available in the user input
	// in order to have a complete set of SSO settings for every provider in the database
	settings.Settings = mergeSettings(settings.Settings, systemSettings.Settings)

	settings.Settings, err = s.encryptSecrets(ctx, settings.Settings)
	if err != nil {
		return err
	}

	err = s.store.Upsert(ctx, settings)
	if err != nil {
		return err
	}

	go func() {
		err = social.Reload(context.Background(), settings)
		if err != nil {
			s.log.Error("failed to reload the provider", "provider", settings.Provider, "error", err)
		}
	}()

	return nil
}

func (s *SSOSettingsService) Patch(ctx context.Context, provider string, data map[string]any) error {
	panic("not implemented") // TODO: Implement
}

func (s *SSOSettingsService) Delete(ctx context.Context, provider string) error {
	return s.store.Delete(ctx, provider)
}

func (s *SSOSettingsService) Reload(ctx context.Context, provider string) {
	panic("not implemented") // TODO: Implement
}

func (s *SSOSettingsService) RegisterReloadable(provider string, reloadable ssosettings.Reloadable) {
	if s.reloadables == nil {
		s.reloadables = make(map[string]ssosettings.Reloadable)
	}
	s.reloadables[provider] = reloadable
}

func (s *SSOSettingsService) RegisterFallbackStrategy(providerRegex string, strategy ssosettings.FallbackStrategy) {
	s.fbStrategies = append(s.fbStrategies, strategy)
}

func (s *SSOSettingsService) loadSettingsUsingFallbackStrategy(ctx context.Context, provider string) (*models.SSOSettings, error) {
	loadStrategy, ok := s.getFallbackStrategyFor(provider)
	if !ok {
		return nil, errors.New("no fallback strategy found for provider: " + provider)
	}

	settingsFromSystem, err := loadStrategy.GetProviderConfig(ctx, provider)
	if err != nil {
		return nil, err
	}

	return &models.SSOSettings{
		Provider: provider,
		Source:   models.System,
		Settings: settingsFromSystem,
	}, nil
}

func getSettingsByProvider(provider string, settings []*models.SSOSettings) []*models.SSOSettings {
	result := make([]*models.SSOSettings, 0)
	for _, item := range settings {
		if item.Provider == provider {
			result = append(result, item)
		}
	}
	return result
}

func (s *SSOSettingsService) getFallbackStrategyFor(provider string) (ssosettings.FallbackStrategy, bool) {
	for _, strategy := range s.fbStrategies {
		if strategy.IsMatch(provider) {
			return strategy, true
		}
	}
	return nil, false
}

func (s *SSOSettingsService) encryptSecrets(ctx context.Context, settings map[string]any) (map[string]any, error) {
	result := make(map[string]any)
	for k, v := range settings {
		if isSecret(k) {
			strValue, ok := v.(string)
			if !ok {
				return result, fmt.Errorf("failed to encrypt %s setting because it is not a string: %v", k, v)
			}

			encryptedSecret, err := s.secrets.Encrypt(ctx, []byte(strValue), secrets.WithoutScope())
			if err != nil {
				return result, err
			}
			result[k] = string(encryptedSecret)
		} else {
			result[k] = v
		}
	}

	return result, nil
}

func isSecret(fieldName string) bool {
	secretFieldPatterns := []string{"secret"}

	for _, v := range secretFieldPatterns {
		if strings.Contains(strings.ToLower(fieldName), strings.ToLower(v)) {
			return true
		}
	}
	return false
}

func mergeSettings(apiSettings, systemSettings map[string]any) map[string]any {
	settings := make(map[string]any)

	for k, v := range apiSettings {
		settings[k] = v
	}

	for k, v := range systemSettings {
		if _, ok := settings[k]; !ok {
			settings[k] = v
		}
	}

	return settings
}

func isProviderConfigurable(provider string) bool {
	for _, configurable := range ssosettings.ConfigurableOAuthProviders {
		if provider == configurable {
			return true
		}
	}

	return false
}
