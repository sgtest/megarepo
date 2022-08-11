package graphql

import (
	"context"

	gql "github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
)

type CodeIntelligenceRangeResolver struct {
	r                AdjustedCodeIntelligenceRange
	locationResolver *CachedLocationResolver
}

func (r *CodeIntelligenceRangeResolver) Range(ctx context.Context) (gql.RangeResolver, error) {
	return gql.NewRangeResolver(convertRange(r.r.Range)), nil
}

func (r *CodeIntelligenceRangeResolver) Definitions(ctx context.Context) (gql.LocationConnectionResolver, error) {
	return NewLocationConnectionResolver(r.r.Definitions, nil, r.locationResolver), nil
}

func (r *CodeIntelligenceRangeResolver) References(ctx context.Context) (gql.LocationConnectionResolver, error) {
	return NewLocationConnectionResolver(r.r.References, nil, r.locationResolver), nil
}

func (r *CodeIntelligenceRangeResolver) Implementations(ctx context.Context) (gql.LocationConnectionResolver, error) {
	return NewLocationConnectionResolver(r.r.Implementations, nil, r.locationResolver), nil
}

func (r *CodeIntelligenceRangeResolver) Hover(ctx context.Context) (gql.HoverResolver, error) {
	return NewHoverResolver(r.r.HoverText, convertRange(r.r.Range)), nil
}
