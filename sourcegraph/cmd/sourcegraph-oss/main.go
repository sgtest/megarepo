// Command sourcegraph-oss is a single program that runs all of Sourcegraph (OSS variant).
package main

import (
	"os"

	blobstore_shared "github.com/sourcegraph/sourcegraph/cmd/blobstore/shared"
	frontend_shared "github.com/sourcegraph/sourcegraph/cmd/frontend/shared"
	githubproxy_shared "github.com/sourcegraph/sourcegraph/cmd/github-proxy/shared"
	gitserver_shared "github.com/sourcegraph/sourcegraph/cmd/gitserver/shared"
	repoupdater_shared "github.com/sourcegraph/sourcegraph/cmd/repo-updater/shared"
	searcher_shared "github.com/sourcegraph/sourcegraph/cmd/searcher/shared"
	"github.com/sourcegraph/sourcegraph/cmd/sourcegraph-oss/osscmd"
	symbols_shared "github.com/sourcegraph/sourcegraph/cmd/symbols/shared"
	worker_shared "github.com/sourcegraph/sourcegraph/cmd/worker/shared"
	"github.com/sourcegraph/sourcegraph/internal/sanitycheck"
	"github.com/sourcegraph/sourcegraph/internal/service"
	"github.com/sourcegraph/sourcegraph/internal/service/servegit"
)

// services is a list of services to run in the OSS build.
var services = []service.Service{
	frontend_shared.Service,
	gitserver_shared.Service,
	repoupdater_shared.Service,
	searcher_shared.Service,
	blobstore_shared.Service,
	symbols_shared.Service,
	worker_shared.Service,
	githubproxy_shared.Service,
	servegit.Service,
}

func main() {
	sanitycheck.Pass()
	osscmd.MainOSS(services, os.Args)
}
