package resolvers

import (
	"context"
	"database/sql"
	"fmt"
	"io"
	"io/ioutil"
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/graph-gophers/graphql-go"
	"github.com/keegancsmith/sqlf"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/authz"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/cmd/repo-updater/repos"
	ee "github.com/sourcegraph/sourcegraph/enterprise/internal/campaigns"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/campaigns/resolvers/apitest"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/campaigns"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/db/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/db/dbtesting"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestPermissionLevels(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	dbtesting.SetupGlobalTestDB(t)

	store := ee.NewStore(dbconn.Global)
	sr := &Resolver{store: store}
	s, err := graphqlbackend.NewSchema(sr, nil, nil)
	if err != nil {
		t.Fatal(err)
	}

	ctx := context.Background()

	// Global test data that we reuse in every test
	adminID := insertTestUser(t, dbconn.Global, "perm-level-admin", true)
	userID := insertTestUser(t, dbconn.Global, "perm-level-user", false)

	reposStore := repos.NewDBStore(dbconn.Global, sql.TxOptions{})
	repo := newGitHubTestRepo("github.com/sourcegraph/sourcegraph", 1)
	if err := reposStore.UpsertRepos(ctx, repo); err != nil {
		t.Fatal(err)
	}

	changeset := &campaigns.Changeset{
		RepoID:              repo.ID,
		ExternalServiceType: "github",
		ExternalID:          "1234",
	}
	if err := store.CreateChangesets(ctx, changeset); err != nil {
		t.Fatal(err)
	}

	createTestData := func(t *testing.T, s *ee.Store, name string, userID int32) (campaignID int64, patchID int64) {
		t.Helper()

		patchSet := &campaigns.PatchSet{UserID: userID}
		if err := s.CreatePatchSet(ctx, patchSet); err != nil {
			t.Fatal(err)
		}

		patch := &campaigns.Patch{
			PatchSetID: patchSet.ID,
			RepoID:     repo.ID,
			BaseRef:    "refs/heads/master",
		}
		if err := s.CreatePatch(ctx, patch); err != nil {
			t.Fatal(err)
		}

		c := &campaigns.Campaign{
			PatchSetID:      patchSet.ID,
			Name:            name,
			AuthorID:        userID,
			NamespaceUserID: userID,
			// We attach the changeset to the campaign so we can test syncChangeset
			ChangesetIDs: []int64{changeset.ID},
		}
		if err := s.CreateCampaign(ctx, c); err != nil {
			t.Fatal(err)
		}

		job := &campaigns.ChangesetJob{CampaignID: c.ID, PatchID: patch.ID, Error: "This is an error"}
		if err := s.CreateChangesetJob(ctx, job); err != nil {
			t.Fatal(err)
		}

		return c.ID, patch.ID
	}

	cleanUpCampaigns := func(t *testing.T, s *ee.Store) {
		t.Helper()

		campaigns, next, err := store.ListCampaigns(ctx, ee.ListCampaignsOpts{Limit: 1000})
		if err != nil {
			t.Fatal(err)
		}
		if next != 0 {
			t.Fatalf("more campaigns in store")
		}

		for _, c := range campaigns {
			if err := store.DeleteCampaign(ctx, c.ID); err != nil {
				t.Fatal(err)
			}
		}
	}

	t.Run("queries", func(t *testing.T) {
		// We need to enable read access so that non-site-admin users can access
		// the API and we can check for their admin rights.
		// This can be removed once we enable campaigns for all users and only
		// check for permissions.
		readAccessEnabled := true
		conf.Mock(&conf.Unified{SiteConfiguration: schema.SiteConfiguration{
			CampaignsReadAccessEnabled: &readAccessEnabled,
		}})
		defer conf.Mock(nil)

		cleanUpCampaigns(t, store)

		adminCampaign, _ := createTestData(t, store, "admin", adminID)
		userCampaign, _ := createTestData(t, store, "user", userID)

		tests := []struct {
			name                    string
			currentUser             int32
			campaign                int64
			wantViewerCanAdminister bool
			wantErrors              []string
		}{
			{
				name:                    "site-admin viewing own campaign",
				currentUser:             adminID,
				campaign:                adminCampaign,
				wantViewerCanAdminister: true,
				wantErrors:              []string{"This is an error"},
			},
			{
				name:                    "non-site-admin viewing other's campaign",
				currentUser:             userID,
				campaign:                adminCampaign,
				wantViewerCanAdminister: false,
				wantErrors:              []string{},
			},
			{
				name:                    "site-admin viewing other's campaign",
				currentUser:             adminID,
				campaign:                userCampaign,
				wantViewerCanAdminister: true,
				wantErrors:              []string{"This is an error"},
			},
			{
				name:                    "non-site-admin viewing own campaign",
				currentUser:             userID,
				campaign:                userCampaign,
				wantViewerCanAdminister: true,
				wantErrors:              []string{"This is an error"},
			},
		}

		for _, tc := range tests {
			t.Run(tc.name, func(t *testing.T) {
				graphqlID := string(campaigns.MarshalCampaignID(tc.campaign))

				var res struct{ Node apitest.Campaign }

				input := map[string]interface{}{"campaign": graphqlID}
				queryCampaign := `
				  query($campaign: ID!) {
				    node(id: $campaign) { ... on Campaign { id, viewerCanAdminister, status { errors } } }
				  }
                `

				actorCtx := actor.WithActor(ctx, actor.FromUser(tc.currentUser))
				apitest.MustExec(actorCtx, t, s, input, &res, queryCampaign)

				if have, want := res.Node.ID, graphqlID; have != want {
					t.Fatalf("queried campaign has wrong id %q, want %q", have, want)
				}
				if have, want := res.Node.ViewerCanAdminister, tc.wantViewerCanAdminister; have != want {
					t.Fatalf("queried campaign's ViewerCanAdminister is wrong %t, want %t", have, want)
				}
				if diff := cmp.Diff(res.Node.Status.Errors, tc.wantErrors); diff != "" {
					t.Fatalf("queried campaign's Errors is wrong: %s", diff)
				}
			})
		}

		t.Run("Campaigns", func(t *testing.T) {
			tests := []struct {
				name                string
				currentUser         int32
				viewerCanAdminister bool
				wantCampaigns       []int64
			}{
				{
					name:                "admin listing viewerCanAdminister: true",
					currentUser:         adminID,
					viewerCanAdminister: true,
					wantCampaigns:       []int64{adminCampaign, userCampaign},
				},
				{
					name:                "user listing viewerCanAdminister: true",
					currentUser:         userID,
					viewerCanAdminister: true,
					wantCampaigns:       []int64{userCampaign},
				},
				{
					name:                "admin listing viewerCanAdminister: false",
					currentUser:         adminID,
					viewerCanAdminister: false,
					wantCampaigns:       []int64{adminCampaign, userCampaign},
				},
				{
					name:                "user listing viewerCanAdminister: false",
					currentUser:         userID,
					viewerCanAdminister: false,
					wantCampaigns:       []int64{adminCampaign, userCampaign},
				},
			}
			for _, tc := range tests {
				t.Run(tc.name, func(t *testing.T) {
					actorCtx := actor.WithActor(context.Background(), actor.FromUser(tc.currentUser))
					expectedIDs := make(map[string]bool, len(tc.wantCampaigns))
					for _, c := range tc.wantCampaigns {
						graphqlID := string(campaigns.MarshalCampaignID(c))
						expectedIDs[graphqlID] = true
					}

					query := fmt.Sprintf(`
				query {
					campaigns(viewerCanAdminister: %t) { totalCount, nodes { id } }
					node(id: %q) {
						id
						... on ExternalChangeset {
							campaigns(viewerCanAdminister: %t) { totalCount, nodes { id } }
						}
					}
					}`, tc.viewerCanAdminister, marshalExternalChangesetID(changeset.ID), tc.viewerCanAdminister)
					var res struct {
						Campaigns apitest.CampaignConnection
						Node      apitest.Changeset
					}
					apitest.MustExec(actorCtx, t, s, nil, &res, query)
					for _, conn := range []apitest.CampaignConnection{res.Campaigns, res.Node.Campaigns} {
						if have, want := conn.TotalCount, len(tc.wantCampaigns); have != want {
							t.Fatalf("wrong count of campaigns returned, want=%d have=%d", want, have)
						}
						if have, want := conn.TotalCount, len(conn.Nodes); have != want {
							t.Fatalf("totalCount and nodes length don't match, want=%d have=%d", want, have)
						}
						for _, node := range conn.Nodes {
							if _, ok := expectedIDs[node.ID]; !ok {
								t.Fatalf("received wrong campaign with id %q", node.ID)
							}
						}
					}
				})
			}
		})
	})

	t.Run("mutations", func(t *testing.T) {
		mutations := []struct {
			name         string
			mutationFunc func(campaignID string, changesetID string, patchID string) string
		}{
			{
				name: "closeCampaign",
				mutationFunc: func(campaignID string, changesetID string, patchID string) string {
					return fmt.Sprintf(`mutation { closeCampaign(campaign: %q, closeChangesets: false) { id } }`, campaignID)
				},
			},
			{
				name: "deleteCampaign",
				mutationFunc: func(campaignID string, changesetID string, patchID string) string {
					return fmt.Sprintf(`mutation { deleteCampaign(campaign: %q, closeChangesets: false) { alwaysNil } } `, campaignID)
				},
			},
			{
				name: "retryCampaignChangesets",
				mutationFunc: func(campaignID string, changesetID string, patchID string) string {
					return fmt.Sprintf(`mutation { retryCampaignChangesets(campaign: %q) { id } }`, campaignID)
				},
			},
			{
				name: "updateCampaign",
				mutationFunc: func(campaignID string, changesetID string, patchID string) string {
					return fmt.Sprintf(`mutation { updateCampaign(input: {id: %q, name: "new name"}) { id } }`, campaignID)
				},
			},
			{
				name: "addChangesetsToCampaign",
				mutationFunc: func(campaignID string, changesetID string, patchID string) string {
					return fmt.Sprintf(
						`mutation { addChangesetsToCampaign(campaign: %q, changesets: [%q]) { id } }`,
						campaignID,
						changesetID,
					)
				},
			},
			{
				name: "publishCampaignChangesets",
				mutationFunc: func(campaignID string, changesetID string, patchID string) string {
					return fmt.Sprintf(
						`mutation { publishCampaignChangesets(campaign: %q) { id } }`,
						campaignID,
					)
				},
			},
			{
				name: "publishChangeset",
				mutationFunc: func(campaignID string, changesetID string, patchID string) string {
					return fmt.Sprintf(
						`mutation { publishChangeset(patch: %q) { alwaysNil } }`,
						patchID,
					)
				},
			},
			{
				name: "syncChangeset",
				mutationFunc: func(campaignID string, changesetID string, patchID string) string {
					return fmt.Sprintf(
						`mutation { syncChangeset(changeset: %q) { alwaysNil } }`,
						changesetID,
					)
				},
			},
		}

		for _, m := range mutations {
			t.Run(m.name, func(t *testing.T) {
				tests := []struct {
					name           string
					currentUser    int32
					campaignAuthor int32
					wantAuthErr    bool
				}{
					{
						name:           "unauthorized",
						currentUser:    userID,
						campaignAuthor: adminID,
						wantAuthErr:    true,
					},
					{
						name:           "authorized campaign owner",
						currentUser:    userID,
						campaignAuthor: userID,
						wantAuthErr:    false,
					},
					{
						name:           "authorized site-admin",
						currentUser:    adminID,
						campaignAuthor: userID,
						wantAuthErr:    false,
					},
				}

				for _, tc := range tests {
					t.Run(tc.name, func(t *testing.T) {
						cleanUpCampaigns(t, store)

						campaignID, patchID := createTestData(t, store, "test-campaign", tc.campaignAuthor)

						// We add the changeset to the campaign. It doesn't matter
						// for the addChangesetsToCampaign mutation, since that is
						// idempotent and we want to solely check for auth errors.
						changeset.CampaignIDs = []int64{campaignID}
						if err := store.UpdateChangesets(ctx, changeset); err != nil {
							t.Fatal(err)
						}

						mutation := m.mutationFunc(
							string(campaigns.MarshalCampaignID(campaignID)),
							string(marshalExternalChangesetID(changeset.ID)),
							string(marshalPatchID(patchID)),
						)

						actorCtx := actor.WithActor(ctx, actor.FromUser(tc.currentUser))

						var response struct{}
						errs := apitest.Exec(actorCtx, t, s, nil, &response, mutation)

						if tc.wantAuthErr {
							if len(errs) != 1 {
								t.Fatalf("expected 1 error, but got %d: %s", len(errs), errs)
							}
							if !strings.Contains(errs[0].Error(), "must be authenticated") {
								t.Fatalf("wrong error: %s %T", errs[0], errs[0])
							}
						} else {
							// We don't care about other errors, we only want to
							// check that we didn't get an auth error.
							for _, e := range errs {
								if strings.Contains(e.Error(), "must be authenticated") {
									t.Fatalf("auth error wrongly returned: %s %T", errs[0], errs[0])
								}
							}
						}
					})
				}
			})
		}
	})
}

func TestRepositoryPermissions(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	now := time.Now().UTC().Truncate(time.Microsecond)

	// We need to enable read access so that non-site-admin users can access
	// the API and we can check for their admin rights.
	// This can be removed once we enable campaigns for all users and only
	// check for permissions.
	readAccessEnabled := true
	conf.Mock(&conf.Unified{SiteConfiguration: schema.SiteConfiguration{
		CampaignsReadAccessEnabled: &readAccessEnabled,
	}})
	defer conf.Mock(nil)

	dbtesting.SetupGlobalTestDB(t)

	store := ee.NewStore(dbconn.Global)
	sr := &Resolver{store: store}
	s, err := graphqlbackend.NewSchema(sr, nil, nil)
	if err != nil {
		t.Fatal(err)
	}

	ctx := context.Background()

	testRev := "b69072d5f687b31b9f6ae3ceafdc24c259c4b9ec"
	mockBackendCommit(t, testRev)

	// Global test data that we reuse in every test
	userID := insertTestUser(t, dbconn.Global, "perm-level-user", false)

	reposStore := repos.NewDBStore(dbconn.Global, sql.TxOptions{})

	// Create 4 repositories
	repos := make([]*repos.Repo, 0, 4)
	for i := 0; i < cap(repos); i++ {
		name := fmt.Sprintf("github.com/sourcegraph/repo-%d", i)
		r := newGitHubTestRepo(name, i)
		if err := reposStore.UpsertRepos(ctx, r); err != nil {
			t.Fatal(err)
		}
		repos = append(repos, r)
	}

	// Create 2 changesets for 2 repositories
	changesetBaseRefOid := "f00b4r"
	changesetHeadRefOid := "b4rf00"
	mockRepoComparison(t, changesetBaseRefOid, changesetHeadRefOid, testDiff)
	changesetDiffStat := apitest.DiffStat{Added: 0, Changed: 2, Deleted: 0}

	changesets := make([]*campaigns.Changeset, 0, 2)
	changesetIDs := make([]int64, 0, cap(changesets))
	for _, r := range repos[0:2] {
		c := &campaigns.Changeset{
			RepoID:              r.ID,
			ExternalServiceType: "github",
			ExternalID:          fmt.Sprintf("external-%d", r.ID),
			ExternalState:       campaigns.ChangesetStateOpen,
			Metadata: &github.PullRequest{
				BaseRefOid: changesetBaseRefOid,
				HeadRefOid: changesetHeadRefOid,
			},
		}
		if err := store.CreateChangesets(ctx, c); err != nil {
			t.Fatal(err)
		}
		changesets = append(changesets, c)
		changesetIDs = append(changesetIDs, c.ID)
	}

	patchSet := &campaigns.PatchSet{UserID: userID}
	if err := store.CreatePatchSet(ctx, patchSet); err != nil {
		t.Fatal(err)
	}

	// Create 2 patches for the other repositories
	patches := make([]*campaigns.Patch, 0, 2)
	patchesDiffStat := apitest.DiffStat{Added: 88, Changed: 66, Deleted: 22}
	for _, r := range repos[2:4] {
		p := &campaigns.Patch{
			PatchSetID:      patchSet.ID,
			RepoID:          r.ID,
			Rev:             api.CommitID(testRev),
			BaseRef:         "refs/heads/master",
			Diff:            "+ foo - bar",
			DiffStatAdded:   &patchesDiffStat.Added,
			DiffStatChanged: &patchesDiffStat.Changed,
			DiffStatDeleted: &patchesDiffStat.Deleted,
		}
		if err := store.CreatePatch(ctx, p); err != nil {
			t.Fatal(err)
		}
		patches = append(patches, p)
	}

	campaign := &campaigns.Campaign{
		PatchSetID:      patchSet.ID,
		Name:            "my campaign",
		AuthorID:        userID,
		NamespaceUserID: userID,
		// We attach the two changesets to the campaign
		// Note: we are mixing a "manual" and "non-manual" campaign here, but
		// that shouldn't matter for the purposes of this test.
		ChangesetIDs: changesetIDs,
	}
	if err := store.CreateCampaign(ctx, campaign); err != nil {
		t.Fatal(err)
	}
	for _, c := range changesets {
		c.CampaignIDs = []int64{campaign.ID}
	}
	if err := store.UpdateChangesets(ctx, changesets...); err != nil {
		t.Fatal(err)
	}

	// Create 2 failed ChangesetJobs for the patchess to produce error messages
	// on the campaign.
	changesetJobs := make([]*campaigns.ChangesetJob, 0, 2)
	for _, p := range patches {
		job := &campaigns.ChangesetJob{
			CampaignID: campaign.ID,
			PatchID:    p.ID,
			Error:      fmt.Sprintf("error patch %d", p.ID),
			StartedAt:  now,
			FinishedAt: now,
		}
		if err := store.CreateChangesetJob(ctx, job); err != nil {
			t.Fatal(err)
		}

		changesetJobs = append(changesetJobs, job)
	}

	// Query campaign and check that we get all changesets and all patches
	userCtx := actor.WithActor(ctx, actor.FromUser(userID))
	testCampaignResponse(t, s, userCtx, campaign.ID, wantCampaignResponse{
		changesetTypes:     map[string]int{"ExternalChangeset": 2},
		openChangesetTypes: map[string]int{"ExternalChangeset": 2},
		errors: []string{
			fmt.Sprintf("error patch %d", patches[0].ID),
			fmt.Sprintf("error patch %d", patches[1].ID),
		},
		patchTypes: map[string]int{"Patch": 2},
		campaignDiffStat: apitest.DiffStat{
			Added:   2*patchesDiffStat.Added + 2*changesetDiffStat.Added,
			Changed: 2*patchesDiffStat.Changed + 2*changesetDiffStat.Changed,
			Deleted: 2*patchesDiffStat.Deleted + 2*changesetDiffStat.Deleted,
		},
		patchSetDiffStat: apitest.DiffStat{
			Added:   2 * patchesDiffStat.Added,
			Changed: 2 * patchesDiffStat.Changed,
			Deleted: 2 * patchesDiffStat.Deleted,
		},
	})

	for _, c := range changesets {
		// Both changesets are visible still, so both should be ExternalChangesets
		testChangesetResponse(t, s, userCtx, c.ID, "ExternalChangeset")
	}

	for _, p := range patches {
		testPatchResponse(t, s, userCtx, p.ID, "Patch")
	}

	// Now we add the authzFilter and filter out 2 repositories
	filteredRepoIDs := map[api.RepoID]bool{
		patches[0].RepoID:    true,
		changesets[0].RepoID: true,
	}

	db.MockAuthzFilter = func(ctx context.Context, repos []*types.Repo, p authz.Perms) ([]*types.Repo, error) {
		var filtered []*types.Repo
		for _, r := range repos {
			if _, ok := filteredRepoIDs[r.ID]; ok {
				continue
			}
			filtered = append(filtered, r)
		}
		return filtered, nil
	}
	defer func() { db.MockAuthzFilter = nil }()

	// Send query again and check that for each filtered repository we get a
	// HiddenChangeset/HiddenPatch and that errors are filtered out
	testCampaignResponse(t, s, userCtx, campaign.ID, wantCampaignResponse{
		changesetTypes: map[string]int{
			"ExternalChangeset":       1,
			"HiddenExternalChangeset": 1,
		},
		openChangesetTypes: map[string]int{
			"ExternalChangeset":       1,
			"HiddenExternalChangeset": 1,
		},
		errors: []string{
			// patches[0] is filtered out
			fmt.Sprintf("error patch %d", patches[1].ID),
		},
		patchTypes: map[string]int{
			"Patch":       1,
			"HiddenPatch": 1,
		},
		campaignDiffStat: apitest.DiffStat{
			Added:   1*patchesDiffStat.Added + 1*changesetDiffStat.Added,
			Changed: 1*patchesDiffStat.Changed + 1*changesetDiffStat.Changed,
			Deleted: 1*patchesDiffStat.Deleted + 1*changesetDiffStat.Deleted,
		},
		patchSetDiffStat: apitest.DiffStat{
			Added:   1 * patchesDiffStat.Added,
			Changed: 1 * patchesDiffStat.Changed,
			Deleted: 1 * patchesDiffStat.Deleted,
		},
	})

	for _, c := range changesets {
		// The changeset whose repository has been filtered should be hidden
		if _, ok := filteredRepoIDs[c.RepoID]; ok {
			testChangesetResponse(t, s, userCtx, c.ID, "HiddenExternalChangeset")
		} else {
			testChangesetResponse(t, s, userCtx, c.ID, "ExternalChangeset")
		}
	}

	for _, p := range patches {
		// The patch whose repository has been filtered should be hidden
		if _, ok := filteredRepoIDs[p.RepoID]; ok {
			testPatchResponse(t, s, userCtx, p.ID, "HiddenPatch")
		} else {
			testPatchResponse(t, s, userCtx, p.ID, "Patch")
		}
	}
}

type wantCampaignResponse struct {
	patchTypes         map[string]int
	changesetTypes     map[string]int
	openChangesetTypes map[string]int
	errors             []string
	campaignDiffStat   apitest.DiffStat
	patchSetDiffStat   apitest.DiffStat
}

func testCampaignResponse(t *testing.T, s *graphql.Schema, ctx context.Context, id int64, w wantCampaignResponse) {
	t.Helper()

	var response struct{ Node apitest.Campaign }
	query := fmt.Sprintf(queryCampaignPermLevels, campaigns.MarshalCampaignID(id))

	apitest.MustExec(ctx, t, s, nil, &response, query)

	if have, want := response.Node.ID, string(campaigns.MarshalCampaignID(id)); have != want {
		t.Fatalf("campaign id is wrong. have %q, want %q", have, want)
	}

	if diff := cmp.Diff(w.errors, response.Node.Status.Errors); diff != "" {
		t.Fatalf("unexpected status errors (-want +got):\n%s", diff)
	}

	changesetTypes := map[string]int{}
	for _, c := range response.Node.Changesets.Nodes {
		changesetTypes[c.Typename]++
	}
	if diff := cmp.Diff(w.changesetTypes, changesetTypes); diff != "" {
		t.Fatalf("unexpected changesettypes (-want +got):\n%s", diff)
	}

	openChangesetTypes := map[string]int{}
	for _, c := range response.Node.OpenChangesets.Nodes {
		openChangesetTypes[c.Typename]++
	}
	if diff := cmp.Diff(w.openChangesetTypes, openChangesetTypes); diff != "" {
		t.Fatalf("unexpected open changeset types (-want +got):\n%s", diff)
	}

	patchTypes := map[string]int{}
	for _, p := range response.Node.Patches.Nodes {
		patchTypes[p.Typename]++
	}
	if diff := cmp.Diff(w.patchTypes, patchTypes); diff != "" {
		t.Fatalf("unexpected patch types (-want +got):\n%s", diff)
	}

	if diff := cmp.Diff(w.campaignDiffStat, response.Node.DiffStat); diff != "" {
		t.Fatalf("unexpected campaign diff stat (-want +got):\n%s", diff)
	}

	patchSetPatchTypes := map[string]int{}
	for _, p := range response.Node.PatchSet.Patches.Nodes {
		patchSetPatchTypes[p.Typename]++
	}
	if diff := cmp.Diff(w.patchTypes, patchSetPatchTypes); diff != "" {
		t.Fatalf("unexpected patch set patch types (-want +got):\n%s", diff)
	}
	if diff := cmp.Diff(w.patchSetDiffStat, response.Node.PatchSet.DiffStat); diff != "" {
		t.Fatalf("unexpected patch set diff stat (-want +got):\n%s", diff)
	}
}

const queryCampaignPermLevels = `
query {
  node(id: %q) {
    ... on Campaign {
      id

	  status {
	    state
		errors
	  }

      changesets(first: 100) {
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

      openChangesets {
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

      patches(first: 100) {
        nodes {
          __typename
          ... on HiddenPatch {
            id
          }
          ... on Patch {
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

      patchSet {
        diffStat {
          added
          changed
          deleted
        }

        patches(first: 100) {
          nodes {
            __typename
            ... on HiddenPatch {
              id
            }
            ... on Patch {
              id
              repository {
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
`

func testChangesetResponse(t *testing.T, s *graphql.Schema, ctx context.Context, id int64, wantType string) {
	t.Helper()

	var res struct{ Node apitest.Changeset }
	query := fmt.Sprintf(queryChangesetPermLevels, marshalExternalChangesetID(id))
	apitest.MustExec(ctx, t, s, nil, &res, query)

	if have, want := res.Node.Typename, wantType; have != want {
		t.Fatalf("changeset has wrong typename. want=%q, have=%q", want, have)
	}

	if have, want := res.Node.State, string(campaigns.ChangesetStateOpen); have != want {
		t.Fatalf("changeset has wrong state. want=%q, have=%q", want, have)
	}

	if have, want := res.Node.Campaigns.TotalCount, 1; have != want {
		t.Fatalf("changeset has wrong campaigns totalcount. want=%d, have=%d", want, have)
	}

	if parseJSONTime(t, res.Node.CreatedAt).IsZero() {
		t.Fatalf("changeset createdAt is zero")
	}

	if parseJSONTime(t, res.Node.UpdatedAt).IsZero() {
		t.Fatalf("changeset updatedAt is zero")
	}

	// TODO: See https://github.com/sourcegraph/sourcegraph/issues/11227
	// if parseJSONTime(t, res.Node.NextSyncAt).IsZero() {
	// 	t.Fatalf("changeset next sync at is zero")
	// }
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
	  campaigns {
	    totalCount
	  }
    }
    ... on ExternalChangeset {
      id

      state
	  createdAt
	  updatedAt
	  nextSyncAt
	  campaigns {
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

func testPatchResponse(t *testing.T, s *graphql.Schema, ctx context.Context, id int64, wantType string) {
	t.Helper()

	var res struct{ Node apitest.Patch }
	query := fmt.Sprintf(queryPatchPermLevels, marshalPatchID(id))
	apitest.MustExec(ctx, t, s, nil, &res, query)

	if have, want := res.Node.Typename, wantType; have != want {
		t.Fatalf("patch has wrong typename. want=%q, have=%q", want, have)
	}
}

const queryPatchPermLevels = `
query {
  node(id: %q) {
    __typename
    ... on HiddenPatch {
      id
    }
    ... on Patch {
      id
      repository {
        id
        name
      }
    }
  }
}
`

func mockBackendCommit(t *testing.T, testRev string) {
	t.Helper()

	backend.Mocks.Repos.ResolveRev = func(_ context.Context, _ *types.Repo, rev string) (api.CommitID, error) {
		if rev != testRev {
			t.Fatalf("ResolveRev received wrong rev: %q", rev)
		}
		return api.CommitID(rev), nil
	}
	t.Cleanup(func() { backend.Mocks.Repos.ResolveRev = nil })

	backend.Mocks.Repos.GetCommit = func(_ context.Context, _ *types.Repo, id api.CommitID) (*git.Commit, error) {
		if string(id) != testRev {
			t.Fatalf("GetCommit received wrong ID: %s", id)
		}
		return &git.Commit{ID: id}, nil
	}
	t.Cleanup(func() { backend.Mocks.Repos.GetCommit = nil })
}

func mockRepoComparison(t *testing.T, baseRev, headRev, diff string) {
	t.Helper()

	spec := fmt.Sprintf("%s...%s", baseRev, headRev)

	git.Mocks.GetCommit = func(id api.CommitID) (*git.Commit, error) {
		if string(id) != baseRev && string(id) != headRev {
			t.Fatalf("git.Mocks.GetCommit received unknown commit id: %s", id)
		}
		return &git.Commit{ID: api.CommitID(id)}, nil
	}
	t.Cleanup(func() { git.Mocks.GetCommit = nil })

	git.Mocks.ExecReader = func(args []string) (io.ReadCloser, error) {
		if len(args) < 1 && args[0] != "diff" {
			t.Fatalf("gitserver.ExecReader received wrong args: %v", args)
		}

		if have, want := args[len(args)-2], spec; have != want {
			t.Fatalf("gitserver.ExecReader received wrong spec: %q, want %q", have, want)
		}
		return ioutil.NopCloser(strings.NewReader(testDiff)), nil
	}
	t.Cleanup(func() { git.Mocks.ExecReader = nil })

	git.Mocks.MergeBase = func(repo gitserver.Repo, a, b api.CommitID) (api.CommitID, error) {
		if string(a) != baseRev && string(b) != headRev {
			t.Fatalf("git.Mocks.MergeBase received unknown commit ids: %s %s", a, b)
		}
		return a, nil
	}
	t.Cleanup(func() { git.Mocks.MergeBase = nil })
}

func insertTestUser(t *testing.T, db *sql.DB, name string, isAdmin bool) (userID int32) {
	t.Helper()

	q := sqlf.Sprintf("INSERT INTO users (username, site_admin) VALUES (%s, %t) RETURNING id", name, isAdmin)

	err := db.QueryRow(q.Query(sqlf.PostgresBindVar), q.Args()...).Scan(&userID)
	if err != nil {
		t.Fatal(err)
	}

	return userID
}
