package renderer

import (
	"context"
	"errors"

	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/plugins"
	"github.com/grafana/grafana/pkg/plugins/backendplugin/pluginextensionv2"
	"github.com/grafana/grafana/pkg/plugins/backendplugin/provider"
	"github.com/grafana/grafana/pkg/plugins/config"
	"github.com/grafana/grafana/pkg/plugins/envvars"
	"github.com/grafana/grafana/pkg/plugins/manager/loader"
	"github.com/grafana/grafana/pkg/plugins/manager/pipeline/bootstrap"
	"github.com/grafana/grafana/pkg/plugins/manager/pipeline/discovery"
	"github.com/grafana/grafana/pkg/plugins/manager/pipeline/initialization"
	"github.com/grafana/grafana/pkg/plugins/manager/pipeline/termination"
	"github.com/grafana/grafana/pkg/plugins/manager/pipeline/validation"
	"github.com/grafana/grafana/pkg/plugins/manager/registry"
	"github.com/grafana/grafana/pkg/plugins/manager/signature"
	"github.com/grafana/grafana/pkg/plugins/manager/sources"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/services/rendering"
)

func ProvideService(cfg *config.Cfg, registry registry.Service, licensing plugins.Licensing,
	features featuremgmt.FeatureToggles) (*Manager, error) {
	l, err := createLoader(cfg, registry, licensing, features)
	if err != nil {
		return nil, err
	}

	return &Manager{
		cfg:    cfg,
		loader: l,
		log:    log.New("plugins.renderer"),
	}, nil
}

type Manager struct {
	cfg    *config.Cfg
	loader loader.Service
	log    log.Logger

	renderer *Plugin
}

type Plugin struct {
	plugin *plugins.Plugin

	started bool
}

func (p *Plugin) Client() (pluginextensionv2.RendererPlugin, error) {
	if !p.started {
		return nil, errors.New("renderer plugin not started")
	}

	if p.plugin.Renderer == nil {
		return nil, errors.New("renderer client not available")
	}

	return p.plugin.Renderer, nil
}

func (p *Plugin) Start(ctx context.Context) error {
	p.started = true
	return p.plugin.Start(ctx)
}

func (p *Plugin) Version() string {
	return p.plugin.JSONData.Info.Version
}

func (m *Manager) Renderer(ctx context.Context) (rendering.Plugin, bool) {
	if m.renderer != nil {
		return m.renderer, true
	}

	ps, err := m.loader.Load(ctx, sources.NewLocalSource(plugins.ClassExternal, []string{m.cfg.PluginsPath}))
	if err != nil {
		m.log.Error("Failed to load renderer plugin", "error", err)
		return nil, false
	}

	if len(ps) >= 1 {
		m.renderer = &Plugin{plugin: ps[0]}
		return m.renderer, true
	}

	return nil, false
}

func createLoader(cfg *config.Cfg, pr registry.Service, l plugins.Licensing,
	features featuremgmt.FeatureToggles) (loader.Service, error) {
	d := discovery.New(cfg, discovery.Opts{
		FindFilterFuncs: []discovery.FindFilterFunc{
			discovery.NewPermittedPluginTypesFilterStep([]plugins.Type{plugins.TypeRenderer}),
			func(ctx context.Context, class plugins.Class, bundles []*plugins.FoundBundle) ([]*plugins.FoundBundle, error) {
				return discovery.NewDuplicatePluginFilterStep(pr).Filter(ctx, bundles)
			},
		},
	})
	b := bootstrap.New(cfg, bootstrap.Opts{
		DecorateFuncs: []bootstrap.DecorateFunc{}, // no decoration required
	})
	v := validation.New(cfg, validation.Opts{
		ValidateFuncs: []validation.ValidateFunc{
			validation.SignatureValidationStep(signature.NewValidator(signature.NewUnsignedAuthorizer(cfg))),
		},
	})
	i := initialization.New(cfg, initialization.Opts{
		InitializeFuncs: []initialization.InitializeFunc{
			initialization.BackendClientInitStep(envvars.NewProvider(cfg, l), provider.New(provider.RendererProvider)),
			initialization.PluginRegistrationStep(pr),
		},
	})
	t, err := termination.New(cfg, termination.Opts{
		TerminateFuncs: []termination.TerminateFunc{
			termination.DeregisterStep(pr),
		},
	})
	if err != nil {
		return nil, err
	}

	return loader.New(d, b, v, i, t), nil
}
