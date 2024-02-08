package bootstrap

import (
	"context"
	"path"
	"slices"
	"strings"

	"github.com/grafana/grafana/pkg/infra/slugify"
	"github.com/grafana/grafana/pkg/plugins"
	"github.com/grafana/grafana/pkg/plugins/config"
	"github.com/grafana/grafana/pkg/plugins/log"
	"github.com/grafana/grafana/pkg/plugins/manager/loader/assetpath"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
)

// DefaultConstructor implements the default ConstructFunc used for the Construct step of the Bootstrap stage.
//
// It uses a pluginFactoryFunc to create plugins and the signatureCalculator to calculate the plugin's signature state.
type DefaultConstructor struct {
	pluginFactoryFunc   pluginFactoryFunc
	signatureCalculator plugins.SignatureCalculator
	log                 log.Logger
}

// DefaultConstructFunc is the default ConstructFunc used for the Construct step of the Bootstrap stage.
func DefaultConstructFunc(signatureCalculator plugins.SignatureCalculator, assetPath *assetpath.Service) ConstructFunc {
	return NewDefaultConstructor(signatureCalculator, assetPath).Construct
}

// DefaultDecorateFuncs are the default DecorateFuncs used for the Decorate step of the Bootstrap stage.
func DefaultDecorateFuncs(cfg *config.Cfg) []DecorateFunc {
	return []DecorateFunc{
		AppDefaultNavURLDecorateFunc,
		TemplateDecorateFunc,
		AppChildDecorateFunc(),
		SkipHostEnvVarsDecorateFunc(cfg),
	}
}

// NewDefaultConstructor returns a new DefaultConstructor.
func NewDefaultConstructor(signatureCalculator plugins.SignatureCalculator, assetPath *assetpath.Service) *DefaultConstructor {
	return &DefaultConstructor{
		pluginFactoryFunc:   NewDefaultPluginFactory(assetPath).createPlugin,
		signatureCalculator: signatureCalculator,
		log:                 log.New("plugins.construct"),
	}
}

// Construct will calculate the plugin's signature state and create the plugin using the pluginFactoryFunc.
func (c *DefaultConstructor) Construct(ctx context.Context, src plugins.PluginSource, bundles []*plugins.FoundBundle) ([]*plugins.Plugin, error) {
	res := make([]*plugins.Plugin, 0, len(bundles))

	for _, bundle := range bundles {
		sig, err := c.signatureCalculator.Calculate(ctx, src, bundle.Primary)
		if err != nil {
			c.log.Warn("Could not calculate plugin signature state", "pluginId", bundle.Primary.JSONData.ID, "error", err)
			continue
		}
		plugin, err := c.pluginFactoryFunc(bundle.Primary, src.PluginClass(ctx), sig)
		if err != nil {
			c.log.Error("Could not create primary plugin base", "pluginId", bundle.Primary.JSONData.ID, "error", err)
			continue
		}
		res = append(res, plugin)

		children := make([]*plugins.Plugin, 0, len(bundle.Children))
		for _, child := range bundle.Children {
			cp, err := c.pluginFactoryFunc(*child, plugin.Class, sig)
			if err != nil {
				c.log.Error("Could not create child plugin base", "pluginId", child.JSONData.ID, "error", err)
				continue
			}
			cp.Parent = plugin
			plugin.Children = append(plugin.Children, cp)

			children = append(children, cp)
		}
		res = append(res, children...)
	}

	return res, nil
}

// AppDefaultNavURLDecorateFunc is a DecorateFunc that sets the default nav URL for app plugins.
func AppDefaultNavURLDecorateFunc(_ context.Context, p *plugins.Plugin) (*plugins.Plugin, error) {
	if p.IsApp() {
		setDefaultNavURL(p)
	}
	return p, nil
}

// TemplateDecorateFunc is a DecorateFunc that removes the placeholder for the version and last_update fields.
func TemplateDecorateFunc(_ context.Context, p *plugins.Plugin) (*plugins.Plugin, error) {
	// %VERSION% and %TODAY% are valid values, according to the plugin schema
	// but it's meant to be replaced by the build system with the actual version and date.
	// If not, it's the same than not having a version or a date.
	if p.Info.Version == "%VERSION%" {
		p.Info.Version = ""
	}

	if p.Info.Updated == "%TODAY%" {
		p.Info.Updated = ""
	}

	return p, nil
}

func setDefaultNavURL(p *plugins.Plugin) {
	// slugify pages
	for _, include := range p.Includes {
		if include.Slug == "" {
			include.Slug = slugify.Slugify(include.Name)
		}

		if !include.DefaultNav {
			continue
		}

		if include.Type == "page" {
			p.DefaultNavURL = path.Join("/plugins/", p.ID, "/page/", include.Slug)
		}
		if include.Type == "dashboard" {
			dboardURL := include.DashboardURLPath()
			if dboardURL == "" {
				p.Logger().Warn("Included dashboard is missing a UID field")
				continue
			}

			p.DefaultNavURL = dboardURL
		}
	}
}

// AppChildDecorateFunc is a DecorateFunc that configures child plugins of app plugins.
func AppChildDecorateFunc() DecorateFunc {
	return func(_ context.Context, p *plugins.Plugin) (*plugins.Plugin, error) {
		if p.Parent != nil && p.Parent.IsApp() {
			configureAppChildPlugin(p.Parent, p)
		}
		return p, nil
	}
}

func configureAppChildPlugin(parent *plugins.Plugin, child *plugins.Plugin) {
	if !parent.IsApp() {
		return
	}
	child.IncludedInAppID = parent.ID
	child.BaseURL = parent.BaseURL

	// TODO move this logic within assetpath package
	appSubPath := strings.ReplaceAll(strings.Replace(child.FS.Base(), parent.FS.Base(), "", 1), "\\", "/")
	if parent.IsCorePlugin() {
		child.Module = path.Join("core:plugin", parent.ID, appSubPath)
	} else {
		child.Module = path.Join("public/plugins", parent.ID, appSubPath, "module.js")
	}
}

// SkipHostEnvVarsDecorateFunc returns a DecorateFunc that configures the SkipHostEnvVars field of the plugin.
// It will be set to true if the FlagPluginsSkipHostEnvVars feature flag is set, and the plugin is not present in the
// ForwardHostEnvVars plugin ids list.
func SkipHostEnvVarsDecorateFunc(cfg *config.Cfg) DecorateFunc {
	return func(_ context.Context, p *plugins.Plugin) (*plugins.Plugin, error) {
		p.SkipHostEnvVars = cfg.Features.IsEnabledGlobally(featuremgmt.FlagPluginsSkipHostEnvVars) &&
			!slices.Contains(cfg.ForwardHostEnvVars, p.ID)
		return p, nil
	}
}
