package background

import (
	"context"
	"sort"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/campaigns/store"
	ct "github.com/sourcegraph/sourcegraph/enterprise/internal/campaigns/testing"
	"github.com/sourcegraph/sourcegraph/internal/campaigns"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtesting"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
)

func TestWorkerView(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	ctx := context.Background()
	db := dbtesting.GetDB(t)

	now := timeutil.Now()
	clock := func() time.Time { return now }
	cstore := store.NewWithClock(db, clock)

	user := ct.CreateTestUser(t, db, true)
	spec := ct.CreateCampaignSpec(t, ctx, cstore, "test-campaign", user.ID)
	campaign := ct.CreateCampaign(t, ctx, cstore, "test-campaign", user.ID, spec.ID)
	repos, _ := ct.CreateTestRepos(t, ctx, cstore.DB(), 2)
	repo := repos[0]
	deletedRepo := repos[1]
	if err := cstore.Repos().Delete(ctx, deletedRepo.ID); err != nil {
		t.Fatal(err)
	}

	t.Run("Queued changeset", func(t *testing.T) {
		c := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
			Repo:            repo.ID,
			Campaign:        campaign.ID,
			ReconcilerState: campaigns.ReconcilerStateQueued,
		})
		t.Cleanup(func() {
			if err := cstore.DeleteChangeset(ctx, c.ID); err != nil {
				t.Fatal(err)
			}
		})
		assertReturnedChangesetIDs(t, ctx, cstore.DB(), []int{int(c.ID)})
	})
	t.Run("Not in campaign", func(t *testing.T) {
		c := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
			Repo:            repo.ID,
			Campaign:        0,
			ReconcilerState: campaigns.ReconcilerStateQueued,
		})
		t.Cleanup(func() {
			if err := cstore.DeleteChangeset(ctx, c.ID); err != nil {
				t.Fatal(err)
			}
		})
		assertReturnedChangesetIDs(t, ctx, cstore.DB(), []int{})
	})
	t.Run("In campaign with deleted user namespace", func(t *testing.T) {
		deletedUser := ct.CreateTestUser(t, db, true)
		if err := database.UsersWith(cstore).Delete(ctx, deletedUser.ID); err != nil {
			t.Fatal(err)
		}
		userCampaign := ct.CreateCampaign(t, ctx, cstore, "test-user-namespace", deletedUser.ID, spec.ID)
		c := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
			Repo:            repo.ID,
			Campaign:        userCampaign.ID,
			ReconcilerState: campaigns.ReconcilerStateQueued,
		})
		t.Cleanup(func() {
			if err := cstore.DeleteChangeset(ctx, c.ID); err != nil {
				t.Fatal(err)
			}
		})
		assertReturnedChangesetIDs(t, ctx, cstore.DB(), []int{})
	})
	t.Run("In campaign with deleted org namespace", func(t *testing.T) {
		orgID := ct.InsertTestOrg(t, db, "deleted-org")
		if err := database.OrgsWith(cstore).Delete(ctx, orgID); err != nil {
			t.Fatal(err)
		}
		orgCampaign := ct.BuildCampaign(cstore, "test-user-namespace", 0, spec.ID)
		orgCampaign.NamespaceOrgID = orgID
		if err := cstore.CreateCampaign(ctx, orgCampaign); err != nil {
			t.Fatal(err)
		}
		c := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
			Repo:            repo.ID,
			Campaign:        orgCampaign.ID,
			ReconcilerState: campaigns.ReconcilerStateQueued,
		})
		t.Cleanup(func() {
			if err := cstore.DeleteChangeset(ctx, c.ID); err != nil {
				t.Fatal(err)
			}
		})
		assertReturnedChangesetIDs(t, ctx, cstore.DB(), []int{})
	})
	t.Run("In campaign with deleted namespace but another campaign with an existing one", func(t *testing.T) {
		deletedUser := ct.CreateTestUser(t, db, true)
		if err := database.UsersWith(cstore).Delete(ctx, deletedUser.ID); err != nil {
			t.Fatal(err)
		}
		userCampaign := ct.CreateCampaign(t, ctx, cstore, "test-user-namespace", deletedUser.ID, spec.ID)
		c := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
			Repo:            repo.ID,
			Campaign:        userCampaign.ID,
			ReconcilerState: campaigns.ReconcilerStateQueued,
		})
		// Attach second campaign
		c.Attach(campaign.ID)
		if err := cstore.UpdateChangeset(ctx, c); err != nil {
			t.Fatal(err)
		}
		t.Cleanup(func() {
			if err := cstore.DeleteChangeset(ctx, c.ID); err != nil {
				t.Fatal(err)
			}
		})
		assertReturnedChangesetIDs(t, ctx, cstore.DB(), []int{int(c.ID)})
	})
	t.Run("In deleted repo", func(t *testing.T) {
		c := ct.CreateChangeset(t, ctx, cstore, ct.TestChangesetOpts{
			Repo:            deletedRepo.ID,
			Campaign:        campaign.ID,
			ReconcilerState: campaigns.ReconcilerStateQueued,
		})
		t.Cleanup(func() {
			if err := cstore.DeleteChangeset(ctx, c.ID); err != nil {
				t.Fatal(err)
			}
		})
		assertReturnedChangesetIDs(t, ctx, cstore.DB(), []int{})
	})
}

func assertReturnedChangesetIDs(t *testing.T, ctx context.Context, db dbutil.DB, want []int) {
	t.Helper()

	have := make([]int, 0)

	q := sqlf.Sprintf("SELECT id FROM reconciler_changesets")
	rows, err := db.QueryContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
	for rows.Next() {
		var id int
		err = rows.Scan(&id)
		have = append(have, id)
		if err != nil {
			t.Fatal(err)
		}
	}
	if rows.Err() != nil {
		t.Fatal(err)
	}
	if rows.Close() != nil {
		t.Fatal(err)
	}

	sort.Ints(have)
	sort.Ints(want)

	if diff := cmp.Diff(have, want); diff != "" {
		t.Fatalf("invalid IDs returned: diff = %s", diff)
	}
}
