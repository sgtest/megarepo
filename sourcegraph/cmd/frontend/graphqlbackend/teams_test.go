package graphqlbackend

import (
	"context"
	"encoding/json"
	"fmt"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/graph-gophers/graphql-go/relay"

	gqlerrors "github.com/graph-gophers/graphql-go/errors"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/fakedb"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func userCtx(userID int32) context.Context {
	a := &actor.Actor{
		UID: userID,
	}
	return actor.WithActor(context.Background(), a)
}

func TestTeamNode(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
		t.Fatalf("failed to create fake team: %s", err)
	}
	team, err := fs.TeamStore.GetTeamByName(ctx, "team")
	if err != nil {
		t.Fatalf("failed to get fake team: %s", err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `query TeamByID($id: ID!){
			node(id: $id) {
				__typename
				... on Team {
				  name
				}
			}
		}`,
		ExpectedResult: `{
			"node": {
				"__typename": "Team",
				"name": "team"
			}
		}`,
		Variables: map[string]any{
			"id": string(relay.MarshalID("Team", team.ID)),
		},
	})
}

func TestTeamNodeURL(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	team := &types.Team{
		Name: "team-刺身", // team-sashimi
	}
	if err := fs.TeamStore.CreateTeam(ctx, team); err != nil {
		t.Fatalf("failed to create fake team: %s", err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `{
			team(name: "team-刺身") {
				... on Team {
					url
				}
			}
		}`,
		ExpectedResult: `{
			"team": {
				"url": "/teams/team-%E5%88%BA%E8%BA%AB"
			}
		}`,
	})
}

func TestTeamNodeSiteAdminCanAdminister(t *testing.T) {
	for _, isAdmin := range []bool{true, false} {
		t.Run(fmt.Sprintf("viewer is admin = %v", isAdmin), func(t *testing.T) {
			fs := fakedb.New()
			db := database.NewMockDB()
			fs.Wire(db)
			ctx := userCtx(fs.AddUser(types.User{SiteAdmin: isAdmin}))
			if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
				t.Fatalf("failed to create fake team: %s", err)
			}
			team, err := fs.TeamStore.GetTeamByName(ctx, "team")
			if err != nil {
				t.Fatalf("failed to get fake team: %s", err)
			}
			RunTest(t, &Test{
				Schema:  mustParseGraphQLSchema(t, db),
				Context: ctx,
				Query: `query TeamByID($id: ID!){
					node(id: $id) {
						__typename
						... on Team {
							viewerCanAdminister
						}
					}
				}`,
				ExpectedResult: fmt.Sprintf(`{
					"node": {
						"__typename": "Team",
						"viewerCanAdminister": %v
					}
				}`, isAdmin),
				Variables: map[string]any{
					"id": string(relay.MarshalID("Team", team.ID)),
				},
			})
		})
	}
}

func TestCreateTeamBare(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation CreateTeam($name: String!) {
			createTeam(name: $name) {
				name
			}
		}`,
		ExpectedResult: `{
			"createTeam": {
				"name": "team-name-testing"
			}
		}`,
		Variables: map[string]any{
			"name": "team-name-testing",
		},
	})
	expected := &types.Team{
		ID:        1,
		Name:      "team-name-testing",
		CreatorID: actor.FromContext(ctx).UID,
	}
	if diff := cmp.Diff([]*types.Team{expected}, fs.ListAllTeams()); diff != "" {
		t.Errorf("unexpected teams in fake database (-want,+got):\n%s", diff)
	}
}

func TestCreateTeamDisplayName(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation CreateTeam($name: String!, $displayName: String!) {
			createTeam(name: $name, displayName: $displayName) {
				displayName
			}
		}`,
		ExpectedResult: `{
			"createTeam": {
				"displayName": "Team Display Name"
			}
		}`,
		Variables: map[string]any{
			"name":        "team-name-testing",
			"displayName": "Team Display Name",
		},
	})
}

func TestCreateTeamReadOnlyDefault(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation CreateTeam($name: String!) {
			createTeam(name: $name) {
				readonly
			}
		}`,
		ExpectedResult: `{
			"createTeam": {
				"readonly": false
			}
		}`,
		Variables: map[string]any{
			"name": "team-name-testing",
		},
	})
}

func TestCreateTeamReadOnlyTrue(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation CreateTeam($name: String!, $readonly: Boolean!) {
			createTeam(name: $name, readonly: $readonly) {
				readonly
			}
		}`,
		ExpectedResult: `{
			"createTeam": {
				"readonly": true
			}
		}`,
		Variables: map[string]any{
			"name":     "team-name-testing",
			"readonly": true,
		},
	})
}

func TestCreateTeamParentByID(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	err := fs.TeamStore.CreateTeam(ctx, &types.Team{
		Name: "team-name-parent",
	})
	if err != nil {
		t.Fatal(err)
	}
	parentTeam, err := fs.TeamStore.GetTeamByName(ctx, "team-name-parent")
	if err != nil {
		t.Fatal(err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation CreateTeam($name: String!, $parentTeamID: ID!) {
			createTeam(name: $name, parentTeam: $parentTeamID) {
				parentTeam {
					name
				}
			}
		}`,
		ExpectedResult: `{
			"createTeam": {
				"parentTeam": {
					"name": "team-name-parent"
				}
			}
		}`,
		Variables: map[string]any{
			"name":         "team-name-testing",
			"parentTeamID": string(relay.MarshalID("Team", parentTeam.ID)),
		},
	})
}

func TestCreateTeamParentByName(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	parentTeam := types.Team{Name: "team-name-parent"}
	if err := fs.TeamStore.CreateTeam(context.Background(), &parentTeam); err != nil {
		t.Fatal(err)
	}
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation CreateTeam($name: String!, $parentTeamName: String!) {
			createTeam(name: $name, parentTeamName: $parentTeamName) {
				parentTeam {
					name
				}
			}
		}`,
		ExpectedResult: `{
			"createTeam": {
				"parentTeam": {
					"name": "team-name-parent"
				}
			}
		}`,
		Variables: map[string]any{
			"name":           "team-name-testing",
			"parentTeamName": "team-name-parent",
		},
	})
}

func TestUpdateTeamByID(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{
		Name:        "team-name-testing",
		DisplayName: "Display Name",
	}); err != nil {
		t.Fatalf("failed to create a team: %s", err)
	}
	team, err := fs.TeamStore.GetTeamByName(ctx, "team-name-testing")
	if err != nil {
		t.Fatalf("failed to get fake team: %s", err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation UpdateTeam($id: ID!, $newDisplayName: String!) {
			updateTeam(id: $id, displayName: $newDisplayName) {
				displayName
			}
		}`,
		ExpectedResult: `{
			"updateTeam": {
				"displayName": "Updated Display Name"
			}
		}`,
		Variables: map[string]any{
			"id":             string(relay.MarshalID("Team", team.ID)),
			"newDisplayName": "Updated Display Name",
		},
	})
	wantTeams := []*types.Team{
		{
			ID:          1,
			Name:        "team-name-testing",
			DisplayName: "Updated Display Name",
		},
	}
	if diff := cmp.Diff(wantTeams, fs.ListAllTeams()); diff != "" {
		t.Errorf("fake teams storage (-want,+got):\n%s", diff)
	}
}

func TestUpdateTeamByName(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{
		Name:        "team-name-testing",
		DisplayName: "Display Name",
	}); err != nil {
		t.Fatalf("failed to create a team: %s", err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation UpdateTeam($name: String!, $newDisplayName: String!) {
			updateTeam(name: $name, displayName: $newDisplayName) {
				displayName
			}
		}`,
		ExpectedResult: `{
			"updateTeam": {
				"displayName": "Updated Display Name"
			}
		}`,
		Variables: map[string]any{
			"name":           "team-name-testing",
			"newDisplayName": "Updated Display Name",
		},
	})
	wantTeams := []*types.Team{
		{
			ID:          1,
			Name:        "team-name-testing",
			DisplayName: "Updated Display Name",
		},
	}
	if diff := cmp.Diff(wantTeams, fs.ListAllTeams()); diff != "" {
		t.Errorf("fake teams storage (-want,+got):\n%s", diff)
	}
}

func TestUpdateTeamErrorBothNameAndID(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{
		Name:        "team-name-testing",
		DisplayName: "Display Name",
	}); err != nil {
		t.Fatalf("failed to create a team: %s", err)
	}
	team, err := fs.TeamStore.GetTeamByName(ctx, "team-name-testing")
	if err != nil {
		t.Fatalf("failed to get fake team: %s", err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation UpdateTeam($name: String!, $id: ID!, $newDisplayName: String!) {
			updateTeam(name: $name, id: $id, displayName: $newDisplayName) {
				displayName
			}
		}`,
		ExpectedResult: "null",
		ExpectedErrors: []*gqlerrors.QueryError{
			{
				Message: "team to update is identified by either id or name, but both were specified",
				Path:    []any{"updateTeam"},
			},
		},
		Variables: map[string]any{
			"id":             string(relay.MarshalID("Team", team.ID)),
			"name":           "team-name-testing",
			"newDisplayName": "Updated Display Name",
		},
	})
}

func TestUpdateParentByID(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "parent"}); err != nil {
		t.Fatalf("failed to create parent team: %s", err)
	}
	parentTeam, err := fs.TeamStore.GetTeamByName(ctx, "parent")
	if err != nil {
		t.Fatalf("failed to fetch fake parent team: %s", err)
	}
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
		t.Fatalf("failed to create a team: %s", err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation UpdateTeam($name: String!, $newParentID: ID!) {
			updateTeam(name: $name, parentTeam: $newParentID) {
				parentTeam {
					name
				}
			}
		}`,
		ExpectedResult: `{
			"updateTeam": {
				"parentTeam": {
					"name": "parent"
				}
			}
		}`,
		Variables: map[string]any{
			"name":        "team",
			"newParentID": string(relay.MarshalID("Team", parentTeam.ID)),
		},
	})
}

func TestUpdateParentByName(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "parent"}); err != nil {
		t.Fatalf("failed to create parent team: %s", err)
	}
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
		t.Fatalf("failed to create a team: %s", err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation UpdateTeam($name: String!, $newParentName: String!) {
			updateTeam(name: $name, parentTeamName: $newParentName) {
				parentTeam {
					name
				}
			}
		}`,
		ExpectedResult: `{
			"updateTeam": {
				"parentTeam": {
					"name": "parent"
				}
			}
		}`,
		Variables: map[string]any{
			"name":          "team",
			"newParentName": "parent",
		},
	})
}

func TestUpdateParentErrorBothNameAndID(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "parent"}); err != nil {
		t.Fatalf("failed to create parent team: %s", err)
	}
	parentTeam, err := fs.TeamStore.GetTeamByName(ctx, "parent")
	if err != nil {
		t.Fatalf("failed to fetch fake parent team: %s", err)
	}
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
		t.Fatalf("failed to create a team: %s", err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation UpdateTeam($name: String!, $newParentID: ID!, $newParentName: String!) {
			updateTeam(name: $name, parentTeam: $newParentID, parentTeamName: $newParentName) {
				parentTeam {
					name
				}
			}
		}`,
		ExpectedResult: "null",
		ExpectedErrors: []*gqlerrors.QueryError{
			{
				Message: "parent team is identified by either id or name, but both were specified",
				Path:    []any{"updateTeam"},
			},
		},
		Variables: map[string]any{
			"name":          "team",
			"newParentID":   string(relay.MarshalID("Team", parentTeam.ID)),
			"newParentName": parentTeam.Name,
		},
	})
}

func TestDeleteTeamByID(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
		t.Fatalf("failed to create a team: %s", err)
	}
	team, err := fs.TeamStore.GetTeamByName(ctx, "team")
	if err != nil {
		t.Fatalf("cannot find fake team: %s", err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation DeleteTeam($id: ID!) {
			deleteTeam(id: $id) {
				alwaysNil
			}
		}`,
		ExpectedResult: `{
			"deleteTeam": {
				"alwaysNil": null
			}
		}`,
		Variables: map[string]any{
			"id": string(relay.MarshalID("Team", team.ID)),
		},
	})
	if diff := cmp.Diff([]*types.Team{}, fs.ListAllTeams()); diff != "" {
		t.Errorf("expected no teams in fake db after deleting, (-want,+got):\n%s", diff)
	}
}

func TestDeleteTeamByName(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
		t.Fatalf("failed to create a team: %s", err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation DeleteTeam($name: String!) {
			deleteTeam(name: $name) {
				alwaysNil
			}
		}`,
		ExpectedResult: `{
			"deleteTeam": {
				"alwaysNil": null
			}
		}`,
		Variables: map[string]any{
			"name": "team",
		},
	})
	if diff := cmp.Diff([]*types.Team{}, fs.ListAllTeams()); diff != "" {
		t.Errorf("expected no teams in fake db after deleting, (-want,+got):\n%s", diff)
	}
}

func TestDeleteTeamErrorBothIDAndNameGiven(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
		t.Fatalf("failed to create a team: %s", err)
	}
	team, err := fs.TeamStore.GetTeamByName(ctx, "team")
	if err != nil {
		t.Fatalf("cannot find fake team: %s", err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation DeleteTeam($id: ID!, $name: String!) {
			deleteTeam(id: $id, name: $name) {
				alwaysNil
			}
		}`,
		ExpectedResult: `{
			"deleteTeam": null
		}`,
		ExpectedErrors: []*gqlerrors.QueryError{
			{
				Message: "team to delete is identified by either id or name, but both were specified",
				Path:    []any{"deleteTeam"},
			},
		},
		Variables: map[string]any{
			"id":   string(relay.MarshalID("Team", team.ID)),
			"name": "team",
		},
	})
}

func TestDeleteTeamNoIdentifierGiven(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation DeleteTeam() {
			deleteTeam() {
				alwaysNil
			}
		}`,
		ExpectedResult: `{
			"deleteTeam": null
		}`,
		ExpectedErrors: []*gqlerrors.QueryError{
			{
				Message: "team to delete is identified by either id or name, but neither was specified",
				Path:    []any{"deleteTeam"},
			},
		},
	})
}

func TestDeleteTeamNotFound(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation DeleteTeam($name: String!) {
			deleteTeam(name: $name) {
				alwaysNil
			}
		}`,
		ExpectedResult: `{
			"deleteTeam": null
		}`,
		ExpectedErrors: []*gqlerrors.QueryError{
			{
				Message: `team name="does-not-exist" not found: team not found: <nil>`,
				Path:    []any{"deleteTeam"},
			},
		},
		Variables: map[string]any{
			"name": "does-not-exist",
		},
	})
}

func TestDeleteTeamUnauthorized(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: false}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
		t.Fatalf("failed to create a team: %s", err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation DeleteTeam($name: String!) {
			deleteTeam(name: $name) {
				alwaysNil
			}
		}`,
		ExpectedResult: `{
			"deleteTeam": null
		}`,
		ExpectedErrors: []*gqlerrors.QueryError{
			{
				Message: "only site admins can delete teams",
				Path:    []any{"deleteTeam"},
			},
		},
		Variables: map[string]any{
			"name": "team",
		},
	})
}

func TestTeamByName(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
		t.Fatalf("failed to create a team: %s", err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `query Team($name: String!) {
			team(name: $name) {
				name
			}
		}`,
		ExpectedResult: `{
			"team": {
				"name": "team"
			}
		}`,
		Variables: map[string]any{
			"name": "team",
		},
	})
}

func TestTeamByNameNotFound(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `query Team($name: String!) {
			team(name: $name) {
				name
			}
		}`,
		ExpectedResult: `{
			"team": null
		}`,
		Variables: map[string]any{
			"name": "does-not-exist",
		},
	})
}

func TestTeamByNameUnauthorized(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: false}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
		t.Fatalf("failed to create a team: %s", err)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `query Team($name: String!) {
			team(name: $name) {
				id
			}
		}`,
		ExpectedResult: `{
			"team": null
		}`,
		ExpectedErrors: []*gqlerrors.QueryError{
			{
				Message: "only site admins can view teams",
				Path:    []any{"team"},
			},
		},
		Variables: map[string]any{
			"name": "team",
		},
	})
}

func TestTeamsPaginated(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	for i := 1; i <= 25; i++ {
		name := fmt.Sprintf("team-%d", i)
		if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: name}); err != nil {
			t.Fatalf("failed to create a team: %s", err)
		}
	}
	var (
		hasNextPage bool = true
		cursor      string
	)
	query := `query Teams($cursor: String!) {
		teams(after: $cursor, first: 10) {
			pageInfo {
				endCursor
				hasNextPage
			}
			nodes {
				name
			}
		}
	}`
	operationName := ""
	var gotNames []string
	for hasNextPage {
		variables := map[string]any{
			"cursor": cursor,
		}
		r := mustParseGraphQLSchema(t, db).Exec(ctx, query, operationName, variables)
		var wantErrors []*gqlerrors.QueryError
		checkErrors(t, wantErrors, r.Errors)
		var result struct {
			Teams *struct {
				PageInfo *struct {
					EndCursor   string
					HasNextPage bool
				}
				Nodes []struct {
					Name string
				}
			}
		}
		if err := json.Unmarshal(r.Data, &result); err != nil {
			t.Fatalf("cannot interpret graphQL query result: %s", err)
		}
		hasNextPage = result.Teams.PageInfo.HasNextPage
		cursor = result.Teams.PageInfo.EndCursor
		for _, node := range result.Teams.Nodes {
			gotNames = append(gotNames, node.Name)
		}
	}
	var wantNames []string
	for _, team := range fs.ListAllTeams() {
		wantNames = append(wantNames, team.Name)
	}
	if diff := cmp.Diff(wantNames, gotNames); diff != "" {
		t.Errorf("unexpected team names (-want,+got):\n%s", diff)
	}
}

// Skip testing DisplayName search as this is the same except the fake behavior.
func TestTeamsNameSearch(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	for _, name := range []string{"hit-1", "Hit-2", "HIT-3", "miss-4", "mIss-5", "MISS-6"} {
		if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: name}); err != nil {
			t.Fatalf("failed to create a team: %s", err)
		}
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `{
			teams(search: "hit") {
				nodes {
					name
				}
			}
		}`,
		ExpectedResult: `{
			"teams": {
				"nodes": [
					{"name": "hit-1"},
					{"name": "Hit-2"},
					{"name": "HIT-3"}
				]
			}
		}`,
	})
}

func TestTeamsCount(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	for i := 1; i <= 25; i++ {
		name := fmt.Sprintf("team-%d", i)
		if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: name}); err != nil {
			t.Fatalf("failed to create a team: %s", err)
		}
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `{
			teams(first: 5) {
				totalCount
				nodes {
					name
				}
			}
		}`,
		ExpectedResult: `{
			"teams": {
				"totalCount": 25,
				"nodes": [
					{"name": "team-1"},
					{"name": "team-2"},
					{"name": "team-3"},
					{"name": "team-4"},
					{"name": "team-5"}
				]
			}
		}`,
	})
}

func TestChildTeams(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "parent"}); err != nil {
		t.Fatalf("failed to create parent team: %s", err)
	}
	parent, err := fs.TeamStore.GetTeamByName(ctx, "parent")
	if err != nil {
		t.Fatalf("cannot fetch parent team: %s", err)
	}
	for i := 1; i <= 5; i++ {
		name := fmt.Sprintf("child-%d", i)
		if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: name, ParentTeamID: parent.ID}); err != nil {
			t.Fatalf("cannot create child team: %s", err)
		}
	}
	for i := 6; i <= 10; i++ {
		name := fmt.Sprintf("not-child-%d", i)
		if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: name}); err != nil {
			t.Fatalf("cannot create a team: %s", err)
		}
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `{
			team(name: "parent") {
				childTeams {
					nodes {
						name
					}
				}
			}
		}`,
		ExpectedResult: `{
			"team": {
				"childTeams": {
					"nodes": [
						{"name": "child-1"},
						{"name": "child-2"},
						{"name": "child-3"},
						{"name": "child-4"},
						{"name": "child-5"}
					]
				}
			}
		}`,
	})
}

func TestMembersPaginated(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team-with-members"}); err != nil {
		t.Fatalf("failed to create team: %s", err)
	}
	teamWithMembers, err := fs.TeamStore.GetTeamByName(ctx, "team-with-members")
	if err != nil {
		t.Fatalf("failed to featch fake team: %s", err)
	}
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "different-team"}); err != nil {
		t.Fatalf("failed to create team: %s", err)
	}
	differentTeam, err := fs.TeamStore.GetTeamByName(ctx, "different-team")
	if err != nil {
		t.Fatalf("failed to featch fake team: %s", err)
	}
	for _, team := range []*types.Team{teamWithMembers, differentTeam} {
		for i := 1; i <= 25; i++ {
			id := fs.AddUser(types.User{Username: fmt.Sprintf("user-%d-%d", team.ID, i)})
			fs.AddTeamMember(&types.TeamMember{
				TeamID: team.ID,
				UserID: id,
			})
		}
	}
	var (
		hasNextPage bool = true
		cursor      string
	)
	query := `query Members($cursor: String!) {
		team(name: "team-with-members") {
			members(after: $cursor, first: 10) {
				totalCount
				pageInfo {
					endCursor
					hasNextPage
				}
				nodes {
					... on User {
						username
					}
				}
			}
		}
	}`
	operationName := ""
	var gotUsernames []string
	for hasNextPage {
		variables := map[string]any{
			"cursor": cursor,
		}
		r := mustParseGraphQLSchema(t, db).Exec(ctx, query, operationName, variables)
		var wantErrors []*gqlerrors.QueryError
		checkErrors(t, wantErrors, r.Errors)
		var result struct {
			Team *struct {
				Members *struct {
					TotalCount int
					PageInfo   *struct {
						EndCursor   string
						HasNextPage bool
					}
					Nodes []struct {
						Username string
					}
				}
			}
		}
		if err := json.Unmarshal(r.Data, &result); err != nil {
			t.Fatalf("cannot interpret graphQL query result: %s", err)
		}
		if got, want := result.Team.Members.TotalCount, 25; got != want {
			t.Errorf("totalCount, got %d, want %d", got, want)
		}
		if got, want := len(result.Team.Members.Nodes), 10; got > want {
			t.Errorf("#nodes, got %d, want at most %d", got, want)
		}
		hasNextPage = result.Team.Members.PageInfo.HasNextPage
		cursor = result.Team.Members.PageInfo.EndCursor
		for _, node := range result.Team.Members.Nodes {
			gotUsernames = append(gotUsernames, node.Username)
		}
	}
	var wantUsernames []string
	for i := 1; i <= 25; i++ {
		wantUsernames = append(wantUsernames, fmt.Sprintf("user-%d-%d", teamWithMembers.ID, i))
	}
	if diff := cmp.Diff(wantUsernames, gotUsernames); diff != "" {
		t.Errorf("unexpected member usernames (-want,+got):\n%s", diff)
	}
}

func TestMembersSearch(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
		t.Fatalf("failed to create parent team: %s", err)
	}
	team, err := fs.TeamStore.GetTeamByName(ctx, "team")
	if err != nil {
		t.Fatalf("failed to fetch fake team by ID: %s", err)
	}
	for _, u := range []types.User{
		{
			Username: "username-hit",
		},
		{
			Username: "username-miss",
		},
		{
			Username:    "look-at-displayname",
			DisplayName: "Display Name Hit",
		},
	} {
		userID := fs.AddUser(u)
		fs.AddTeamMember(&types.TeamMember{
			TeamID: team.ID,
			UserID: userID,
		})
	}
	idOfMissingUser := -7
	fs.AddTeamMember(&types.TeamMember{
		TeamID: team.ID,
		UserID: int32(idOfMissingUser),
	})
	fs.AddUser(types.User{Username: "search-hit-but-not-team-member"})
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `{
			team(name: "team") {
				members(search: "hit") {
					nodes {
						... on User {
							username
						}
					}
				}
			}
		}`,
		ExpectedResult: `{
			"team": {
				"members": {
					"nodes": [
						{"username": "username-hit"},
						{"username": "look-at-displayname"}
					]
				}
			}
		}`,
	})
}

func TestMembersAdd(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
		t.Fatalf("failed to create team: %s", err)
	}
	team, err := fs.TeamStore.GetTeamByName(ctx, "team")
	if err != nil {
		t.Fatalf("cannot fetch team: %s", err)
	}
	userExistingID := fs.AddUser(types.User{Username: "existing"})
	userExistingAndAddedID := fs.AddUser(types.User{Username: "existingAndAdded"})
	userAddedID := fs.AddUser(types.User{Username: "added"})
	fs.AddTeamMember(
		&types.TeamMember{TeamID: team.ID, UserID: userExistingID},
		&types.TeamMember{TeamID: team.ID, UserID: userExistingAndAddedID},
	)
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation AddTeamMembers($existingAndAddedId: ID!, $addedId: ID!) {
			addTeamMembers(teamName: "team", members: [
				{ userID: $existingAndAddedId },
				{ userID: $addedId }
			]) {
				members {
					nodes {
						... on User {
							username
						}
					}
				}
			}
		}`,
		ExpectedResult: `{
			"addTeamMembers": {
				"members": {
					"nodes": [
						{"username": "existing"},
						{"username": "existingAndAdded"},
						{"username": "added"}
					]
				}
			}
		}`,
		Variables: map[string]any{
			"existingAndAddedId": string(relay.MarshalID("User", userExistingAndAddedID)),
			"addedId":            string(relay.MarshalID("User", userAddedID)),
		},
	})
}

func TestMembersRemove(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
		t.Fatalf("failed to create team: %s", err)
	}
	team, err := fs.TeamStore.GetTeamByName(ctx, "team")
	if err != nil {
		t.Fatalf("cannot fetch team: %s", err)
	}
	var removedIDs []int32
	for i := 1; i <= 3; i++ {
		fs.AddTeamMember(&types.TeamMember{
			TeamID: team.ID,
			UserID: fs.AddUser(types.User{Username: fmt.Sprintf("retained-%d", i)}),
		})
		id := fs.AddUser(types.User{Username: fmt.Sprintf("removed-%d", i)})
		fs.AddTeamMember(&types.TeamMember{
			TeamID: team.ID,
			UserID: id,
		})
		removedIDs = append(removedIDs, id)
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation RemoveTeamMembers($r1: ID!, $r2: ID!, $r3: ID!) {
			removeTeamMembers(teamName: "team", members: [{ userID: $r1 }, { userID: $r2 }, { userID: $r3 }]) {
				members {
					nodes {
						... on User {
							username
						}
					}
				}
			}
		}`,
		ExpectedResult: `{
			"removeTeamMembers": {
				"members": {
					"nodes": [
						{"username": "retained-1"},
						{"username": "retained-2"},
						{"username": "retained-3"}
					]
				}
			}
		}`,
		Variables: map[string]any{
			"r1": string(relay.MarshalID("User", removedIDs[0])),
			"r2": string(relay.MarshalID("User", removedIDs[1])),
			"r3": string(relay.MarshalID("User", removedIDs[2])),
		},
	})
}

func TestMembersSet(t *testing.T) {
	fs := fakedb.New()
	db := database.NewMockDB()
	fs.Wire(db)
	ctx := userCtx(fs.AddUser(types.User{SiteAdmin: true}))
	if err := fs.TeamStore.CreateTeam(ctx, &types.Team{Name: "team"}); err != nil {
		t.Fatalf("failed to create team: %s", err)
	}
	team, err := fs.TeamStore.GetTeamByName(ctx, "team")
	if err != nil {
		t.Fatalf("cannot fetch team: %s", err)
	}
	var setIDs []int32
	for i := 1; i <= 2; i++ {
		fs.AddTeamMember(&types.TeamMember{
			TeamID: team.ID,
			UserID: fs.AddUser(types.User{Username: fmt.Sprintf("before-%d", i)}),
		})
		id := fs.AddUser(types.User{Username: fmt.Sprintf("before-and-after-%d", i)})
		fs.AddTeamMember(&types.TeamMember{
			TeamID: team.ID,
			UserID: id,
		})
		setIDs = append(setIDs, id)
		setIDs = append(setIDs, fs.AddUser(types.User{Username: fmt.Sprintf("after-%d", i)}))
	}
	RunTest(t, &Test{
		Schema:  mustParseGraphQLSchema(t, db),
		Context: ctx,
		Query: `mutation SetTeamMembers($r1: ID!, $r2: ID!, $r3: ID!, $r4: ID!) {
			setTeamMembers(teamName: "team", members: [{ userID: $r1 }, { userID: $r2 }, { userID: $r3 }, { userID: $r4 }]) {
				members {
					nodes {
						... on User {
							username
						}
					}
				}
			}
		}`,
		ExpectedResult: `{
			"setTeamMembers": {
				"members": {
					"nodes": [
						{"username": "before-and-after-1"},
						{"username": "after-1"},
						{"username": "before-and-after-2"},
						{"username": "after-2"}
					]
				}
			}
		}`,
		Variables: map[string]any{
			"r1": string(relay.MarshalID("User", setIDs[0])),
			"r2": string(relay.MarshalID("User", setIDs[1])),
			"r3": string(relay.MarshalID("User", setIDs[2])),
			"r4": string(relay.MarshalID("User", setIDs[3])),
		},
	})
}
