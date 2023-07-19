package grpcserver

import (
	"context"
	"fmt"
	"net"

	"github.com/grafana/grafana-plugin-sdk-go/backend"
	grpc_middleware "github.com/grpc-ecosystem/go-grpc-middleware"
	grpcAuth "github.com/grpc-ecosystem/go-grpc-middleware/auth"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/weaveworks/common/instrument"
	"github.com/weaveworks/common/middleware"
	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials"

	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/infra/tracing"
	"github.com/grafana/grafana/pkg/registry"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/services/grpcserver/interceptors"
	"github.com/grafana/grafana/pkg/setting"
)

var (
	grpcRequestDuration *prometheus.HistogramVec
)

type Provider interface {
	registry.BackgroundService
	GetServer() *grpc.Server
	GetAddress() string
}

type GPRCServerService struct {
	cfg     *setting.Cfg
	logger  log.Logger
	server  *grpc.Server
	address string
}

func ProvideService(cfg *setting.Cfg, authenticator interceptors.Authenticator, tracer tracing.Tracer, registerer prometheus.Registerer) (Provider, error) {
	s := &GPRCServerService{
		cfg:    cfg,
		logger: log.New("grpc-server"),
	}

	// Register the metric here instead of an init() function so that we do
	// nothing unless the feature is actually enabled.
	if grpcRequestDuration == nil {
		grpcRequestDuration = prometheus.NewHistogramVec(prometheus.HistogramOpts{
			Namespace: "grafana",
			Name:      "grpc_request_duration_seconds",
			Help:      "Time (in seconds) spent serving HTTP requests.",
			Buckets:   instrument.DefBuckets,
		}, []string{"method", "route", "status_code", "ws"})

		if err := registerer.Register(grpcRequestDuration); err != nil {
			return nil, err
		}
	}

	var opts []grpc.ServerOption

	// Default auth is admin token check, but this can be overridden by
	// services which implement ServiceAuthFuncOverride interface.
	// See https://github.com/grpc-ecosystem/go-grpc-middleware/blob/master/auth/auth.go#L30.
	opts = append(opts, []grpc.ServerOption{
		grpc.UnaryInterceptor(
			grpc_middleware.ChainUnaryServer(
				grpcAuth.UnaryServerInterceptor(authenticator.Authenticate),
				interceptors.TracingUnaryInterceptor(tracer),
				middleware.UnaryServerInstrumentInterceptor(grpcRequestDuration),
			),
		),
		grpc.StreamInterceptor(
			grpc_middleware.ChainStreamServer(
				interceptors.TracingStreamInterceptor(tracer),
				grpcAuth.StreamServerInterceptor(authenticator.Authenticate),
				middleware.StreamServerInstrumentInterceptor(grpcRequestDuration),
			),
		),
	}...)

	if s.cfg.GRPCServerTLSConfig != nil {
		opts = append(opts, grpc.Creds(credentials.NewTLS(cfg.GRPCServerTLSConfig)))
	}

	s.server = grpc.NewServer(opts...)
	return s, nil
}

func (s *GPRCServerService) Run(ctx context.Context) error {
	s.logger.Info("Running GRPC server", "address", s.cfg.GRPCServerAddress, "network", s.cfg.GRPCServerNetwork, "tls", s.cfg.GRPCServerTLSConfig != nil)

	listener, err := net.Listen(s.cfg.GRPCServerNetwork, s.cfg.GRPCServerAddress)
	if err != nil {
		return fmt.Errorf("GRPC server: failed to listen: %w", err)
	}

	s.address = listener.Addr().String()

	serveErr := make(chan error, 1)
	go func() {
		s.logger.Info("GRPC server: starting")
		err := s.server.Serve(listener)
		if err != nil {
			backend.Logger.Error("GRPC server: failed to serve", "err", err)
			serveErr <- err
		}
	}()

	select {
	case err := <-serveErr:
		backend.Logger.Error("GRPC server: failed to serve", "err", err)
		return err
	case <-ctx.Done():
	}
	s.logger.Warn("GRPC server: shutting down")
	s.server.Stop()
	return ctx.Err()
}

func (s *GPRCServerService) IsDisabled() bool {
	if s.cfg == nil {
		return true
	}
	return !s.cfg.IsFeatureToggleEnabled(featuremgmt.FlagGrpcServer)
}

func (s *GPRCServerService) GetServer() *grpc.Server {
	return s.server
}

func (s *GPRCServerService) GetAddress() string {
	return s.address
}
