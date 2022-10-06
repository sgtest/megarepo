package graphqlbackend

import (
	"context"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/auth"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func (r *schemaResolver) Organizations(args *struct {
	graphqlutil.ConnectionArgs
	Query *string
}) *orgConnectionResolver {
	var opt database.OrgsListOptions
	if args.Query != nil {
		opt.Query = *args.Query
	}
	args.ConnectionArgs.Set(&opt.LimitOffset)
	return &orgConnectionResolver{db: r.db, opt: opt}
}

type orgConnectionResolver struct {
	db  database.DB
	opt database.OrgsListOptions
}

func (r *orgConnectionResolver) Nodes(ctx context.Context) ([]*OrgResolver, error) {
	// 🚨 SECURITY: Not allowed on Cloud.
	if envvar.SourcegraphDotComMode() {
		return nil, errors.New("listing organizations is not allowed")
	}

	// 🚨 SECURITY: Only site admins can list organisations.
	if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}

	orgs, err := r.db.Orgs().List(ctx, &r.opt)
	if err != nil {
		return nil, err
	}

	var l []*OrgResolver
	for _, org := range orgs {
		l = append(l, &OrgResolver{
			db:  r.db,
			org: org,
		})
	}
	return l, nil
}

func (r *orgConnectionResolver) TotalCount(ctx context.Context) (int32, error) {
	// 🚨 SECURITY: Only site admins can count organisations.
	if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return 0, err
	}

	count, err := r.db.Orgs().Count(ctx, r.opt)
	return int32(count), err
}

type orgConnectionStaticResolver struct {
	nodes []*OrgResolver
}

func (r *orgConnectionStaticResolver) Nodes() []*OrgResolver { return r.nodes }
func (r *orgConnectionStaticResolver) TotalCount() int32     { return int32(len(r.nodes)) }
func (r *orgConnectionStaticResolver) PageInfo() *graphqlutil.PageInfo {
	return graphqlutil.HasNextPage(false)
}
