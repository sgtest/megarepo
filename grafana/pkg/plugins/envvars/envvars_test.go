package envvars

import (
	"context"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/grafana/grafana/pkg/plugins"
	"github.com/grafana/grafana/pkg/plugins/auth"
	"github.com/grafana/grafana/pkg/plugins/config"
	"github.com/grafana/grafana/pkg/plugins/manager/fakes"
	"github.com/grafana/grafana/pkg/plugins/plugindef"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/setting"
)

func TestInitializer_envVars(t *testing.T) {
	t.Run("backend datasource with license", func(t *testing.T) {
		p := &plugins.Plugin{
			JSONData: plugins.JSONData{
				ID: "test",
			},
		}

		licensing := &fakes.FakeLicensingService{
			LicenseEdition: "test",
			TokenRaw:       "token",
			LicensePath:    "/path/to/ent/license",
			LicenseAppURL:  "https://myorg.com/",
		}

		envVarsProvider := NewProvider(&config.Cfg{
			PluginSettings: map[string]map[string]string{
				"test": {
					"custom_env_var": "customVal",
				},
			},
		}, licensing)

		envVars := envVarsProvider.Get(context.Background(), p)
		assert.Len(t, envVars, 6)
		assert.Equal(t, "GF_PLUGIN_CUSTOM_ENV_VAR=customVal", envVars[0])
		assert.Equal(t, "GF_VERSION=", envVars[1])
		assert.Equal(t, "GF_EDITION=test", envVars[2])
		assert.Equal(t, "GF_ENTERPRISE_LICENSE_PATH=/path/to/ent/license", envVars[3])
		assert.Equal(t, "GF_ENTERPRISE_APP_URL=https://myorg.com/", envVars[4])
		assert.Equal(t, "GF_ENTERPRISE_LICENSE_TEXT=token", envVars[5])
	})
}

func TestInitializer_tracingEnvironmentVariables(t *testing.T) {
	const pluginID = "plugin_id"

	defaultPlugin := &plugins.Plugin{
		JSONData: plugins.JSONData{
			ID:   pluginID,
			Info: plugins.Info{Version: "1.0.0"},
		},
	}
	pluginWithoutVersion := &plugins.Plugin{
		JSONData: plugins.JSONData{ID: pluginID},
	}

	defaultOTelCfg := config.OpenTelemetryCfg{
		Address:     "127.0.0.1:4317",
		Propagation: "",
	}

	expDefaultOtlp := func(t *testing.T, envVars []string) {
		found := map[string]bool{
			"address":        false,
			"plugin_version": false,
			"propagation":    false,
		}
		setFound := func(v string) {
			require.False(t, found[v], "duplicate env var found")
			found[v] = true
		}
		for _, v := range envVars {
			switch v {
			case "GF_PLUGIN_VERSION=1.0.0":
				setFound("plugin_version")
			case "GF_INSTANCE_OTLP_ADDRESS=127.0.0.1:4317":
				setFound("address")
			case "GF_INSTANCE_OTLP_PROPAGATION=":
				setFound("propagation")
			}
		}
		for k, f := range found {
			require.Truef(t, f, "%q env var not found: %+v", k, envVars)
		}
	}
	expNoTracing := func(t *testing.T, envVars []string) {
		for _, v := range envVars {
			assert.False(t, strings.HasPrefix(v, "GF_TRACING"), "should not have tracing env var")
			assert.False(
				t,
				strings.HasPrefix(v, "GF_PLUGIN_VERSION"),
				"GF_PLUGIN_VERSION is tracing-only and should not be present when tracing is disabled",
			)
		}
	}
	expGfPluginVersionNotPresent := func(t *testing.T, envVars []string) {
		for _, e := range envVars {
			assert.False(t, strings.HasPrefix("GF_PLUGIN_VERSION=", e), "GF_PLUGIN_VERSION shouldn't be present")
		}
	}
	expGfPluginVersionPresent := func(t *testing.T, envVars []string) {
		var found bool
		for _, e := range envVars {
			if e != "GF_PLUGIN_VERSION=1.0.0" {
				continue
			}
			assert.False(t, found, "GF_PLUGIN_VERSION is present multiple times")
			found = true
		}
		assert.Truef(t, found, "GF_PLUGIN_VERSION is not present: %+v", envVars)
	}

	for _, tc := range []struct {
		name   string
		cfg    *config.Cfg
		plugin *plugins.Plugin
		exp    func(t *testing.T, envVars []string)
	}{
		{
			name: "otel not configured",
			cfg: &config.Cfg{
				Tracing: config.Tracing{},
			},
			plugin: defaultPlugin,
			exp:    expNoTracing,
		},
		{
			name: "otel not configured but plugin-tracing enabled",
			cfg: &config.Cfg{
				Tracing:        config.Tracing{},
				PluginSettings: map[string]map[string]string{pluginID: {"tracing": "true"}},
			},
			plugin: defaultPlugin,
			exp:    expNoTracing,
		},
		{
			name: "otlp no propagation plugin enabled",
			cfg: &config.Cfg{
				Tracing: config.Tracing{
					OpenTelemetry: defaultOTelCfg,
				},
				PluginSettings: map[string]map[string]string{
					pluginID: {"tracing": "true"},
				},
			},
			plugin: defaultPlugin,
			exp:    expDefaultOtlp,
		},
		{
			name: "otlp no propagation disabled by default",
			cfg: &config.Cfg{
				Tracing: config.Tracing{
					OpenTelemetry: defaultOTelCfg,
				},
			},
			plugin: defaultPlugin,
			exp:    expNoTracing,
		},
		{
			name: "otlp propagation plugin enabled",
			cfg: &config.Cfg{
				Tracing: config.Tracing{
					OpenTelemetry: config.OpenTelemetryCfg{
						Address:     "127.0.0.1:4317",
						Propagation: "w3c",
					},
				},
				PluginSettings: map[string]map[string]string{
					pluginID: {"tracing": "true"},
				},
			},
			plugin: defaultPlugin,
			exp: func(t *testing.T, envVars []string) {
				assert.Len(t, envVars, 5)
				assert.Equal(t, "GF_PLUGIN_TRACING=true", envVars[0])
				assert.Equal(t, "GF_VERSION=", envVars[1])
				assert.Equal(t, "GF_INSTANCE_OTLP_ADDRESS=127.0.0.1:4317", envVars[2])
				assert.Equal(t, "GF_INSTANCE_OTLP_PROPAGATION=w3c", envVars[3])
				assert.Equal(t, "GF_PLUGIN_VERSION=1.0.0", envVars[4])
			},
		},
		{
			name: "otlp enabled composite propagation",
			cfg: &config.Cfg{
				Tracing: config.Tracing{
					OpenTelemetry: config.OpenTelemetryCfg{
						Address:     "127.0.0.1:4317",
						Propagation: "w3c,jaeger",
					},
				},
				PluginSettings: map[string]map[string]string{
					pluginID: {"tracing": "true"},
				},
			},
			plugin: defaultPlugin,
			exp: func(t *testing.T, envVars []string) {
				assert.Len(t, envVars, 5)
				assert.Equal(t, "GF_PLUGIN_TRACING=true", envVars[0])
				assert.Equal(t, "GF_VERSION=", envVars[1])
				assert.Equal(t, "GF_INSTANCE_OTLP_ADDRESS=127.0.0.1:4317", envVars[2])
				assert.Equal(t, "GF_INSTANCE_OTLP_PROPAGATION=w3c,jaeger", envVars[3])
				assert.Equal(t, "GF_PLUGIN_VERSION=1.0.0", envVars[4])
			},
		},
		{
			name: "otlp no propagation disabled by default",
			cfg: &config.Cfg{
				Tracing: config.Tracing{
					OpenTelemetry: config.OpenTelemetryCfg{
						Address:     "127.0.0.1:4317",
						Propagation: "w3c",
					},
				},
			},
			plugin: defaultPlugin,
			exp:    expNoTracing,
		},
		{
			name: "disabled on plugin",
			cfg: &config.Cfg{
				Tracing: config.Tracing{
					OpenTelemetry: defaultOTelCfg,
				},
				PluginSettings: setting.PluginSettings{
					pluginID: map[string]string{"tracing": "false"},
				},
			},
			plugin: defaultPlugin,
			exp:    expNoTracing,
		},
		{
			name: "disabled on plugin with other plugin settings",
			cfg: &config.Cfg{
				Tracing: config.Tracing{
					OpenTelemetry: defaultOTelCfg,
				},
				PluginSettings: map[string]map[string]string{
					pluginID: {"some_other_option": "true"},
				},
			},
			plugin: defaultPlugin,
			exp:    expNoTracing,
		},
		{
			name: "enabled on plugin with other plugin settings",
			cfg: &config.Cfg{
				Tracing: config.Tracing{
					OpenTelemetry: defaultOTelCfg,
				},
				PluginSettings: map[string]map[string]string{
					pluginID: {"some_other_option": "true", "tracing": "true"},
				},
			},
			plugin: defaultPlugin,
			exp:    expDefaultOtlp,
		},
		{
			name: "GF_PLUGIN_VERSION is not present if tracing is disabled",
			cfg: &config.Cfg{
				Tracing: config.Tracing{
					OpenTelemetry: config.OpenTelemetryCfg{},
				},
				PluginSettings: map[string]map[string]string{pluginID: {"tracing": "true"}},
			},
			plugin: defaultPlugin,
			exp:    expGfPluginVersionNotPresent,
		},
		{
			name: "GF_PLUGIN_VERSION is present if tracing is enabled and plugin has version",
			cfg: &config.Cfg{
				Tracing: config.Tracing{
					OpenTelemetry: defaultOTelCfg,
				},
				PluginSettings: map[string]map[string]string{pluginID: {"tracing": "true"}},
			},
			plugin: defaultPlugin,
			exp:    expGfPluginVersionPresent,
		},
		{
			name: "GF_PLUGIN_VERSION is not present if tracing is enabled but plugin doesn't have a version",
			cfg: &config.Cfg{
				Tracing: config.Tracing{
					OpenTelemetry: config.OpenTelemetryCfg{},
				},
				PluginSettings: map[string]map[string]string{pluginID: {"tracing": "true"}},
			},
			plugin: pluginWithoutVersion,
			exp:    expGfPluginVersionNotPresent,
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			envVarsProvider := NewProvider(tc.cfg, nil)
			envVars := envVarsProvider.Get(context.Background(), tc.plugin)
			tc.exp(t, envVars)
		})
	}
}

func TestInitializer_oauthEnvVars(t *testing.T) {
	t.Run("backend datasource with oauth registration", func(t *testing.T) {
		p := &plugins.Plugin{
			JSONData: plugins.JSONData{
				ID:                          "test",
				ExternalServiceRegistration: &plugindef.ExternalServiceRegistration{},
			},
			ExternalService: &auth.ExternalService{
				ClientID:     "clientID",
				ClientSecret: "clientSecret",
				PrivateKey:   "privatePem",
			},
		}

		envVarsProvider := NewProvider(&config.Cfg{
			GrafanaAppURL: "https://myorg.com/",
			Features:      featuremgmt.WithFeatures(featuremgmt.FlagExternalServiceAuth),
		}, nil)
		envVars := envVarsProvider.Get(context.Background(), p)
		assert.Equal(t, "GF_VERSION=", envVars[0])
		assert.Equal(t, "GF_APP_URL=https://myorg.com/", envVars[1])
		assert.Equal(t, "GF_PLUGIN_APP_CLIENT_ID=clientID", envVars[2])
		assert.Equal(t, "GF_PLUGIN_APP_CLIENT_SECRET=clientSecret", envVars[3])
		assert.Equal(t, "GF_PLUGIN_APP_PRIVATE_KEY=privatePem", envVars[4])
	})
}

func TestInitalizer_awsEnvVars(t *testing.T) {
	t.Run("backend datasource with aws settings", func(t *testing.T) {
		p := &plugins.Plugin{}
		envVarsProvider := NewProvider(&config.Cfg{
			AWSAssumeRoleEnabled:    true,
			AWSAllowedAuthProviders: []string{"grafana_assume_role", "keys"},
			AWSExternalId:           "mock_external_id",
		}, nil)
		envVars := envVarsProvider.Get(context.Background(), p)
		assert.ElementsMatch(t, []string{"GF_VERSION=", "AWS_AUTH_AssumeRoleEnabled=true", "AWS_AUTH_AllowedAuthProviders=grafana_assume_role,keys", "AWS_AUTH_EXTERNAL_ID=mock_external_id"}, envVars)
	})
}

func TestInitializer_featureToggleEnvVar(t *testing.T) {
	t.Run("backend datasource with feature toggle", func(t *testing.T) {
		expectedFeatures := []string{"feat-1", "feat-2"}
		featuresLookup := map[string]bool{
			expectedFeatures[0]: true,
			expectedFeatures[1]: true,
		}

		p := &plugins.Plugin{}
		envVarsProvider := NewProvider(&config.Cfg{
			Features: featuremgmt.WithFeatures(expectedFeatures[0], true, expectedFeatures[1], true),
		}, nil)
		envVars := envVarsProvider.Get(context.Background(), p)

		assert.Equal(t, 2, len(envVars))

		toggleExpression := strings.Split(envVars[1], "=")
		assert.Equal(t, 2, len(toggleExpression))

		assert.Equal(t, "GF_INSTANCE_FEATURE_TOGGLES_ENABLE", toggleExpression[0])

		toggleArgs := toggleExpression[1]
		features := strings.Split(toggleArgs, ",")

		assert.Equal(t, len(expectedFeatures), len(features))

		// this is necessary because the features are not returned in the order they are provided
		for _, f := range features {
			_, ok := featuresLookup[f]
			assert.True(t, ok)
		}
	})
}

func TestService_GetConfigMap(t *testing.T) {
	tcs := []struct {
		name     string
		cfg      *config.Cfg
		expected map[string]string
	}{
		{
			name: "Both features and proxy settings enabled",
			cfg: &config.Cfg{
				Features: featuremgmt.WithFeatures("feat-2", "feat-500", "feat-1"),
				ProxySettings: setting.SecureSocksDSProxySettings{
					Enabled:      true,
					ShowUI:       true,
					ClientCert:   "c3rt",
					ClientKey:    "k3y",
					RootCA:       "ca",
					ProxyAddress: "https://proxy.grafana.com",
					ServerName:   "secureProxy",
				},
			},
			expected: map[string]string{
				"GF_INSTANCE_FEATURE_TOGGLES_ENABLE":              "feat-1,feat-2,feat-500",
				"GF_SECURE_SOCKS_DATASOURCE_PROXY_SERVER_ENABLED": "true",
				"GF_SECURE_SOCKS_DATASOURCE_PROXY_CLIENT_CERT":    "c3rt",
				"GF_SECURE_SOCKS_DATASOURCE_PROXY_CLIENT_KEY":     "k3y",
				"GF_SECURE_SOCKS_DATASOURCE_PROXY_ROOT_CA_CERT":   "ca",
				"GF_SECURE_SOCKS_DATASOURCE_PROXY_PROXY_ADDRESS":  "https://proxy.grafana.com",
				"GF_SECURE_SOCKS_DATASOURCE_PROXY_SERVER_NAME":    "secureProxy",
			},
		},
		{
			name: "Features enabled but proxy settings disabled",
			cfg: &config.Cfg{
				Features: featuremgmt.WithFeatures("feat-2", "feat-500", "feat-1"),
				ProxySettings: setting.SecureSocksDSProxySettings{
					Enabled:      false,
					ShowUI:       true,
					ClientCert:   "c3rt",
					ClientKey:    "k3y",
					RootCA:       "ca",
					ProxyAddress: "https://proxy.grafana.com",
					ServerName:   "secureProxy",
				},
			},
			expected: map[string]string{
				"GF_INSTANCE_FEATURE_TOGGLES_ENABLE": "feat-1,feat-2,feat-500",
			},
		},
		{
			name: "Both features and proxy settings disabled",
			cfg: &config.Cfg{
				Features: featuremgmt.WithFeatures("feat-2", false),
				ProxySettings: setting.SecureSocksDSProxySettings{
					Enabled:      false,
					ShowUI:       true,
					ClientCert:   "c3rt",
					ClientKey:    "k3y",
					RootCA:       "ca",
					ProxyAddress: "https://proxy.grafana.com",
					ServerName:   "secureProxy",
				},
			},
			expected: map[string]string{},
		},
		{
			name: "Both features and proxy settings empty",
			cfg: &config.Cfg{
				Features:      nil,
				ProxySettings: setting.SecureSocksDSProxySettings{},
			},
			expected: map[string]string{},
		},
	}
	for _, tc := range tcs {
		t.Run(tc.name, func(t *testing.T) {
			s := &Service{
				cfg: tc.cfg,
			}
			require.Equal(t, tc.expected, s.GetConfigMap(context.Background(), "", nil))
		})
	}
}

func TestService_GetConfigMap_featureToggles(t *testing.T) {
	t.Run("Feature toggles list is deterministic", func(t *testing.T) {
		tcs := []struct {
			enabledFeatures []string
			expectedConfig  map[string]string
		}{
			{
				enabledFeatures: nil,
				expectedConfig:  map[string]string{},
			},
			{
				enabledFeatures: []string{},
				expectedConfig:  map[string]string{},
			},
			{
				enabledFeatures: []string{"A", "B", "C"},
				expectedConfig:  map[string]string{"GF_INSTANCE_FEATURE_TOGGLES_ENABLE": "A,B,C"},
			},
			{
				enabledFeatures: []string{"C", "B", "A"},
				expectedConfig:  map[string]string{"GF_INSTANCE_FEATURE_TOGGLES_ENABLE": "A,B,C"},
			},
			{
				enabledFeatures: []string{"b", "a", "c", "d"},
				expectedConfig:  map[string]string{"GF_INSTANCE_FEATURE_TOGGLES_ENABLE": "a,b,c,d"},
			},
		}

		for _, tc := range tcs {
			s := &Service{
				cfg: &config.Cfg{
					Features: fakes.NewFakeFeatureToggles(tc.enabledFeatures...),
				},
			}
			require.Equal(t, tc.expectedConfig, s.GetConfigMap(context.Background(), "", nil))
		}
	})
}
