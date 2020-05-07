package graphqlbackend

import (
	"context"

	graphql "github.com/graph-gophers/graphql-go"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/schema"
)

type versionContextResolver struct {
	vc *schema.VersionContext
}

func (v *versionContextResolver) ID() graphql.ID {
	return graphql.ID(v.vc.Name)
}

func (v *versionContextResolver) Name() string {
	return v.vc.Name
}

func (v *versionContextResolver) Description() string {
	return v.vc.Description
}

func NewVersionContextResolver(vc *schema.VersionContext) *versionContextResolver {
	return &versionContextResolver{
		vc: vc,
	}
}

func (r *schemaResolver) VersionContexts(ctx context.Context) ([]*versionContextResolver, error) {
	var versionContexts []*versionContextResolver

	for _, vc := range conf.Get().ExperimentalFeatures.VersionContexts {
		versionContexts = append(versionContexts, NewVersionContextResolver(vc))
	}

	return versionContexts, nil
}
