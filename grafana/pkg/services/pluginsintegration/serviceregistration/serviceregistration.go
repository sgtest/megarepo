package serviceregistration

import (
	"context"
	"errors"

	"github.com/grafana/grafana/pkg/plugins/auth"
	"github.com/grafana/grafana/pkg/plugins/config"
	"github.com/grafana/grafana/pkg/plugins/log"
	"github.com/grafana/grafana/pkg/plugins/plugindef"
	"github.com/grafana/grafana/pkg/services/accesscontrol"
	"github.com/grafana/grafana/pkg/services/extsvcauth"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/services/pluginsintegration/pluginsettings"
)

type Service struct {
	featureEnabled bool
	log            log.Logger
	reg            extsvcauth.ExternalServiceRegistry
	settingsSvc    pluginsettings.Service
}

func ProvideService(cfg *config.Cfg, reg extsvcauth.ExternalServiceRegistry, settingsSvc pluginsettings.Service) *Service {
	s := &Service{
		featureEnabled: cfg.Features.IsEnabledGlobally(featuremgmt.FlagExternalServiceAuth) || cfg.Features.IsEnabledGlobally(featuremgmt.FlagExternalServiceAccounts),
		log:            log.New("plugins.external.registration"),
		reg:            reg,
		settingsSvc:    settingsSvc,
	}
	return s
}

func (s *Service) HasExternalService(ctx context.Context, pluginID string) (bool, error) {
	if !s.featureEnabled {
		s.log.Debug("Skipping HasExternalService call. The feature is behind a feature toggle and needs to be enabled.")
		return false, nil
	}

	return s.reg.HasExternalService(ctx, pluginID)
}

// RegisterExternalService is a simplified wrapper around SaveExternalService for the plugin use case.
func (s *Service) RegisterExternalService(ctx context.Context, pluginID string, pType plugindef.Type, svc *plugindef.IAM) (*auth.ExternalService, error) {
	if !s.featureEnabled {
		s.log.Warn("Skipping External Service Registration. The feature is behind a feature toggle and needs to be enabled.")
		return nil, nil
	}

	// Datasource plugins can only be enabled
	enabled := true
	// App plugins can be disabled
	if pType == plugindef.TypeApp {
		settings, err := s.settingsSvc.GetPluginSettingByPluginID(ctx, &pluginsettings.GetByPluginIDArgs{PluginID: pluginID})
		if err != nil && !errors.Is(err, pluginsettings.ErrPluginSettingNotFound) {
			return nil, err
		}

		enabled = (settings != nil) && settings.Enabled
	}

	impersonation := extsvcauth.ImpersonationCfg{}
	if svc.Impersonation != nil {
		impersonation.Permissions = toAccessControlPermissions(svc.Impersonation.Permissions)
		impersonation.Enabled = enabled
		if svc.Impersonation.Groups != nil {
			impersonation.Groups = *svc.Impersonation.Groups
		} else {
			impersonation.Groups = true
		}
	}

	self := extsvcauth.SelfCfg{}
	self.Enabled = enabled
	if len(svc.Permissions) > 0 {
		self.Permissions = toAccessControlPermissions(svc.Permissions)
	}

	registration := &extsvcauth.ExternalServiceRegistration{
		Name:          pluginID,
		Impersonation: impersonation,
		Self:          self,
	}

	// Default authProvider now is ServiceAccounts
	registration.AuthProvider = extsvcauth.ServiceAccounts
	if svc.Impersonation != nil {
		registration.AuthProvider = extsvcauth.OAuth2Server
		registration.OAuthProviderCfg = &extsvcauth.OAuthProviderCfg{Key: &extsvcauth.KeyOption{Generate: true}}
	}

	extSvc, err := s.reg.SaveExternalService(ctx, registration)
	if err != nil || extSvc == nil {
		return nil, err
	}

	privateKey := ""
	if extSvc.OAuthExtra != nil {
		privateKey = extSvc.OAuthExtra.KeyResult.PrivatePem
	}

	return &auth.ExternalService{
		ClientID:     extSvc.ID,
		ClientSecret: extSvc.Secret,
		PrivateKey:   privateKey}, nil
}

func toAccessControlPermissions(ps []plugindef.Permission) []accesscontrol.Permission {
	res := make([]accesscontrol.Permission, 0, len(ps))
	for _, p := range ps {
		scope := ""
		if p.Scope != nil {
			scope = *p.Scope
		}
		res = append(res, accesscontrol.Permission{
			Action: p.Action,
			Scope:  scope,
		})
	}
	return res
}

// RemoveExternalService removes the external service account associated to a plugin
func (s *Service) RemoveExternalService(ctx context.Context, pluginID string) error {
	if !s.featureEnabled {
		s.log.Debug("Skipping External Service Removal. The feature is behind a feature toggle and needs to be enabled.")
		return nil
	}

	return s.reg.RemoveExternalService(ctx, pluginID)
}
