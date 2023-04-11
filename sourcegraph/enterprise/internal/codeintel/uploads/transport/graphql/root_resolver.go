package graphql

import (
	sharedresolvers "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared/resolvers"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared/resolvers/gitresolvers"
	resolverstubs "github.com/sourcegraph/sourcegraph/internal/codeintel/resolvers"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type rootResolver struct {
	uploadSvc                   UploadsService
	autoindexSvc                AutoIndexingService
	siteAdminChecker            sharedresolvers.SiteAdminChecker
	uploadLoaderFactory         UploadLoaderFactory
	indexLoaderFactory          IndexLoaderFactory
	locationResolverFactory     *gitresolvers.CachedLocationResolverFactory
	preciseIndexResolverFactory *PreciseIndexResolverFactory
	operations                  *operations
}

func NewRootResolver(
	observationCtx *observation.Context,
	uploadSvc UploadsService,
	autoindexSvc AutoIndexingService,
	siteAdminChecker sharedresolvers.SiteAdminChecker,
	uploadLoaderFactory UploadLoaderFactory,
	indexLoaderFactory IndexLoaderFactory,
	locationResolverFactory *gitresolvers.CachedLocationResolverFactory,
	preciseIndexResolverFactory *PreciseIndexResolverFactory,
) resolverstubs.UploadsServiceResolver {
	return &rootResolver{
		uploadSvc:                   uploadSvc,
		autoindexSvc:                autoindexSvc,
		siteAdminChecker:            siteAdminChecker,
		uploadLoaderFactory:         uploadLoaderFactory,
		indexLoaderFactory:          indexLoaderFactory,
		locationResolverFactory:     locationResolverFactory,
		preciseIndexResolverFactory: preciseIndexResolverFactory,
		operations:                  newOperations(observationCtx),
	}
}
