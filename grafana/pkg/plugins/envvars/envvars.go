package envvars

import (
	"context"
	"fmt"
	"os"
	"sort"
	"strconv"
	"strings"

	"github.com/grafana/grafana-aws-sdk/pkg/awsds"
	"github.com/grafana/grafana-azure-sdk-go/azsettings"
	"github.com/grafana/grafana-plugin-sdk-go/backend"
	"github.com/grafana/grafana-plugin-sdk-go/backend/proxy"
	"github.com/grafana/grafana-plugin-sdk-go/experimental/featuretoggles"

	"github.com/grafana/grafana/pkg/plugins"
	"github.com/grafana/grafana/pkg/plugins/auth"
	"github.com/grafana/grafana/pkg/plugins/config"
)

const (
	customConfigPrefix = "GF_PLUGIN"
)

// allowedHostEnvVarNames is the list of environment variables that can be passed from Grafana's process to the
// plugin's process
var allowedHostEnvVarNames = []string{
	// Env vars used by net/http (Go stdlib) for http/https proxy
	// https://github.com/golang/net/blob/fbaf41277f28102c36926d1368dafbe2b54b4c1d/http/httpproxy/proxy.go#L91-L93
	"HTTP_PROXY",
	"http_proxy",
	"HTTPS_PROXY",
	"https_proxy",
	"NO_PROXY",
	"no_proxy",
}

type Provider interface {
	Get(ctx context.Context, p *plugins.Plugin) []string
}

type Service struct {
	cfg     *config.Cfg
	license plugins.Licensing
}

func NewProvider(cfg *config.Cfg, license plugins.Licensing) *Service {
	return &Service{
		cfg:     cfg,
		license: license,
	}
}

func (s *Service) Get(ctx context.Context, p *plugins.Plugin) []string {
	hostEnv := []string{
		fmt.Sprintf("GF_VERSION=%s", s.cfg.BuildVersion),
	}

	if s.license != nil {
		hostEnv = append(
			hostEnv,
			fmt.Sprintf("GF_EDITION=%s", s.license.Edition()),
			fmt.Sprintf("GF_ENTERPRISE_LICENSE_PATH=%s", s.license.Path()),
			fmt.Sprintf("GF_ENTERPRISE_APP_URL=%s", s.license.AppURL()),
		)
		hostEnv = append(hostEnv, s.license.Environment()...)
	}

	if p.ExternalService != nil {
		hostEnv = append(
			hostEnv,
			fmt.Sprintf("GF_APP_URL=%s", s.cfg.GrafanaAppURL),
			fmt.Sprintf("GF_PLUGIN_APP_CLIENT_ID=%s", p.ExternalService.ClientID),
			fmt.Sprintf("GF_PLUGIN_APP_CLIENT_SECRET=%s", p.ExternalService.ClientSecret),
		)
		if p.ExternalService.PrivateKey != "" {
			hostEnv = append(hostEnv, fmt.Sprintf("GF_PLUGIN_APP_PRIVATE_KEY=%s", p.ExternalService.PrivateKey))
		}
	}

	hostEnv = append(hostEnv, s.featureToggleEnableVar(ctx)...)
	hostEnv = append(hostEnv, s.awsEnvVars()...)
	hostEnv = append(hostEnv, s.secureSocksProxyEnvVars()...)
	hostEnv = append(hostEnv, azsettings.WriteToEnvStr(s.cfg.Azure)...)
	hostEnv = append(hostEnv, s.tracingEnvVars(p)...)

	// If SkipHostEnvVars is enabled, get some allowed variables from the current process and pass
	// them down to the plugin. If the flag is not set, do not add anything else because ALL env vars
	// from the current process (os.Environ()) will be forwarded to the plugin's process by go-plugin
	if p.SkipHostEnvVars {
		hostEnv = append(hostEnv, s.allowedHostEnvVars()...)
	}

	ev := getPluginSettings(p.ID, s.cfg).asEnvVar(customConfigPrefix, hostEnv...)

	return ev
}

// GetConfigMap returns a map of configuration that should be passed in a plugin request.
func (s *Service) GetConfigMap(ctx context.Context, _ string, _ *auth.ExternalService) map[string]string {
	m := make(map[string]string)

	if s.cfg.GrafanaAppURL != "" {
		m[backend.AppURL] = s.cfg.GrafanaAppURL
	}

	// TODO add support via plugin SDK
	//if externalService != nil {
	//	m[oauthtokenretriever.AppURL] = s.cfg.GrafanaAppURL
	//	m[oauthtokenretriever.AppClientID] = externalService.ClientID
	//	m[oauthtokenretriever.AppClientSecret] = externalService.ClientSecret
	//	m[oauthtokenretriever.AppPrivateKey] = externalService.PrivateKey
	//}

	if s.cfg.Features != nil {
		enabledFeatures := s.cfg.Features.GetEnabled(ctx)
		if len(enabledFeatures) > 0 {
			features := make([]string, 0, len(enabledFeatures))
			for feat := range enabledFeatures {
				features = append(features, feat)
			}
			sort.Strings(features)
			m[featuretoggles.EnabledFeatures] = strings.Join(features, ",")
		}
	}
	// TODO add support via plugin SDK
	//if s.cfg.AWSAssumeRoleEnabled {
	//	m[awsds.AssumeRoleEnabledEnvVarKeyName] = "true"
	//}
	//if len(s.cfg.AWSAllowedAuthProviders) > 0 {
	//	m[awsds.AllowedAuthProvidersEnvVarKeyName] = strings.Join(s.cfg.AWSAllowedAuthProviders, ",")
	//}
	//if s.cfg.AWSExternalId != "" {
	//	m[awsds.GrafanaAssumeRoleExternalIdKeyName] = s.cfg.AWSExternalId
	//}

	if s.cfg.ProxySettings.Enabled {
		m[proxy.PluginSecureSocksProxyEnabled] = "true"
		m[proxy.PluginSecureSocksProxyClientCert] = s.cfg.ProxySettings.ClientCert
		m[proxy.PluginSecureSocksProxyClientKey] = s.cfg.ProxySettings.ClientKey
		m[proxy.PluginSecureSocksProxyRootCACert] = s.cfg.ProxySettings.RootCA
		m[proxy.PluginSecureSocksProxyProxyAddress] = s.cfg.ProxySettings.ProxyAddress
		m[proxy.PluginSecureSocksProxyServerName] = s.cfg.ProxySettings.ServerName
	}

	// TODO add support via plugin SDK
	//azureSettings := s.cfg.Azure
	//if azureSettings != nil {
	//	if azureSettings.Cloud != "" {
	//		m[azsettings.AzureCloud] = azureSettings.Cloud
	//	}
	//
	//	if azureSettings.ManagedIdentityEnabled {
	//		m[azsettings.ManagedIdentityEnabled] = "true"
	//
	//		if azureSettings.ManagedIdentityClientId != "" {
	//			m[azsettings.ManagedIdentityClientID] = azureSettings.ManagedIdentityClientId
	//		}
	//	}
	//
	//	if azureSettings.UserIdentityEnabled {
	//		m[azsettings.UserIdentityEnabled] = "true"
	//
	//		if azureSettings.UserIdentityTokenEndpoint != nil {
	//			if azureSettings.UserIdentityTokenEndpoint.TokenUrl != "" {
	//				m[azsettings.UserIdentityTokenURL] = azureSettings.UserIdentityTokenEndpoint.TokenUrl
	//			}
	//			if azureSettings.UserIdentityTokenEndpoint.ClientId != "" {
	//				m[azsettings.UserIdentityClientID] = azureSettings.UserIdentityTokenEndpoint.ClientId
	//			}
	//			if azureSettings.UserIdentityTokenEndpoint.ClientSecret != "" {
	//				m[azsettings.UserIdentityClientSecret] = azureSettings.UserIdentityTokenEndpoint.ClientSecret
	//			}
	//			if azureSettings.UserIdentityTokenEndpoint.UsernameAssertion {
	//				m[azsettings.UserIdentityAssertion] = "username"
	//			}
	//		}
	//	}
	//}

	// TODO add support via plugin SDK
	//ps := getPluginSettings(pluginID, s.cfg)
	//for k, v := range ps {
	//	m[fmt.Sprintf("%s_%s", customConfigPrefix, strings.ToUpper(k))] = v
	//}

	return m
}

func (s *Service) tracingEnvVars(plugin *plugins.Plugin) []string {
	var pluginTracingEnabled bool
	if v, exists := s.cfg.PluginSettings[plugin.ID]["tracing"]; exists {
		pluginTracingEnabled = v == "true"
	}
	if !s.cfg.Tracing.IsEnabled() || !pluginTracingEnabled {
		return nil
	}

	vars := []string{
		fmt.Sprintf("GF_INSTANCE_OTLP_ADDRESS=%s", s.cfg.Tracing.OpenTelemetry.Address),
		fmt.Sprintf("GF_INSTANCE_OTLP_PROPAGATION=%s", s.cfg.Tracing.OpenTelemetry.Propagation),

		fmt.Sprintf("GF_INSTANCE_OTLP_SAMPLER_TYPE=%s", s.cfg.Tracing.OpenTelemetry.Sampler),
		fmt.Sprintf("GF_INSTANCE_OTLP_SAMPLER_PARAM=%.6f", s.cfg.Tracing.OpenTelemetry.SamplerParam),
		fmt.Sprintf("GF_INSTANCE_OTLP_SAMPLER_REMOTE_URL=%s", s.cfg.Tracing.OpenTelemetry.SamplerRemoteURL),
	}
	if plugin.Info.Version != "" {
		vars = append(vars, fmt.Sprintf("GF_PLUGIN_VERSION=%s", plugin.Info.Version))
	}
	return vars
}

func (s *Service) featureToggleEnableVar(ctx context.Context) []string {
	var variables []string // an array is used for consistency and keep the logic simpler for no features case

	if s.cfg.Features == nil {
		return variables
	}

	enabledFeatures := s.cfg.Features.GetEnabled(ctx)
	if len(enabledFeatures) > 0 {
		features := make([]string, 0, len(enabledFeatures))
		for feat := range enabledFeatures {
			features = append(features, feat)
		}
		variables = append(variables, fmt.Sprintf("GF_INSTANCE_FEATURE_TOGGLES_ENABLE=%s", strings.Join(features, ",")))
	}

	return variables
}

func (s *Service) awsEnvVars() []string {
	var variables []string
	if s.cfg.AWSAssumeRoleEnabled {
		variables = append(variables, awsds.AssumeRoleEnabledEnvVarKeyName+"=true")
	}
	if len(s.cfg.AWSAllowedAuthProviders) > 0 {
		variables = append(variables, awsds.AllowedAuthProvidersEnvVarKeyName+"="+strings.Join(s.cfg.AWSAllowedAuthProviders, ","))
	}
	if s.cfg.AWSExternalId != "" {
		variables = append(variables, awsds.GrafanaAssumeRoleExternalIdKeyName+"="+s.cfg.AWSExternalId)
	}

	return variables
}

func (s *Service) secureSocksProxyEnvVars() []string {
	if s.cfg.ProxySettings.Enabled {
		return []string{
			proxy.PluginSecureSocksProxyClientCert + "=" + s.cfg.ProxySettings.ClientCert,
			proxy.PluginSecureSocksProxyClientKey + "=" + s.cfg.ProxySettings.ClientKey,
			proxy.PluginSecureSocksProxyRootCACert + "=" + s.cfg.ProxySettings.RootCA,
			proxy.PluginSecureSocksProxyProxyAddress + "=" + s.cfg.ProxySettings.ProxyAddress,
			proxy.PluginSecureSocksProxyServerName + "=" + s.cfg.ProxySettings.ServerName,
			proxy.PluginSecureSocksProxyEnabled + "=" + strconv.FormatBool(s.cfg.ProxySettings.Enabled),
		}
	}
	return nil
}

// allowedHostEnvVars returns the variables that can be passed from Grafana's process
// (current process, also known as: "host") to the plugin process.
// A string in format "k=v" is returned for each variable in allowedHostEnvVarNames, if it's set.
func (s *Service) allowedHostEnvVars() []string {
	var r []string
	for _, envVarName := range allowedHostEnvVarNames {
		if envVarValue, ok := os.LookupEnv(envVarName); ok {
			r = append(r, envVarName+"="+envVarValue)
		}
	}
	return r
}

type pluginSettings map[string]string

func getPluginSettings(pluginID string, cfg *config.Cfg) pluginSettings {
	ps := pluginSettings{}
	for k, v := range cfg.PluginSettings[pluginID] {
		if k == "path" || strings.ToLower(k) == "id" {
			continue
		}
		ps[k] = v
	}

	return ps
}

func (ps pluginSettings) asEnvVar(prefix string, hostEnv ...string) []string {
	env := make([]string, 0, len(ps))
	for k, v := range ps {
		key := fmt.Sprintf("%s_%s", prefix, strings.ToUpper(k))
		if value := os.Getenv(key); value != "" {
			v = value
		}

		env = append(env, fmt.Sprintf("%s=%s", key, v))
	}

	env = append(env, hostEnv...)

	return env
}
