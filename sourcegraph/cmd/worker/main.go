package main

import (
	"os"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/worker/shared"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/version"
)

func main() {
	liblog := log.Init(log.Resource{
		Name:    env.MyName,
		Version: version.Version(),
	})
	defer liblog.Sync()

	logger := log.Scoped("worker", "worker oss edition")

	authz.SetProviders(true, []authz.Provider{})
	if err := shared.Start(logger, nil, nil); err != nil {
		logger.Error(err.Error())
		os.Exit(1)
	}
}
