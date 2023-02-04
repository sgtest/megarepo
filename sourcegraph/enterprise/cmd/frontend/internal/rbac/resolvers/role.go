package resolvers

import (
	"context"

	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"

	gql "github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/auth"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gqlutil"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

type roleResolver struct {
	db   database.DB
	role *types.Role
}

var _ gql.RoleResolver = &roleResolver{}

const roleIDKind = "Role"

func marshalRoleID(id int32) graphql.ID { return relay.MarshalID(roleIDKind, id) }

func unmarshalRoleID(id graphql.ID) (roleID int32, err error) {
	err = relay.UnmarshalSpec(id, &roleID)
	return
}

func (r *roleResolver) ID() graphql.ID {
	return marshalRoleID(r.role.ID)
}

func (r *roleResolver) Name() string {
	return r.role.Name
}

func (r *roleResolver) System() bool {
	return r.role.System
}

func (r *roleResolver) Permissions(ctx context.Context, args *gql.ListPermissionArgs) (*graphqlutil.ConnectionResolver[gql.PermissionResolver], error) {
	// 🚨 SECURITY: Only viewable by site admins.
	if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}

	rid := marshalRoleID(r.role.ID)
	args.Role = &rid
	args.User = nil
	connectionStore := &permisionConnectionStore{
		db:     r.db,
		roleID: r.role.ID,
	}
	return graphqlutil.NewConnectionResolver[gql.PermissionResolver](
		connectionStore,
		&args.ConnectionResolverArgs,
		nil,
	)
}

func (r *roleResolver) CreatedAt() gqlutil.DateTime {
	return gqlutil.DateTime{Time: r.role.CreatedAt}
}
