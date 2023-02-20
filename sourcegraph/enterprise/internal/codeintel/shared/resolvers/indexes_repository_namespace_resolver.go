package sharedresolvers

import (
	autoindexingShared "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/autoindexing/shared"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared/types"
	resolverstubs "github.com/sourcegraph/sourcegraph/internal/codeintel/resolvers"
)

type lsifIndexesWithRepositoryNamespaceResolver struct {
	indexesSummary autoindexingShared.IndexesWithRepositoryNamespace
	indexResolvers []resolverstubs.LSIFIndexResolver
}

func NewLSIFIndexesWithRepositoryNamespaceResolver(indexesSummary autoindexingShared.IndexesWithRepositoryNamespace, indexResolvers []resolverstubs.LSIFIndexResolver) resolverstubs.LSIFIndexesWithRepositoryNamespaceResolver {
	return &lsifIndexesWithRepositoryNamespaceResolver{
		indexesSummary: indexesSummary,
		indexResolvers: indexResolvers,
	}
}

func (r *lsifIndexesWithRepositoryNamespaceResolver) Root() string {
	return r.indexesSummary.Root
}

func (r *lsifIndexesWithRepositoryNamespaceResolver) Indexer() resolverstubs.CodeIntelIndexerResolver {
	return types.NewCodeIntelIndexerResolver(r.indexesSummary.Indexer)
}

func (r *lsifIndexesWithRepositoryNamespaceResolver) Indexes() []resolverstubs.LSIFIndexResolver {
	return r.indexResolvers
}
