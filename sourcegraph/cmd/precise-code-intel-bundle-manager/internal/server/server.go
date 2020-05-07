package server

import (
	"net"
	"net/http"
	"os"
	"strconv"

	"github.com/inconshreveable/log15"
	"github.com/sourcegraph/sourcegraph/cmd/precise-code-intel-bundle-manager/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
)

type Server struct {
	host                 string
	port                 int
	bundleDir            string
	databaseCache        *database.DatabaseCache
	documentDataCache    *database.DocumentDataCache
	resultChunkDataCache *database.ResultChunkDataCache
	observationContext   *observation.Context
}

type ServerOpts struct {
	Host                 string
	Port                 int
	BundleDir            string
	DatabaseCache        *database.DatabaseCache
	DocumentDataCache    *database.DocumentDataCache
	ResultChunkDataCache *database.ResultChunkDataCache
	ObservationContext   *observation.Context
}

func New(opts ServerOpts) *Server {
	return &Server{
		host:                 opts.Host,
		port:                 opts.Port,
		bundleDir:            opts.BundleDir,
		databaseCache:        opts.DatabaseCache,
		documentDataCache:    opts.DocumentDataCache,
		resultChunkDataCache: opts.ResultChunkDataCache,
		observationContext:   opts.ObservationContext,
	}
}

func (s *Server) Start() {
	addr := net.JoinHostPort(s.host, strconv.FormatInt(int64(s.port), 10))
	handler := ot.Middleware(s.handler())
	server := &http.Server{Addr: addr, Handler: handler}

	if err := server.ListenAndServe(); err != http.ErrServerClosed {
		log15.Error("Failed to start server", "err", err)
		os.Exit(1)
	}
}
