package graphqlbackend

import (
	"context"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"sync"

	graphql "github.com/graph-gophers/graphql-go"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
)

func (r *schemaResolver) AddExternalService(ctx context.Context, args *struct {
	Kind        string
	DisplayName string
	Config      string
}) (*externalServiceResolver, error) {
	// 🚨 SECURITY: Only site admins may add external services.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}
	externalService := &types.ExternalService{
		Kind:        args.Kind,
		DisplayName: args.DisplayName,
		Config:      args.Config,
	}
	err := db.ExternalServices.Create(ctx, externalService)
	return &externalServiceResolver{externalService: externalService}, err
}

func (*schemaResolver) UpdateExternalService(ctx context.Context, args *struct {
	ID          graphql.ID
	DisplayName *string
	Config      *string
}) (*externalServiceResolver, error) {
	externalServiceID, err := unmarshalExternalServiceID(args.ID)
	if err != nil {
		return nil, err
	}

	// 🚨 SECURITY: Only site admins are allowed to update the user.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	update := &db.ExternalServiceUpdate{
		DisplayName: args.DisplayName,
		Config:      args.Config,
	}
	if err := db.ExternalServices.Update(ctx, externalServiceID, update); err != nil {
		return nil, err
	}

	externalService, err := db.ExternalServices.GetByID(ctx, externalServiceID)
	if err != nil {
		return nil, err
	}
	return &externalServiceResolver{externalService: externalService}, nil
}

func (r *schemaResolver) ExternalServices(ctx context.Context, args *struct {
	graphqlutil.ConnectionArgs
}) (*externalServiceConnectionResolver, error) {
	// 🚨 SECURITY: Only site admins may read external services (they have secrets).
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}
	var opt db.ExternalServicesListOptions
	args.ConnectionArgs.Set(&opt.LimitOffset)
	return &externalServiceConnectionResolver{opt: opt}, nil
}

type externalServiceConnectionResolver struct {
	opt db.ExternalServicesListOptions

	// cache results because they are used by multiple fields
	once             sync.Once
	externalServices []*types.ExternalService
	err              error
}

func (r *externalServiceConnectionResolver) compute(ctx context.Context) ([]*types.ExternalService, error) {
	r.once.Do(func() {
		r.externalServices, r.err = db.ExternalServices.List(ctx, r.opt)
	})
	return r.externalServices, r.err
}

func (r *externalServiceConnectionResolver) Nodes(ctx context.Context) ([]*externalServiceResolver, error) {
	externalServices, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}
	resolvers := make([]*externalServiceResolver, 0, len(externalServices))
	for _, externalService := range externalServices {
		resolvers = append(resolvers, &externalServiceResolver{externalService: externalService})
	}
	return resolvers, nil
}

func (r *externalServiceConnectionResolver) TotalCount(ctx context.Context) (int32, error) {
	count, err := db.ExternalServices.Count(ctx, r.opt)
	return int32(count), err
}

func (r *externalServiceConnectionResolver) PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error) {
	externalServices, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}
	return graphqlutil.HasNextPage(r.opt.LimitOffset != nil && len(externalServices) >= r.opt.Limit), nil
}
