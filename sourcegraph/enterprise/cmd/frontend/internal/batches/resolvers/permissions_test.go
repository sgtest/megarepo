package resolvers

import (
	"context"
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/graph-gophers/graphql-go"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/batches/resolvers/apitest"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/service"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	ct "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/testing"
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	et "github.com/sourcegraph/sourcegraph/internal/encryption/testing"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/auth"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/repoupdater"
	"github.com/sourcegraph/sourcegraph/internal/types"
	batcheslib "github.com/sourcegraph/sourcegraph/lib/batches"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestPermissionLevels(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	ct.MockRSAKeygen(t)

	db := database.NewDB(dbtest.NewDB(t))
	key := et.TestKey{}

	cstore := store.New(db, &observation.TestContext, key)
	sr := New(cstore)
	s, err := graphqlbackend.NewSchema(database.NewDB(db), sr, nil, nil, nil, nil, nil, nil, nil, nil, nil, nil)
	if err != nil {
		t.Fatal(err)
	}

	// SyncChangeset uses EnqueueChangesetSync and tries to talk to repo-updater, hence we need to mock it.
	repoupdater.MockEnqueueChangesetSync = func(ctx context.Context, ids []int64) error {
		return nil
	}
	t.Cleanup(func() { repoupdater.MockEnqueueChangesetSync = nil })

	ctx := context.Background()

	// Global test data that we reuse in every test
	adminID := ct.CreateTestUser(t, db, true).ID
	userID := ct.CreateTestUser(t, db, false).ID

	repoStore := database.ReposWith(cstore)
	esStore := database.ExternalServicesWith(cstore)

	repo := newGitHubTestRepo("github.com/sourcegraph/permission-levels-test", newGitHubExternalService(t, esStore))
	if err := repoStore.Create(ctx, repo); err != nil {
		t.Fatal(err)
	}

	changeset := &btypes.Changeset{
		RepoID:              repo.ID,
		ExternalServiceType: "github",
		ExternalID:          "1234",
	}
	if err := cstore.CreateChangeset(ctx, changeset); err != nil {
		t.Fatal(err)
	}

	createBatchChange := func(t *testing.T, s *store.Store, name string, userID int32, batchSpecID int64) (batchChangeID int64) {
		t.Helper()

		c := &btypes.BatchChange{
			Name:            name,
			CreatorID:       userID,
			NamespaceUserID: userID,
			LastApplierID:   userID,
			LastAppliedAt:   time.Now(),
			BatchSpecID:     batchSpecID,
		}
		if err := s.CreateBatchChange(ctx, c); err != nil {
			t.Fatal(err)
		}

		// We attach the changeset to the batch change so we can test syncChangeset
		changeset.BatchChanges = append(changeset.BatchChanges, btypes.BatchChangeAssoc{BatchChangeID: c.ID})
		if err := s.UpdateChangeset(ctx, changeset); err != nil {
			t.Fatal(err)
		}

		cs := &btypes.BatchSpec{UserID: userID, NamespaceUserID: userID}
		if err := s.CreateBatchSpec(ctx, cs); err != nil {
			t.Fatal(err)
		}

		return c.ID
	}

	createBatchSpec := func(t *testing.T, s *store.Store, userID int32) (randID string, id int64) {
		t.Helper()

		cs := &btypes.BatchSpec{UserID: userID, NamespaceUserID: userID}
		if err := s.CreateBatchSpec(ctx, cs); err != nil {
			t.Fatal(err)
		}

		return cs.RandID, cs.ID
	}

	createBatchSpecFromRaw := func(t *testing.T, s *store.Store, userID int32) (randID string, id int64) {
		t.Helper()

		// userCtx causes CreateBatchSpecFromRaw to set batchSpec.UserID to userID
		userCtx := actor.WithActor(ctx, actor.FromUser(userID))

		// We're using the service method here since it also creates a resolution job
		svc := service.New(s)
		spec, err := svc.CreateBatchSpecFromRaw(userCtx, service.CreateBatchSpecFromRawOpts{
			RawSpec:         ct.TestRawBatchSpecYAML,
			NamespaceUserID: userID,
		})
		if err != nil {
			t.Fatal(err)
		}

		return spec.RandID, spec.ID
	}

	createBatchSpecWorkspace := func(t *testing.T, s *store.Store, batchSpecID int64) (id int64) {
		t.Helper()

		ws := &btypes.BatchSpecWorkspace{
			BatchSpecID: batchSpecID,
			RepoID:      repo.ID,
		}
		if err := s.CreateBatchSpecWorkspace(ctx, ws); err != nil {
			t.Fatal(err)
		}

		return ws.ID
	}

	cleanUpBatchChanges := func(t *testing.T, s *store.Store) {
		t.Helper()

		batchChanges, next, err := s.ListBatchChanges(ctx, store.ListBatchChangesOpts{LimitOpts: store.LimitOpts{Limit: 1000}})
		if err != nil {
			t.Fatal(err)
		}
		if next != 0 {
			t.Fatalf("more batch changes in store")
		}

		for _, c := range batchChanges {
			if err := s.DeleteBatchChange(ctx, c.ID); err != nil {
				t.Fatal(err)
			}
		}
	}

	cleanUpBatchSpecs := func(t *testing.T, s *store.Store) {
		t.Helper()

		batchChanges, next, err := s.ListBatchSpecs(ctx, store.ListBatchSpecsOpts{LimitOpts: store.LimitOpts{Limit: 1000}})
		if err != nil {
			t.Fatal(err)
		}
		if next != 0 {
			t.Fatalf("more batch specs in store")
		}

		for _, c := range batchChanges {
			if err := s.DeleteBatchSpec(ctx, c.ID); err != nil {
				t.Fatal(err)
			}
		}
	}

	t.Run("queries", func(t *testing.T) {
		cleanUpBatchChanges(t, cstore)

		adminBatchSpec, adminBatchSpecID := createBatchSpec(t, cstore, adminID)
		adminBatchChange := createBatchChange(t, cstore, "admin", adminID, adminBatchSpecID)
		userBatchSpec, userBatchSpecID := createBatchSpec(t, cstore, userID)
		userBatchChange := createBatchChange(t, cstore, "user", userID, userBatchSpecID)

		adminBatchSpecCreatedFromRawRandID, _ := createBatchSpecFromRaw(t, cstore, adminID)
		userBatchSpecCreatedFromRawRandID, _ := createBatchSpecFromRaw(t, cstore, userID)

		t.Run("BatchChangeByID", func(t *testing.T) {
			tests := []struct {
				name                    string
				currentUser             int32
				batchChange             int64
				wantViewerCanAdminister bool
			}{
				{
					name:                    "site-admin viewing own batch change",
					currentUser:             adminID,
					batchChange:             adminBatchChange,
					wantViewerCanAdminister: true,
				},
				{
					name:                    "non-site-admin viewing other's batch change",
					currentUser:             userID,
					batchChange:             adminBatchChange,
					wantViewerCanAdminister: false,
				},
				{
					name:                    "site-admin viewing other's batch change",
					currentUser:             adminID,
					batchChange:             userBatchChange,
					wantViewerCanAdminister: true,
				},
				{
					name:                    "non-site-admin viewing own batch change",
					currentUser:             userID,
					batchChange:             userBatchChange,
					wantViewerCanAdminister: true,
				},
			}

			for _, tc := range tests {
				t.Run(tc.name, func(t *testing.T) {
					graphqlID := string(marshalBatchChangeID(tc.batchChange))

					var res struct{ Node apitest.BatchChange }

					input := map[string]any{"batchChange": graphqlID}
					queryBatchChange := `
				  query($batchChange: ID!) {
				    node(id: $batchChange) { ... on BatchChange { id, viewerCanAdminister } }
				  }`

					actorCtx := actor.WithActor(ctx, actor.FromUser(tc.currentUser))
					apitest.MustExec(actorCtx, t, s, input, &res, queryBatchChange)

					if have, want := res.Node.ID, graphqlID; have != want {
						t.Fatalf("queried batch change has wrong id %q, want %q", have, want)
					}
					if have, want := res.Node.ViewerCanAdminister, tc.wantViewerCanAdminister; have != want {
						t.Fatalf("queried batch change's ViewerCanAdminister is wrong %t, want %t", have, want)
					}
				})
			}
		})

		t.Run("BatchSpecByID", func(t *testing.T) {
			tests := []struct {
				name                    string
				currentUser             int32
				batchSpec               string
				wantViewerCanAdminister bool
			}{
				{
					name:                    "site-admin viewing own batch spec",
					currentUser:             adminID,
					batchSpec:               adminBatchSpec,
					wantViewerCanAdminister: true,
				},
				{
					name:                    "site-admin viewing own created-from-raw batch spec",
					currentUser:             adminID,
					batchSpec:               adminBatchSpecCreatedFromRawRandID,
					wantViewerCanAdminister: true,
				},
				{
					name:                    "site-admin viewing other's batch spec",
					currentUser:             adminID,
					batchSpec:               userBatchSpec,
					wantViewerCanAdminister: true,
				},
				{
					name:                    "site-admin viewing other's created-from-raw batch spec",
					currentUser:             adminID,
					batchSpec:               userBatchSpecCreatedFromRawRandID,
					wantViewerCanAdminister: true,
				},
				{
					name:                    "non-site-admin viewing own batch spec",
					currentUser:             userID,
					batchSpec:               userBatchSpec,
					wantViewerCanAdminister: true,
				},
				{
					name:                    "non-site-admin viewing own created-from-raw batch spec",
					currentUser:             userID,
					batchSpec:               userBatchSpecCreatedFromRawRandID,
					wantViewerCanAdminister: true,
				},
				{
					name:                    "non-site-admin viewing other's batch spec",
					currentUser:             userID,
					batchSpec:               adminBatchSpec,
					wantViewerCanAdminister: false,
				},
				{
					name:                    "non-site-admin viewing other's created-from-raw batch spec",
					currentUser:             userID,
					batchSpec:               adminBatchSpecCreatedFromRawRandID,
					wantViewerCanAdminister: false,
				},
			}

			for _, tc := range tests {
				t.Run(tc.name, func(t *testing.T) {
					graphqlID := string(marshalBatchSpecRandID(tc.batchSpec))

					var res struct{ Node apitest.BatchSpec }

					input := map[string]any{"batchSpec": graphqlID}
					queryBatchSpec := `
				  query($batchSpec: ID!) {
				    node(id: $batchSpec) { ... on BatchSpec { id, viewerCanAdminister } }
				  }`

					actorCtx := actor.WithActor(ctx, actor.FromUser(tc.currentUser))
					apitest.MustExec(actorCtx, t, s, input, &res, queryBatchSpec)

					if have, want := res.Node.ID, graphqlID; have != want {
						t.Fatalf("queried batch spec has wrong id %q, want %q", have, want)
					}
					if have, want := res.Node.ViewerCanAdminister, tc.wantViewerCanAdminister; have != want {
						t.Fatalf("queried batch spec's ViewerCanAdminister is wrong %t, want %t", have, want)
					}
				})
			}
		})

		t.Run("User.BatchChangesCodeHosts", func(t *testing.T) {
			tests := []struct {
				name        string
				currentUser int32
				user        int32
				wantErr     bool
			}{
				{
					name:        "site-admin viewing other user",
					currentUser: adminID,
					user:        userID,
					wantErr:     false,
				},
				{
					name:        "non-site-admin viewing other's hosts",
					currentUser: userID,
					user:        adminID,
					wantErr:     true,
				},
				{
					name:        "non-site-admin viewing own hosts",
					currentUser: userID,
					user:        userID,
					wantErr:     false,
				},
			}

			for _, tc := range tests {
				t.Run(tc.name, func(t *testing.T) {
					pruneUserCredentials(t, db, key)
					pruneSiteCredentials(t, cstore)

					graphqlID := string(graphqlbackend.MarshalUserID(tc.user))

					var res struct{ Node apitest.User }

					input := map[string]any{"user": graphqlID}
					queryCodeHosts := `
				  query($user: ID!) {
				    node(id: $user) { ... on User { batchChangesCodeHosts { totalCount } } }
				  }`

					actorCtx := actor.WithActor(ctx, actor.FromUser(tc.currentUser))
					errors := apitest.Exec(actorCtx, t, s, input, &res, queryCodeHosts)
					if !tc.wantErr && len(errors) != 0 {
						t.Fatalf("got error but didn't expect one: %+v", errors)
					} else if tc.wantErr && len(errors) == 0 {
						t.Fatal("expected error but got none")
					}
				})
			}
		})

		t.Run("BatchChangesCredentialByID", func(t *testing.T) {
			tests := []struct {
				name        string
				currentUser int32
				user        int32
				wantErr     bool
			}{
				{
					name:        "site-admin viewing other user",
					currentUser: adminID,
					user:        userID,
					wantErr:     false,
				},
				{
					name:        "non-site-admin viewing other's credential",
					currentUser: userID,
					user:        adminID,
					wantErr:     true,
				},
				{
					name:        "non-site-admin viewing own credential",
					currentUser: userID,
					user:        userID,
					wantErr:     false,
				},

				{
					name:        "site-admin viewing site-credential",
					currentUser: adminID,
					user:        0,
					wantErr:     false,
				},
				{
					name:        "non-site-admin viewing site-credential",
					currentUser: userID,
					user:        0,
					wantErr:     true,
				},
			}

			for _, tc := range tests {
				t.Run(tc.name, func(t *testing.T) {
					pruneUserCredentials(t, db, key)
					pruneSiteCredentials(t, cstore)

					var graphqlID graphql.ID
					if tc.user != 0 {
						cred, err := cstore.UserCredentials().Create(ctx, database.UserCredentialScope{
							Domain:              database.UserCredentialDomainBatches,
							ExternalServiceID:   "https://github.com/",
							ExternalServiceType: extsvc.TypeGitHub,
							UserID:              tc.user,
						}, &auth.OAuthBearerToken{Token: "SOSECRET"})
						if err != nil {
							t.Fatal(err)
						}
						graphqlID = marshalBatchChangesCredentialID(cred.ID, false)
					} else {
						cred := &btypes.SiteCredential{
							ExternalServiceID:   "https://github.com/",
							ExternalServiceType: extsvc.TypeGitHub,
						}
						token := &auth.OAuthBearerToken{Token: "SOSECRET"}
						if err := cstore.CreateSiteCredential(ctx, cred, token); err != nil {
							t.Fatal(err)
						}
						graphqlID = marshalBatchChangesCredentialID(cred.ID, true)
					}

					var res struct {
						Node apitest.BatchChangesCredential
					}

					input := map[string]any{"id": graphqlID}
					queryCodeHosts := `
				  query($id: ID!) {
				    node(id: $id) { ... on BatchChangesCredential { id } }
				  }`

					actorCtx := actor.WithActor(ctx, actor.FromUser(tc.currentUser))
					errors := apitest.Exec(actorCtx, t, s, input, &res, queryCodeHosts)
					if !tc.wantErr && len(errors) != 0 {
						t.Fatalf("got error but didn't expect one: %v", errors)
					} else if tc.wantErr && len(errors) == 0 {
						t.Fatal("expected error but got none")
					}
					if !tc.wantErr {
						if have, want := res.Node.ID, string(graphqlID); have != want {
							t.Fatalf("invalid node returned, wanted ID=%q, have=%q", want, have)
						}
					}
				})
			}
		})

		t.Run("BatchChanges", func(t *testing.T) {
			tests := []struct {
				name                string
				currentUser         int32
				viewerCanAdminister bool
				wantBatchChanges    []int64
			}{
				{
					name:                "admin listing viewerCanAdminister: true",
					currentUser:         adminID,
					viewerCanAdminister: true,
					wantBatchChanges:    []int64{adminBatchChange, userBatchChange},
				},
				{
					name:                "user listing viewerCanAdminister: true",
					currentUser:         userID,
					viewerCanAdminister: true,
					wantBatchChanges:    []int64{userBatchChange},
				},
				{
					name:                "admin listing viewerCanAdminister: false",
					currentUser:         adminID,
					viewerCanAdminister: false,
					wantBatchChanges:    []int64{adminBatchChange, userBatchChange},
				},
				{
					name:                "user listing viewerCanAdminister: false",
					currentUser:         userID,
					viewerCanAdminister: false,
					wantBatchChanges:    []int64{adminBatchChange, userBatchChange},
				},
			}
			for _, tc := range tests {
				t.Run(tc.name, func(t *testing.T) {
					actorCtx := actor.WithActor(context.Background(), actor.FromUser(tc.currentUser))
					expectedIDs := make(map[string]bool, len(tc.wantBatchChanges))
					for _, c := range tc.wantBatchChanges {
						graphqlID := string(marshalBatchChangeID(c))
						expectedIDs[graphqlID] = true
					}

					query := fmt.Sprintf(`
				query {
					batchChanges(viewerCanAdminister: %t) { totalCount, nodes { id } }
					node(id: %q) {
						id
						... on ExternalChangeset {
							batchChanges(viewerCanAdminister: %t) { totalCount, nodes { id } }
						}
					}
					}`, tc.viewerCanAdminister, marshalChangesetID(changeset.ID), tc.viewerCanAdminister)
					var res struct {
						BatchChanges apitest.BatchChangeConnection
						Node         apitest.Changeset
					}
					apitest.MustExec(actorCtx, t, s, nil, &res, query)
					for _, conn := range []apitest.BatchChangeConnection{res.BatchChanges, res.Node.BatchChanges} {
						if have, want := conn.TotalCount, len(tc.wantBatchChanges); have != want {
							t.Fatalf("wrong count of batch changes returned, want=%d have=%d", want, have)
						}
						if have, want := conn.TotalCount, len(conn.Nodes); have != want {
							t.Fatalf("totalCount and nodes length don't match, want=%d have=%d", want, have)
						}
						for _, node := range conn.Nodes {
							if _, ok := expectedIDs[node.ID]; !ok {
								t.Fatalf("received wrong batch change with id %q", node.ID)
							}
						}
					}
				})
			}
		})

		t.Run("BatchSpecs", func(t *testing.T) {
			cleanUpBatchChanges(t, cstore)
			cleanUpBatchSpecs(t, cstore)

			adminBatchSpecCreatedFromRawRandID, adminBatchSpecCreatedFromRawID := createBatchSpecFromRaw(t, cstore, adminID)
			adminBatchSpecCreatedRandID, adminBatchSpecCreatedID := createBatchSpec(t, cstore, adminID)

			userBatchSpecCreatedFromRawRandID, userBatchSpecCreatedFromRawID := createBatchSpecFromRaw(t, cstore, userID)
			userBatchSpecCreatedRandID, userBatchSpecCreatedID := createBatchSpec(t, cstore, userID)

			type ids struct {
				randID string
				id     int64
			}

			tests := []struct {
				name           string
				currentUser    int32
				wantBatchSpecs []ids
			}{
				{
					name:        "admin listing",
					currentUser: adminID,
					wantBatchSpecs: []ids{
						{adminBatchSpecCreatedRandID, adminBatchSpecCreatedID},
						{userBatchSpecCreatedRandID, userBatchSpecCreatedID},
						{adminBatchSpecCreatedFromRawRandID, adminBatchSpecCreatedFromRawID},
						{userBatchSpecCreatedFromRawRandID, userBatchSpecCreatedFromRawID},
					},
				},
				{
					name:        "user listing",
					currentUser: userID,
					wantBatchSpecs: []ids{
						{adminBatchSpecCreatedRandID, adminBatchSpecCreatedID},
						{userBatchSpecCreatedRandID, userBatchSpecCreatedID},
						{userBatchSpecCreatedFromRawRandID, userBatchSpecCreatedFromRawID},
					},
				},
			}
			for _, tc := range tests {
				t.Run(tc.name, func(t *testing.T) {
					actorCtx := actor.WithActor(context.Background(), actor.FromUser(tc.currentUser))
					expectedIDs := make(map[string]bool, len(tc.wantBatchSpecs))
					for _, ids := range tc.wantBatchSpecs {
						graphqlID := string(marshalBatchSpecRandID(ids.randID))
						expectedIDs[graphqlID] = true
					}

					query := `query { batchSpecs() { totalCount, nodes { id } } }`

					var res struct{ BatchSpecs apitest.BatchSpecConnection }
					apitest.MustExec(actorCtx, t, s, nil, &res, query)

					if have, want := res.BatchSpecs.TotalCount, len(tc.wantBatchSpecs); have != want {
						t.Fatalf("wrong count of batch changes returned, want=%d have=%d", want, have)
					}
					if have, want := res.BatchSpecs.TotalCount, len(res.BatchSpecs.Nodes); have != want {
						t.Fatalf("totalCount and nodes length don't match, want=%d have=%d", want, have)
					}
					for _, node := range res.BatchSpecs.Nodes {
						if _, ok := expectedIDs[node.ID]; !ok {
							t.Fatalf("received wrong batch change with id %q", node.ID)
						}
					}
				})
			}
		})

		t.Run("BatchSpecWorkspaceByID", func(t *testing.T) {
			tests := []struct {
				name        string
				currentUser int32
				user        int32
				wantErr     bool
			}{
				{
					name:        "site-admin viewing other user",
					currentUser: adminID,
					user:        userID,
					wantErr:     false,
				},
				{
					name:        "non-site-admin viewing other's workspace",
					currentUser: userID,
					user:        adminID,
					wantErr:     false,
				},
				{
					name:        "non-site-admin viewing own workspace",
					currentUser: userID,
					user:        userID,
					wantErr:     false,
				},
			}

			for _, tc := range tests {
				t.Run(tc.name, func(t *testing.T) {
					_, batchSpecID := createBatchSpecFromRaw(t, cstore, tc.user)
					workspaceID := createBatchSpecWorkspace(t, cstore, batchSpecID)

					graphqlID := string(marshalBatchSpecWorkspaceID(workspaceID))

					var res struct{ Node apitest.BatchSpecWorkspace }

					input := map[string]any{"id": graphqlID}
					query := `query($id: ID!) { node(id: $id) { ... on BatchSpecWorkspace { id } } }`

					actorCtx := actor.WithActor(ctx, actor.FromUser(tc.currentUser))

					errors := apitest.Exec(actorCtx, t, s, input, &res, query)
					if !tc.wantErr && len(errors) != 0 {
						t.Fatalf("got error but didn't expect one: %v", errors)
					} else if tc.wantErr && len(errors) == 0 {
						t.Fatal("expected error but got none")
					}
					if !tc.wantErr {
						if have, want := res.Node.ID, graphqlID; have != want {
							t.Fatalf("invalid node returned, wanted ID=%q, have=%q", want, have)
						}
					}
				})
			}
		})
	})

	t.Run("batch change mutations", func(t *testing.T) {
		mutations := []struct {
			name         string
			mutationFunc func(userID, batchChangeID, changesetID, batchSpecID string) string
		}{
			{
				name: "createBatchChange",
				mutationFunc: func(userID, batchChangeID, changesetID, batchSpecID string) string {
					return fmt.Sprintf(`mutation { createBatchChange(batchSpec: %q) { id } }`, batchSpecID)
				},
			},
			{
				name: "closeBatchChange",
				mutationFunc: func(userID, batchChangeID, changesetID, batchSpecID string) string {
					return fmt.Sprintf(`mutation { closeBatchChange(batchChange: %q, closeChangesets: false) { id } }`, batchChangeID)
				},
			},
			{
				name: "deleteBatchChange",
				mutationFunc: func(userID, batchChangeID, changesetID, batchSpecID string) string {
					return fmt.Sprintf(`mutation { deleteBatchChange(batchChange: %q) { alwaysNil } } `, batchChangeID)
				},
			},
			{
				name: "syncChangeset",
				mutationFunc: func(userID, batchChangeID, changesetID, batchSpecID string) string {
					return fmt.Sprintf(`mutation { syncChangeset(changeset: %q) { alwaysNil } }`, changesetID)
				},
			},
			{
				name: "reenqueueChangeset",
				mutationFunc: func(userID, batchChangeID, changesetID, batchSpecID string) string {
					return fmt.Sprintf(`mutation { reenqueueChangeset(changeset: %q) { id } }`, changesetID)
				},
			},
			{
				name: "applyBatchChange",
				mutationFunc: func(userID, batchChangeID, changesetID, batchSpecID string) string {
					return fmt.Sprintf(`mutation { applyBatchChange(batchSpec: %q) { id } }`, batchSpecID)
				},
			},
			{
				name: "moveBatchChange",
				mutationFunc: func(userID, batchChangeID, changesetID, batchSpecID string) string {
					return fmt.Sprintf(`mutation { moveBatchChange(batchChange: %q, newName: "foobar") { id } }`, batchChangeID)
				},
			},
			{
				name: "createChangesetComments",
				mutationFunc: func(userID, batchChangeID, changesetID, batchSpecID string) string {
					return fmt.Sprintf(`mutation { createChangesetComments(batchChange: %q, changesets: [%q], body: "test") { id } }`, batchChangeID, changesetID)
				},
			},
			{
				name: "reenqueueChangesets",
				mutationFunc: func(userID, batchChangeID, changesetID, batchSpecID string) string {
					return fmt.Sprintf(`mutation { reenqueueChangesets(batchChange: %q, changesets: [%q]) { id } }`, batchChangeID, changesetID)
				},
			},
			{
				name: "mergeChangesets",
				mutationFunc: func(userID, batchChangeID, changesetID, batchSpecID string) string {
					return fmt.Sprintf(`mutation { mergeChangesets(batchChange: %q, changesets: [%q]) { id } }`, batchChangeID, changesetID)
				},
			},
			{
				name: "closeChangesets",
				mutationFunc: func(userID, batchChangeID, changesetID, batchSpecID string) string {
					return fmt.Sprintf(`mutation { closeChangesets(batchChange: %q, changesets: [%q]) { id } }`, batchChangeID, changesetID)
				},
			},
			{
				name: "createEmptyBatchChange",
				mutationFunc: func(userID, batchChangeID, changesetID, batchSpecID string) string {
					return fmt.Sprintf(`mutation { createEmptyBatchChange(namespace: %q, name: "testing") { id } }`, userID)
				},
			},
		}

		for _, m := range mutations {
			t.Run(m.name, func(t *testing.T) {
				tests := []struct {
					name              string
					currentUser       int32
					batchChangeAuthor int32
					wantAuthErr       bool

					// If batches.restrictToAdmins is enabled, should an error
					// be generated?
					wantDisabledErr bool
				}{
					{
						name:              "unauthorized",
						currentUser:       userID,
						batchChangeAuthor: adminID,
						wantAuthErr:       true,
						wantDisabledErr:   true,
					},
					{
						name:              "authorized batch change owner",
						currentUser:       userID,
						batchChangeAuthor: userID,
						wantAuthErr:       false,
						wantDisabledErr:   true,
					},
					{
						name:              "authorized site-admin",
						currentUser:       adminID,
						batchChangeAuthor: userID,
						wantAuthErr:       false,
						wantDisabledErr:   false,
					},
				}

				for _, tc := range tests {
					for _, restrict := range []bool{true, false} {
						t.Run(fmt.Sprintf("%s restrict: %v", tc.name, restrict), func(t *testing.T) {
							cleanUpBatchChanges(t, cstore)

							batchSpecRandID, batchSpecID := createBatchSpec(t, cstore, tc.batchChangeAuthor)
							batchChagneID := createBatchChange(t, cstore, "test-batch-change", tc.batchChangeAuthor, batchSpecID)

							// We add the changeset to the batch change. It doesn't
							// matter for the addChangesetsToBatchChange mutation,
							// since that is idempotent and we want to solely
							// check for auth errors.
							changeset.BatchChanges = []btypes.BatchChangeAssoc{{BatchChangeID: batchChagneID}}
							if err := cstore.UpdateChangeset(ctx, changeset); err != nil {
								t.Fatal(err)
							}

							mutation := m.mutationFunc(
								string(graphqlbackend.MarshalUserID(tc.batchChangeAuthor)),
								string(marshalBatchChangeID(batchChagneID)),
								string(marshalChangesetID(changeset.ID)),
								string(marshalBatchSpecRandID(batchSpecRandID)),
							)

							assertAuthorizationResponse(t, ctx, s, nil, mutation, tc.currentUser, restrict, tc.wantDisabledErr, tc.wantAuthErr)
						})
					}
				}
			})
		}
	})

	t.Run("spec mutations", func(t *testing.T) {
		mutations := []struct {
			name         string
			mutationFunc func(userID string) string
		}{
			{
				name: "createChangesetSpec",
				mutationFunc: func(_ string) string {
					return `mutation { createChangesetSpec(changesetSpec: "{}") { type } }`
				},
			},
			{
				name: "createBatchSpec",
				mutationFunc: func(userID string) string {
					return fmt.Sprintf(`
					mutation {
						createBatchSpec(namespace: %q, batchSpec: "{}", changesetSpecs: []) {
							id
						}
					}`, userID)
				},
			},
			{
				name: "createBatchSpecFromRaw",
				mutationFunc: func(userID string) string {
					return fmt.Sprintf(`
					mutation {
						createBatchSpecFromRaw(namespace: %q, batchSpec: "name: testing") {
							id
						}
					}`, userID)
				},
			},
		}

		for _, m := range mutations {
			t.Run(m.name, func(t *testing.T) {
				tests := []struct {
					name        string
					currentUser int32
					wantAuthErr bool
				}{
					{name: "no user", currentUser: 0, wantAuthErr: true},
					{name: "user", currentUser: userID, wantAuthErr: false},
					{name: "site-admin", currentUser: adminID, wantAuthErr: false},
				}

				for _, tc := range tests {
					t.Run(tc.name, func(t *testing.T) {
						cleanUpBatchChanges(t, cstore)

						namespaceID := string(graphqlbackend.MarshalUserID(tc.currentUser))
						if tc.currentUser == 0 {
							// If we don't have a currentUser we try to create
							// a batch change in another namespace, solely for the
							// purposes of this test.
							namespaceID = string(graphqlbackend.MarshalUserID(userID))
						}
						mutation := m.mutationFunc(namespaceID)

						assertAuthorizationResponse(t, ctx, s, nil, mutation, tc.currentUser, false, false, tc.wantAuthErr)
					})
				}
			})
		}
	})

	t.Run("batch spec execution mutations", func(t *testing.T) {
		mutations := []struct {
			name         string
			mutationFunc func(batchSpecID, workspaceID string) string
		}{
			{
				name: "executeBatchSpec",
				mutationFunc: func(batchSpecID, _ string) string {
					return fmt.Sprintf(`mutation { executeBatchSpec(batchSpec: %q) { id } }`, batchSpecID)
				},
			},
			{
				name: "replaceBatchSpecInput",
				mutationFunc: func(batchSpecID, _ string) string {
					return fmt.Sprintf(`mutation { replaceBatchSpecInput(previousSpec: %q, batchSpec: "name: testing2") { id } }`, batchSpecID)
				},
			},
			{
				name: "retryBatchSpecWorkspaceExecution",
				mutationFunc: func(_, workspaceID string) string {
					return fmt.Sprintf(`mutation { retryBatchSpecWorkspaceExecution(batchSpecWorkspaces: [%q]) { alwaysNil } }`, workspaceID)
				},
			},
			{
				name: "retryBatchSpecExecution",
				mutationFunc: func(batchSpecID, _ string) string {
					return fmt.Sprintf(`mutation { retryBatchSpecExecution(batchSpec: %q) { id } }`, batchSpecID)
				},
			},
			{
				name: "cancelBatchSpecExecution",
				mutationFunc: func(batchSpecID, _ string) string {
					return fmt.Sprintf(`mutation { cancelBatchSpecExecution(batchSpec: %q) { id } }`, batchSpecID)
				},
			},
			// TODO: Uncomment once implemented.
			// {
			// 	name: "cancelBatchSpecWorkspaceExecution",
			// 	mutationFunc: func(_, workspaceID string) string {
			// 		return fmt.Sprintf(`mutation { cancelBatchSpecWorkspaceExecution(batchSpecWorkspaces: [%q]) { alwaysNil } }`, workspaceID)
			// 	},
			// },
			// TODO: Once implemented, add test for EnqueueBatchSpecWorkspaceExecution
			// TODO: Once implemented, add test for ToggleBatchSpecAutoApply
			// TODO: Once implemented, add test for DeleteBatchSpec
		}

		for _, m := range mutations {
			t.Run(m.name, func(t *testing.T) {
				tests := []struct {
					name            string
					currentUser     int32
					batchSpecAuthor int32
					wantAuthErr     bool

					// If batches.restrictToAdmins is enabled, should an error
					// be generated?
					wantDisabledErr bool
				}{
					{
						name:            "unauthorized",
						currentUser:     userID,
						batchSpecAuthor: adminID,
						wantAuthErr:     true,
						wantDisabledErr: true,
					},
					{
						name:            "authorized batch change owner",
						currentUser:     userID,
						batchSpecAuthor: userID,
						wantAuthErr:     false,
						wantDisabledErr: true,
					},
					{
						name:            "authorized site-admin",
						currentUser:     adminID,
						batchSpecAuthor: userID,
						wantAuthErr:     false,
						wantDisabledErr: false,
					},
				}

				for _, tc := range tests {
					for _, restrict := range []bool{true, false} {
						t.Run(fmt.Sprintf("%s restrict: %v", tc.name, restrict), func(t *testing.T) {
							cleanUpBatchChanges(t, cstore)

							batchSpecRandID, batchSpecID := createBatchSpecFromRaw(t, cstore, tc.batchSpecAuthor)
							workspaceID := createBatchSpecWorkspace(t, cstore, batchSpecID)

							mutation := m.mutationFunc(
								string(marshalBatchSpecRandID(batchSpecRandID)),
								string(marshalBatchSpecWorkspaceID(workspaceID)),
							)

							assertAuthorizationResponse(t, ctx, s, nil, mutation, tc.currentUser, restrict, tc.wantDisabledErr, tc.wantAuthErr)
						})
					}
				}
			})
		}
	})

	t.Run("credentials mutations", func(t *testing.T) {
		t.Run("CreateBatchChangesCredential", func(t *testing.T) {
			tests := []struct {
				name        string
				currentUser int32
				user        int32
				wantAuthErr bool
			}{
				{
					name:        "site-admin for other user",
					currentUser: adminID,
					user:        userID,
					wantAuthErr: false,
				},
				{
					name:        "non-site-admin for other user",
					currentUser: userID,
					user:        adminID,
					wantAuthErr: true,
				},
				{
					name:        "non-site-admin for self",
					currentUser: userID,
					user:        userID,
					wantAuthErr: false,
				},

				{
					name:        "site-admin for site-wide",
					currentUser: adminID,
					user:        0,
					wantAuthErr: false,
				},
				{
					name:        "non-site-admin for site-wide",
					currentUser: userID,
					user:        0,
					wantAuthErr: true,
				},
			}

			for _, tc := range tests {
				t.Run(tc.name, func(t *testing.T) {
					pruneUserCredentials(t, db, key)
					pruneSiteCredentials(t, cstore)

					input := map[string]any{
						"externalServiceKind": extsvc.KindGitHub,
						"externalServiceURL":  "https://github.com/",
						"credential":          "SOSECRET",
					}
					if tc.user != 0 {
						input["user"] = graphqlbackend.MarshalUserID(tc.user)
					}
					mutationCreateBatchChangesCredential := `
					mutation($user: ID, $externalServiceKind: ExternalServiceKind!, $externalServiceURL: String!, $credential: String!) {
						createBatchChangesCredential(
							user: $user,
							externalServiceKind: $externalServiceKind,
							externalServiceURL: $externalServiceURL,
							credential: $credential
						) { id }
					}`

					assertAuthorizationResponse(t, ctx, s, input, mutationCreateBatchChangesCredential, tc.currentUser, false, false, tc.wantAuthErr)
				})
			}
		})

		t.Run("DeleteBatchChangesCredential", func(t *testing.T) {
			tests := []struct {
				name        string
				currentUser int32
				user        int32
				wantAuthErr bool
			}{
				{
					name:        "site-admin for other user",
					currentUser: adminID,
					user:        userID,
					wantAuthErr: false,
				},
				{
					name:        "non-site-admin for other user",
					currentUser: userID,
					user:        adminID,
					wantAuthErr: true,
				},
				{
					name:        "non-site-admin for self",
					currentUser: userID,
					user:        userID,
					wantAuthErr: false,
				},

				{
					name:        "site-admin for site-credential",
					currentUser: adminID,
					user:        0,
					wantAuthErr: false,
				},
				{
					name:        "non-site-admin for site-credential",
					currentUser: userID,
					user:        0,
					wantAuthErr: true,
				},
			}

			for _, tc := range tests {
				t.Run(tc.name, func(t *testing.T) {
					pruneUserCredentials(t, db, key)
					pruneSiteCredentials(t, cstore)

					var batchChangesCredentialID graphql.ID
					if tc.user != 0 {
						cred, err := cstore.UserCredentials().Create(ctx, database.UserCredentialScope{
							Domain:              database.UserCredentialDomainBatches,
							ExternalServiceID:   "https://github.com/",
							ExternalServiceType: extsvc.TypeGitHub,
							UserID:              tc.user,
						}, &auth.OAuthBearerToken{Token: "SOSECRET"})
						if err != nil {
							t.Fatal(err)
						}
						batchChangesCredentialID = marshalBatchChangesCredentialID(cred.ID, false)
					} else {
						cred := &btypes.SiteCredential{
							ExternalServiceID:   "https://github.com/",
							ExternalServiceType: extsvc.TypeGitHub,
						}
						token := &auth.OAuthBearerToken{Token: "SOSECRET"}
						if err := cstore.CreateSiteCredential(ctx, cred, token); err != nil {
							t.Fatal(err)
						}
						batchChangesCredentialID = marshalBatchChangesCredentialID(cred.ID, true)
					}

					input := map[string]any{
						"batchChangesCredential": batchChangesCredentialID,
					}
					mutationDeleteBatchChangesCredential := `
					mutation($batchChangesCredential: ID!) {
						deleteBatchChangesCredential(batchChangesCredential: $batchChangesCredential) { alwaysNil }
					}`

					assertAuthorizationResponse(t, ctx, s, input, mutationDeleteBatchChangesCredential, tc.currentUser, false, false, tc.wantAuthErr)
				})
			}
		})
	})
}

func TestRepositoryPermissions(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	db := database.NewDB(dbtest.NewDB(t))

	bstore := store.New(db, &observation.TestContext, nil)
	sr := &Resolver{store: bstore}
	s, err := graphqlbackend.NewSchema(database.NewDB(db), sr, nil, nil, nil, nil, nil, nil, nil, nil, nil, nil)
	if err != nil {
		t.Fatal(err)
	}

	ctx := context.Background()

	testRev := api.CommitID("b69072d5f687b31b9f6ae3ceafdc24c259c4b9ec")
	mockBackendCommits(t, testRev)

	// Global test data that we reuse in every test
	userID := ct.CreateTestUser(t, db, false).ID

	repoStore := database.ReposWith(bstore)
	esStore := database.ExternalServicesWith(bstore)

	// Create 2 repositories
	repos := make([]*types.Repo, 0, 2)
	for i := 0; i < cap(repos); i++ {
		name := fmt.Sprintf("github.com/sourcegraph/test-repository-permissions-repo-%d", i)
		r := newGitHubTestRepo(name, newGitHubExternalService(t, esStore))
		if err := repoStore.Create(ctx, r); err != nil {
			t.Fatal(err)
		}
		repos = append(repos, r)
	}

	t.Run("BatchChange and changesets", func(t *testing.T) {
		// Create 2 changesets for 2 repositories
		changesetBaseRefOid := "f00b4r"
		changesetHeadRefOid := "b4rf00"
		mockRepoComparison(t, changesetBaseRefOid, changesetHeadRefOid, testDiff)
		changesetDiffStat := apitest.DiffStat{Added: 0, Changed: 2, Deleted: 0}

		changesets := make([]*btypes.Changeset, 0, len(repos))
		for _, r := range repos {
			c := &btypes.Changeset{
				RepoID:              r.ID,
				ExternalServiceType: extsvc.TypeGitHub,
				ExternalID:          fmt.Sprintf("external-%d", r.ID),
				ExternalState:       btypes.ChangesetExternalStateOpen,
				ExternalCheckState:  btypes.ChangesetCheckStatePassed,
				ExternalReviewState: btypes.ChangesetReviewStateChangesRequested,
				PublicationState:    btypes.ChangesetPublicationStatePublished,
				ReconcilerState:     btypes.ReconcilerStateCompleted,
				Metadata: &github.PullRequest{
					BaseRefOid: changesetBaseRefOid,
					HeadRefOid: changesetHeadRefOid,
				},
			}
			c.SetDiffStat(changesetDiffStat.ToDiffStat())
			if err := bstore.CreateChangeset(ctx, c); err != nil {
				t.Fatal(err)
			}
			changesets = append(changesets, c)
		}

		spec := &btypes.BatchSpec{
			NamespaceUserID: userID,
			UserID:          userID,
		}
		if err := bstore.CreateBatchSpec(ctx, spec); err != nil {
			t.Fatal(err)
		}

		batchChange := &btypes.BatchChange{
			Name:            "my batch change",
			CreatorID:       userID,
			NamespaceUserID: userID,
			LastApplierID:   userID,
			LastAppliedAt:   time.Now(),
			BatchSpecID:     spec.ID,
		}
		if err := bstore.CreateBatchChange(ctx, batchChange); err != nil {
			t.Fatal(err)
		}
		// We attach the two changesets to the batch change
		for _, c := range changesets {
			c.BatchChanges = []btypes.BatchChangeAssoc{{BatchChangeID: batchChange.ID}}
			if err := bstore.UpdateChangeset(ctx, c); err != nil {
				t.Fatal(err)
			}
		}

		// Query batch change and check that we get all changesets
		userCtx := actor.WithActor(ctx, actor.FromUser(userID))

		input := map[string]any{
			"batchChange": string(marshalBatchChangeID(batchChange.ID)),
		}
		testBatchChangeResponse(t, s, userCtx, input, wantBatchChangeResponse{
			changesetTypes:  map[string]int{"ExternalChangeset": 2},
			changesetsCount: 2,
			changesetStats:  apitest.ChangesetsStats{Open: 2, Total: 2},
			batchChangeDiffStat: apitest.DiffStat{
				Added:   2 * changesetDiffStat.Added,
				Changed: 2 * changesetDiffStat.Changed,
				Deleted: 2 * changesetDiffStat.Deleted,
			},
		})

		for _, c := range changesets {
			// Both changesets are visible still, so both should be ExternalChangesets
			testChangesetResponse(t, s, userCtx, c.ID, "ExternalChangeset")
		}

		// Now we set permissions and filter out the repository of one changeset
		filteredRepo := changesets[0].RepoID
		accessibleRepo := changesets[1].RepoID
		ct.MockRepoPermissions(t, db, userID, accessibleRepo)

		// Send query again and check that for each filtered repository we get a
		// HiddenChangeset
		want := wantBatchChangeResponse{
			changesetTypes: map[string]int{
				"ExternalChangeset":       1,
				"HiddenExternalChangeset": 1,
			},
			changesetsCount: 2,
			changesetStats:  apitest.ChangesetsStats{Open: 2, Total: 2},
			batchChangeDiffStat: apitest.DiffStat{
				Added:   1 * changesetDiffStat.Added,
				Changed: 1 * changesetDiffStat.Changed,
				Deleted: 1 * changesetDiffStat.Deleted,
			},
		}
		testBatchChangeResponse(t, s, userCtx, input, want)

		for _, c := range changesets {
			// The changeset whose repository has been filtered should be hidden
			if c.RepoID == filteredRepo {
				testChangesetResponse(t, s, userCtx, c.ID, "HiddenExternalChangeset")
			} else {
				testChangesetResponse(t, s, userCtx, c.ID, "ExternalChangeset")
			}
		}

		// Now we query with more filters for the changesets. The hidden changesets
		// should not be returned, since that would leak information about the
		// hidden changesets.
		input = map[string]any{
			"batchChange": string(marshalBatchChangeID(batchChange.ID)),
			"checkState":  string(btypes.ChangesetCheckStatePassed),
		}
		wantCheckStateResponse := want
		wantCheckStateResponse.changesetsCount = 1
		wantCheckStateResponse.changesetTypes = map[string]int{
			"ExternalChangeset": 1,
			// No HiddenExternalChangeset
		}
		testBatchChangeResponse(t, s, userCtx, input, wantCheckStateResponse)

		input = map[string]any{
			"batchChange": string(marshalBatchChangeID(batchChange.ID)),
			"reviewState": string(btypes.ChangesetReviewStateChangesRequested),
		}
		wantReviewStateResponse := wantCheckStateResponse
		testBatchChangeResponse(t, s, userCtx, input, wantReviewStateResponse)
	})

	t.Run("BatchSpec and changesetSpecs", func(t *testing.T) {
		batchSpec := &btypes.BatchSpec{
			UserID:          userID,
			NamespaceUserID: userID,
			Spec:            &batcheslib.BatchSpec{Name: "batch-spec-and-changeset-specs"},
		}
		if err := bstore.CreateBatchSpec(ctx, batchSpec); err != nil {
			t.Fatal(err)
		}

		changesetSpecs := make([]*btypes.ChangesetSpec, 0, len(repos))
		for _, r := range repos {
			c := &btypes.ChangesetSpec{
				RepoID:          r.ID,
				UserID:          userID,
				BatchSpecID:     batchSpec.ID,
				DiffStatAdded:   4,
				DiffStatChanged: 4,
				DiffStatDeleted: 4,
			}
			if err := bstore.CreateChangesetSpec(ctx, c); err != nil {
				t.Fatal(err)
			}
			changesetSpecs = append(changesetSpecs, c)
		}

		// Query BatchSpec and check that we get all changesetSpecs
		userCtx := actor.WithActor(ctx, actor.FromUser(userID))
		testBatchSpecResponse(t, s, userCtx, batchSpec.RandID, wantBatchSpecResponse{
			changesetSpecTypes:    map[string]int{"VisibleChangesetSpec": 2},
			changesetSpecsCount:   2,
			changesetPreviewTypes: map[string]int{"VisibleChangesetApplyPreview": 2},
			changesetPreviewCount: 2,
			batchSpecDiffStat: apitest.DiffStat{
				Added: 8, Changed: 8, Deleted: 8,
			},
		})

		// Now query the changesetSpecs as single nodes, to make sure that fetching/preloading
		// of repositories works
		for _, c := range changesetSpecs {
			// Both changesetSpecs are visible still, so both should be VisibleChangesetSpec
			testChangesetSpecResponse(t, s, userCtx, c.RandID, "VisibleChangesetSpec")
		}

		// Now we set permissions and filter out the repository of one changeset
		filteredRepo := changesetSpecs[0].RepoID
		accessibleRepo := changesetSpecs[1].RepoID
		ct.MockRepoPermissions(t, db, userID, accessibleRepo)

		// Send query again and check that for each filtered repository we get a
		// HiddenChangesetSpec.
		testBatchSpecResponse(t, s, userCtx, batchSpec.RandID, wantBatchSpecResponse{
			changesetSpecTypes: map[string]int{
				"VisibleChangesetSpec": 1,
				"HiddenChangesetSpec":  1,
			},
			changesetSpecsCount:   2,
			changesetPreviewTypes: map[string]int{"VisibleChangesetApplyPreview": 1, "HiddenChangesetApplyPreview": 1},
			changesetPreviewCount: 2,
			batchSpecDiffStat: apitest.DiffStat{
				Added: 4, Changed: 4, Deleted: 4,
			},
		})

		// Query the single changesetSpec nodes again
		for _, c := range changesetSpecs {
			// The changesetSpec whose repository has been filtered should be hidden
			if c.RepoID == filteredRepo {
				testChangesetSpecResponse(t, s, userCtx, c.RandID, "HiddenChangesetSpec")
			} else {
				testChangesetSpecResponse(t, s, userCtx, c.RandID, "VisibleChangesetSpec")
			}
		}
	})

	t.Run("BatchSpec and workspaces", func(t *testing.T) {
		batchSpec := &btypes.BatchSpec{
			UserID:          userID,
			NamespaceUserID: userID,
			CreatedFromRaw:  true,
			Spec:            &batcheslib.BatchSpec{Name: "batch-spec-and-changeset-specs"},
		}
		if err := bstore.CreateBatchSpec(ctx, batchSpec); err != nil {
			t.Fatal(err)
		}

		if err := bstore.CreateBatchSpecResolutionJob(ctx, &btypes.BatchSpecResolutionJob{
			BatchSpecID: batchSpec.ID,
			InitiatorID: userID,
			State:       btypes.BatchSpecResolutionJobStateCompleted,
		}); err != nil {
			t.Fatal(err)
		}

		workspaces := make([]*btypes.BatchSpecWorkspace, 0, len(repos))
		for _, r := range repos {
			w := &btypes.BatchSpecWorkspace{
				RepoID:      r.ID,
				BatchSpecID: batchSpec.ID,
			}
			if err := bstore.CreateBatchSpecWorkspace(ctx, w); err != nil {
				t.Fatal(err)
			}
			workspaces = append(workspaces, w)
		}

		// Query BatchSpec and check that we get all workspaces
		userCtx := actor.WithActor(ctx, actor.FromUser(userID))
		testBatchSpecWorkspacesResponse(t, s, userCtx, batchSpec.RandID, wantBatchSpecWorkspacesResponse{
			types: map[string]int{"VisibleBatchSpecWorkspace": 2},
			count: 2,
		})

		// Now query the workspaces as single nodes, to make sure that fetching/preloading
		// of repositories works.
		for _, w := range workspaces {
			// Both workspaces are visible still, so both should be VisibleBatchSpecWorkspace
			testWorkspaceResponse(t, s, userCtx, w.ID, "VisibleBatchSpecWorkspace")
		}

		// Now we set permissions and filter out the repository of one workspace.
		filteredRepo := workspaces[0].RepoID
		accessibleRepo := workspaces[1].RepoID
		ct.MockRepoPermissions(t, db, userID, accessibleRepo)

		// Send query again and check that for each filtered repository we get a
		// HiddenBatchSpecWorkspace.
		testBatchSpecWorkspacesResponse(t, s, userCtx, batchSpec.RandID, wantBatchSpecWorkspacesResponse{
			types: map[string]int{
				"VisibleBatchSpecWorkspace": 1,
				"HiddenBatchSpecWorkspace":  1,
			},
			count: 2,
		})

		// Query the single workspace nodes again.
		for _, w := range workspaces {
			// The workspace whose repository has been filtered should be hidden.
			if w.RepoID == filteredRepo {
				testWorkspaceResponse(t, s, userCtx, w.ID, "HiddenBatchSpecWorkspace")
			} else {
				testWorkspaceResponse(t, s, userCtx, w.ID, "VisibleBatchSpecWorkspace")
			}
		}
	})
}

type wantBatchChangeResponse struct {
	changesetTypes      map[string]int
	changesetsCount     int
	changesetStats      apitest.ChangesetsStats
	batchChangeDiffStat apitest.DiffStat
}

func testBatchChangeResponse(t *testing.T, s *graphql.Schema, ctx context.Context, in map[string]any, w wantBatchChangeResponse) {
	t.Helper()

	var response struct{ Node apitest.BatchChange }
	apitest.MustExec(ctx, t, s, in, &response, queryBatchChangePermLevels)

	if have, want := response.Node.ID, in["batchChange"]; have != want {
		t.Fatalf("batch change id is wrong. have %q, want %q", have, want)
	}

	if diff := cmp.Diff(w.changesetsCount, response.Node.Changesets.TotalCount); diff != "" {
		t.Fatalf("unexpected changesets total count (-want +got):\n%s", diff)
	}

	if diff := cmp.Diff(w.changesetStats, response.Node.ChangesetsStats); diff != "" {
		t.Fatalf("unexpected changesets stats (-want +got):\n%s", diff)
	}

	changesetTypes := map[string]int{}
	for _, c := range response.Node.Changesets.Nodes {
		changesetTypes[c.Typename]++
	}
	if diff := cmp.Diff(w.changesetTypes, changesetTypes); diff != "" {
		t.Fatalf("unexpected changesettypes (-want +got):\n%s", diff)
	}

	if diff := cmp.Diff(w.batchChangeDiffStat, response.Node.DiffStat); diff != "" {
		t.Fatalf("unexpected batch change diff stat (-want +got):\n%s", diff)
	}
}

const queryBatchChangePermLevels = `
query($batchChange: ID!, $reviewState: ChangesetReviewState, $checkState: ChangesetCheckState) {
  node(id: $batchChange) {
    ... on BatchChange {
	  id

	  changesetsStats { unpublished, open, merged, closed, total }

      changesets(first: 100, reviewState: $reviewState, checkState: $checkState) {
        totalCount
        nodes {
          __typename
          ... on HiddenExternalChangeset {
            id
          }
          ... on ExternalChangeset {
            id
            repository {
              id
              name
            }
          }
        }
      }

      diffStat {
        added
        changed
        deleted
      }
    }
  }
}
`

func testChangesetResponse(t *testing.T, s *graphql.Schema, ctx context.Context, id int64, wantType string) {
	t.Helper()

	var res struct{ Node apitest.Changeset }
	query := fmt.Sprintf(queryChangesetPermLevels, marshalChangesetID(id))
	apitest.MustExec(ctx, t, s, nil, &res, query)

	if have, want := res.Node.Typename, wantType; have != want {
		t.Fatalf("changeset has wrong typename. want=%q, have=%q", want, have)
	}

	if have, want := res.Node.State, string(btypes.ChangesetStateOpen); have != want {
		t.Fatalf("changeset has wrong state. want=%q, have=%q", want, have)
	}

	if have, want := res.Node.BatchChanges.TotalCount, 1; have != want {
		t.Fatalf("changeset has wrong batch changes totalcount. want=%d, have=%d", want, have)
	}

	if parseJSONTime(t, res.Node.CreatedAt).IsZero() {
		t.Fatalf("changeset createdAt is zero")
	}

	if parseJSONTime(t, res.Node.UpdatedAt).IsZero() {
		t.Fatalf("changeset updatedAt is zero")
	}

	if parseJSONTime(t, res.Node.NextSyncAt).IsZero() {
		t.Fatalf("changeset next sync at is zero")
	}
}

const queryChangesetPermLevels = `
query {
  node(id: %q) {
    __typename

    ... on HiddenExternalChangeset {
      id

	  state
	  createdAt
	  updatedAt
	  nextSyncAt
	  batchChanges {
	    totalCount
	  }
    }
    ... on ExternalChangeset {
      id

	  state
	  createdAt
	  updatedAt
	  nextSyncAt
	  batchChanges {
	    totalCount
	  }

      repository {
        id
        name
      }
    }
  }
}
`

type wantBatchSpecWorkspacesResponse struct {
	types map[string]int
	count int
}

func testBatchSpecWorkspacesResponse(t *testing.T, s *graphql.Schema, ctx context.Context, batchSpecRandID string, w wantBatchSpecWorkspacesResponse) {
	t.Helper()

	in := map[string]any{
		"batchSpec": string(marshalBatchSpecRandID(batchSpecRandID)),
	}

	var response struct{ Node apitest.BatchSpec }
	apitest.MustExec(ctx, t, s, in, &response, queryBatchSpecWorkspaces)

	if have, want := response.Node.ID, in["batchSpec"]; have != want {
		t.Fatalf("batch spec id is wrong. have %q, want %q", have, want)
	}

	if diff := cmp.Diff(w.count, response.Node.WorkspaceResolution.Workspaces.TotalCount); diff != "" {
		t.Fatalf("unexpected workspaces total count (-want +got):\n%s", diff)
	}

	types := map[string]int{}
	for _, c := range response.Node.WorkspaceResolution.Workspaces.Nodes {
		types[c.Typename]++
	}
	if diff := cmp.Diff(w.types, types); diff != "" {
		t.Fatalf("unexpected workspace types (-want +got):\n%s", diff)
	}
}

const queryBatchSpecWorkspaces = `
query($batchSpec: ID!) {
  node(id: $batchSpec) {
    ... on BatchSpec {
      id

     workspaceResolution {
        workspaces(first: 100) {
          totalCount
          nodes {
            __typename
            ... on HiddenBatchSpecWorkspace {
              id
            }

            ... on VisibleBatchSpecWorkspace {
              id
            }
          }
        }
      }
    }
  }
}
`

func testWorkspaceResponse(t *testing.T, s *graphql.Schema, ctx context.Context, id int64, wantType string) {
	t.Helper()

	var res struct{ Node apitest.BatchSpecWorkspace }
	query := fmt.Sprintf(queryWorkspacePerm, marshalBatchSpecWorkspaceID(id))
	apitest.MustExec(ctx, t, s, nil, &res, query)

	if have, want := res.Node.Typename, wantType; have != want {
		t.Fatalf("changeset has wrong typename. want=%q, have=%q", want, have)
	}

	if wantType == "HiddenBatchSpecWorkspace" {
		if res.Node.Repository.ID != "" {
			t.Fatal("includes repo but shouldn't")
		}
	}
}

const queryWorkspacePerm = `
query {
  node(id: %q) {
    __typename

    ... on HiddenBatchSpecWorkspace {
      id
    }
    ... on VisibleBatchSpecWorkspace {
      id
      repository {
        id
        name
      }
    }
  }
}
`

type wantBatchSpecResponse struct {
	changesetPreviewTypes map[string]int
	changesetPreviewCount int
	changesetSpecTypes    map[string]int
	changesetSpecsCount   int
	batchSpecDiffStat     apitest.DiffStat
}

func testBatchSpecResponse(t *testing.T, s *graphql.Schema, ctx context.Context, batchSpecRandID string, w wantBatchSpecResponse) {
	t.Helper()

	in := map[string]any{
		"batchSpec": string(marshalBatchSpecRandID(batchSpecRandID)),
	}

	var response struct{ Node apitest.BatchSpec }
	apitest.MustExec(ctx, t, s, in, &response, queryBatchSpecPermLevels)

	if have, want := response.Node.ID, in["batchSpec"]; have != want {
		t.Fatalf("batch spec id is wrong. have %q, want %q", have, want)
	}

	if diff := cmp.Diff(w.changesetSpecsCount, response.Node.ChangesetSpecs.TotalCount); diff != "" {
		t.Fatalf("unexpected changesetSpecs total count (-want +got):\n%s", diff)
	}

	if diff := cmp.Diff(w.changesetPreviewCount, response.Node.ApplyPreview.TotalCount); diff != "" {
		t.Fatalf("unexpected applyPreview total count (-want +got):\n%s", diff)
	}

	changesetSpecTypes := map[string]int{}
	for _, c := range response.Node.ChangesetSpecs.Nodes {
		changesetSpecTypes[c.Typename]++
	}
	if diff := cmp.Diff(w.changesetSpecTypes, changesetSpecTypes); diff != "" {
		t.Fatalf("unexpected changesetSpec types (-want +got):\n%s", diff)
	}

	changesetPreviewTypes := map[string]int{}
	for _, c := range response.Node.ApplyPreview.Nodes {
		changesetPreviewTypes[c.Typename]++
	}
	if diff := cmp.Diff(w.changesetPreviewTypes, changesetPreviewTypes); diff != "" {
		t.Fatalf("unexpected applyPreview types (-want +got):\n%s", diff)
	}
}

const queryBatchSpecPermLevels = `
query($batchSpec: ID!) {
  node(id: $batchSpec) {
    ... on BatchSpec {
      id

      applyPreview(first: 100) {
        totalCount
        nodes {
          __typename
          ... on HiddenChangesetApplyPreview {
              targets {
                  __typename
              }
          }
          ... on VisibleChangesetApplyPreview {
              targets {
                  __typename
              }
          }
        }
      }
      changesetSpecs(first: 100) {
        totalCount
        nodes {
          __typename
          type
          ... on HiddenChangesetSpec {
            id
          }

          ... on VisibleChangesetSpec {
            id

            description {
              ... on ExistingChangesetReference {
                baseRepository {
                  id
                  name
                }
              }

              ... on GitBranchChangesetDescription {
                baseRepository {
                  id
                  name
                }
              }
            }
          }
        }
      }
    }
  }
}
`

func testChangesetSpecResponse(t *testing.T, s *graphql.Schema, ctx context.Context, randID, wantType string) {
	t.Helper()

	var res struct{ Node apitest.ChangesetSpec }
	query := fmt.Sprintf(queryChangesetSpecPermLevels, marshalChangesetSpecRandID(randID))
	apitest.MustExec(ctx, t, s, nil, &res, query)

	if have, want := res.Node.Typename, wantType; have != want {
		t.Fatalf("changesetspec has wrong typename. want=%q, have=%q", want, have)
	}
}

const queryChangesetSpecPermLevels = `
query {
  node(id: %q) {
    __typename

    ... on HiddenChangesetSpec {
      id
      type
    }

    ... on VisibleChangesetSpec {
      id
      type

      description {
        ... on ExistingChangesetReference {
          baseRepository {
            id
            name
          }
        }

        ... on GitBranchChangesetDescription {
          baseRepository {
            id
            name
          }
        }
      }
    }
  }
}
`

func assertAuthorizationResponse(
	t *testing.T,
	ctx context.Context,
	s *graphql.Schema,
	input map[string]any,
	mutation string,
	userID int32,
	restrictToAdmins, wantDisabledErr, wantAuthErr bool,
) {
	t.Helper()

	actorCtx := actor.WithActor(ctx, actor.FromUser(userID))

	conf.Mock(&conf.Unified{
		SiteConfiguration: schema.SiteConfiguration{
			BatchChangesRestrictToAdmins: &restrictToAdmins,
		},
	})
	defer conf.Mock(nil)

	var response struct{}
	errs := apitest.Exec(actorCtx, t, s, input, &response, mutation)

	errLooksLikeAuthErr := func(err error) bool {
		return strings.Contains(err.Error(), "must be authenticated") ||
			strings.Contains(err.Error(), "not authenticated") ||
			strings.Contains(err.Error(), "must be site admin")
	}

	// We don't care about other errors, we only want to
	// check that we didn't get an auth error.
	if restrictToAdmins && wantDisabledErr {
		if len(errs) != 1 {
			t.Fatalf("expected 1 error, but got %d: %s", len(errs), errs)
		}
		if !strings.Contains(errs[0].Error(), "batch changes are disabled for non-site-admin users") {
			t.Fatalf("wrong error: %s %T", errs[0], errs[0])
		}
	} else if wantAuthErr {
		if len(errs) != 1 {
			t.Fatalf("expected 1 error, but got %d: %s", len(errs), errs)
		}
		if !errLooksLikeAuthErr(errs[0]) {
			t.Fatalf("wrong error: %s %T", errs[0], errs[0])
		}
	} else {
		// We don't care about other errors, we only
		// want to check that we didn't get an auth
		// or site admin error.
		for _, e := range errs {
			if errLooksLikeAuthErr(e) {
				t.Fatalf("auth error wrongly returned: %s %T", errs[0], errs[0])
			} else if strings.Contains(e.Error(), "batch changes are disabled for non-site-admin users") {
				t.Fatalf("site admin error wrongly returned: %s %T", errs[0], errs[0])
			}
		}
	}
}
