package resolvers

import (
	"context"
	"fmt"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/log/logtest"
	"github.com/stretchr/testify/assert"

	gql "github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/rbac/resolvers/apitest"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/auth"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestRoleConnectionResolver(t *testing.T) {
	logger := logtest.Scoped(t)
	if testing.Short() {
		t.Skip()
	}

	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	userID := createTestUser(t, db, false).ID
	userCtx := actor.WithActor(ctx, actor.FromUser(userID))

	adminID := createTestUser(t, db, true).ID
	adminCtx := actor.WithActor(ctx, actor.FromUser(adminID))

	s, err := newSchema(db, &Resolver{logger: logger, db: db})
	if err != nil {
		t.Fatal(err)
	}

	// All sourcegraph instances are seeded with two system roles at migration,
	// so we take those into account when querying roles.
	siteAdminRole, err := db.Roles().Get(ctx, database.GetRoleOpts{
		Name: string(types.SiteAdministratorSystemRole),
	})
	assert.NoError(t, err)

	userRole, err := db.Roles().Get(ctx, database.GetRoleOpts{
		Name: string(types.UserSystemRole),
	})
	assert.NoError(t, err)

	r, err := db.Roles().Create(ctx, "TEST-ROLE", false)
	assert.NoError(t, err)

	t.Run("as non site-administrator", func(t *testing.T) {
		input := map[string]any{"first": 1}
		var response struct{ Permissions apitest.PermissionConnection }
		errs := apitest.Exec(userCtx, t, s, input, &response, queryPermissionConnection)

		assert.Len(t, errs, 1)
		assert.Equal(t, errs[0].Message, "must be site admin")
	})

	t.Run("as site-administrator", func(t *testing.T) {
		want := []apitest.Role{
			{
				ID: string(marshalRoleID(r.ID)),
			},
			{
				ID: string(marshalRoleID(siteAdminRole.ID)),
			},
			{
				ID: string(marshalRoleID(userRole.ID)),
			},
		}

		tests := []struct {
			firstParam          int
			wantHasNextPage     bool
			wantHasPreviousPage bool
			wantTotalCount      int
			wantNodes           []apitest.Role
		}{
			{firstParam: 1, wantHasNextPage: true, wantHasPreviousPage: false, wantTotalCount: 3, wantNodes: want[:1]},
			{firstParam: 2, wantHasNextPage: true, wantHasPreviousPage: false, wantTotalCount: 3, wantNodes: want[:2]},
			{firstParam: 3, wantHasNextPage: false, wantHasPreviousPage: false, wantTotalCount: 3, wantNodes: want},
			{firstParam: 4, wantHasNextPage: false, wantHasPreviousPage: false, wantTotalCount: 3, wantNodes: want},
		}

		for _, tc := range tests {
			t.Run(fmt.Sprintf("first=%d", tc.firstParam), func(t *testing.T) {
				input := map[string]any{"first": int64(tc.firstParam)}
				var response struct{ Roles apitest.RoleConnection }
				apitest.MustExec(adminCtx, t, s, input, &response, queryRoleConnection)

				wantConnection := apitest.RoleConnection{
					TotalCount: tc.wantTotalCount,
					PageInfo: apitest.PageInfo{
						HasNextPage:     tc.wantHasNextPage,
						HasPreviousPage: tc.wantHasPreviousPage,
					},
					Nodes: tc.wantNodes,
				}

				if diff := cmp.Diff(wantConnection, response.Roles); diff != "" {
					t.Fatalf("wrong roles response (-want +got):\n%s", diff)
				}
			})
		}
	})
}

const queryRoleConnection = `
query($first: Int!) {
	roles(first: $first) {
		totalCount
		pageInfo {
			hasNextPage
			hasPreviousPage
		}
		nodes {
			id
		}
	}
}
`

func TestUserRoleListing(t *testing.T) {
	logger := logtest.Scoped(t)
	if testing.Short() {
		t.Skip()
	}

	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	userID := createTestUser(t, db, false).ID
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

	adminUserID := createTestUser(t, db, true).ID
	adminActorCtx := actor.WithActor(ctx, actor.FromUser(adminUserID))

	r := &Resolver{logger: logger, db: db}
	s, err := newSchema(db, r)
	assert.NoError(t, err)

	// create a new role
	role, err := db.Roles().Create(ctx, "TEST-ROLE", false)
	assert.NoError(t, err)

	err = db.UserRoles().Assign(ctx, database.AssignUserRoleOpts{
		RoleID: role.ID,
		UserID: userID,
	})
	assert.NoError(t, err)

	t.Run("listing a user's roles (same user)", func(t *testing.T) {
		userAPIID := string(gql.MarshalUserID(userID))
		input := map[string]any{"node": userAPIID}

		want := apitest.User{
			ID: userAPIID,
			Roles: apitest.RoleConnection{
				TotalCount: 1,
				Nodes: []apitest.Role{
					{
						ID: string(marshalRoleID(role.ID)),
					},
				},
			},
		}

		var response struct{ Node apitest.User }
		apitest.MustExec(actorCtx, t, s, input, &response, listUserRoles)

		if diff := cmp.Diff(want, response.Node); diff != "" {
			t.Fatalf("wrong role response (-want +got):\n%s", diff)
		}
	})

	t.Run("listing a user's roles (site admin)", func(t *testing.T) {
		userAPIID := string(gql.MarshalUserID(userID))
		input := map[string]any{"node": userAPIID}

		want := apitest.User{
			ID: userAPIID,
			Roles: apitest.RoleConnection{
				TotalCount: 1,
				Nodes: []apitest.Role{
					{
						ID: string(marshalRoleID(role.ID)),
					},
				},
			},
		}

		var response struct{ Node apitest.User }
		apitest.MustExec(adminActorCtx, t, s, input, &response, listUserRoles)

		if diff := cmp.Diff(want, response.Node); diff != "" {
			t.Fatalf("wrong roles response (-want +got):\n%s", diff)
		}
	})

	t.Run("non site-admin listing another user's roles", func(t *testing.T) {
		userAPIID := string(gql.MarshalUserID(adminUserID))
		input := map[string]any{"node": userAPIID}

		var response struct{}
		errs := apitest.Exec(actorCtx, t, s, input, &response, listUserRoles)
		assert.Len(t, errs, 1)
		assert.Equal(t, auth.ErrMustBeSiteAdminOrSameUser.Error(), errs[0].Message)
	})
}

const listUserRoles = `
query ($node: ID!) {
	node(id: $node) {
		... on User {
			id
			roles(first: 50) {
				totalCount
				nodes {
					id
				}
			}
		}
	}
}
`

func TestDeleteRole(t *testing.T) {
	logger := logtest.Scoped(t)
	if testing.Short() {
		t.Skip()
	}

	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	userID := createTestUser(t, db, false).ID
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

	adminUserID := createTestUser(t, db, true).ID
	adminActorCtx := actor.WithActor(ctx, actor.FromUser(adminUserID))

	r := &Resolver{logger: logger, db: db}
	s, err := newSchema(db, r)
	assert.NoError(t, err)

	// create a new role
	role, err := db.Roles().Create(ctx, "TEST-ROLE", false)
	assert.NoError(t, err)

	t.Run("as non site-admin", func(t *testing.T) {
		roleID := string(marshalRoleID(role.ID))
		input := map[string]any{"role": roleID}

		var response struct{ DeleteRole apitest.EmptyResponse }
		errs := apitest.Exec(actorCtx, t, s, input, &response, deleteRoleMutation)

		if len(errs) != 1 {
			t.Fatalf("expected single errors, but got %d", len(errs))
		}
		if have, want := errs[0].Message, "must be site admin"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("as site-admin", func(t *testing.T) {
		roleID := string(marshalRoleID(role.ID))
		input := map[string]any{"role": roleID}

		var response struct{ DeleteRole apitest.EmptyResponse }

		// First time it should work, because the role exists
		apitest.MustExec(adminActorCtx, t, s, input, &response, deleteRoleMutation)

		// Second time it should fail
		errs := apitest.Exec(adminActorCtx, t, s, input, &response, deleteRoleMutation)

		if len(errs) != 1 {
			t.Fatalf("expected a single error, but got %d", len(errs))
		}
		if have, want := errs[0].Message, fmt.Sprintf("failed to delete role: role with ID %d not found", role.ID); have != want {
			t.Fatalf("wrong error code. want=%q, have=%q", want, have)
		}
	})
}

const deleteRoleMutation = `
mutation DeleteRole($role: ID!) {
	deleteRole(role: $role) {
		alwaysNil
	}
}
`

func TestCreateRole(t *testing.T) {
	logger := logtest.Scoped(t)
	if testing.Short() {
		t.Skip()
	}

	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	userID := createTestUser(t, db, false).ID
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

	adminUserID := createTestUser(t, db, true).ID
	adminActorCtx := actor.WithActor(ctx, actor.FromUser(adminUserID))

	r := &Resolver{logger: logger, db: db}
	s, err := newSchema(db, r)
	assert.NoError(t, err)

	t.Run("as non site-admin", func(t *testing.T) {
		input := map[string]any{"name": "TEST-ROLE"}

		var response struct{ CreateRole apitest.Role }
		errs := apitest.Exec(actorCtx, t, s, input, &response, createRoleMutation)

		if len(errs) != 1 {
			t.Fatalf("expected a single error, but got %d", len(errs))
		}
		if have, want := errs[0].Message, "must be site admin"; have != want {
			t.Fatalf("wrong error. want=%q, have=%q", want, have)
		}
	})

	t.Run("as site-admin", func(t *testing.T) {
		input := map[string]any{"name": "TEST-ROLE"}

		var response struct{ CreateRole apitest.Role }
		// First time it should work, because the role exists
		apitest.MustExec(adminActorCtx, t, s, input, &response, createRoleMutation)

		// Second time it should fail because role names must be unique
		errs := apitest.Exec(adminActorCtx, t, s, input, &response, createRoleMutation)
		if len(errs) != 1 {
			t.Fatalf("expected a single error, but got %d", len(errs))
		}
		if have, want := errs[0].Message, "cannot create role: err_name_exists"; have != want {
			t.Fatalf("wrong error code. want=%q, have=%q", want, have)
		}
	})
}

const createRoleMutation = `
mutation CreateRole($name: String!) {
	createRole(name: $name) {
		id
		name
		system
	}
}
`
