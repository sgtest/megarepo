package bootstrap

import (
	"context"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/grafana/grafana/pkg/plugins"
	"github.com/grafana/grafana/pkg/plugins/config"
	"github.com/grafana/grafana/pkg/plugins/log"
	"github.com/grafana/grafana/pkg/plugins/manager/fakes"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/setting"
)

func TestSetDefaultNavURL(t *testing.T) {
	t.Run("When including a dashboard with DefaultNav: true", func(t *testing.T) {
		pluginWithDashboard := &plugins.Plugin{
			JSONData: plugins.JSONData{Includes: []*plugins.Includes{
				{
					Type:       "dashboard",
					DefaultNav: true,
					UID:        "",
				},
			}},
		}
		logger := log.NewTestLogger()
		pluginWithDashboard.SetLogger(logger)

		t.Run("Default nav URL is not set if dashboard UID field not is set", func(t *testing.T) {
			setDefaultNavURL(pluginWithDashboard)
			require.Equal(t, "", pluginWithDashboard.DefaultNavURL)
			require.NotZero(t, logger.WarnLogs.Calls)
			require.Equal(t, "Included dashboard is missing a UID field", logger.WarnLogs.Message)
		})

		t.Run("Default nav URL is set if dashboard UID field is set", func(t *testing.T) {
			pluginWithDashboard.Includes[0].UID = "a1b2c3"

			setDefaultNavURL(pluginWithDashboard)
			require.Equal(t, "/d/a1b2c3", pluginWithDashboard.DefaultNavURL)
		})
	})

	t.Run("When including a page with DefaultNav: true", func(t *testing.T) {
		pluginWithPage := &plugins.Plugin{
			JSONData: plugins.JSONData{Includes: []*plugins.Includes{
				{
					Type:       "page",
					DefaultNav: true,
					Slug:       "testPage",
				},
			}},
		}

		t.Run("Default nav URL is set using slug", func(t *testing.T) {
			setDefaultNavURL(pluginWithPage)
			require.Equal(t, "/plugins/page/testPage", pluginWithPage.DefaultNavURL)
		})

		t.Run("Default nav URL is set using slugified Name field if Slug field is empty", func(t *testing.T) {
			pluginWithPage.Includes[0].Slug = ""
			pluginWithPage.Includes[0].Name = "My Test Page"

			setDefaultNavURL(pluginWithPage)
			require.Equal(t, "/plugins/page/my-test-page", pluginWithPage.DefaultNavURL)
		})
	})
}

func TestTemplateDecorateFunc(t *testing.T) {
	t.Run("Removes %VERSION%", func(t *testing.T) {
		pluginWithoutVersion := &plugins.Plugin{
			JSONData: plugins.JSONData{
				Info: plugins.Info{
					Version: "%VERSION%",
				},
			},
		}
		p, err := TemplateDecorateFunc(context.TODO(), pluginWithoutVersion)
		require.NoError(t, err)
		require.Equal(t, "", p.Info.Version)
	})

	t.Run("Removes %TODAY%", func(t *testing.T) {
		pluginWithoutVersion := &plugins.Plugin{
			JSONData: plugins.JSONData{
				Info: plugins.Info{
					Version: "%TODAY%",
				},
			},
		}
		p, err := TemplateDecorateFunc(context.TODO(), pluginWithoutVersion)
		require.NoError(t, err)
		require.Equal(t, "", p.Info.Updated)
	})
}

func Test_configureAppChildPlugin(t *testing.T) {
	t.Run("When setting paths based on core plugin on Windows", func(t *testing.T) {
		child := &plugins.Plugin{
			FS: fakes.NewFakePluginFiles("c:\\grafana\\public\\app\\plugins\\app\\testdata-app\\datasources\\datasource"),
		}
		parent := &plugins.Plugin{
			JSONData: plugins.JSONData{
				Type: plugins.TypeApp,
				ID:   "testdata-app",
			},
			Class:   plugins.ClassCore,
			FS:      fakes.NewFakePluginFiles("c:\\grafana\\public\\app\\plugins\\app\\testdata-app"),
			BaseURL: "/public/app/plugins/app/testdata-app",
		}

		configureAppChildPlugin(&config.Cfg{}, parent, child)

		require.Equal(t, "core:plugin/testdata-app/datasources/datasource", child.Module)
		require.Equal(t, "testdata-app", child.IncludedInAppID)
		require.Equal(t, "/public/app/plugins/app/testdata-app", child.BaseURL)

		t.Run("App sub URL has no effect on Core plugins", func(t *testing.T) {
			configureAppChildPlugin(&config.Cfg{GrafanaAppSubURL: "/grafana"}, parent, child)

			require.Equal(t, "core:plugin/testdata-app/datasources/datasource", child.Module)
			require.Equal(t, "testdata-app", child.IncludedInAppID)
			require.Equal(t, "/public/app/plugins/app/testdata-app", child.BaseURL)
		})
	})

	t.Run("When setting paths based on external plugin with app sub URL", func(t *testing.T) {
		child := &plugins.Plugin{
			FS: fakes.NewFakePluginFiles("/plugins/parent-app/child-panel"),
		}
		parent := &plugins.Plugin{
			JSONData: plugins.JSONData{
				Type: plugins.TypeApp,
				ID:   "testdata-app",
			},
			Class:   plugins.ClassExternal,
			FS:      fakes.NewFakePluginFiles("/plugins/parent-app"),
			BaseURL: "/grafana/plugins/parent-app",
		}

		configureAppChildPlugin(&config.Cfg{GrafanaAppSubURL: "/grafana"}, parent, child)

		require.Equal(t, "/grafana/public/plugins/testdata-app/child-panel/module.js", child.Module)
		require.Equal(t, "testdata-app", child.IncludedInAppID)
		require.Equal(t, "/grafana/plugins/parent-app", child.BaseURL)
	})
}

func TestSkipEnvVarsDecorateFunc(t *testing.T) {
	const pluginID = "plugin-id"

	t.Run("feature flag is not present", func(t *testing.T) {
		f := SkipHostEnvVarsDecorateFunc(&config.Cfg{Features: featuremgmt.WithFeatures()})
		p, err := f(context.Background(), &plugins.Plugin{JSONData: plugins.JSONData{ID: pluginID}})
		require.NoError(t, err)
		require.False(t, p.SkipHostEnvVars)
	})

	t.Run("feature flag is present", func(t *testing.T) {
		t.Run("no plugin settings should set SkipHostEnvVars to true", func(t *testing.T) {
			f := SkipHostEnvVarsDecorateFunc(&config.Cfg{
				Features: featuremgmt.WithFeatures(featuremgmt.FlagPluginsSkipHostEnvVars),
			})
			p, err := f(context.Background(), &plugins.Plugin{JSONData: plugins.JSONData{ID: pluginID}})
			require.NoError(t, err)
			require.True(t, p.SkipHostEnvVars)
		})

		t.Run("plugin setting", func(t *testing.T) {
			for _, tc := range []struct {
				name               string
				pluginSettings     setting.PluginSettings
				expSkipHostEnvVars bool
			}{
				{
					name:               "forward_host_env_vars = false should set SkipHostEnvVars to true",
					pluginSettings:     setting.PluginSettings{pluginID: map[string]string{"forward_host_env_vars": "false"}},
					expSkipHostEnvVars: true,
				},
				{
					name:               "forward_host_env_vars = true should set SkipHostEnvVars to false",
					pluginSettings:     setting.PluginSettings{pluginID: map[string]string{"forward_host_env_vars": "true"}},
					expSkipHostEnvVars: false,
				},
				{
					name:               "invalid forward_host_env_vars should set SkipHostEnvVars to true",
					pluginSettings:     setting.PluginSettings{pluginID: map[string]string{"forward_host_env_vars": "grilled cheese sandwich with bacon"}},
					expSkipHostEnvVars: true,
				},
				{
					name:               "forward_host_env_vars absent should set SkipHostEnvVars to true",
					pluginSettings:     setting.PluginSettings{pluginID: nil},
					expSkipHostEnvVars: true,
				},
			} {
				t.Run(tc.name, func(t *testing.T) {
					f := SkipHostEnvVarsDecorateFunc(&config.Cfg{
						Features:       featuremgmt.WithFeatures(featuremgmt.FlagPluginsSkipHostEnvVars),
						PluginSettings: tc.pluginSettings,
					})
					p, err := f(context.Background(), &plugins.Plugin{JSONData: plugins.JSONData{ID: pluginID}})
					require.NoError(t, err)
					require.Equal(t, tc.expSkipHostEnvVars, p.SkipHostEnvVars)
				})
			}
		})
	})
}
