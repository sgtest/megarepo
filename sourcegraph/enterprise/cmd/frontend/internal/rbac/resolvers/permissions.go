package resolvers

import (
	"context"

	"github.com/graph-gophers/graphql-go"

	"github.com/sourcegraph/sourcegraph/lib/errors"

	gql "github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/auth"
	"github.com/sourcegraph/sourcegraph/internal/database"
)

func (r *Resolver) permissionByID(ctx context.Context, id graphql.ID) (gql.PermissionResolver, error) {
	// 🚨 SECURITY: Only site admins can query role permissions or all permissions.
	if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}

	permissionID, err := unmarshalPermissionID(id)
	if err != nil {
		return nil, err
	}

	if permissionID == 0 {
		return nil, ErrIDIsZero{}
	}

	permission, err := r.db.Permissions().GetByID(ctx, database.GetPermissionOpts{
		ID: permissionID,
	})
	if err != nil {
		return nil, err
	}
	return &permissionResolver{permission: permission}, nil
}

func (r *Resolver) Permissions(ctx context.Context, args *gql.ListPermissionArgs) (*graphqlutil.ConnectionResolver[gql.PermissionResolver], error) {
	connectionStore := permisionConnectionStore{
		db: r.db,
	}

	if args.User != nil {
		userID, err := gql.UnmarshalUserID(*args.User)
		if err != nil {
			return nil, err
		}

		if userID == 0 {
			return nil, errors.New("invalid user id provided")
		}

		// 🚨 SECURITY: Only viewable for self or by site admins.
		if err := auth.CheckSiteAdminOrSameUser(ctx, r.db, userID); err != nil {
			return nil, err
		}

		connectionStore.userID = userID
	} else if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil { // 🚨 SECURITY: Only site admins can query role permissions or all permissions.
		return nil, err
	}

	if args.Role != nil {
		roleID, err := unmarshalRoleID(*args.Role)
		if err != nil {
			return nil, err
		}

		if roleID == 0 {
			return nil, errors.New("invalid role id provided")
		}

		connectionStore.roleID = roleID
	}

	return graphqlutil.NewConnectionResolver[gql.PermissionResolver](
		&connectionStore,
		&args.ConnectionResolverArgs,
		&graphqlutil.ConnectionResolverOptions{
			OrderBy: database.OrderBy{
				{Field: "permissions.id"},
			},
			// We want to be able to retrieve all permissions belonging to a user at once on startup,
			// hence we are removing pagination from this resolver. Ideally, we shouldn't have performance
			// issues since permissions aren't created by users, and it'd take a while before we start having
			// thousands of permissions in a database, so we are able to get by with disabling pagination
			// for the permissions resolver.
			AllowNoLimit: true,
		},
	)
}
