package store

import (
	"context"
	"fmt"
	"strconv"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/campaigns/search"
	ct "github.com/sourcegraph/sourcegraph/enterprise/internal/campaigns/testing"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/campaigns"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func testStoreChangesetSpecs(t *testing.T, ctx context.Context, s *Store, clock ct.Clock) {
	repoStore := database.ReposWith(s)
	esStore := database.ExternalServicesWith(s)

	repo := ct.TestRepo(t, esStore, extsvc.KindGitHub)
	deletedRepo := ct.TestRepo(t, esStore, extsvc.KindGitHub).With(types.Opt.RepoDeletedAt(clock.Now()))

	if err := repoStore.Create(ctx, repo); err != nil {
		t.Fatal(err)
	}
	if err := repoStore.Delete(ctx, deletedRepo.ID); err != nil {
		t.Fatal(err)
	}

	changesetSpecs := make(campaigns.ChangesetSpecs, 0, 3)
	for i := 0; i < cap(changesetSpecs); i++ {
		c := &campaigns.ChangesetSpec{
			RawSpec: `{"externalID":"12345"}`,
			Spec: &campaigns.ChangesetSpecDescription{
				ExternalID: "123456",
			},
			UserID:         int32(i + 1234),
			CampaignSpecID: int64(i + 910),
			RepoID:         repo.ID,

			DiffStatAdded:   123,
			DiffStatChanged: 456,
			DiffStatDeleted: 789,
		}

		if i == cap(changesetSpecs)-1 {
			c.CampaignSpecID = 0
		}
		changesetSpecs = append(changesetSpecs, c)
	}

	// We create this ChangesetSpec to make sure that it's not returned when
	// listing or getting ChangesetSpecs, since we don't want to load
	// ChangesetSpecs whose repository has been (soft-)deleted.
	changesetSpecDeletedRepo := &campaigns.ChangesetSpec{
		UserID:         int32(424242),
		Spec:           &campaigns.ChangesetSpecDescription{},
		CampaignSpecID: int64(424242),
		RawSpec:        `{}`,
		RepoID:         deletedRepo.ID,
	}

	t.Run("Create", func(t *testing.T) {
		toCreate := make(campaigns.ChangesetSpecs, 0, len(changesetSpecs)+1)
		toCreate = append(toCreate, changesetSpecDeletedRepo)
		toCreate = append(toCreate, changesetSpecs...)

		for _, c := range toCreate {
			want := c.Clone()
			have := c

			err := s.CreateChangesetSpec(ctx, have)
			if err != nil {
				t.Fatal(err)
			}

			if have.ID == 0 {
				t.Fatal("ID should not be zero")
			}

			if have.RandID == "" {
				t.Fatal("RandID should not be empty")
			}

			want.ID = have.ID
			want.RandID = have.RandID
			want.CreatedAt = clock.Now()
			want.UpdatedAt = clock.Now()

			if diff := cmp.Diff(have, want); diff != "" {
				t.Fatal(diff)
			}
		}
	})

	t.Run("Count", func(t *testing.T) {
		count, err := s.CountChangesetSpecs(ctx, CountChangesetSpecsOpts{})
		if err != nil {
			t.Fatal(err)
		}

		if have, want := count, len(changesetSpecs); have != want {
			t.Fatalf("have count: %d, want: %d", have, want)
		}

		t.Run("WithCampaignSpecID", func(t *testing.T) {
			testsRan := false
			for _, c := range changesetSpecs {
				if c.CampaignSpecID == 0 {
					continue
				}

				opts := CountChangesetSpecsOpts{CampaignSpecID: c.CampaignSpecID}
				subCount, err := s.CountChangesetSpecs(ctx, opts)
				if err != nil {
					t.Fatal(err)
				}

				if have, want := subCount, 1; have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}
				testsRan = true
			}

			if !testsRan {
				t.Fatal("no changesetSpec has a non-zero CampaignSpecID")
			}
		})
	})

	t.Run("List", func(t *testing.T) {
		t.Run("NoLimit", func(t *testing.T) {
			// Empty limit should return all entries.
			opts := ListChangesetSpecsOpts{}
			ts, next, err := s.ListChangesetSpecs(ctx, opts)
			if err != nil {
				t.Fatal(err)
			}

			if have, want := next, int64(0); have != want {
				t.Fatalf("opts: %+v: have next %v, want %v", opts, have, want)
			}

			have, want := ts, changesetSpecs
			if len(have) != len(want) {
				t.Fatalf("listed %d changesetSpecs, want: %d", len(have), len(want))
			}

			if diff := cmp.Diff(have, want); diff != "" {
				t.Fatalf("opts: %+v, diff: %s", opts, diff)
			}
		})

		t.Run("WithLimit", func(t *testing.T) {
			for i := 1; i <= len(changesetSpecs); i++ {
				cs, next, err := s.ListChangesetSpecs(ctx, ListChangesetSpecsOpts{LimitOpts: LimitOpts{Limit: i}})
				if err != nil {
					t.Fatal(err)
				}

				{
					have, want := next, int64(0)
					if i < len(changesetSpecs) {
						want = changesetSpecs[i].ID
					}

					if have != want {
						t.Fatalf("limit: %v: have next %v, want %v", i, have, want)
					}
				}

				{
					have, want := cs, changesetSpecs[:i]
					if len(have) != len(want) {
						t.Fatalf("listed %d changesetSpecs, want: %d", len(have), len(want))
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				}
			}
		})

		t.Run("WithLimitAndCursor", func(t *testing.T) {
			var cursor int64
			for i := 1; i <= len(changesetSpecs); i++ {
				opts := ListChangesetSpecsOpts{Cursor: cursor, LimitOpts: LimitOpts{Limit: 1}}
				have, next, err := s.ListChangesetSpecs(ctx, opts)
				if err != nil {
					t.Fatal(err)
				}

				want := changesetSpecs[i-1 : i]
				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", opts, diff)
				}

				cursor = next
			}
		})

		t.Run("WithCampaignSpecID", func(t *testing.T) {
			for _, c := range changesetSpecs {
				if c.CampaignSpecID == 0 {
					continue
				}
				opts := ListChangesetSpecsOpts{CampaignSpecID: c.CampaignSpecID}
				have, _, err := s.ListChangesetSpecs(ctx, opts)
				if err != nil {
					t.Fatal(err)
				}

				want := campaigns.ChangesetSpecs{c}
				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", opts, diff)
				}
			}
		})

		t.Run("WithRandIDs", func(t *testing.T) {
			for _, c := range changesetSpecs {
				opts := ListChangesetSpecsOpts{RandIDs: []string{c.RandID}}
				have, _, err := s.ListChangesetSpecs(ctx, opts)
				if err != nil {
					t.Fatal(err)
				}

				want := campaigns.ChangesetSpecs{c}
				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", opts, diff)
				}
			}

			opts := ListChangesetSpecsOpts{}
			for _, c := range changesetSpecs {
				opts.RandIDs = append(opts.RandIDs, c.RandID)
			}

			have, _, err := s.ListChangesetSpecs(ctx, opts)
			if err != nil {
				t.Fatal(err)
			}

			// ListChangesetSpecs should not return ChangesetSpecs whose
			// repository was (soft-)deleted.
			if diff := cmp.Diff(have, changesetSpecs); diff != "" {
				t.Fatalf("opts: %+v, diff: %s", opts, diff)
			}
		})

		t.Run("WithIDs", func(t *testing.T) {
			for _, c := range changesetSpecs {
				opts := ListChangesetSpecsOpts{IDs: []int64{c.ID}}
				have, _, err := s.ListChangesetSpecs(ctx, opts)
				if err != nil {
					t.Fatal(err)
				}

				want := campaigns.ChangesetSpecs{c}
				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", opts, diff)
				}
			}

			opts := ListChangesetSpecsOpts{}
			for _, c := range changesetSpecs {
				opts.IDs = append(opts.IDs, c.ID)
			}

			have, _, err := s.ListChangesetSpecs(ctx, opts)
			if err != nil {
				t.Fatal(err)
			}

			// ListChangesetSpecs should not return ChangesetSpecs whose
			// repository was (soft-)deleted.
			if diff := cmp.Diff(have, changesetSpecs); diff != "" {
				t.Fatalf("opts: %+v, diff: %s", opts, diff)
			}
		})
	})

	t.Run("Update", func(t *testing.T) {
		for _, c := range changesetSpecs {
			c.UserID += 1234
			c.DiffStatAdded += 1234
			c.DiffStatChanged += 1234
			c.DiffStatDeleted += 1234

			clock.Add(1 * time.Second)

			want := c
			want.UpdatedAt = clock.Now()

			have := c.Clone()
			if err := s.UpdateChangesetSpec(ctx, have); err != nil {
				t.Fatal(err)
			}

			if diff := cmp.Diff(have, want); diff != "" {
				t.Fatal(diff)
			}
		}
	})

	t.Run("Get", func(t *testing.T) {
		want := changesetSpecs[1]
		tests := map[string]GetChangesetSpecOpts{
			"ByID":          {ID: want.ID},
			"ByRandID":      {RandID: want.RandID},
			"ByIDAndRandID": {ID: want.ID, RandID: want.RandID},
		}

		for name, opts := range tests {
			t.Run(name, func(t *testing.T) {
				have, err := s.GetChangesetSpec(ctx, opts)
				if err != nil {
					t.Fatal(err)
				}

				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatal(diff)
				}
			})
		}

		t.Run("NoResults", func(t *testing.T) {
			opts := GetChangesetSpecOpts{ID: 0xdeadbeef}

			_, have := s.GetChangesetSpec(ctx, opts)
			want := ErrNoResults

			if have != want {
				t.Fatalf("have err %v, want %v", have, want)
			}
		})
	})

	t.Run("Delete", func(t *testing.T) {
		for i := range changesetSpecs {
			err := s.DeleteChangesetSpec(ctx, changesetSpecs[i].ID)
			if err != nil {
				t.Fatal(err)
			}

			count, err := s.CountChangesetSpecs(ctx, CountChangesetSpecsOpts{})
			if err != nil {
				t.Fatal(err)
			}

			if have, want := count, len(changesetSpecs)-(i+1); have != want {
				t.Fatalf("have count: %d, want: %d", have, want)
			}
		}
	})

	t.Run("DeleteExpiredChangesetSpecs", func(t *testing.T) {
		underTTL := clock.Now().Add(-campaigns.ChangesetSpecTTL + 24*time.Hour)
		overTTL := clock.Now().Add(-campaigns.ChangesetSpecTTL - 24*time.Hour)

		type testCase struct {
			createdAt time.Time

			hasCampaignSpec     bool
			campaignSpecApplied bool

			isCurrentSpec  bool
			isPreviousSpec bool

			wantDeleted bool
		}

		printTestCase := func(tc testCase) string {
			var tooOld bool
			if tc.createdAt.Equal(overTTL) {
				tooOld = true
			}

			return fmt.Sprintf(
				"[tooOld=%t, hasCampaignSpec=%t, campaignSpecApplied=%t, isCurrentSpec=%t, isPreviousSpec=%t]",
				tooOld, tc.hasCampaignSpec, tc.campaignSpecApplied, tc.isCurrentSpec, tc.isPreviousSpec,
			)
		}

		tests := []testCase{
			// ChangesetSpec was created but never attached to a CampaignSpec
			{hasCampaignSpec: false, createdAt: underTTL, wantDeleted: false},
			{hasCampaignSpec: false, createdAt: overTTL, wantDeleted: true},

			// Attached to CampaignSpec that's applied to a Campaign
			{hasCampaignSpec: true, campaignSpecApplied: true, isCurrentSpec: true, createdAt: underTTL, wantDeleted: false},
			{hasCampaignSpec: true, campaignSpecApplied: true, isCurrentSpec: true, createdAt: overTTL, wantDeleted: false},

			// CampaignSpec is not applied to a Campaign anymore and the
			// ChangesetSpecs are now the PreviousSpec.
			{hasCampaignSpec: true, isPreviousSpec: true, createdAt: underTTL, wantDeleted: false},
			{hasCampaignSpec: true, isPreviousSpec: true, createdAt: overTTL, wantDeleted: false},

			// Has a CampaignSpec, but that CampaignSpec is not applied
			// anymore, and the ChangesetSpec is neither the current, nor the
			// previous spec.
			{hasCampaignSpec: true, createdAt: underTTL, wantDeleted: false},
			{hasCampaignSpec: true, createdAt: overTTL, wantDeleted: true},
		}

		for _, tc := range tests {
			campaignSpec := &campaigns.CampaignSpec{UserID: 4567, NamespaceUserID: 4567}

			if tc.hasCampaignSpec {
				if err := s.CreateCampaignSpec(ctx, campaignSpec); err != nil {
					t.Fatal(err)
				}

				if tc.campaignSpecApplied {
					campaign := &campaigns.Campaign{
						Name:             fmt.Sprintf("campaign for spec %d", campaignSpec.ID),
						CampaignSpecID:   campaignSpec.ID,
						InitialApplierID: campaignSpec.UserID,
						NamespaceUserID:  campaignSpec.NamespaceUserID,
						LastApplierID:    campaignSpec.UserID,
						LastAppliedAt:    time.Now(),
					}
					if err := s.CreateCampaign(ctx, campaign); err != nil {
						t.Fatal(err)
					}
				}
			}

			changesetSpec := &campaigns.ChangesetSpec{
				CampaignSpecID: campaignSpec.ID,
				// Need to set a RepoID otherwise GetChangesetSpec filters it out.
				RepoID:    repo.ID,
				CreatedAt: tc.createdAt,
			}

			if err := s.CreateChangesetSpec(ctx, changesetSpec); err != nil {
				t.Fatal(err)
			}

			if tc.isCurrentSpec {
				changeset := &campaigns.Changeset{
					ExternalServiceType: "github",
					RepoID:              1,
					CurrentSpecID:       changesetSpec.ID,
				}
				if err := s.CreateChangeset(ctx, changeset); err != nil {
					t.Fatal(err)
				}
			}

			if tc.isPreviousSpec {
				changeset := &campaigns.Changeset{
					ExternalServiceType: "github",
					RepoID:              1,
					PreviousSpecID:      changesetSpec.ID,
				}
				if err := s.CreateChangeset(ctx, changeset); err != nil {
					t.Fatal(err)
				}
			}

			if err := s.DeleteExpiredChangesetSpecs(ctx); err != nil {
				t.Fatal(err)
			}

			_, err := s.GetChangesetSpec(ctx, GetChangesetSpecOpts{ID: changesetSpec.ID})
			if err != nil && err != ErrNoResults {
				t.Fatal(err)
			}

			if tc.wantDeleted && err == nil {
				t.Fatalf("tc=%s\n\t want changeset spec to be deleted, but was NOT", printTestCase(tc))
			}

			if !tc.wantDeleted && err == ErrNoResults {
				t.Fatalf("tc=%s\n\t want changeset spec NOT to be deleted, but got deleted", printTestCase(tc))
			}
		}
	})

	t.Run("GetRewirerMappings", func(t *testing.T) {
		// Create some test data
		user := ct.CreateTestUser(t, true)
		campaignSpec := ct.CreateCampaignSpec(t, ctx, s, "get-rewirer-mappings", user.ID)
		var mappings RewirerMappings = make(RewirerMappings, 3)
		changesetSpecIDs := make([]int64, 0, cap(mappings))
		for i := 0; i < cap(mappings); i++ {
			spec := ct.CreateChangesetSpec(t, ctx, s, ct.TestSpecOpts{
				HeadRef:      fmt.Sprintf("refs/heads/test-get-rewirer-mappings-%d", i),
				Repo:         repo.ID,
				CampaignSpec: campaignSpec.ID,
			})
			changesetSpecIDs = append(changesetSpecIDs, spec.ID)
			mappings[i] = &RewirerMapping{
				ChangesetSpecID: spec.ID,
				RepoID:          repo.ID,
			}
		}

		t.Run("NoLimit", func(t *testing.T) {
			// Empty limit should return all entries.
			opts := GetRewirerMappingsOpts{
				CampaignSpecID: campaignSpec.ID,
			}
			ts, err := s.GetRewirerMappings(ctx, opts)
			if err != nil {
				t.Fatal(err)
			}

			{
				have, want := ts.RepoIDs(), []api.RepoID{repo.ID}
				if len(have) != len(want) {
					t.Fatalf("listed %d repo ids, want: %d", len(have), len(want))
				}

				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", opts, diff)
				}
			}

			{
				have, want := ts.ChangesetIDs(), []int64{}
				if len(have) != len(want) {
					t.Fatalf("listed %d changeset ids, want: %d", len(have), len(want))
				}

				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", opts, diff)
				}
			}

			{
				have, want := ts.ChangesetSpecIDs(), changesetSpecIDs
				if len(have) != len(want) {
					t.Fatalf("listed %d changeset spec ids, want: %d", len(have), len(want))
				}

				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", opts, diff)
				}
			}

			{
				have, want := ts, mappings
				if len(have) != len(want) {
					t.Fatalf("listed %d mappings, want: %d", len(have), len(want))
				}

				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", opts, diff)
				}
			}
		})

		t.Run("WithLimit", func(t *testing.T) {
			for i := 1; i <= len(mappings); i++ {
				t.Run(strconv.Itoa(i), func(t *testing.T) {
					opts := GetRewirerMappingsOpts{
						CampaignSpecID: campaignSpec.ID,
						LimitOffset:    &database.LimitOffset{Limit: i},
					}
					ts, err := s.GetRewirerMappings(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					{
						have, want := ts.RepoIDs(), []api.RepoID{repo.ID}
						if len(have) != len(want) {
							t.Fatalf("listed %d repo ids, want: %d", len(have), len(want))
						}

						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}
					}

					{
						have, want := ts.ChangesetIDs(), []int64{}
						if len(have) != len(want) {
							t.Fatalf("listed %d changeset ids, want: %d", len(have), len(want))
						}

						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}
					}

					{
						have, want := ts.ChangesetSpecIDs(), changesetSpecIDs[:i]
						if len(have) != len(want) {
							t.Fatalf("listed %d changeset spec ids, want: %d", len(have), len(want))
						}

						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}
					}

					{
						have, want := ts, mappings[:i]
						if len(have) != len(want) {
							t.Fatalf("listed %d mappings, want: %d", len(have), len(want))
						}

						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatal(diff)
						}
					}
				})
			}
		})

		t.Run("WithLimitAndOffset", func(t *testing.T) {
			offset := 0
			for i := 1; i <= len(mappings); i++ {
				t.Run(strconv.Itoa(i), func(t *testing.T) {
					opts := GetRewirerMappingsOpts{
						CampaignSpecID: campaignSpec.ID,
						LimitOffset:    &database.LimitOffset{Limit: 1, Offset: offset},
					}
					ts, err := s.GetRewirerMappings(ctx, opts)
					if err != nil {
						t.Fatal(err)
					}

					{
						have, want := ts.RepoIDs(), []api.RepoID{repo.ID}
						if len(have) != len(want) {
							t.Fatalf("listed %d repo ids, want: %d", len(have), len(want))
						}

						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}
					}

					{
						have, want := ts.ChangesetIDs(), []int64{}
						if len(have) != len(want) {
							t.Fatalf("listed %d changeset ids, want: %d", len(have), len(want))
						}

						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}
					}

					{
						have, want := ts.ChangesetSpecIDs(), changesetSpecIDs[i-1:i]
						if len(have) != len(want) {
							t.Fatalf("listed %d changeset spec ids, want: %d", len(have), len(want))
						}

						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}
					}

					{
						have, want := ts, mappings[i-1:i]
						if diff := cmp.Diff(have, want); diff != "" {
							t.Fatalf("opts: %+v, diff: %s", opts, diff)
						}
					}

					offset++
				})
			}
		})
	})
}

func testStoreChangesetSpecsTextSearch(t *testing.T, ctx context.Context, s *Store, clock ct.Clock) {
	repoStore := database.ReposWith(s)
	esStore := database.ExternalServicesWith(s)

	// OK, let's set up an interesting scenario. We're going to set up a
	// campaign that tracks two changesets in different repositories, and
	// creates two changesets in those same repositories with different names.

	// First up, let's create the repos.
	repos := []*types.Repo{
		ct.TestRepo(t, esStore, extsvc.KindGitHub),
		ct.TestRepo(t, esStore, extsvc.KindGitLab),
	}
	for _, repo := range repos {
		if err := repoStore.Create(ctx, repo); err != nil {
			t.Fatal(err)
		}
	}

	// Create a user.
	user := ct.CreateTestUser(t, false)

	// Next, we need a campaign spec.
	oldCampaignSpec := ct.CreateCampaignSpec(t, ctx, s, "text", user.ID)

	// That's enough to create a campaign, so let's do that.
	campaign := ct.CreateCampaign(t, ctx, s, "text", user.ID, oldCampaignSpec.ID)

	// Now we can create the changeset specs.
	oldTrackedGitHubSpec := ct.CreateChangesetSpec(t, ctx, s, ct.TestSpecOpts{
		User:         user.ID,
		Repo:         repos[0].ID,
		CampaignSpec: oldCampaignSpec.ID,
		ExternalID:   "1234",
	})
	oldTrackedGitLabSpec := ct.CreateChangesetSpec(t, ctx, s, ct.TestSpecOpts{
		User:         user.ID,
		Repo:         repos[1].ID,
		CampaignSpec: oldCampaignSpec.ID,
		ExternalID:   "1234",
	})
	oldBranchGitHubSpec := ct.CreateChangesetSpec(t, ctx, s, ct.TestSpecOpts{
		User:         user.ID,
		Repo:         repos[0].ID,
		CampaignSpec: oldCampaignSpec.ID,
		HeadRef:      "main",
		Published:    true,
		Title:        "GitHub branch",
	})
	oldBranchGitLabSpec := ct.CreateChangesetSpec(t, ctx, s, ct.TestSpecOpts{
		User:         user.ID,
		Repo:         repos[1].ID,
		CampaignSpec: oldCampaignSpec.ID,
		HeadRef:      "main",
		Published:    true,
		Title:        "GitLab branch",
	})

	// We also need actual changesets.
	oldTrackedGitHub := ct.CreateChangeset(t, ctx, s, ct.TestChangesetOpts{
		Repo:                repos[0].ID,
		Campaign:            campaign.ID,
		CurrentSpec:         oldTrackedGitHubSpec.ID,
		ExternalServiceType: repos[0].ExternalRepo.ServiceType,
		ExternalID:          "1234",
		OwnedByCampaign:     campaign.ID,
		Metadata: map[string]interface{}{
			"Title": "Tracked GitHub",
		},
	})
	oldTrackedGitLab := ct.CreateChangeset(t, ctx, s, ct.TestChangesetOpts{
		Repo:                repos[1].ID,
		Campaign:            campaign.ID,
		CurrentSpec:         oldTrackedGitLabSpec.ID,
		ExternalServiceType: repos[1].ExternalRepo.ServiceType,
		ExternalID:          "1234",
		OwnedByCampaign:     campaign.ID,
		Metadata: map[string]interface{}{
			"title": "Tracked GitLab",
		},
	})
	oldBranchGitHub := ct.CreateChangeset(t, ctx, s, ct.TestChangesetOpts{
		Repo:                repos[0].ID,
		Campaign:            campaign.ID,
		CurrentSpec:         oldBranchGitHubSpec.ID,
		ExternalServiceType: repos[0].ExternalRepo.ServiceType,
		ExternalID:          "5678",
		OwnedByCampaign:     campaign.ID,
		Metadata: map[string]interface{}{
			"Title": "GitHub branch",
		},
	})
	oldBranchGitLab := ct.CreateChangeset(t, ctx, s, ct.TestChangesetOpts{
		Repo:                repos[1].ID,
		Campaign:            campaign.ID,
		CurrentSpec:         oldBranchGitLabSpec.ID,
		ExternalServiceType: repos[1].ExternalRepo.ServiceType,
		ExternalID:          "5678",
		OwnedByCampaign:     campaign.ID,
		Metadata: map[string]interface{}{
			"title": "GitLab branch",
		},
	})
	// Cool. Now let's set up a new campaign spec.
	newCampaignSpec := ct.CreateCampaignSpec(t, ctx, s, "text", user.ID)

	// And we need all new changeset specs to go into that spec.
	newTrackedGitHub := ct.CreateChangesetSpec(t, ctx, s, ct.TestSpecOpts{
		User:         user.ID,
		Repo:         repos[0].ID,
		CampaignSpec: newCampaignSpec.ID,
		ExternalID:   "1234",
	})
	newTrackedGitLab := ct.CreateChangesetSpec(t, ctx, s, ct.TestSpecOpts{
		User:         user.ID,
		Repo:         repos[1].ID,
		CampaignSpec: newCampaignSpec.ID,
		ExternalID:   "1234",
	})
	newBranchGitHub := ct.CreateChangesetSpec(t, ctx, s, ct.TestSpecOpts{
		User:         user.ID,
		Repo:         repos[0].ID,
		CampaignSpec: newCampaignSpec.ID,
		HeadRef:      "main",
		Published:    true,
		Title:        "New GitHub branch",
	})
	newBranchGitLab := ct.CreateChangesetSpec(t, ctx, s, ct.TestSpecOpts{
		User:         user.ID,
		Repo:         repos[1].ID,
		CampaignSpec: newCampaignSpec.ID,
		HeadRef:      "main",
		Published:    true,
		Title:        "New GitLab branch",
	})

	// A couple of hundred lines of boilerplate later, we have a scenario! Let's
	// use it.

	// Well, OK, I lied: we're not _quite_ done with the boilerplate. To keep
	// the test cases somewhat readable, we'll define the four possible mappings
	// we can get before we get to defining the test cases.
	trackedGitHub := &RewirerMapping{
		ChangesetSpecID: newTrackedGitHub.ID,
		ChangesetID:     oldTrackedGitHub.ID,
		RepoID:          repos[0].ID,
	}
	trackedGitLab := &RewirerMapping{
		ChangesetSpecID: newTrackedGitLab.ID,
		ChangesetID:     oldTrackedGitLab.ID,
		RepoID:          repos[1].ID,
	}
	branchGitHub := &RewirerMapping{
		ChangesetSpecID: newBranchGitHub.ID,
		ChangesetID:     oldBranchGitHub.ID,
		RepoID:          repos[0].ID,
	}
	branchGitLab := &RewirerMapping{
		ChangesetSpecID: newBranchGitLab.ID,
		ChangesetID:     oldBranchGitLab.ID,
		RepoID:          repos[1].ID,
	}

	for name, tc := range map[string]struct {
		search []search.TextSearchTerm
		want   RewirerMappings
	}{
		"nil search": {
			want: RewirerMappings{trackedGitHub, trackedGitLab, branchGitHub, branchGitLab},
		},
		"empty search": {
			search: []search.TextSearchTerm{},
			want:   RewirerMappings{trackedGitHub, trackedGitLab, branchGitHub, branchGitLab},
		},
		"no matches": {
			search: []search.TextSearchTerm{{Term: "this is not a thing"}},
			want:   nil,
		},
		"no matches due to conflicting requirements": {
			search: []search.TextSearchTerm{
				{Term: "GitHub"},
				{Term: "GitLab"},
			},
			want: nil,
		},
		"no matches due to even more conflicting requirements": {
			search: []search.TextSearchTerm{
				{Term: "GitHub"},
				{Term: "GitHub", Not: true},
			},
			want: nil,
		},
		"one term, matched on title": {
			search: []search.TextSearchTerm{{Term: "New GitHub branch"}},
			want:   RewirerMappings{branchGitHub},
		},
		"two terms, matched on title AND title": {
			search: []search.TextSearchTerm{
				{Term: "New GitHub"},
				{Term: "branch"},
			},
			want: RewirerMappings{branchGitHub},
		},
		"two terms, matched on title AND repo": {
			search: []search.TextSearchTerm{
				{Term: "New"},
				{Term: string(repos[0].Name)},
			},
			want: RewirerMappings{branchGitHub},
		},
		"one term, matched on repo": {
			search: []search.TextSearchTerm{{Term: string(repos[0].Name)}},
			want:   RewirerMappings{trackedGitHub, branchGitHub},
		},
		"one negated term, three title matches": {
			search: []search.TextSearchTerm{{Term: "New GitHub branch", Not: true}},
			want:   RewirerMappings{trackedGitHub, trackedGitLab, branchGitLab},
		},
		"two negated terms, one title AND repo match": {
			search: []search.TextSearchTerm{
				{Term: "New", Not: true},
				{Term: string(repos[0].Name), Not: true},
			},
			want: RewirerMappings{trackedGitLab},
		},
		"mixed positive and negative terms": {
			search: []search.TextSearchTerm{
				{Term: "New", Not: true},
				{Term: string(repos[0].Name)},
			},
			want: RewirerMappings{trackedGitHub},
		},
	} {
		t.Run(name, func(t *testing.T) {
			t.Run("no limits", func(t *testing.T) {
				have, err := s.GetRewirerMappings(ctx, GetRewirerMappingsOpts{
					CampaignSpecID: newCampaignSpec.ID,
					CampaignID:     campaign.ID,
					TextSearch:     tc.search,
				})
				if err != nil {
					t.Errorf("unexpected error: %+v", err)
				}

				if diff := cmp.Diff(have, tc.want); diff != "" {
					t.Errorf("unexpected mappings (-have +want):\n%s", diff)
				}
			})

			t.Run("with limit", func(t *testing.T) {
				have, err := s.GetRewirerMappings(ctx, GetRewirerMappingsOpts{
					CampaignSpecID: newCampaignSpec.ID,
					CampaignID:     campaign.ID,
					LimitOffset:    &database.LimitOffset{Limit: 1},
					TextSearch:     tc.search,
				})
				if err != nil {
					t.Errorf("unexpected error: %+v", err)
				}

				var want RewirerMappings
				if len(tc.want) > 0 {
					want = tc.want[0:1]
				}
				if diff := cmp.Diff(have, want); diff != "" {
					t.Errorf("unexpected mappings (-have +want):\n%s", diff)
				}
			})

			t.Run("with offset and limit", func(t *testing.T) {
				have, err := s.GetRewirerMappings(ctx, GetRewirerMappingsOpts{
					CampaignSpecID: newCampaignSpec.ID,
					CampaignID:     campaign.ID,
					LimitOffset:    &database.LimitOffset{Offset: 1, Limit: 1},
					TextSearch:     tc.search,
				})
				if err != nil {
					t.Errorf("unexpected error: %+v", err)
				}

				var want RewirerMappings
				if len(tc.want) > 1 {
					want = tc.want[1:2]
				}
				if diff := cmp.Diff(have, want); diff != "" {
					t.Errorf("unexpected mappings (-have +want):\n%s", diff)
				}
			})
		})
	}
}
