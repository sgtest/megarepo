package repos

import (
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies"
	"github.com/sourcegraph/sourcegraph/internal/conf/reposource"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/crates"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

// NewRustPackagesSource returns a new RustPackagesSource from the given external service.
func NewRustPackagesSource(svc *types.ExternalService, cf *httpcli.Factory) (*PackagesSource, error) {
	var c schema.RustPackagesConnection
	if err := jsonc.Unmarshal(svc.Config, &c); err != nil {
		return nil, errors.Errorf("external service id=%d config error: %s", svc.ID, err)
	}

	cli, err := cf.Doer()
	if err != nil {
		return nil, err
	}

	return &PackagesSource{
		svc:        svc,
		configDeps: c.Dependencies,
		scheme:     dependencies.RustPackagesScheme,
		src:        &rustPackagesSource{client: crates.NewClient(svc.URN(), cli)},
	}, nil
}

type rustPackagesSource struct {
	client *crates.Client
}

var _ packagesSource = &rustPackagesSource{}

func (rustPackagesSource) ParseVersionedPackageFromConfiguration(dep string) (reposource.VersionedPackage, error) {
	return reposource.ParseRustVersionedPackage(dep)
}

func (rustPackagesSource) ParsePackageFromName(name reposource.PackageName) (reposource.Package, error) {
	return reposource.ParseRustPackageFromName(name)
}
func (rustPackagesSource) ParsePackageFromRepoName(repoName api.RepoName) (reposource.Package, error) {
	return reposource.ParseRustPackageFromRepoName(repoName)
}
