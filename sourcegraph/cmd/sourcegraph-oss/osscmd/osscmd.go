// Package osscmd defines entrypoint functions for the OSS build of Sourcegraph's single-binary
// distribution. It is invoked by all OSS commands' main functions.
package osscmd

import (
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/service"
	"github.com/sourcegraph/sourcegraph/internal/service/svcmain"
)

var config = svcmain.Config{
	AfterConfigure: func() {
		// Set dummy authz provider to unblock channel for checking permissions in GraphQL APIs.
		// See https://github.com/sourcegraph/sourcegraph/issues/3847 for details.
		authz.SetProviders(true, []authz.Provider{})
	},
}

// Main is called from the `main` function of the `sourcegraph-oss` command.
func MainOSS(services []service.Service, args []string) {
	svcmain.Main(services, config, args)
}

// SingleServiceMainOSS is called from the `main` function of a command in the OSS build
// to start a single service (such as frontend or gitserver).
func SingleServiceMainOSS(service service.Service) {
	svcmain.SingleServiceMain(service, config)
}
