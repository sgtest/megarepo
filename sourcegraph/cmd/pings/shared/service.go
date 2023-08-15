package shared

import (
	"context"

	"github.com/sourcegraph/sourcegraph/internal/debugserver"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/service"
)

// Service is the shared ping service.
var Service service.Service = svc{}

type svc struct{}

func (svc) Name() string { return "pings" }

func (svc) Configure() (env.Config, []debugserver.Endpoint) {
	c := &Config{}
	c.Load()
	return c, []debugserver.Endpoint{}
}

func (svc) Start(ctx context.Context, observationCtx *observation.Context, ready service.ReadyFunc, config env.Config) error {
	return Main(ctx, observationCtx, ready, config.(*Config))
}
