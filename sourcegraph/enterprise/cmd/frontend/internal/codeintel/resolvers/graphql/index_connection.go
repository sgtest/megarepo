package graphql

import (
	"context"

	"github.com/opentracing/opentracing-go/log"

	gql "github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/codeintel/resolvers"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/policies"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type IndexConnectionResolver struct {
	db               database.DB
	gitserver        policies.GitserverClient
	resolver         resolvers.Resolver
	indexesResolver  *resolvers.IndexesResolver
	prefetcher       *Prefetcher
	locationResolver *CachedLocationResolver
	errTracer        *observation.ErrCollector
}

func NewIndexConnectionResolver(db database.DB, gitserver policies.GitserverClient, resolver resolvers.Resolver, indexesResolver *resolvers.IndexesResolver, prefetcher *Prefetcher, locationResolver *CachedLocationResolver, errTracer *observation.ErrCollector) gql.LSIFIndexConnectionResolver {
	return &IndexConnectionResolver{
		db:               db,
		resolver:         resolver,
		indexesResolver:  indexesResolver,
		prefetcher:       prefetcher,
		locationResolver: locationResolver,
		errTracer:        errTracer,
	}
}

func (r *IndexConnectionResolver) Nodes(ctx context.Context) ([]gql.LSIFIndexResolver, error) {
	if err := r.indexesResolver.Resolve(ctx); err != nil {
		return nil, err
	}

	resolvers := make([]gql.LSIFIndexResolver, 0, len(r.indexesResolver.Indexes))
	for i := range r.indexesResolver.Indexes {
		resolvers = append(resolvers, NewIndexResolver(r.db, r.gitserver, r.resolver, r.indexesResolver.Indexes[i], r.prefetcher, r.locationResolver, r.errTracer))
	}
	return resolvers, nil
}

func (r *IndexConnectionResolver) TotalCount(ctx context.Context) (_ *int32, err error) {
	defer r.errTracer.Collect(&err, log.String("indexConnectionResolver.field", "totalCount"))

	if err := r.indexesResolver.Resolve(ctx); err != nil {
		return nil, err
	}
	return toInt32(&r.indexesResolver.TotalCount), nil
}

func (r *IndexConnectionResolver) PageInfo(ctx context.Context) (_ *graphqlutil.PageInfo, err error) {
	defer r.errTracer.Collect(&err, log.String("indexConnectionResolver.field", "pageInfo"))

	if err := r.indexesResolver.Resolve(ctx); err != nil {
		return nil, err
	}
	return graphqlutil.EncodeIntCursor(toInt32(r.indexesResolver.NextOffset)), nil
}
