package resolvers

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/batches/resolvers/apitest"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	ct "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/testing"
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func TestBatchChangeConnectionResolver(t *testing.T) {
	logger := logtest.Scoped(t)
	if testing.Short() {
		t.Skip()
	}

	ctx := actor.WithInternalActor(context.Background())
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	userID := ct.CreateTestUser(t, db, true).ID

	cstore := store.New(db, &observation.TestContext, nil)
	repoStore := database.ReposWith(logger, cstore)
	esStore := database.ExternalServicesWith(logger, cstore)

	repo := newGitHubTestRepo("github.com/sourcegraph/batch-change-connection-test", newGitHubExternalService(t, esStore))
	if err := repoStore.Create(ctx, repo); err != nil {
		t.Fatal(err)
	}

	spec1 := &btypes.BatchSpec{
		NamespaceUserID: userID,
		UserID:          userID,
	}
	if err := cstore.CreateBatchSpec(ctx, spec1); err != nil {
		t.Fatal(err)
	}
	spec2 := &btypes.BatchSpec{
		NamespaceUserID: userID,
		UserID:          userID,
	}
	if err := cstore.CreateBatchSpec(ctx, spec2); err != nil {
		t.Fatal(err)
	}

	batchChange1 := &btypes.BatchChange{
		Name:            "my-unique-name",
		NamespaceUserID: userID,
		CreatorID:       userID,
		LastApplierID:   userID,
		LastAppliedAt:   time.Now(),
		BatchSpecID:     spec1.ID,
	}
	if err := cstore.CreateBatchChange(ctx, batchChange1); err != nil {
		t.Fatal(err)
	}
	batchChange2 := &btypes.BatchChange{
		Name:            "my-other-unique-name",
		NamespaceUserID: userID,
		CreatorID:       userID,
		LastApplierID:   userID,
		LastAppliedAt:   time.Now(),
		BatchSpecID:     spec2.ID,
	}
	if err := cstore.CreateBatchChange(ctx, batchChange2); err != nil {
		t.Fatal(err)
	}

	s, err := graphqlbackend.NewSchema(db, &Resolver{store: cstore}, nil, nil, nil, nil, nil, nil, nil, nil, nil, nil, nil)
	if err != nil {
		t.Fatal(err)
	}

	// Batch changes are returned in reverse order.
	nodes := []apitest.BatchChange{
		{
			ID: string(marshalBatchChangeID(batchChange2.ID)),
		},
		{
			ID: string(marshalBatchChangeID(batchChange1.ID)),
		},
	}

	tests := []struct {
		firstParam      int
		wantHasNextPage bool
		wantTotalCount  int
		wantNodes       []apitest.BatchChange
	}{
		{firstParam: 1, wantHasNextPage: true, wantTotalCount: 2, wantNodes: nodes[:1]},
		{firstParam: 2, wantHasNextPage: false, wantTotalCount: 2, wantNodes: nodes},
		{firstParam: 3, wantHasNextPage: false, wantTotalCount: 2, wantNodes: nodes},
	}

	for _, tc := range tests {
		t.Run(fmt.Sprintf("first=%d", tc.firstParam), func(t *testing.T) {
			input := map[string]any{"first": int64(tc.firstParam)}
			var response struct{ BatchChanges apitest.BatchChangeConnection }
			apitest.MustExec(actor.WithActor(context.Background(), actor.FromUser(userID)), t, s, input, &response, queryBatchChangesConnection)

			wantConnection := apitest.BatchChangeConnection{
				TotalCount: tc.wantTotalCount,
				PageInfo: apitest.PageInfo{
					HasNextPage: tc.wantHasNextPage,
					// We don't test on the cursor here.
					EndCursor: response.BatchChanges.PageInfo.EndCursor,
				},
				Nodes: tc.wantNodes,
			}

			if diff := cmp.Diff(wantConnection, response.BatchChanges); diff != "" {
				t.Fatalf("wrong batchChanges response (-want +got):\n%s", diff)
			}
		})
	}

	t.Run("Cursor based pagination", func(t *testing.T) {
		var endCursor *string
		for i := range nodes {
			input := map[string]any{"first": 1}
			if endCursor != nil {
				input["after"] = *endCursor
			}
			wantHasNextPage := i != len(nodes)-1

			var response struct{ BatchChanges apitest.BatchChangeConnection }
			apitest.MustExec(ctx, t, s, input, &response, queryBatchChangesConnection)

			if diff := cmp.Diff(1, len(response.BatchChanges.Nodes)); diff != "" {
				t.Fatalf("unexpected number of nodes (-want +got):\n%s", diff)
			}

			if diff := cmp.Diff(len(nodes), response.BatchChanges.TotalCount); diff != "" {
				t.Fatalf("unexpected total count (-want +got):\n%s", diff)
			}

			if diff := cmp.Diff(wantHasNextPage, response.BatchChanges.PageInfo.HasNextPage); diff != "" {
				t.Fatalf("unexpected hasNextPage (-want +got):\n%s", diff)
			}

			endCursor = response.BatchChanges.PageInfo.EndCursor
			if want, have := wantHasNextPage, endCursor != nil; have != want {
				t.Fatalf("unexpected endCursor existence. want=%t, have=%t", want, have)
			}
		}
	})
}

const queryBatchChangesConnection = `
query($first: Int, $after: String) {
  batchChanges(first: $first, after: $after) {
    totalCount
    pageInfo {
	  hasNextPage
	  endCursor
    }
    nodes {
      id
    }
  }
}
`

func TestBatchChangesListing(t *testing.T) {
	logger := logtest.Scoped(t)
	if testing.Short() {
		t.Skip()
	}

	ctx := context.Background()
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	userID := ct.CreateTestUser(t, db, false).ID
	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

	orgID := ct.InsertTestOrg(t, db, "org")

	store := store.New(db, &observation.TestContext, nil)

	r := &Resolver{store: store}
	s, err := graphqlbackend.NewSchema(db, r, nil, nil, nil, nil, nil, nil, nil, nil, nil, nil, nil)
	if err != nil {
		t.Fatal(err)
	}

	createBatchSpec := func(t *testing.T, spec *btypes.BatchSpec) {
		t.Helper()

		spec.UserID = userID
		spec.NamespaceUserID = userID
		if err := store.CreateBatchSpec(ctx, spec); err != nil {
			t.Fatal(err)
		}
	}

	createBatchChange := func(t *testing.T, c *btypes.BatchChange) {
		t.Helper()

		if err := store.CreateBatchChange(ctx, c); err != nil {
			t.Fatal(err)
		}
	}

	t.Run("listing a user's batch changes", func(t *testing.T) {
		spec := &btypes.BatchSpec{}
		createBatchSpec(t, spec)

		batchChange := &btypes.BatchChange{
			Name:            "batch-change-1",
			NamespaceUserID: userID,
			BatchSpecID:     spec.ID,
			CreatorID:       userID,
			LastApplierID:   userID,
			LastAppliedAt:   time.Now(),
		}
		createBatchChange(t, batchChange)

		userAPIID := string(graphqlbackend.MarshalUserID(userID))
		input := map[string]any{"node": userAPIID}

		var response struct{ Node apitest.User }
		apitest.MustExec(actorCtx, t, s, input, &response, listNamespacesBatchChanges)

		wantOne := apitest.User{
			ID: userAPIID,
			BatchChanges: apitest.BatchChangeConnection{
				TotalCount: 1,
				Nodes: []apitest.BatchChange{
					{ID: string(marshalBatchChangeID(batchChange.ID))},
				},
			},
		}

		if diff := cmp.Diff(wantOne, response.Node); diff != "" {
			t.Fatalf("wrong batch change response (-want +got):\n%s", diff)
		}

		spec2 := &btypes.BatchSpec{}
		createBatchSpec(t, spec2)

		// This batch change has never been applied -- it is a draft.
		batchChange2 := &btypes.BatchChange{
			Name:            "batch-change-2",
			NamespaceUserID: userID,
			BatchSpecID:     spec2.ID,
		}
		createBatchChange(t, batchChange2)

		// DRAFTS CASE 1: USERS CAN VIEW THEIR OWN DRAFTS.
		apitest.MustExec(actorCtx, t, s, input, &response, listNamespacesBatchChanges)

		wantBoth := apitest.User{
			ID: userAPIID,
			BatchChanges: apitest.BatchChangeConnection{
				TotalCount: 2,
				Nodes: []apitest.BatchChange{
					{ID: string(marshalBatchChangeID(batchChange2.ID))},
					{ID: string(marshalBatchChangeID(batchChange.ID))},
				},
			},
		}

		if diff := cmp.Diff(wantBoth, response.Node); diff != "" {
			t.Fatalf("wrong batch change response (-want +got):\n%s", diff)
		}

		// DRAFTS CASE 2: ADMIN USERS CAN VIEW OTHER USERS' DRAFTS
		adminUserID := ct.CreateTestUser(t, db, true).ID
		adminActorCtx := actor.WithActor(ctx, actor.FromUser(adminUserID))

		apitest.MustExec(adminActorCtx, t, s, input, &response, listNamespacesBatchChanges)

		if diff := cmp.Diff(wantBoth, response.Node); diff != "" {
			t.Fatalf("wrong batch change response (-want +got):\n%s", diff)
		}

		// DRAFTS CASE 3: NON-ADMIN USERS CANNOT VIEW OTHER USERS' DRAFTS.
		otherUserID := ct.CreateTestUser(t, db, false).ID
		otherActorCtx := actor.WithActor(ctx, actor.FromUser(otherUserID))

		apitest.MustExec(otherActorCtx, t, s, input, &response, listNamespacesBatchChanges)

		if diff := cmp.Diff(wantOne, response.Node); diff != "" {
			t.Fatalf("wrong batch change response (-want +got):\n%s", diff)
		}
	})

	t.Run("listing an orgs batch changes", func(t *testing.T) {
		spec := &btypes.BatchSpec{}
		createBatchSpec(t, spec)

		batchChange := &btypes.BatchChange{
			Name:           "batch-change-1",
			NamespaceOrgID: orgID,
			BatchSpecID:    spec.ID,
			CreatorID:      userID,
			LastApplierID:  userID,
			LastAppliedAt:  time.Now(),
		}
		createBatchChange(t, batchChange)

		orgAPIID := string(graphqlbackend.MarshalOrgID(orgID))
		input := map[string]any{"node": orgAPIID}

		var response struct{ Node apitest.Org }
		apitest.MustExec(actorCtx, t, s, input, &response, listNamespacesBatchChanges)

		wantOne := apitest.Org{
			ID: orgAPIID,
			BatchChanges: apitest.BatchChangeConnection{
				TotalCount: 1,
				Nodes: []apitest.BatchChange{
					{ID: string(marshalBatchChangeID(batchChange.ID))},
				},
			},
		}

		if diff := cmp.Diff(wantOne, response.Node); diff != "" {
			t.Fatalf("wrong batch change response (-want +got):\n%s", diff)
		}

		spec2 := &btypes.BatchSpec{UserID: userID}
		createBatchSpec(t, spec2)

		// This batch change has never been applied -- it is a draft.
		batchChange2 := &btypes.BatchChange{
			Name:           "batch-change-2",
			NamespaceOrgID: orgID,
			BatchSpecID:    spec2.ID,
		}
		createBatchChange(t, batchChange2)

		// DRAFTS CASE 1: USERS CAN VIEW THEIR OWN DRAFTS.
		apitest.MustExec(actorCtx, t, s, input, &response, listNamespacesBatchChanges)

		wantBoth := apitest.Org{
			ID: orgAPIID,
			BatchChanges: apitest.BatchChangeConnection{
				TotalCount: 2,
				Nodes: []apitest.BatchChange{
					{ID: string(marshalBatchChangeID(batchChange2.ID))},
					{ID: string(marshalBatchChangeID(batchChange.ID))},
				},
			},
		}

		if diff := cmp.Diff(wantBoth, response.Node); diff != "" {
			t.Fatalf("wrong batch change response (-want +got):\n%s", diff)
		}

		// DRAFTS CASE 2: ADMIN USERS CAN VIEW OTHER USERS' DRAFTS
		adminUserID := ct.CreateTestUser(t, db, true).ID
		adminActorCtx := actor.WithActor(ctx, actor.FromUser(adminUserID))

		apitest.MustExec(adminActorCtx, t, s, input, &response, listNamespacesBatchChanges)

		if diff := cmp.Diff(wantBoth, response.Node); diff != "" {
			t.Fatalf("wrong batch change response (-want +got):\n%s", diff)
		}

		// DRAFTS CASE 3: NON-ADMIN USERS CANNOT VIEW OTHER USERS' DRAFTS.
		otherUserID := ct.CreateTestUser(t, db, false).ID
		otherActorCtx := actor.WithActor(ctx, actor.FromUser(otherUserID))

		apitest.MustExec(otherActorCtx, t, s, input, &response, listNamespacesBatchChanges)

		if diff := cmp.Diff(wantOne, response.Node); diff != "" {
			t.Fatalf("wrong batch change response (-want +got):\n%s", diff)
		}
	})
}

const listNamespacesBatchChanges = `
query($node: ID!) {
  node(id: $node) {
    ... on User {
      id
      batchChanges {
        totalCount
        nodes {
          id
        }
      }
    }

    ... on Org {
      id
      batchChanges {
        totalCount
        nodes {
          id
        }
      }
    }
  }
}
`
