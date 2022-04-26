package repos

import (
	"context"

	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies"
	"github.com/sourcegraph/sourcegraph/internal/conf/reposource"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/jvmpackages"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/jvmpackages/coursier"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

// A JVMPackagesSource creates git repositories from `*-sources.jar` files of
// published Maven dependencies from the JVM ecosystem.
type JVMPackagesSource struct {
	svc     *types.ExternalService
	config  *schema.JVMPackagesConnection
	depsSvc *dependencies.Service
}

// NewJVMPackagesSource returns a new MavenSource from the given external
// service.
func NewJVMPackagesSource(svc *types.ExternalService) (*JVMPackagesSource, error) {
	var c schema.JVMPackagesConnection
	if err := jsonc.Unmarshal(svc.Config, &c); err != nil {
		return nil, errors.Newf("external service id=%d config error: %s", svc.ID, err)
	}
	return newJVMPackagesSource(svc, &c)
}

func (s *JVMPackagesSource) SetDependenciesService(depsSvc *dependencies.Service) {
	s.depsSvc = depsSvc
}

func newJVMPackagesSource(svc *types.ExternalService, c *schema.JVMPackagesConnection) (*JVMPackagesSource, error) {
	return &JVMPackagesSource{
		svc:     svc,
		config:  c,
		depsSvc: nil, // set via SetDependenciesService decorator
	}, nil
}

// ListRepos returns all Maven artifacts accessible to all connections
// configured in Sourcegraph via the external services configuration.
//
// [FIXME: deduplicate-listed-repos] The current implementation will return
// multiple repos with the same URL if there are different versions of it.
func (s *JVMPackagesSource) ListRepos(ctx context.Context, results chan SourceResult) {
	s.listDependentRepos(ctx, results)
}

func (s *JVMPackagesSource) listDependentRepos(ctx context.Context, results chan SourceResult) {
	modules, err := MavenModules(*s.config)
	if err != nil {
		results <- SourceResult{Err: err}
		return
	}
	for _, module := range modules {
		repo := s.makeRepo(module)
		results <- SourceResult{
			Source: s,
			Repo:   repo,
		}
	}
	if err != nil {
		results <- SourceResult{Err: err}
		return
	}

	var (
		totalDBFetched  int
		totalDBResolved int
		lastID          int
	)
	for {
		dbDeps, err := s.depsSvc.ListDependencyRepos(ctx, dependencies.ListDependencyReposOpts{
			Scheme:      dependencies.JVMPackagesScheme,
			After:       lastID,
			Limit:       100,
			NewestFirst: true,
		})
		if err != nil {
			results <- SourceResult{Err: err}
			return
		}

		if len(dbDeps) == 0 {
			break
		}

		totalDBFetched += len(dbDeps)

		lastID = dbDeps[len(dbDeps)-1].ID

		for _, dep := range dbDeps {
			parsedModule, err := reposource.ParseMavenModule(dep.Name)
			if err != nil {
				log15.Warn("error parsing maven module", "error", err, "module", dep.Name)
				continue
			}
			mavenDependency := &reposource.MavenDependency{MavenModule: parsedModule, Version: dep.Version}

			// We dont return anything that isnt resolvable here, to reduce logspam from gitserver. This codepath
			// should be hit much less frequently than gitservers attempts to get packages, so there should be less
			// logspam. This may no longer hold true if the extsvc syncs more often than gitserver would, but I
			// don't foresee that happening (not soon at least).
			err = coursier.Exists(ctx, s.config, mavenDependency)
			if err != nil {
				log15.Warn("jvm package not resolvable from coursier", "package", mavenDependency.PackageManagerSyntax())
				continue
			}

			repo := s.makeRepo(mavenDependency.MavenModule)
			totalDBResolved++
			results <- SourceResult{
				Source: s,
				Repo:   repo,
			}
		}
	}

	log15.Info("finished listing resolvable maven artifacts", "totalDB", totalDBFetched, "resolvedDB", totalDBResolved, "totalConfig", len(modules))
}

func (s *JVMPackagesSource) makeRepo(module *reposource.MavenModule) *types.Repo {
	urn := s.svc.URN()
	cloneURL := module.CloneURL()
	return &types.Repo{
		Name: module.RepoName(),
		URI:  string(module.RepoName()),
		ExternalRepo: api.ExternalRepoSpec{
			ID:          string(module.RepoName()),
			ServiceID:   extsvc.TypeJVMPackages,
			ServiceType: extsvc.TypeJVMPackages,
		},
		Private: false,
		Sources: map[string]*types.SourceInfo{
			urn: {
				ID:       urn,
				CloneURL: cloneURL,
			},
		},
		Metadata: &jvmpackages.Metadata{
			Module: module,
		},
	}
}

// ExternalServices returns a singleton slice containing the external service.
func (s *JVMPackagesSource) ExternalServices() types.ExternalServices {
	return types.ExternalServices{s.svc}
}

func MavenDependencies(connection schema.JVMPackagesConnection) (dependencies []*reposource.MavenDependency, err error) {
	for _, dep := range connection.Maven.Dependencies {
		dependency, err := reposource.ParseMavenDependency(dep)
		if err != nil {
			return nil, err
		}
		dependencies = append(dependencies, dependency)
	}
	return dependencies, nil
}

func MavenModules(connection schema.JVMPackagesConnection) ([]*reposource.MavenModule, error) {
	isAdded := make(map[string]bool)
	modules := []*reposource.MavenModule{}
	dependencies, err := MavenDependencies(connection)
	if err != nil {
		return nil, err
	}
	for _, dep := range dependencies {
		if key := dep.PackageSyntax(); !isAdded[key] {
			modules = append(modules, dep.MavenModule)
			isAdded[key] = true
		}
	}
	return modules, nil
}
