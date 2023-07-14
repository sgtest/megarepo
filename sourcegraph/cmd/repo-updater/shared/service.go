package shared

import (
	"context"

	"github.com/sourcegraph/sourcegraph/internal/debugserver"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/service"
)

type svc struct {
	ready                chan struct{}
	debugserverEndpoints LazyDebugserverEndpoint
}

func (svc) Name() string { return "repo-updater" }

func (s *svc) Configure() (env.Config, []debugserver.Endpoint) {
	// Signals health of startup.
	s.ready = make(chan struct{})

	return nil, createDebugServerEndpoints(s.ready, &s.debugserverEndpoints)
}

func (s *svc) Start(ctx context.Context, observationCtx *observation.Context, signalReadyToParent service.ReadyFunc, _ env.Config) error {
	// This service's debugserver endpoints should start responding when this service is ready (and
	// not ewait for *all* services to be ready). Therefore, we need to track whether we are ready
	// separately.
	ready := service.ReadyFunc(func() {
		close(s.ready)
		signalReadyToParent()
	})

	return Main(ctx, observationCtx, ready, &s.debugserverEndpoints)
}

var Service service.Service = &svc{}
