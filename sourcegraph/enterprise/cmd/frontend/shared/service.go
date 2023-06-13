package shared

import (
	"context"

	frontend_shared "github.com/sourcegraph/sourcegraph/cmd/frontend/shared"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/codeintel"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/oobmigration/migrations"
	"github.com/sourcegraph/sourcegraph/internal/debugserver"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/service"

	_ "github.com/sourcegraph/sourcegraph/cmd/frontend/registry/api"
	_ "github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/registry"
)

type svc struct{}

func (svc) Name() string { return "frontend" }

func (svc) Configure() (env.Config, []debugserver.Endpoint) {
	frontend_shared.CLILoadConfig()
	codeintel.LoadConfig()
	return nil, frontend_shared.GRPCWebUIDebugEndpoints()
}

func (svc) Start(ctx context.Context, observationCtx *observation.Context, ready service.ReadyFunc, config env.Config) error {
	return frontend_shared.CLIMain(ctx, observationCtx, ready, EnterpriseSetupHook, migrations.RegisterEnterpriseMigratorsUsingConfAndStoreFactory)
}

var Service service.Service = svc{}
