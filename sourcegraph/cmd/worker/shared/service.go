package shared

import (
	"context"

	"github.com/sourcegraph/sourcegraph/internal/auth/userpasswd"
	"github.com/sourcegraph/sourcegraph/internal/debugserver"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/oobmigration/migrations/register"
	"github.com/sourcegraph/sourcegraph/internal/service"
)

type svc struct{}

func (svc) Name() string { return "worker" }

func (svc) Configure() (env.Config, []debugserver.Endpoint) {
	return LoadConfig(register.RegisterEnterpriseMigrators), nil
}

func (svc) Start(ctx context.Context, observationCtx *observation.Context, ready service.ReadyFunc, config env.Config) error {
	go setAuthzProviders(ctx, observationCtx)

	// internal/auth/providers.{GetProviderByConfigID,GetProviderbyServiceType} are potentially in the call-graph in worker,
	// so we init the built-in auth provider just in case.
	userpasswd.Init()

	return Start(ctx, observationCtx, ready, config.(*Config))
}

var Service service.Service = svc{}
