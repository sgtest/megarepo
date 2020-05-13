package campaigns

import (
	"context"
	"database/sql"
	"fmt"
	"sort"
	"sync/atomic"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/repo-updater/repos"
	"github.com/sourcegraph/sourcegraph/internal/api"
	cmpgn "github.com/sourcegraph/sourcegraph/internal/campaigns"
	"github.com/sourcegraph/sourcegraph/internal/db/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/bitbucketserver"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
)

// Ran in integration_test.go
func testStore(db *sql.DB) func(*testing.T) {
	return func(t *testing.T) {
		tx := dbtest.NewTx(t, db)

		now := time.Now().UTC().Truncate(time.Microsecond)
		clock := func() time.Time {
			return now.UTC().Truncate(time.Microsecond)
		}
		s := NewStoreWithClock(tx, clock)

		ctx := context.Background()

		// Create a test repo
		reposStore := repos.NewDBStore(db, sql.TxOptions{})
		repo := &repos.Repo{
			Name: "github.com/sourcegraph/sourcegraph-test-repo",
			ExternalRepo: api.ExternalRepoSpec{
				ID:          "external-id",
				ServiceType: "github",
				ServiceID:   "https://github.com/",
			},
			Sources: map[string]*repos.SourceInfo{
				"extsvc:github:4": {
					ID:       "extsvc:github:4",
					CloneURL: "https://secrettoken@github.com/sourcegraph/sourcegraph",
				},
			},
		}
		deletedRepo := &repos.Repo{
			Name: "github.com/sourcegraph/sourcegraph-old",
			ExternalRepo: api.ExternalRepoSpec{
				ID:          "external-id",
				ServiceType: "github",
				ServiceID:   "https://github.com/",
			},
			Sources: map[string]*repos.SourceInfo{
				"extsvc:github:4": {
					ID:       "extsvc:github:4",
					CloneURL: "https://secrettoken@github.com/sourcegraph/sourcegraph-old",
				},
			},
			DeletedAt: time.Now(),
		}
		if err := reposStore.UpsertRepos(ctx, deletedRepo, repo); err != nil {
			t.Fatal(err)
		}

		t.Run("Campaigns", func(t *testing.T) {
			campaigns := make([]*cmpgn.Campaign, 0, 3)

			t.Run("Create", func(t *testing.T) {
				for i := 0; i < cap(campaigns); i++ {
					c := &cmpgn.Campaign{
						Name:         fmt.Sprintf("Upgrade ES-Lint %d", i),
						Description:  "All the Javascripts are belong to us",
						Branch:       "upgrade-es-lint",
						AuthorID:     23,
						ChangesetIDs: []int64{int64(i) + 1},
						PatchSetID:   42 + int64(i),
						ClosedAt:     now,
					}
					if i == 0 {
						// don't have a patch set for the first one
						c.PatchSetID = 0
						// Don't close the first one
						c.ClosedAt = time.Time{}
					}

					if i%2 == 0 {
						c.NamespaceOrgID = 23
					} else {
						c.NamespaceUserID = 42
					}

					want := c.Clone()
					have := c

					err := s.CreateCampaign(ctx, have)
					if err != nil {
						t.Fatal(err)
					}

					if have.ID == 0 {
						t.Fatal("ID should not be zero")
					}

					want.ID = have.ID
					want.CreatedAt = now
					want.UpdatedAt = now

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}

					campaigns = append(campaigns, c)
				}
			})

			t.Run("Count", func(t *testing.T) {
				count, err := s.CountCampaigns(ctx, CountCampaignsOpts{})
				if err != nil {
					t.Fatal(err)
				}

				if have, want := count, int64(len(campaigns)); have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}

				count, err = s.CountCampaigns(ctx, CountCampaignsOpts{ChangesetID: 1})
				if err != nil {
					t.Fatal(err)
				}

				if have, want := count, int64(1); have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}

				hasPatchSet := false
				count, err = s.CountCampaigns(ctx, CountCampaignsOpts{HasPatchSet: &hasPatchSet})
				if err != nil {
					t.Fatal(err)
				}

				if have, want := count, int64(1); have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}

				hasPatchSet = true
				count, err = s.CountCampaigns(ctx, CountCampaignsOpts{HasPatchSet: &hasPatchSet})
				if err != nil {
					t.Fatal(err)
				}

				if have, want := count, int64(2); have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}
			})

			t.Run("List", func(t *testing.T) {
				for i := 1; i <= len(campaigns); i++ {
					opts := ListCampaignsOpts{ChangesetID: int64(i)}

					ts, next, err := s.ListCampaigns(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if have, want := next, int64(0); have != want {
						t.Fatalf("opts: %+v: have next %v, want %v", opts, have, want)
					}

					have, want := ts, campaigns[i-1:i]
					if len(have) != len(want) {
						t.Fatalf("listed %d campaigns, want: %d", len(have), len(want))
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatalf("opts: %+v, diff: %s", opts, diff)
					}
				}

				for i := 1; i <= len(campaigns); i++ {
					cs, next, err := s.ListCampaigns(ctx, ListCampaignsOpts{Limit: i})
					if err != nil {
						t.Fatal(err)
					}

					{
						have, want := next, int64(0)
						if i < len(campaigns) {
							want = campaigns[i].ID
						}

						if have != want {
							t.Fatalf("limit: %v: have next %v, want %v", i, have, want)
						}
					}

					{
						have, want := cs, campaigns[:i]
						if len(have) != len(want) {
							t.Fatalf("listed %d campaigns, want: %d", len(have), len(want))
						}

						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatal(diff)
						}
					}
				}

				{
					var cursor int64
					for i := 1; i <= len(campaigns); i++ {
						opts := ListCampaignsOpts{Cursor: cursor, Limit: 1}
						have, next, err := s.ListCampaigns(ctx, opts)
						if err != nil {
							t.Fatal(err)
						}

						want := campaigns[i-1 : i]
						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}

						cursor = next
					}
				}

				filterTests := []struct {
					name  string
					state cmpgn.CampaignState
					want  []*cmpgn.Campaign
				}{
					{
						name:  "Any",
						state: cmpgn.CampaignStateAny,
						want:  campaigns,
					},
					{
						name:  "Closed",
						state: cmpgn.CampaignStateClosed,
						want:  campaigns[1:],
					},
					{
						name:  "Open",
						state: cmpgn.CampaignStateOpen,
						want:  campaigns[0:1],
					},
				}

				for _, tc := range filterTests {
					t.Run("ListCampaigns State "+tc.name, func(t *testing.T) {
						have, _, err := s.ListCampaigns(ctx, ListCampaignsOpts{State: tc.state})
						if err != nil {
							t.Fatal(err)
						}
						if diff := cmp.Diff(have, tc.want); diff != "" {
							t.Fatal(diff)
						}
					})
				}

				t.Run("ListCampaigns HasPatchSet true", func(t *testing.T) {
					hasPatchSet := true
					have, _, err := s.ListCampaigns(ctx, ListCampaignsOpts{HasPatchSet: &hasPatchSet})
					if err != nil {
						t.Fatal(err)
					}
					if diff := cmp.Diff(have, campaigns[1:]); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("ListCampaigns HasPatchSet false", func(t *testing.T) {
					hasPatchSet := false
					have, _, err := s.ListCampaigns(ctx, ListCampaignsOpts{HasPatchSet: &hasPatchSet})
					if err != nil {
						t.Fatal(err)
					}
					if diff := cmp.Diff(have, campaigns[0:1]); diff != "" {
						t.Fatal(diff)
					}
				})
			})

			t.Run("Update", func(t *testing.T) {
				for _, c := range campaigns {
					c.Name += "-updated"
					c.Description += "-updated"
					c.AuthorID++
					c.ClosedAt = c.ClosedAt.Add(5 * time.Second)

					if c.NamespaceUserID != 0 {
						c.NamespaceUserID++
					}

					if c.NamespaceOrgID != 0 {
						c.NamespaceOrgID++
					}

					now = now.Add(time.Second)
					want := c
					want.UpdatedAt = now

					have := c.Clone()
					if err := s.UpdateCampaign(ctx, have); err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}

					// Test that duplicates are not introduced.
					have.ChangesetIDs = append(have.ChangesetIDs, have.ChangesetIDs...)
					if err := s.UpdateCampaign(ctx, have); err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}

					// Test we can add to the set.
					have.ChangesetIDs = append(have.ChangesetIDs, 42)
					want.ChangesetIDs = append(want.ChangesetIDs, 42)

					if err := s.UpdateCampaign(ctx, have); err != nil {
						t.Fatal(err)
					}

					sort.Slice(have.ChangesetIDs, func(a, b int) bool {
						return have.ChangesetIDs[a] < have.ChangesetIDs[b]
					})

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}

					// Test we can remove from the set.
					have.ChangesetIDs = have.ChangesetIDs[:0]
					want.ChangesetIDs = want.ChangesetIDs[:0]

					if err := s.UpdateCampaign(ctx, have); err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				}
			})

			t.Run("Get", func(t *testing.T) {
				t.Run("ByID", func(t *testing.T) {
					want := campaigns[0]
					opts := GetCampaignOpts{ID: want.ID}

					have, err := s.GetCampaign(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("ByPatchSetID", func(t *testing.T) {
					want := campaigns[0]
					opts := GetCampaignOpts{PatchSetID: want.PatchSetID}

					have, err := s.GetCampaign(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("NoResults", func(t *testing.T) {
					opts := GetCampaignOpts{ID: 0xdeadbeef}

					_, have := s.GetCampaign(ctx, opts)
					want := ErrNoResults

					if have != want {
						t.Fatalf("have err %v, want %v", have, want)
					}
				})
			})

			t.Run("Delete", func(t *testing.T) {
				for i := range campaigns {
					err := s.DeleteCampaign(ctx, campaigns[i].ID)
					if err != nil {
						t.Fatal(err)
					}

					count, err := s.CountCampaigns(ctx, CountCampaignsOpts{})
					if err != nil {
						t.Fatal(err)
					}

					if have, want := count, int64(len(campaigns)-(i+1)); have != want {
						t.Fatalf("have count: %d, want: %d", have, want)
					}
				}
			})

		})

		t.Run("Changesets", func(t *testing.T) {
			githubActor := github.Actor{
				AvatarURL: "https://avatars2.githubusercontent.com/u/1185253",
				Login:     "mrnugget",
				URL:       "https://github.com/mrnugget",
			}
			githubPR := &github.PullRequest{
				ID:           "FOOBARID",
				Title:        "Fix a bunch of bugs",
				Body:         "This fixes a bunch of bugs",
				URL:          "https://github.com/sourcegraph/sourcegraph/pull/12345",
				Number:       12345,
				Author:       githubActor,
				Participants: []github.Actor{githubActor},
				CreatedAt:    now,
				UpdatedAt:    now,
				HeadRefName:  "campaigns/test",
			}

			changesets := make([]*cmpgn.Changeset, 0, 3)

			t.Run("Create", func(t *testing.T) {
				var i int
				for i = 0; i < cap(changesets); i++ {
					th := &cmpgn.Changeset{
						RepoID:              repo.ID,
						CreatedAt:           now,
						UpdatedAt:           now,
						Metadata:            githubPR,
						CampaignIDs:         []int64{int64(i) + 1},
						ExternalID:          fmt.Sprintf("foobar-%d", i),
						ExternalServiceType: "github",
						ExternalBranch:      "campaigns/test",
						ExternalUpdatedAt:   now,
						ExternalState:       cmpgn.ChangesetStateOpen,
						ExternalReviewState: cmpgn.ChangesetReviewStateApproved,
						ExternalCheckState:  cmpgn.ChangesetCheckStatePassed,
					}

					changesets = append(changesets, th)
				}

				err := s.CreateChangesets(ctx, changesets...)
				if err != nil {
					t.Fatal(err)
				}

				err = s.CreateChangesets(ctx, &cmpgn.Changeset{
					RepoID:              deletedRepo.ID,
					CreatedAt:           now,
					UpdatedAt:           now,
					Metadata:            githubPR,
					CampaignIDs:         []int64{int64(i) + 1},
					ExternalID:          fmt.Sprintf("foobar-%d", i),
					ExternalServiceType: "github",
					ExternalBranch:      "campaigns/test",
					ExternalUpdatedAt:   now,
					ExternalState:       cmpgn.ChangesetStateOpen,
					ExternalReviewState: cmpgn.ChangesetReviewStateApproved,
					ExternalCheckState:  cmpgn.ChangesetCheckStatePassed,
				})
				if err != nil {
					t.Fatal(err)
				}

				for _, have := range changesets {
					if have.ID == 0 {
						t.Fatal("id should not be zero")
					}

					if have.IsDeleted() {
						t.Fatal("changeset is deleted")
					}

					want := have.Clone()

					want.ID = have.ID
					want.CreatedAt = now
					want.UpdatedAt = now

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				}
			})

			t.Run("GetChangesetExternalIDs", func(t *testing.T) {
				spec := api.ExternalRepoSpec{
					ID:          "external-id",
					ServiceType: "github",
					ServiceID:   "https://github.com/",
				}
				have, err := s.GetChangesetExternalIDs(ctx, spec, []string{"campaigns/test"})
				if err != nil {
					t.Fatal(err)
				}
				want := []string{"foobar-0", "foobar-1", "foobar-2"}
				if diff := cmp.Diff(want, have); diff != "" {
					t.Fatal(diff)
				}
			})

			t.Run("GetChangesetExternalIDs no branch", func(t *testing.T) {
				spec := api.ExternalRepoSpec{
					ID:          "external-id",
					ServiceType: "github",
					ServiceID:   "https://github.com/",
				}
				have, err := s.GetChangesetExternalIDs(ctx, spec, []string{"foo"})
				if err != nil {
					t.Fatal(err)
				}
				want := []string{}
				if diff := cmp.Diff(want, have); diff != "" {
					t.Fatal(diff)
				}
			})

			t.Run("GetChangesetExternalIDs invalid external-id", func(t *testing.T) {
				spec := api.ExternalRepoSpec{
					ID:          "invalid",
					ServiceType: "github",
					ServiceID:   "https://github.com/",
				}
				have, err := s.GetChangesetExternalIDs(ctx, spec, []string{"campaigns/test"})
				if err != nil {
					t.Fatal(err)
				}
				want := []string{}
				if diff := cmp.Diff(want, have); diff != "" {
					t.Fatal(diff)
				}
			})

			t.Run("GetChangesetExternalIDs invalid external service id", func(t *testing.T) {
				spec := api.ExternalRepoSpec{
					ID:          "external-id",
					ServiceType: "github",
					ServiceID:   "invalid",
				}
				have, err := s.GetChangesetExternalIDs(ctx, spec, []string{"campaigns/test"})
				if err != nil {
					t.Fatal(err)
				}
				want := []string{}
				if diff := cmp.Diff(want, have); diff != "" {
					t.Fatal(diff)
				}
			})

			t.Run("CreateAlreadyExistingChangesets", func(t *testing.T) {
				ids := make([]int64, len(changesets))
				for i, c := range changesets {
					ids[i] = c.ID
				}

				clones := make([]*cmpgn.Changeset, len(changesets))

				for i, c := range changesets {
					// Set only the fields on which we have a unique constraint
					clones[i] = &cmpgn.Changeset{
						RepoID:              c.RepoID,
						ExternalID:          c.ExternalID,
						ExternalServiceType: c.ExternalServiceType,
					}
				}

				// Advance clock so store can determine whether Changeset was
				// inserted or not
				now = now.Add(time.Second)

				err := s.CreateChangesets(ctx, clones...)
				ae, ok := err.(AlreadyExistError)
				if !ok {
					t.Fatal(err)
				}

				{
					sort.Slice(ae.ChangesetIDs, func(i, j int) bool { return ae.ChangesetIDs[i] < ae.ChangesetIDs[j] })
					sort.Slice(ids, func(i, j int) bool { return ids[i] < ids[j] })

					have, want := ae.ChangesetIDs, ids
					if len(have) != len(want) {
						t.Fatalf("%d changesets already exist, want: %d", len(have), len(want))
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				}

				{
					// Verify that we got the original changesets back
					have, want := clones, changesets
					if len(have) != len(want) {
						t.Fatalf("created %d changesets, want: %d", len(have), len(want))
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				}
			})

			t.Run("Count", func(t *testing.T) {
				count, err := s.CountChangesets(ctx, CountChangesetsOpts{})
				if err != nil {
					t.Fatal(err)
				}

				if have, want := count, int64(len(changesets)); have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}

				count, err = s.CountChangesets(ctx, CountChangesetsOpts{CampaignID: 1})
				if err != nil {
					t.Fatal(err)
				}

				if have, want := count, int64(1); have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}
			})

			t.Run("List", func(t *testing.T) {
				for i := 1; i <= len(changesets); i++ {
					opts := ListChangesetsOpts{CampaignID: int64(i)}

					ts, next, err := s.ListChangesets(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if have, want := next, int64(0); have != want {
						t.Fatalf("opts: %+v: have next %v, want %v", opts, have, want)
					}

					have, want := ts, changesets[i-1:i]
					if len(have) != len(want) {
						t.Fatalf("listed %d changesets, want: %d", len(have), len(want))
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatalf("opts: %+v, diff: %s", opts, diff)
					}
				}

				for i := 1; i <= len(changesets); i++ {
					ts, next, err := s.ListChangesets(ctx, ListChangesetsOpts{Limit: i})
					if err != nil {
						t.Fatal(err)
					}

					{
						have, want := next, int64(0)
						if i < len(changesets) {
							want = changesets[i].ID
						}

						if have != want {
							t.Fatalf("limit: %v: have next %v, want %v", i, have, want)
						}
					}

					{
						have, want := ts, changesets[:i]
						if len(have) != len(want) {
							t.Fatalf("listed %d changesets, want: %d", len(have), len(want))
						}

						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatal(diff)
						}
					}
				}

				{
					ids := make([]int64, len(changesets))
					for i := range changesets {
						ids[i] = changesets[i].ID
					}

					have, _, err := s.ListChangesets(ctx, ListChangesetsOpts{IDs: ids})
					if err != nil {
						t.Fatal(err)
					}

					want := changesets
					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				}

				{
					var cursor int64
					for i := 1; i <= len(changesets); i++ {
						opts := ListChangesetsOpts{Cursor: cursor, Limit: 1}
						have, next, err := s.ListChangesets(ctx, opts)
						if err != nil {
							t.Fatal(err)
						}

						want := changesets[i-1 : i]
						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}

						cursor = next
					}
				}

				{
					have, _, err := s.ListChangesets(ctx, ListChangesetsOpts{WithoutDeleted: true})
					if err != nil {
						t.Fatal(err)
					}

					if len(have) != len(changesets) {
						t.Fatalf("have 0 changesets. want %d", len(changesets))
					}

					for _, c := range changesets {
						c.SetDeleted()
						c.UpdatedAt = now
					}

					if err := s.UpdateChangesets(ctx, changesets...); err != nil {
						t.Fatal(err)
					}

					have, _, err = s.ListChangesets(ctx, ListChangesetsOpts{WithoutDeleted: true})
					if err != nil {
						t.Fatal(err)
					}

					if len(have) != 0 {
						t.Fatalf("have %d changesets. want 0", len(changesets))
					}
				}

				// Limit of -1 should return all ChangeSets
				{
					have, _, err := s.ListChangesets(ctx, ListChangesetsOpts{Limit: -1})
					if err != nil {
						t.Fatal(err)
					}

					if len(have) != 3 {
						t.Fatalf("have %d changesets. want 3", len(have))
					}
				}

				stateOpen := cmpgn.ChangesetStateOpen
				stateClosed := cmpgn.ChangesetStateClosed
				stateApproved := cmpgn.ChangesetReviewStateApproved
				stateChangesRequested := cmpgn.ChangesetReviewStateChangesRequested
				statePassed := cmpgn.ChangesetCheckStatePassed
				stateFailed := cmpgn.ChangesetCheckStateFailed

				filterCases := []struct {
					opts      ListChangesetsOpts
					wantCount int
				}{
					{
						opts: ListChangesetsOpts{
							ExternalState: &stateOpen,
						},
						wantCount: 3,
					},
					{
						opts: ListChangesetsOpts{
							ExternalState: &stateClosed,
						},
						wantCount: 0,
					},
					{
						opts: ListChangesetsOpts{
							ExternalReviewState: &stateApproved,
						},
						wantCount: 3,
					},
					{
						opts: ListChangesetsOpts{
							ExternalReviewState: &stateChangesRequested,
						},
						wantCount: 0,
					},
					{
						opts: ListChangesetsOpts{
							ExternalCheckState: &statePassed,
						},
						wantCount: 3,
					},
					{
						opts: ListChangesetsOpts{
							ExternalCheckState: &stateFailed,
						},
						wantCount: 0,
					},
					{
						opts: ListChangesetsOpts{
							ExternalState:      &stateOpen,
							ExternalCheckState: &stateFailed,
						},
						wantCount: 0,
					},
					{
						opts: ListChangesetsOpts{
							ExternalState:       &stateOpen,
							ExternalReviewState: &stateChangesRequested,
						},
						wantCount: 0,
					},
				}

				for _, tc := range filterCases {
					t.Run("", func(t *testing.T) {
						have, _, err := s.ListChangesets(ctx, tc.opts)
						if err != nil {
							t.Fatal(err)
						}
						if len(have) != tc.wantCount {
							t.Fatalf("have %d changesets. want %d", len(have), tc.wantCount)
						}
					})
				}
			})

			t.Run("Null changeset state", func(t *testing.T) {
				cs := &cmpgn.Changeset{
					RepoID:              repo.ID,
					Metadata:            githubPR,
					CampaignIDs:         []int64{1},
					ExternalID:          fmt.Sprintf("foobar-%d", 42),
					ExternalServiceType: "github",
					ExternalBranch:      "campaigns/test",
					ExternalUpdatedAt:   now,
					ExternalState:       "",
					ExternalReviewState: "",
					ExternalCheckState:  "",
				}

				err := s.CreateChangesets(ctx, cs)
				if err != nil {
					t.Fatal(err)
				}
				defer func() {
					err := s.DeleteChangeset(ctx, cs.ID)
					if err != nil {
						t.Fatal(err)
					}
				}()

				fromDB, err := s.GetChangeset(ctx, GetChangesetOpts{
					ID: cs.ID,
				})
				if err != nil {
					t.Fatal(err)
				}

				if diff := cmp.Diff(cs.ExternalState, fromDB.ExternalState); diff != "" {
					t.Error(diff)
				}
				if diff := cmp.Diff(cs.ExternalReviewState, fromDB.ExternalReviewState); diff != "" {
					t.Error(diff)
				}
				if diff := cmp.Diff(cs.ExternalCheckState, fromDB.ExternalCheckState); diff != "" {
					t.Error(diff)
				}
			})

			t.Run("Get", func(t *testing.T) {
				t.Run("ByID", func(t *testing.T) {
					want := changesets[0]
					opts := GetChangesetOpts{ID: want.ID}

					have, err := s.GetChangeset(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("ByExternalID", func(t *testing.T) {
					want := changesets[0]
					opts := GetChangesetOpts{
						ExternalID:          want.ExternalID,
						ExternalServiceType: want.ExternalServiceType,
					}

					have, err := s.GetChangeset(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("ByRepoID", func(t *testing.T) {
					want := changesets[0]
					opts := GetChangesetOpts{
						RepoID: want.RepoID,
					}

					have, err := s.GetChangeset(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("NoResults", func(t *testing.T) {
					opts := GetChangesetOpts{ID: 0xdeadbeef}

					_, have := s.GetChangeset(ctx, opts)
					want := ErrNoResults

					if have != want {
						t.Fatalf("have err %v, want %v", have, want)
					}
				})
			})

			t.Run("Update", func(t *testing.T) {
				want := make([]*cmpgn.Changeset, 0, len(changesets))
				have := make([]*cmpgn.Changeset, 0, len(changesets))

				now = now.Add(time.Second)
				for _, c := range changesets {
					c.Metadata = &bitbucketserver.PullRequest{ID: 1234}
					c.ExternalServiceType = bitbucketserver.ServiceType

					have = append(have, c.Clone())

					c.UpdatedAt = now
					want = append(want, c)
				}

				if err := s.UpdateChangesets(ctx, have...); err != nil {
					t.Fatal(err)
				}

				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatal(diff)
				}

				for i := range have {
					// Test that duplicates are not introduced.
					have[i].CampaignIDs = append(have[i].CampaignIDs, have[i].CampaignIDs...)
				}

				if err := s.UpdateChangesets(ctx, have...); err != nil {
					t.Fatal(err)
				}

				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatal(diff)
				}

				for i := range have {
					// Test we can add to the set.
					have[i].CampaignIDs = append(have[i].CampaignIDs, 42)
					want[i].CampaignIDs = append(want[i].CampaignIDs, 42)
				}

				if err := s.UpdateChangesets(ctx, have...); err != nil {
					t.Fatal(err)
				}

				for i := range have {
					sort.Slice(have[i].CampaignIDs, func(a, b int) bool {
						return have[i].CampaignIDs[a] < have[i].CampaignIDs[b]
					})

					if diff := cmp.Diff(have[i], want[i]); diff != "" {
						t.Fatal(diff)
					}
				}

				for i := range have {
					// Test we can remove from the set.
					have[i].CampaignIDs = have[i].CampaignIDs[:0]
					want[i].CampaignIDs = want[i].CampaignIDs[:0]
				}

				if err := s.UpdateChangesets(ctx, have...); err != nil {
					t.Fatal(err)
				}

				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatal(diff)
				}
			})
		})

		t.Run("ChangesetEvents", func(t *testing.T) {
			events := make([]*cmpgn.ChangesetEvent, 0, 3)

			issueComment := &github.IssueComment{
				DatabaseID: 443827703,
				Author: github.Actor{
					AvatarURL: "https://avatars0.githubusercontent.com/u/1976?v=4",
					Login:     "sqs",
					URL:       "https://github.com/sqs",
				},
				Editor:              nil,
				AuthorAssociation:   "MEMBER",
				Body:                "> Just to be sure: you mean the \"searchFilters\" \"Filters\" should be lowercase, not the \"Search Filters\" from the description, right?\r\n\r\nNo, the prose “Search Filters” should have the F lowercased to fit with our style guide preference for sentence case over title case. (Can’t find this comment on the GitHub mobile interface anymore so quoting the email.)",
				URL:                 "https://github.com/sourcegraph/sourcegraph/pull/999#issuecomment-443827703",
				CreatedAt:           now,
				UpdatedAt:           now,
				IncludesCreatedEdit: false,
			}

			t.Run("Upsert", func(t *testing.T) {
				for i := 1; i < cap(events); i++ {
					e := &cmpgn.ChangesetEvent{
						ChangesetID: int64(i),
						Kind:        cmpgn.ChangesetEventKindGitHubCommented,
						Key:         issueComment.Key(),
						CreatedAt:   now,
						Metadata:    issueComment,
					}

					events = append(events, e)
				}

				// Verify that no duplicates are introduced and no error is returned.
				for i := 0; i < 2; i++ {
					err := s.UpsertChangesetEvents(ctx, events...)
					if err != nil {
						t.Fatal(err)
					}
				}

				for _, have := range events {
					if have.ID == 0 {
						t.Fatal("id should not be zero")
					}

					want := have.Clone()

					want.ID = have.ID
					want.CreatedAt = now
					want.UpdatedAt = now

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				}
			})

			t.Run("Count", func(t *testing.T) {
				count, err := s.CountChangesetEvents(ctx, CountChangesetEventsOpts{})
				if err != nil {
					t.Fatal(err)
				}

				if have, want := count, int64(len(events)); have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}

				count, err = s.CountChangesetEvents(ctx, CountChangesetEventsOpts{ChangesetID: 1})
				if err != nil {
					t.Fatal(err)
				}

				if have, want := count, int64(1); have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}
			})

			t.Run("Get", func(t *testing.T) {
				t.Run("ByID", func(t *testing.T) {
					want := events[0]
					opts := GetChangesetEventOpts{ID: want.ID}

					have, err := s.GetChangesetEvent(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("ByKey", func(t *testing.T) {
					want := events[0]
					opts := GetChangesetEventOpts{
						ChangesetID: want.ChangesetID,
						Kind:        want.Kind,
						Key:         want.Key,
					}

					have, err := s.GetChangesetEvent(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("NoResults", func(t *testing.T) {
					opts := GetChangesetEventOpts{ID: 0xdeadbeef}

					_, have := s.GetChangesetEvent(ctx, opts)
					want := ErrNoResults

					if have != want {
						t.Fatalf("have err %v, want %v", have, want)
					}
				})
			})

			t.Run("List", func(t *testing.T) {
				t.Run("ByChangesetIDs", func(t *testing.T) {
					for i := 1; i <= len(events); i++ {
						opts := ListChangesetEventsOpts{ChangesetIDs: []int64{int64(i)}}

						ts, next, err := s.ListChangesetEvents(ctx, opts)
						if err != nil {
							t.Fatal(err)
						}

						if have, want := next, int64(0); have != want {
							t.Fatalf("opts: %+v: have next %v, want %v", opts, have, want)
						}

						have, want := ts, events[i-1:i]
						if len(have) != len(want) {
							t.Fatalf("listed %d events, want: %d", len(have), len(want))
						}

						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}
					}

					{
						opts := ListChangesetEventsOpts{ChangesetIDs: []int64{}}

						for i := 1; i <= len(events); i++ {
							opts.ChangesetIDs = append(opts.ChangesetIDs, int64(i))
						}

						ts, next, err := s.ListChangesetEvents(ctx, opts)
						if err != nil {
							t.Fatal(err)
						}

						if have, want := next, int64(0); have != want {
							t.Fatalf("opts: %+v: have next %v, want %v", opts, have, want)
						}

						have, want := ts, events
						if len(have) != len(want) {
							t.Fatalf("listed %d events, want: %d", len(have), len(want))
						}
					}
				})

				t.Run("WithLimit", func(t *testing.T) {
					for i := 1; i <= len(events); i++ {
						cs, next, err := s.ListChangesetEvents(ctx, ListChangesetEventsOpts{Limit: i})
						if err != nil {
							t.Fatal(err)
						}

						{
							have, want := next, int64(0)
							if i < len(events) {
								want = events[i].ID
							}

							if have != want {
								t.Fatalf("limit: %v: have next %v, want %v", i, have, want)
							}
						}

						{
							have, want := cs, events[:i]
							if len(have) != len(want) {
								t.Fatalf("listed %d events, want: %d", len(have), len(want))
							}

							if diff := cmp.Diff(have, want); diff != "" {
								t.Fatal(diff)
							}
						}
					}
				})

				t.Run("WithCursor", func(t *testing.T) {
					var cursor int64
					for i := 1; i <= len(events); i++ {
						opts := ListChangesetEventsOpts{Cursor: cursor, Limit: 1}
						have, next, err := s.ListChangesetEvents(ctx, opts)
						if err != nil {
							t.Fatal(err)
						}

						want := events[i-1 : i]
						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}

						cursor = next
					}
				})

				t.Run("EmptyResultListingAll", func(t *testing.T) {
					opts := ListChangesetEventsOpts{ChangesetIDs: []int64{99999}, Limit: -1}

					ts, next, err := s.ListChangesetEvents(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if have, want := next, int64(0); have != want {
						t.Fatalf("opts: %+v: have next %v, want %v", opts, have, want)
					}

					if len(ts) != 0 {
						t.Fatalf("listed %d events, want: %d", len(ts), 0)
					}
				})
			})
		})

		t.Run("ListChangesetSyncData", func(t *testing.T) {
			changesets, _, err := s.ListChangesets(ctx, ListChangesetsOpts{
				Limit: -1,
			})
			if err != nil {
				t.Fatal(err)
			}

			// A previous test decreases the repoID, set it back
			for i := range changesets {
				changesets[i].RepoID = repo.ID
			}

			err = s.UpdateChangesets(ctx, changesets...)
			if err != nil {
				t.Fatal(err)
			}

			// We need campaigns attached to each changeset
			for i, cs := range changesets {
				c := &cmpgn.Campaign{
					Name:           fmt.Sprintf("ListChangesetSyncData test"),
					ChangesetIDs:   []int64{cs.ID},
					NamespaceOrgID: 23,
				}
				err := s.CreateCampaign(ctx, c)
				if err != nil {
					t.Fatal(err)
				}
				changesets[i].CampaignIDs = []int64{c.ID}
				if err := s.UpdateChangesets(ctx, changesets[i]); err != nil {
					t.Fatal(err)
				}
			}

			// Differs from clock() due to updates higher up
			externalUpdatedAt := clock().Add(-2 * time.Second)
			hs, err := s.ListChangesetSyncData(ctx, ListChangesetSyncDataOpts{})
			if err != nil {
				t.Fatal(err)
			}
			want := []cmpgn.ChangesetSyncData{
				{
					ChangesetID:        1,
					UpdatedAt:          clock(),
					LatestEvent:        clock(),
					ExternalUpdatedAt:  externalUpdatedAt,
					ExternalServiceIDs: []int64{4},
				},
				{
					ChangesetID:        2,
					UpdatedAt:          clock(),
					LatestEvent:        clock(),
					ExternalUpdatedAt:  externalUpdatedAt,
					ExternalServiceIDs: []int64{4},
				},
				{
					// No events
					ChangesetID:        3,
					UpdatedAt:          clock(),
					ExternalUpdatedAt:  externalUpdatedAt,
					ExternalServiceIDs: []int64{4},
				},
			}
			if diff := cmp.Diff(want, hs); diff != "" {
				t.Fatal(diff)
			}
		})

		t.Run("ListChangesetSyncData ignores closed campaign", func(t *testing.T) {
			changesets, _, err := s.ListChangesets(ctx, ListChangesetsOpts{
				Limit: 10,
			})
			if err != nil {
				t.Fatal(err)
			}
			if len(changesets) != 3 {
				t.Fatalf("Expected 3 changesets, got %d", len(changesets))
			}
			oldCampaign := changesets[0].CampaignIDs[0]

			// Close a campaign
			c, err := s.GetCampaign(ctx, GetCampaignOpts{
				ID: oldCampaign,
			})
			if err != nil {
				t.Fatal(err)
			}
			c.ClosedAt = now
			err = s.UpdateCampaign(ctx, c)
			if err != nil {
				t.Fatal(err)
			}

			// Differs from clock() due to updates higher up
			externalUpdatedAt := clock().Add(-2 * time.Second)
			hs, err := s.ListChangesetSyncData(ctx, ListChangesetSyncDataOpts{})
			if err != nil {
				t.Fatal(err)
			}
			want := []cmpgn.ChangesetSyncData{
				{
					ChangesetID:        2,
					UpdatedAt:          clock(),
					LatestEvent:        clock(),
					ExternalUpdatedAt:  externalUpdatedAt,
					ExternalServiceIDs: []int64{4},
				},
				{
					// No events
					ChangesetID:        3,
					UpdatedAt:          clock(),
					ExternalUpdatedAt:  externalUpdatedAt,
					ExternalServiceIDs: []int64{4},
				},
			}
			if diff := cmp.Diff(want, hs); diff != "" {
				t.Fatal(diff)
			}

			// If a changeset has ANY open campaigns we should list it
			cs2, err := s.GetChangeset(ctx, GetChangesetOpts{
				ID: 2,
			})
			if err != nil {
				t.Fatal(err)
			}

			// Attach cs1 to both an open and closed campaign
			changesets[0].CampaignIDs = []int64{oldCampaign, cs2.CampaignIDs[0]}
			err = s.UpdateChangesets(ctx, changesets[0])
			if err != nil {
				t.Fatal(err)
			}

			c1, err := s.GetCampaign(ctx, GetCampaignOpts{
				ID: cs2.CampaignIDs[0],
			})
			if err != nil {
				t.Fatal(err)
			}
			c1.ChangesetIDs = []int64{changesets[0].ID, cs2.ID}
			err = s.UpdateCampaign(ctx, c1)
			if err != nil {
				t.Fatal(err)
			}

			hs, err = s.ListChangesetSyncData(ctx, ListChangesetSyncDataOpts{})
			if err != nil {
				t.Fatal(err)
			}
			want = []cmpgn.ChangesetSyncData{
				{
					ChangesetID:        1,
					UpdatedAt:          clock(),
					LatestEvent:        clock(),
					ExternalUpdatedAt:  externalUpdatedAt,
					ExternalServiceIDs: []int64{4},
				},
				{
					ChangesetID:        2,
					UpdatedAt:          clock(),
					LatestEvent:        clock(),
					ExternalUpdatedAt:  externalUpdatedAt,
					ExternalServiceIDs: []int64{4},
				},
				{
					// No events
					ChangesetID:        3,
					UpdatedAt:          clock(),
					ExternalUpdatedAt:  externalUpdatedAt,
					ExternalServiceIDs: []int64{4},
				},
			}
			if diff := cmp.Diff(want, hs); diff != "" {
				t.Fatal(diff)
			}
		})

		t.Run("PatchSets", func(t *testing.T) {
			patchSets := make([]*cmpgn.PatchSet, 0, 3)

			t.Run("Create", func(t *testing.T) {
				for i := 0; i < cap(patchSets); i++ {
					c := &cmpgn.PatchSet{UserID: 999}

					want := c.Clone()
					have := c

					err := s.CreatePatchSet(ctx, have)
					if err != nil {
						t.Fatal(err)
					}

					if have.ID == 0 {
						t.Fatal("ID should not be zero")
					}

					want.ID = have.ID
					want.CreatedAt = now
					want.UpdatedAt = now

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}

					patchSets = append(patchSets, c)
				}
			})

			t.Run("Count", func(t *testing.T) {
				count, err := s.CountPatchSets(ctx)
				if err != nil {
					t.Fatal(err)
				}

				if have, want := count, int64(len(patchSets)); have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}
			})

			t.Run("List", func(t *testing.T) {
				opts := ListPatchSetsOpts{}

				ts, next, err := s.ListPatchSets(ctx, opts)
				if err != nil {
					t.Fatal(err)
				}

				if have, want := next, int64(0); have != want {
					t.Fatalf("opts: %+v: have next %v, want %v", opts, have, want)
				}

				have, want := ts, patchSets
				if len(have) != len(want) {
					t.Fatalf("listed %d patchSets, want: %d", len(have), len(want))
				}

				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", opts, diff)
				}

				for i := 1; i <= len(patchSets); i++ {
					cs, next, err := s.ListPatchSets(ctx, ListPatchSetsOpts{Limit: i})
					if err != nil {
						t.Fatal(err)
					}

					{
						have, want := next, int64(0)
						if i < len(patchSets) {
							want = patchSets[i].ID
						}

						if have != want {
							t.Fatalf("limit: %v: have next %v, want %v", i, have, want)
						}
					}

					{
						have, want := cs, patchSets[:i]
						if len(have) != len(want) {
							t.Fatalf("listed %d patchSets, want: %d", len(have), len(want))
						}

						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatal(diff)
						}
					}
				}

				{
					var cursor int64
					for i := 1; i <= len(patchSets); i++ {
						opts := ListPatchSetsOpts{Cursor: cursor, Limit: 1}
						have, next, err := s.ListPatchSets(ctx, opts)
						if err != nil {
							t.Fatal(err)
						}

						want := patchSets[i-1 : i]
						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}

						cursor = next
					}
				}
			})

			t.Run("Update", func(t *testing.T) {
				for _, c := range patchSets {
					c.UserID += 1234

					now = now.Add(time.Second)
					want := c
					want.UpdatedAt = now

					have := c.Clone()
					if err := s.UpdatePatchSet(ctx, have); err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				}
			})

			t.Run("Get", func(t *testing.T) {
				t.Run("ByID", func(t *testing.T) {
					if len(patchSets) == 0 {
						t.Fatalf("patchSets is empty")
					}
					want := patchSets[0]
					opts := GetPatchSetOpts{ID: want.ID}

					have, err := s.GetPatchSet(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("NoResults", func(t *testing.T) {
					opts := GetPatchSetOpts{ID: 0xdeadbeef}

					_, have := s.GetPatchSet(ctx, opts)
					want := ErrNoResults

					if have != want {
						t.Fatalf("have err %v, want %v", have, want)
					}
				})
			})

			t.Run("Delete", func(t *testing.T) {
				for i := range patchSets {
					err := s.DeletePatchSet(ctx, patchSets[i].ID)
					if err != nil {
						t.Fatal(err)
					}

					count, err := s.CountPatchSets(ctx)
					if err != nil {
						t.Fatal(err)
					}

					if have, want := count, int64(len(patchSets)-(i+1)); have != want {
						t.Fatalf("have count: %d, want: %d", have, want)
					}
				}
			})
		})

		t.Run("Patches", func(t *testing.T) {
			patches := make([]*cmpgn.Patch, 0, 3)

			var (
				added   int32 = 77
				deleted int32 = 88
				changed int32 = 99
			)

			t.Run("Create", func(t *testing.T) {
				for i := 0; i < cap(patches); i++ {
					p := &cmpgn.Patch{
						PatchSetID: int64(i + 1),
						RepoID:     1,
						Rev:        api.CommitID("deadbeef"),
						BaseRef:    "master",
						Diff:       "+ foobar - barfoo",
					}

					// Only set the diff stats on a subset to make sure that
					// we handle nil pointers correctly
					if i != cap(patches)-1 {
						p.DiffStatAdded = &added
						p.DiffStatChanged = &changed
						p.DiffStatDeleted = &deleted
					}

					want := p.Clone()
					have := p

					err := s.CreatePatch(ctx, have)
					if err != nil {
						t.Fatal(err)
					}

					if have.ID == 0 {
						t.Fatal("ID should not be zero")
					}

					want.ID = have.ID
					want.CreatedAt = now
					want.UpdatedAt = now

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}

					patches = append(patches, p)
				}
			})

			t.Run("Count", func(t *testing.T) {
				count, err := s.CountPatches(ctx, CountPatchesOpts{})
				if err != nil {
					t.Fatal(err)
				}

				if have, want := count, int64(len(patches)); have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}

				count, err = s.CountPatches(ctx, CountPatchesOpts{PatchSetID: 1})
				if err != nil {
					t.Fatal(err)
				}

				if have, want := count, int64(1); have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}
			})

			t.Run("List", func(t *testing.T) {
				t.Run("WithPatchSetID", func(t *testing.T) {
					for i := 1; i <= len(patches); i++ {
						opts := ListPatchesOpts{PatchSetID: int64(i)}

						ts, next, err := s.ListPatches(ctx, opts)
						if err != nil {
							t.Fatal(err)
						}

						if have, want := next, int64(0); have != want {
							t.Fatalf("opts: %+v: have next %v, want %v", opts, have, want)
						}

						have, want := ts, patches[i-1:i]
						if len(have) != len(want) {
							t.Fatalf("listed %d patches, want: %d", len(have), len(want))
						}

						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}
					}
				})

				t.Run("WithPositiveLimit", func(t *testing.T) {
					for i := 1; i <= len(patches); i++ {
						cs, next, err := s.ListPatches(ctx, ListPatchesOpts{Limit: i})
						if err != nil {
							t.Fatal(err)
						}

						{
							have, want := next, int64(0)
							if i < len(patches) {
								want = patches[i].ID
							}

							if have != want {
								t.Fatalf("limit: %v: have next %v, want %v", i, have, want)
							}
						}

						{
							have, want := cs, patches[:i]
							if len(have) != len(want) {
								t.Fatalf("listed %d patches, want: %d", len(have), len(want))
							}

							if diff := cmp.Diff(have, want); diff != "" {
								t.Fatal(diff)
							}
						}
					}
				})

				t.Run("WithNegativeLimitToListAll", func(t *testing.T) {
					cs, next, err := s.ListPatches(ctx, ListPatchesOpts{Limit: -1})
					if err != nil {
						t.Fatal(err)
					}

					if have, want := next, int64(0); have != want {
						t.Fatalf("have next %v, want %v", have, want)
					}

					have, want := cs, patches
					if len(have) != len(want) {
						t.Fatalf("listed %d patches, want: %d", len(have), len(want))
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("EmptyResultListingAll", func(t *testing.T) {
					opts := ListPatchesOpts{PatchSetID: 99999, Limit: -1}

					js, next, err := s.ListPatches(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if have, want := next, int64(0); have != want {
						t.Fatalf("opts: %+v: have next %v, want %v", opts, have, want)
					}

					if len(js) != 0 {
						t.Fatalf("listed %d jobs, want: %d", len(js), 0)
					}
				})

				t.Run("WithCursor", func(t *testing.T) {
					{
						var cursor int64
						for i := 1; i <= len(patches); i++ {
							opts := ListPatchesOpts{Cursor: cursor, Limit: 1}
							have, next, err := s.ListPatches(ctx, opts)
							if err != nil {
								t.Fatal(err)
							}

							want := patches[i-1 : i]
							if diff := cmp.Diff(have, want); diff != "" {
								t.Fatalf("opts: %+v, diff: %s", opts, diff)
							}

							cursor = next
						}
					}
				})
			})

			t.Run("Listing OnlyWithoutDiffStats", func(t *testing.T) {
				listOpts := ListPatchesOpts{OnlyWithoutDiffStats: true}
				have, _, err := s.ListPatches(ctx, listOpts)
				if err != nil {
					t.Fatal(err)
				}

				want := []*cmpgn.Patch{}
				for _, p := range patches {
					_, ok := p.DiffStat()
					if !ok {
						want = append(want, p)
					}
				}

				if len(want) == 0 {
					t.Fatalf("test needs patches without diff stats")
				}
				if len(have) != len(want) {
					t.Fatalf("listed %d patches, want: %d", len(have), len(want))
				}

				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", listOpts, diff)
				}
			})

			t.Run("Listing and Counting OnlyWithDiff", func(t *testing.T) {
				listOpts := ListPatchesOpts{OnlyWithDiff: true}
				countOpts := CountPatchesOpts{OnlyWithDiff: true}

				have, _, err := s.ListPatches(ctx, listOpts)
				if err != nil {
					t.Fatal(err)
				}

				have, want := have, patches
				if len(have) != len(want) {
					t.Fatalf("listed %d patches, want: %d", len(have), len(want))
				}

				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", listOpts, diff)
				}

				count, err := s.CountPatches(ctx, countOpts)
				if err != nil {
					t.Fatal(err)
				}

				if int(count) != len(want) {
					t.Errorf("jobs counted: %d", count)
				}

				for _, p := range patches {
					p.Diff = ""

					err := s.UpdatePatch(ctx, p)
					if err != nil {
						t.Fatal(err)
					}
				}

				have, _, err = s.ListPatches(ctx, listOpts)
				if err != nil {
					t.Fatal(err)
				}

				if len(have) != 0 {
					t.Errorf("jobs returned: %d", len(have))
				}

				count, err = s.CountPatches(ctx, countOpts)
				if err != nil {
					t.Fatal(err)
				}

				if count != 0 {
					t.Errorf("jobs counted: %d", count)
				}
			})

			t.Run("Listing and Counting OnlyUnpublishedInCampaign", func(t *testing.T) {
				campaignID := int64(999)
				changesetJob := &cmpgn.ChangesetJob{
					PatchID:     patches[0].ID,
					CampaignID:  campaignID,
					ChangesetID: 789,
					StartedAt:   now,
					FinishedAt:  now,
				}
				err := s.CreateChangesetJob(ctx, changesetJob)
				if err != nil {
					t.Fatal(err)
				}

				listOpts := ListPatchesOpts{OnlyUnpublishedInCampaign: campaignID}
				countOpts := CountPatchesOpts{OnlyUnpublishedInCampaign: campaignID}

				have, _, err := s.ListPatches(ctx, listOpts)
				if err != nil {
					t.Fatal(err)
				}

				have, want := have, patches[1:] // Except patches[0]
				if len(have) != len(want) {
					t.Fatalf("listed %d patches, want: %d", len(have), len(want))
				}

				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", listOpts, diff)
				}

				count, err := s.CountPatches(ctx, countOpts)
				if err != nil {
					t.Fatal(err)
				}

				if int(count) != len(want) {
					t.Errorf("jobs counted: %d", count)
				}

				// Update ChangesetJob so condition does not apply
				changesetJob.ChangesetID = 0
				err = s.UpdateChangesetJob(ctx, changesetJob)
				if err != nil {
					t.Fatal(err)
				}

				have, _, err = s.ListPatches(ctx, listOpts)
				if err != nil {
					t.Fatal(err)
				}

				want = patches // All Patches
				if len(have) != len(want) {
					t.Fatalf("listed %d patches, want: %d", len(have), len(want))
				}

				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", listOpts, diff)
				}

				count, err = s.CountPatches(ctx, countOpts)
				if err != nil {
					t.Fatal(err)
				}

				if int(count) != len(want) {
					t.Errorf("jobs counted: %d", count)
				}
			})

			t.Run("Update", func(t *testing.T) {
				var (
					newAdded   int32 = 333
					newDeleted int32 = 444
					newChanged int32 = 555
				)

				for _, p := range patches {
					now = now.Add(time.Second)
					p.Diff += "-updated"

					p.DiffStatAdded = &newAdded
					p.DiffStatDeleted = &newDeleted
					p.DiffStatChanged = &newChanged

					want := p
					want.UpdatedAt = now

					have := p.Clone()
					if err := s.UpdatePatch(ctx, have); err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				}
			})

			t.Run("Get", func(t *testing.T) {
				t.Run("ByID", func(t *testing.T) {
					if len(patches) == 0 {
						t.Fatal("patches is empty")
					}
					want := patches[0]
					opts := GetPatchOpts{ID: want.ID}

					have, err := s.GetPatch(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("NoResults", func(t *testing.T) {
					opts := GetPatchOpts{ID: 0xdeadbeef}

					_, have := s.GetPatch(ctx, opts)
					want := ErrNoResults

					if have != want {
						t.Fatalf("have err %v, want %v", have, want)
					}
				})
			})

			t.Run("Delete", func(t *testing.T) {
				for i := range patches {
					err := s.DeletePatch(ctx, patches[i].ID)
					if err != nil {
						t.Fatal(err)
					}

					count, err := s.CountPatches(ctx, CountPatchesOpts{})
					if err != nil {
						t.Fatal(err)
					}

					if have, want := count, int64(len(patches)-(i+1)); have != want {
						t.Fatalf("have count: %d, want: %d", have, want)
					}
				}
			})
		})

		t.Run("PatchSet DeleteExpired", func(t *testing.T) {
			tests := []struct {
				createdAt                      time.Time
				hasCampaign                    bool
				patchesAttachedToOtherCampaign bool
				patches                        []*cmpgn.Patch
				wantDeleted                    bool
				want                           *cmpgn.BackgroundProcessStatus
			}{
				{
					hasCampaign: false,
					createdAt:   now,
					wantDeleted: false,
				},
				{
					hasCampaign: false,
					createdAt:   now.Add(-500 * time.Minute),
					wantDeleted: true,
				},
				{
					hasCampaign: true,
					createdAt:   now,
					wantDeleted: false,
				},
				{
					hasCampaign: true,
					createdAt:   now.Add(-500 * time.Minute),
					wantDeleted: false,
				},
				{
					hasCampaign: false,
					createdAt:   now.Add(-500 * time.Minute),

					patchesAttachedToOtherCampaign: true,
					patches: []*cmpgn.Patch{
						{Diff: "foobar", Rev: "f00b4r", BaseRef: "refs/heads/master"},
						{Diff: "barfoo", Rev: "b4rf00", BaseRef: "refs/heads/master"},
					},

					wantDeleted: false,
				},
			}

			for _, tc := range tests {
				patchSet := &cmpgn.PatchSet{CreatedAt: tc.createdAt}

				err := s.CreatePatchSet(ctx, patchSet)
				if err != nil {
					t.Fatal(err)
				}

				for _, p := range tc.patches {
					p.PatchSetID = patchSet.ID
					err := s.CreatePatch(ctx, p)
					if err != nil {
						t.Fatal(err)
					}
				}

				if tc.hasCampaign {
					err = s.CreateCampaign(ctx, &cmpgn.Campaign{
						PatchSetID:      patchSet.ID,
						Name:            "Test",
						AuthorID:        4567,
						NamespaceUserID: 4567,
					})
					if err != nil {
						t.Fatal(err)
					}
				}

				if tc.patchesAttachedToOtherCampaign {
					otherPatchSet := &cmpgn.PatchSet{}
					err = s.CreatePatchSet(ctx, otherPatchSet)
					if err != nil {
						t.Fatal(err)
					}
					otherCampaign := &cmpgn.Campaign{
						PatchSetID:      otherPatchSet.ID,
						AuthorID:        4567,
						Name:            "Other campaign",
						NamespaceUserID: 4567,
					}

					err = s.CreateCampaign(ctx, otherCampaign)
					if err != nil {
						t.Fatal(err)
					}

					for i, p := range tc.patches {
						changeset := &cmpgn.Changeset{
							RepoID:              api.RepoID(99 + i),
							CreatedAt:           now,
							UpdatedAt:           now,
							Metadata:            &github.PullRequest{},
							CampaignIDs:         []int64{otherCampaign.ID},
							ExternalID:          fmt.Sprintf("foobar-%d", i),
							ExternalServiceType: "github",
							ExternalBranch:      "campaigns/test",
							ExternalUpdatedAt:   now,
							ExternalState:       cmpgn.ChangesetStateOpen,
							ExternalReviewState: cmpgn.ChangesetReviewStateApproved,
							ExternalCheckState:  cmpgn.ChangesetCheckStatePassed,
						}
						err = s.CreateChangesets(ctx, changeset)
						if err != nil {
							t.Fatal(err)
						}
						job := &cmpgn.ChangesetJob{
							CampaignID:  otherCampaign.ID,
							PatchID:     p.ID,
							ChangesetID: changeset.ID,
						}
						err = s.CreateChangesetJob(ctx, job)
						if err != nil {
							t.Fatal(err)
						}
						defer func() {
							err = s.DeleteChangesetJob(ctx, job.ID)
							if err != nil {
								t.Fatal(err)
							}
						}()
					}
				}

				err = s.DeleteExpiredPatchSets(ctx)
				if err != nil {
					t.Fatal(err)
				}

				havePatchSet, err := s.GetPatchSet(ctx, GetPatchSetOpts{ID: patchSet.ID})
				if err != nil && err != ErrNoResults {
					t.Fatal(err)
				}

				if tc.wantDeleted && err == nil {
					t.Fatalf("tc=%+v\n\t want patch set to be deleted. got: %v", tc, havePatchSet)
				}

				if !tc.wantDeleted && err == ErrNoResults {
					t.Fatalf("want patch set not to be deleted, but got deleted")
				}
			}
		})

		t.Run("ChangesetJobs", func(t *testing.T) {
			changesetJobs := make([]*cmpgn.ChangesetJob, 0, 3)

			t.Run("Create", func(t *testing.T) {
				for i := 0; i < cap(changesetJobs); i++ {
					c := &cmpgn.ChangesetJob{
						CampaignID:  int64(i + 1),
						PatchID:     int64(i + 1),
						ChangesetID: int64(i + 1),
						Branch:      "test-branch",
						Error:       "only set on error",
						StartedAt:   now,
						FinishedAt:  now,
					}

					want := c.Clone()
					have := c

					err := s.CreateChangesetJob(ctx, have)
					if err != nil {
						t.Fatal(err)
					}

					if have.ID == 0 {
						t.Fatal("ID should not be zero")
					}

					want.ID = have.ID
					want.CreatedAt = now
					want.UpdatedAt = now

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}

					changesetJobs = append(changesetJobs, c)
				}
			})

			t.Run("Count", func(t *testing.T) {
				count, err := s.CountChangesetJobs(ctx, CountChangesetJobsOpts{})
				if err != nil {
					t.Fatal(err)
				}

				if have, want := count, int64(len(changesetJobs)); have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}

				count, err = s.CountChangesetJobs(ctx, CountChangesetJobsOpts{CampaignID: 1})
				if err != nil {
					t.Fatal(err)
				}

				if have, want := count, int64(1); have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}
			})

			t.Run("List", func(t *testing.T) {
				t.Run("WithCampaignID", func(t *testing.T) {
					for i := 1; i <= len(changesetJobs); i++ {
						opts := ListChangesetJobsOpts{CampaignID: int64(i)}

						ts, next, err := s.ListChangesetJobs(ctx, opts)
						if err != nil {
							t.Fatal(err)
						}

						if have, want := next, int64(0); have != want {
							t.Fatalf("opts: %+v: have next %v, want %v", opts, have, want)
						}

						have, want := ts, changesetJobs[i-1:i]
						if len(have) != len(want) {
							t.Fatalf("listed %d changesetJobs, want: %d", len(have), len(want))
						}

						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}
					}
				})

				t.Run("WithPositiveLimit", func(t *testing.T) {
					for i := 1; i <= len(changesetJobs); i++ {
						cs, next, err := s.ListChangesetJobs(ctx, ListChangesetJobsOpts{Limit: i})
						if err != nil {
							t.Fatal(err)
						}

						{
							have, want := next, int64(0)
							if i < len(changesetJobs) {
								want = changesetJobs[i].ID
							}

							if have != want {
								t.Fatalf("limit: %v: have next %v, want %v", i, have, want)
							}
						}

						{
							have, want := cs, changesetJobs[:i]
							if len(have) != len(want) {
								t.Fatalf("listed %d changesetJobs, want: %d", len(have), len(want))
							}

							if diff := cmp.Diff(have, want); diff != "" {
								t.Fatal(diff)
							}
						}
					}
				})

				t.Run("WithNegativeLimitToListAll", func(t *testing.T) {
					cs, next, err := s.ListChangesetJobs(ctx, ListChangesetJobsOpts{Limit: -1})
					if err != nil {
						t.Fatal(err)
					}

					if have, want := next, int64(0); have != want {
						t.Fatalf("have next %v, want %v", have, want)
					}

					have, want := cs, changesetJobs
					if len(have) != len(want) {
						t.Fatalf("listed %d patches, want: %d", len(have), len(want))
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("EmptyResultListingAll", func(t *testing.T) {
					opts := ListChangesetJobsOpts{CampaignID: 99999, Limit: -1}

					cs, next, err := s.ListChangesetJobs(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if have, want := next, int64(0); have != want {
						t.Fatalf("opts: %+v: have next %v, want %v", opts, have, want)
					}

					if len(cs) != 0 {
						t.Fatalf("listed %d jobs, want: %d", len(cs), 0)
					}
				})

				t.Run("WithCursor", func(t *testing.T) {
					var cursor int64
					for i := 1; i <= len(changesetJobs); i++ {
						opts := ListChangesetJobsOpts{Cursor: cursor, Limit: 1}
						have, next, err := s.ListChangesetJobs(ctx, opts)
						if err != nil {
							t.Fatal(err)
						}

						want := changesetJobs[i-1 : i]
						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}

						cursor = next
					}
				})

				t.Run("WithPatchSetID", func(t *testing.T) {
					for i := 1; i <= len(changesetJobs); i++ {
						c := &cmpgn.Campaign{
							Name:            fmt.Sprintf("Upgrade ES-Lint %d", i),
							Description:     "All the Javascripts are belong to us",
							AuthorID:        4567,
							NamespaceUserID: 4567,
							PatchSetID:      1234 + int64(i),
						}

						err := s.CreateCampaign(ctx, c)
						if err != nil {
							t.Fatal(err)
						}
						job := changesetJobs[i-1]

						job.CampaignID = c.ID
						err = s.UpdateChangesetJob(ctx, job)
						if err != nil {
							t.Fatal(err)
						}

						opts := ListChangesetJobsOpts{PatchSetID: c.PatchSetID}
						ts, next, err := s.ListChangesetJobs(ctx, opts)
						if err != nil {
							t.Fatal(err)
						}

						if have, want := next, int64(0); have != want {
							t.Fatalf("opts: %+v: have next %v, want %v", opts, have, want)
						}

						have, want := ts, changesetJobs[i-1:i]
						if len(have) != len(want) {
							t.Fatalf("listed %d changesetJobs, want: %d", len(have), len(want))
						}

						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}
					}
				})
			})

			t.Run("Update", func(t *testing.T) {
				for _, c := range changesetJobs {
					now = now.Add(time.Second)
					c.StartedAt = now.Add(1 * time.Second)
					c.FinishedAt = now.Add(1 * time.Second)
					c.Branch = "upgrade-es-lint"
					c.Error = "updated-error"

					want := c
					want.UpdatedAt = now

					have := c.Clone()
					if err := s.UpdateChangesetJob(ctx, have); err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				}
			})

			t.Run("Get", func(t *testing.T) {
				t.Run("ByID", func(t *testing.T) {
					if len(changesetJobs) == 0 {
						t.Fatal("changesetJobs is empty")
					}
					want := changesetJobs[0]
					opts := GetChangesetJobOpts{ID: want.ID}

					have, err := s.GetChangesetJob(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("ByPatchID", func(t *testing.T) {
					if len(changesetJobs) == 0 {
						t.Fatal("changesetJobs is empty")
					}
					want := changesetJobs[0]
					opts := GetChangesetJobOpts{PatchID: want.PatchID}

					have, err := s.GetChangesetJob(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("ByChangesetID", func(t *testing.T) {
					if len(changesetJobs) == 0 {
						t.Fatal("changesetJobs is empty")
					}
					want := changesetJobs[0]
					opts := GetChangesetJobOpts{ChangesetID: want.ChangesetID}

					have, err := s.GetChangesetJob(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("ByCampaignID", func(t *testing.T) {
					if len(changesetJobs) == 0 {
						t.Fatal("changesetJobs is empty")
					}
					// Use the last changesetJob, which we don't get by
					// accident when selecting all with LIMIT 1
					want := changesetJobs[2]
					opts := GetChangesetJobOpts{CampaignID: want.CampaignID}

					have, err := s.GetChangesetJob(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				})

				t.Run("NoResults", func(t *testing.T) {
					opts := GetChangesetJobOpts{ID: 0xdeadbeef}

					_, have := s.GetChangesetJob(ctx, opts)
					want := ErrNoResults

					if have != want {
						t.Fatalf("have err %v, want %v", have, want)
					}
				})
			})

			t.Run("Delete", func(t *testing.T) {
				for i := range changesetJobs {
					err := s.DeleteChangesetJob(ctx, changesetJobs[i].ID)
					if err != nil {
						t.Fatal(err)
					}

					count, err := s.CountChangesetJobs(ctx, CountChangesetJobsOpts{})
					if err != nil {
						t.Fatal(err)
					}

					if have, want := count, int64(len(changesetJobs)-(i+1)); have != want {
						t.Fatalf("have count: %d, want: %d", have, want)
					}
				}
			})

			t.Run("BackgroundProcessStatus", func(t *testing.T) {
				tests := []struct {
					jobs []*cmpgn.ChangesetJob
					want *cmpgn.BackgroundProcessStatus
				}{
					{
						jobs: []*cmpgn.ChangesetJob{}, // no jobs
						want: &cmpgn.BackgroundProcessStatus{
							ProcessState:  cmpgn.BackgroundProcessStateCompleted,
							Total:         0,
							Completed:     0,
							Pending:       0,
							ProcessErrors: nil,
						},
					},
					{
						jobs: []*cmpgn.ChangesetJob{
							// not started (pending)
							{},
							// started (pending)
							{StartedAt: now},
						},
						want: &cmpgn.BackgroundProcessStatus{
							ProcessState:  cmpgn.BackgroundProcessStateProcessing,
							Total:         2,
							Completed:     0,
							Pending:       2,
							ProcessErrors: nil,
						},
					},
					{
						jobs: []*cmpgn.ChangesetJob{
							// completed, no errors
							{StartedAt: now, FinishedAt: now, ChangesetID: 23},
						},
						want: &cmpgn.BackgroundProcessStatus{
							ProcessState:  cmpgn.BackgroundProcessStateCompleted,
							Total:         1,
							Completed:     1,
							Pending:       0,
							ProcessErrors: nil,
						},
					},
					{
						jobs: []*cmpgn.ChangesetJob{
							// completed, error
							{StartedAt: now, FinishedAt: now, Error: "error1"},
						},
						want: &cmpgn.BackgroundProcessStatus{
							ProcessState:  cmpgn.BackgroundProcessStateErrored,
							Total:         1,
							Completed:     1,
							Pending:       0,
							ProcessErrors: []string{"error1"},
						},
					},
					{
						jobs: []*cmpgn.ChangesetJob{
							// not started (pending)
							{},
							// started (pending)
							{StartedAt: now},
							// completed, no errors
							{StartedAt: now, FinishedAt: now, ChangesetID: 23},
							// completed, error
							{StartedAt: now, FinishedAt: now, Error: "error1"},
							// completed, another error
							{StartedAt: now, FinishedAt: now, Error: "error2"},
						},
						want: &cmpgn.BackgroundProcessStatus{
							ProcessState:  cmpgn.BackgroundProcessStateProcessing,
							Total:         5,
							Completed:     3,
							Pending:       2,
							ProcessErrors: []string{"error1", "error2"},
						},
					},
				}

				for campaignID, tc := range tests {
					for i, j := range tc.jobs {
						j.CampaignID = int64(campaignID)
						j.PatchID = int64(i)

						err := s.CreateChangesetJob(ctx, j)
						if err != nil {
							t.Fatal(err)
						}
					}

					status, err := s.GetCampaignStatus(ctx, int64(campaignID))
					if err != nil {
						t.Fatal(err)
					}

					if diff := cmp.Diff(status, tc.want); diff != "" {
						t.Fatalf("wrong diff: %s", diff)
					}
				}
			})

			t.Run("ResetFailedChangesetJobs", func(t *testing.T) {
				campaignID := 9999
				jobs := []*cmpgn.ChangesetJob{
					// completed, no errors
					{StartedAt: now, FinishedAt: now, ChangesetID: 23},
					// completed, error
					{StartedAt: now, FinishedAt: now, Error: "error1"},
					// completed, another error
					{StartedAt: now, FinishedAt: now, Error: "error2"},
				}

				for i, j := range jobs {
					j.CampaignID = int64(campaignID)
					j.PatchID = int64(i)

					err := s.CreateChangesetJob(ctx, j)
					if err != nil {
						t.Fatal(err)
					}

				}

				mustReset := map[int64]bool{
					jobs[1].ID: true,
					jobs[2].ID: true,
				}

				err := s.ResetFailedChangesetJobs(ctx, int64(campaignID))
				if err != nil {
					t.Fatal(err)
				}

				have, _, err := s.ListChangesetJobs(ctx, ListChangesetJobsOpts{CampaignID: int64(campaignID)})
				if err != nil {
					t.Fatal(err)
				}

				if len(have) != len(jobs) {
					t.Fatalf("wrong number of jobs returned. have=%d, want=%d", len(have), len(jobs))
				}

				for _, job := range have {
					if _, ok := mustReset[job.ID]; ok {
						if job.Error != "" {
							t.Errorf("job should be reset but has error: %+v", job.Error)
						}
						if !job.FinishedAt.IsZero() {
							t.Errorf("job should be reset but has FinishedAt: %+v", job.FinishedAt)
						}
						if !job.StartedAt.IsZero() {
							t.Errorf("job should be reset but has StartedAt: %+v", job.StartedAt)
						}
					} else {
						if job.StartedAt.IsZero() {
							t.Errorf("job should not be reset but StartedAt is zero: %+v", job.StartedAt)
						}
						if job.FinishedAt.IsZero() {
							t.Errorf("job should not be reset but FinishedAt is zero: %+v", job.FinishedAt)
						}
					}
				}
			})

			t.Run("ResetChangesetJobs", func(t *testing.T) {
				campaignID := 12345
				jobs := []*cmpgn.ChangesetJob{
					// completed, no errors
					{StartedAt: now, FinishedAt: now, ChangesetID: 12345},
					// completed, error
					{StartedAt: now, FinishedAt: now, Error: "error1"},
				}

				for i, j := range jobs {
					j.CampaignID = int64(campaignID)
					j.PatchID = int64(i)

					err := s.CreateChangesetJob(ctx, j)
					if err != nil {
						t.Fatal(err)
					}

				}

				err := s.ResetChangesetJobs(ctx, int64(campaignID))
				if err != nil {
					t.Fatal(err)
				}

				have, _, err := s.ListChangesetJobs(ctx, ListChangesetJobsOpts{CampaignID: int64(campaignID)})
				if err != nil {
					t.Fatal(err)
				}

				if len(have) != len(jobs) {
					t.Fatalf("wrong number of jobs returned. have=%d, want=%d", len(have), len(jobs))
				}

				for _, job := range have {
					if job.Error != "" {
						t.Errorf("job should be reset but has error: %+v", job.Error)
					}
					if !job.FinishedAt.IsZero() {
						t.Errorf("job should be reset but has FinishedAt: %+v", job.FinishedAt)
					}
					if !job.StartedAt.IsZero() {
						t.Errorf("job should be reset but has StartedAt: %+v", job.StartedAt)
					}
				}
			})

			t.Run("GetLatestChangesetJobCreatedAt", func(t *testing.T) {
				patchSet := &cmpgn.PatchSet{}
				err := s.CreatePatchSet(ctx, patchSet)
				if err != nil {
					t.Fatal(err)
				}

				campaign := testCampaign(123, patchSet.ID)
				err = s.CreateCampaign(ctx, campaign)
				if err != nil {
					t.Fatal(err)
				}

				// Cleanup existing ChangesetJobs so we don't have interference
				// between the previous tests and this one.
				chjs, _, err := s.ListChangesetJobs(ctx, ListChangesetJobsOpts{Limit: -1})
				if err != nil {
					t.Fatal(err)
				}
				for _, j := range chjs {
					err := s.DeleteChangesetJob(ctx, j.ID)
					if err != nil {
						t.Fatal(err)
					}
				}

				patch := &cmpgn.Patch{
					PatchSetID: patchSet.ID,
					BaseRef:    "x",
					RepoID:     api.RepoID(123),
				}
				err = s.CreatePatch(ctx, patch)
				if err != nil {
					t.Fatal(err)
				}

				// 0 ChangesetJob, 1 Patches
				have, err := s.GetLatestChangesetJobCreatedAt(ctx, campaign.ID)
				if err != nil {
					t.Fatal(err)
				}
				// Job counts don't match, should get back null
				if !have.IsZero() {
					t.Fatalf("publishedAt is not zero: %v", have)
				}

				changesetJob1 := &cmpgn.ChangesetJob{
					CampaignID: campaign.ID,
					PatchID:    patch.ID,
				}
				err = s.CreateChangesetJob(ctx, changesetJob1)
				if err != nil {
					t.Fatal(err)
				}

				// 1 ChangesetJob, 1 Patches
				have, err = s.GetLatestChangesetJobCreatedAt(ctx, campaign.ID)
				if err != nil {
					t.Fatal(err)
				}
				// Job counts are the same, we should get a valid time
				if !have.Equal(clock()) {
					t.Fatalf("want %v, got %v", clock(), have)
				}

				// Create another round to ensure that we get the latest date
				// when there are more than one
				oldClock := clock
				defer func() {
					clock = oldClock
				}()
				clock = func() time.Time {
					return oldClock().Add(5 * time.Minute)
				}
				oldStore := s
				defer func() {
					s = oldStore
				}()
				s = NewStoreWithClock(tx, clock)
				patch = &cmpgn.Patch{
					PatchSetID: patchSet.ID,
					BaseRef:    "x",
					RepoID:     api.RepoID(123),
				}
				err = s.CreatePatch(ctx, patch)
				if err != nil {
					t.Fatal(err)
				}

				// 1 ChangesetJob, 2 Patches
				have, err = s.GetLatestChangesetJobCreatedAt(ctx, campaign.ID)
				if err != nil {
					t.Fatal(err)
				}
				// Job counts don't match, should get back null
				if !have.IsZero() {
					t.Fatalf("publishedAt is not zero: %v", have)
				}

				// Add another changesetjob
				changesetJob2 := &cmpgn.ChangesetJob{
					CampaignID: campaign.ID,
					PatchID:    patch.ID,
				}
				err = s.CreateChangesetJob(ctx, changesetJob2)
				if err != nil {
					t.Fatal(err)
				}

				// 2 ChangesetJob, 2 Patches
				have, err = s.GetLatestChangesetJobCreatedAt(ctx, campaign.ID)
				if err != nil {
					t.Fatal(err)
				}
				// Job counts are the same, we should get a valid time
				if !have.Equal(clock()) {
					t.Fatalf("want %v, got %v", clock(), have)
				}
			})

		})
	}
}

func testProcessChangesetJob(db *sql.DB, userID int32) func(*testing.T) {
	return func(t *testing.T) {
		now := time.Now().UTC().Truncate(time.Microsecond)
		clock := func() time.Time { return now.UTC().Truncate(time.Microsecond) }
		ctx := context.Background()

		// Create a test repo
		reposStore := repos.NewDBStore(db, sql.TxOptions{})
		repo := &repos.Repo{
			Name: "github.com/sourcegraph/changeset-job-test",
			ExternalRepo: api.ExternalRepoSpec{
				ID:          "external-id",
				ServiceType: "github",
				ServiceID:   "https://github.com/",
			},
			Sources: map[string]*repos.SourceInfo{
				"extsvc:github:4": {
					ID:       "extsvc:github:4",
					CloneURL: "https://secrettoken@github.com/sourcegraph/sourcegraph",
				},
			},
		}
		if err := reposStore.UpsertRepos(context.Background(), repo); err != nil {
			t.Fatal(err)
		}

		s := NewStoreWithClock(db, clock)
		patchSet := &cmpgn.PatchSet{UserID: userID}
		err := s.CreatePatchSet(context.Background(), patchSet)
		if err != nil {
			t.Fatal(err)
		}

		patch := &cmpgn.Patch{
			PatchSetID: patchSet.ID,
			RepoID:     repo.ID,
			BaseRef:    "abc",
		}
		err = s.CreatePatch(context.Background(), patch)
		if err != nil {
			t.Fatal(err)
		}

		campaign := &cmpgn.Campaign{
			PatchSetID:      patchSet.ID,
			Name:            "testcampaign",
			Description:     "testcampaign",
			AuthorID:        userID,
			NamespaceUserID: userID,
		}
		err = s.CreateCampaign(context.Background(), campaign)
		if err != nil {
			t.Fatal(err)
		}

		t.Run("GetPendingChangesetJobsWhenNoneAvailable", func(t *testing.T) {
			tx := dbtest.NewTx(t, db)
			s := NewStoreWithClock(tx, clock)

			process := func(ctx context.Context, s *Store, job cmpgn.ChangesetJob) error {
				return errors.New("rollback")
			}

			ran, err := s.ProcessPendingChangesetJobs(ctx, process)
			if err != nil {
				t.Fatal(err)
			}
			if ran {
				// We shouldn't have any pending jobs yet
				t.Fatalf("process function should not have run")
			}
		})

		t.Run("GetPendingChangesetJobWhenAvailable", func(t *testing.T) {
			tx := dbtest.NewTx(t, db)
			s := NewStoreWithClock(tx, clock)

			process := func(ctx context.Context, s *Store, job cmpgn.ChangesetJob) error {
				return errors.New("rollback")
			}

			job := &cmpgn.ChangesetJob{
				CampaignID: campaign.ID,
				PatchID:    patch.ID,
			}
			err := s.CreateChangesetJob(ctx, job)
			if err != nil {
				t.Fatal(err)
			}

			ran, err := s.ProcessPendingChangesetJobs(ctx, process)
			if err != nil && err.Error() != "rollback" {
				t.Fatal(err)
			}
			if !ran {
				// We shouldn't have any pending jobs yet
				t.Fatalf("process function should have run")
			}
		})

		t.Run("GetPendingChangesetJobsWhenAvailableLocking", func(t *testing.T) {
			s := NewStoreWithClock(db, clock)

			process := func(ctx context.Context, s *Store, job cmpgn.ChangesetJob) error {
				time.Sleep(100 * time.Millisecond)
				return errors.New("rollback")
			}

			job := &cmpgn.ChangesetJob{
				CampaignID: campaign.ID,
				PatchID:    patch.ID,
			}

			err := s.CreateChangesetJob(ctx, job)
			if err != nil {
				t.Fatal(err)
			}
			var runCount int64
			errChan := make(chan error, 2)

			for i := 0; i < 2; i++ {
				go func() {
					ran, err := s.ProcessPendingChangesetJobs(ctx, process)
					if ran {
						atomic.AddInt64(&runCount, 1)
					}
					errChan <- err
				}()
			}
			for i := 0; i < 2; i++ {
				err := <-errChan
				if err != nil && err.Error() != "rollback" {
					t.Fatal(err)
				}
			}

			rc := atomic.LoadInt64(&runCount)
			if rc != 1 {
				t.Errorf("Want %d, got %d", 1, rc)
			}
		})
	}
}

func testStoreLocking(db *sql.DB) func(*testing.T) {
	return func(t *testing.T) {
		now := time.Now().UTC().Truncate(time.Microsecond)
		s := NewStoreWithClock(db, func() time.Time {
			return now.UTC().Truncate(time.Microsecond)
		})

		testKey := "test-acquire"
		s1, err := s.Transact(context.Background())
		if err != nil {
			t.Fatal(err)
		}
		defer s1.Done(nil)

		s2, err := s.Transact(context.Background())
		if err != nil {
			t.Fatal(err)
		}
		defer s2.Done(nil)

		// Get lock
		ok, err := s1.TryAcquireAdvisoryLock(context.Background(), testKey)
		if err != nil {
			t.Fatal(err)
		}
		if !ok {
			t.Fatalf("Could not acquire lock")
		}

		// Try and get acquired lock
		ok, err = s2.TryAcquireAdvisoryLock(context.Background(), testKey)
		if err != nil {
			t.Fatal(err)
		}
		if ok {
			t.Fatal("Should not have acquired lock")
		}

		// Release lock
		s1.Done(nil)

		// Try and get released lock
		ok, err = s2.TryAcquireAdvisoryLock(context.Background(), testKey)
		if err != nil {
			t.Fatal(err)
		}
		if !ok {
			t.Fatal("Could not acquire lock")
		}
	}
}
