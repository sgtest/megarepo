package store

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/lib/batches/overridable"

	ct "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/testing"
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	batcheslib "github.com/sourcegraph/sourcegraph/lib/batches"
)

func testStoreBatchSpecs(t *testing.T, ctx context.Context, s *Store, clock ct.Clock) {
	batchSpecs := make([]*btypes.BatchSpec, 0, 3)

	t.Run("Create", func(t *testing.T) {
		for i := 0; i < cap(batchSpecs); i++ {
			falsy := overridable.FromBoolOrString(false)
			c := &btypes.BatchSpec{
				RawSpec: `{"name": "Foobar", "description": "My description"}`,
				Spec: &batcheslib.BatchSpec{
					Name:        "Foobar",
					Description: "My description",
					ChangesetTemplate: &batcheslib.ChangesetTemplate{
						Title:  "Hello there",
						Body:   "This is the body",
						Branch: "my-branch",
						Commit: batcheslib.ExpandedGitCommitDescription{
							Message: "commit message",
						},
						Published: &falsy,
					},
				},
				CreatedFromRaw:   true,
				AllowUnsupported: true,
				AllowIgnored:     true,
				UserID:           int32(i + 1234),
			}

			if i%2 == 0 {
				c.NamespaceOrgID = 23
			} else {
				c.NamespaceUserID = c.UserID
			}

			want := c.Clone()
			have := c

			err := s.CreateBatchSpec(ctx, have)
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

			batchSpecs = append(batchSpecs, c)
		}
	})

	if len(batchSpecs) != cap(batchSpecs) {
		t.Fatalf("batchSpecs is empty. creation failed")
	}

	t.Run("Count", func(t *testing.T) {
		count, err := s.CountBatchSpecs(ctx, CountBatchSpecsOpts{})
		if err != nil {
			t.Fatal(err)
		}

		if have, want := count, len(batchSpecs); have != want {
			t.Fatalf("have count: %d, want: %d", have, want)
		}

		t.Run("ExcludeCreatedFromRawNotOwnedByUser", func(t *testing.T) {
			for _, spec := range batchSpecs {
				spec.CreatedFromRaw = true
				if err := s.UpdateBatchSpec(ctx, spec); err != nil {
					t.Fatal(err)
				}

				count, err = s.CountBatchSpecs(ctx, CountBatchSpecsOpts{ExcludeCreatedFromRawNotOwnedByUser: spec.UserID})
				if err != nil {
					t.Fatal(err)
				}

				if have, want := count, 1; have != want {
					t.Fatalf("have count: %d, want: %d", have, want)
				}
			}
		})
	})

	t.Run("List", func(t *testing.T) {
		t.Run("NewestFirst", func(t *testing.T) {
			ts, _, err := s.ListBatchSpecs(ctx, ListBatchSpecsOpts{NewestFirst: true})
			if err != nil {
				t.Fatal(err)
			}

			have, want := ts, batchSpecs
			if len(have) != len(want) {
				t.Fatalf("listed %d batchSpecs, want: %d", len(have), len(want))
			}

			for i := 0; i < len(have); i++ {
				haveID, wantID := int(have[i].ID), len(have)-i
				if haveID != wantID {
					t.Fatalf("found batch specs out of order: have ID: %d, want: %d", haveID, wantID)
				}
			}
		})

		t.Run("OldestFirst", func(t *testing.T) {
			ts, _, err := s.ListBatchSpecs(ctx, ListBatchSpecsOpts{NewestFirst: false})
			if err != nil {
				t.Fatal(err)
			}

			have, want := ts, batchSpecs
			if len(have) != len(want) {
				t.Fatalf("listed %d batchSpecs, want: %d", len(have), len(want))
			}

			for i := 0; i < len(have); i++ {
				haveID, wantID := int(have[i].ID), i+1
				if haveID != wantID {
					t.Fatalf("found batch specs out of order: have ID: %d, want: %d", haveID, wantID)
				}
			}
		})

		t.Run("NewestFirstWithCursor", func(t *testing.T) {
			var cursor int64
			lastID := 99999
			for i := 1; i <= len(batchSpecs); i++ {
				opts := ListBatchSpecsOpts{Cursor: cursor, NewestFirst: true}
				ts, next, err := s.ListBatchSpecs(ctx, opts)
				if err != nil {
					t.Fatal(err)
				}

				haveID := int(ts[0].ID)
				if haveID > lastID {
					t.Fatalf("found batch specs out of order: expected descending but ID %d was before %d", lastID, haveID)
				}

				lastID = haveID
				cursor = next
			}
		})

		t.Run("OldestFirstWithCursor", func(t *testing.T) {
			var cursor int64
			var lastID int
			for i := 1; i <= len(batchSpecs); i++ {
				opts := ListBatchSpecsOpts{Cursor: cursor, NewestFirst: false}
				ts, next, err := s.ListBatchSpecs(ctx, opts)
				if err != nil {
					t.Fatal(err)
				}

				haveID := int(ts[0].ID)
				if haveID < lastID {
					t.Fatalf("found batch specs out of order: expected ascending but ID %d was before %d", lastID, haveID)
				}

				lastID = haveID
				cursor = next
			}
		})

		t.Run("NoLimit", func(t *testing.T) {
			// Empty should return all entries
			opts := ListBatchSpecsOpts{}

			ts, next, err := s.ListBatchSpecs(ctx, opts)
			if err != nil {
				t.Fatal(err)
			}

			if have, want := next, int64(0); have != want {
				t.Fatalf("opts: %+v: have next %v, want %v", opts, have, want)
			}

			have, want := ts, batchSpecs
			if len(have) != len(want) {
				t.Fatalf("listed %d batchSpecs, want: %d", len(have), len(want))
			}

			if diff := cmp.Diff(have, want); diff != "" {
				t.Fatalf("opts: %+v, diff: %s", opts, diff)
			}
		})

		t.Run("WithLimit", func(t *testing.T) {
			for i := 1; i <= len(batchSpecs); i++ {
				cs, next, err := s.ListBatchSpecs(ctx, ListBatchSpecsOpts{LimitOpts: LimitOpts{Limit: i}})
				if err != nil {
					t.Fatal(err)
				}

				{
					have, want := next, int64(0)
					if i < len(batchSpecs) {
						want = batchSpecs[i].ID
					}

					if have != want {
						t.Fatalf("limit: %v: have next %v, want %v", i, have, want)
					}
				}

				{
					have, want := cs, batchSpecs[:i]
					if len(have) != len(want) {
						t.Fatalf("listed %d batchSpecs, want: %d", len(have), len(want))
					}

					if diff := cmp.Diff(have, want); diff != "" {
						t.Fatal(diff)
					}
				}
			}

		})

		t.Run("WithLimitAndCursor", func(t *testing.T) {
			var cursor int64
			for i := 1; i <= len(batchSpecs); i++ {
				opts := ListBatchSpecsOpts{Cursor: cursor, LimitOpts: LimitOpts{Limit: 1}}
				have, next, err := s.ListBatchSpecs(ctx, opts)
				if err != nil {
					t.Fatal(err)
				}

				want := batchSpecs[i-1 : i]
				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", opts, diff)
				}

				cursor = next
			}
		})

		t.Run("ExcludeCreatedFromRawNotOwnedByUser", func(t *testing.T) {
			for _, spec := range batchSpecs {
				spec.CreatedFromRaw = true
				if err := s.UpdateBatchSpec(ctx, spec); err != nil {
					t.Fatal(err)
				}

				opts := ListBatchSpecsOpts{ExcludeCreatedFromRawNotOwnedByUser: spec.UserID}
				have, _, err := s.ListBatchSpecs(ctx, opts)
				if err != nil {
					t.Fatal(err)
				}

				want := []*btypes.BatchSpec{spec}
				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatalf("opts: %+v, diff: %s", opts, diff)
				}
			}
		})
	})

	t.Run("Update", func(t *testing.T) {
		for _, c := range batchSpecs {
			c.UserID += 1234
			c.CreatedFromRaw = false
			c.AllowUnsupported = false
			c.AllowIgnored = false

			clock.Add(1 * time.Second)

			want := c
			want.UpdatedAt = clock.Now()

			have := c.Clone()
			if err := s.UpdateBatchSpec(ctx, have); err != nil {
				t.Fatal(err)
			}

			if diff := cmp.Diff(have, want); diff != "" {
				t.Fatal(diff)
			}
		}
	})

	t.Run("Get", func(t *testing.T) {
		want := batchSpecs[1]
		tests := map[string]GetBatchSpecOpts{
			"ByID":          {ID: want.ID},
			"ByRandID":      {RandID: want.RandID},
			"ByIDAndRandID": {ID: want.ID, RandID: want.RandID},
		}

		for name, opts := range tests {
			t.Run(name, func(t *testing.T) {
				have, err := s.GetBatchSpec(ctx, opts)
				if err != nil {
					t.Fatal(err)
				}

				if diff := cmp.Diff(have, want); diff != "" {
					t.Fatal(diff)
				}
			})
		}

		t.Run("NoResults", func(t *testing.T) {
			opts := GetBatchSpecOpts{ID: 0xdeadbeef}

			_, have := s.GetBatchSpec(ctx, opts)
			want := ErrNoResults

			if have != want {
				t.Fatalf("have err %v, want %v", have, want)
			}
		})

		t.Run("ExcludeCreatedFromRawNotOwnedByUser", func(t *testing.T) {
			for _, spec := range batchSpecs {
				opts := GetBatchSpecOpts{ID: spec.ID, ExcludeCreatedFromRawNotOwnedByUser: spec.UserID}
				have, err := s.GetBatchSpec(ctx, opts)
				if err != nil {
					t.Fatal(err)
				}
				if diff := cmp.Diff(have, spec); diff != "" {
					t.Fatal(diff)
				}

				spec.CreatedFromRaw = true
				if err := s.UpdateBatchSpec(ctx, spec); err != nil {
					t.Fatal(err)
				}

				// Confirm that it won't be returned if another user looks at it
				opts.ExcludeCreatedFromRawNotOwnedByUser += 9999
				if _, err = s.GetBatchSpec(ctx, opts); err != ErrNoResults {
					t.Fatalf("have err %v, want %v", err, ErrNoResults)
				}

				spec.CreatedFromRaw = false
				if err := s.UpdateBatchSpec(ctx, spec); err != nil {
					t.Fatal(err)
				}

				if _, err = s.GetBatchSpec(ctx, opts); err == ErrNoResults {
					t.Fatalf("unexpected ErrNoResults")
				}
			}
		})
	})

	t.Run("GetNewestBatchSpec", func(t *testing.T) {
		t.Run("NotFound", func(t *testing.T) {
			opts := GetNewestBatchSpecOpts{
				NamespaceUserID: 1235,
				Name:            "Foobar",
				UserID:          1234567,
			}

			_, err := s.GetNewestBatchSpec(ctx, opts)
			if err != ErrNoResults {
				t.Errorf("unexpected error: have=%v want=%v", err, ErrNoResults)
			}
		})

		t.Run("NamespaceUser", func(t *testing.T) {
			opts := GetNewestBatchSpecOpts{
				NamespaceUserID: 1235,
				Name:            "Foobar",
				UserID:          1235 + 1234,
			}

			have, err := s.GetNewestBatchSpec(ctx, opts)
			if err != nil {
				t.Errorf("unexpected error: %v", err)
			}

			want := batchSpecs[1]
			if diff := cmp.Diff(have, want); diff != "" {
				t.Errorf("unexpected batch spec:\n%s", diff)
			}
		})

		t.Run("NamespaceOrg", func(t *testing.T) {
			opts := GetNewestBatchSpecOpts{
				NamespaceOrgID: 23,
				Name:           "Foobar",
				UserID:         1234 + 1234,
			}

			have, err := s.GetNewestBatchSpec(ctx, opts)
			if err != nil {
				t.Errorf("unexpected error: %v", err)
			}

			want := batchSpecs[0]
			if diff := cmp.Diff(have, want); diff != "" {
				t.Errorf("unexpected batch spec:\n%s", diff)
			}
		})
	})

	t.Run("Delete", func(t *testing.T) {
		for i := range batchSpecs {
			err := s.DeleteBatchSpec(ctx, batchSpecs[i].ID)
			if err != nil {
				t.Fatal(err)
			}

			count, err := s.CountBatchSpecs(ctx, CountBatchSpecsOpts{})
			if err != nil {
				t.Fatal(err)
			}

			if have, want := count, len(batchSpecs)-(i+1); have != want {
				t.Fatalf("have count: %d, want: %d", have, want)
			}
		}
	})

	t.Run("DeleteExpiredBatchSpecs", func(t *testing.T) {
		underTTL := clock.Now().Add(-btypes.BatchSpecTTL + 1*time.Minute)
		overTTL := clock.Now().Add(-btypes.BatchSpecTTL - 1*time.Minute)

		tests := []struct {
			createdAt         time.Time
			hasBatchChange    bool
			hasChangesetSpecs bool
			wantDeleted       bool
		}{
			{createdAt: underTTL, wantDeleted: false},
			{createdAt: overTTL, wantDeleted: true},

			{hasChangesetSpecs: true, createdAt: underTTL, wantDeleted: false},
			{hasChangesetSpecs: true, createdAt: overTTL, wantDeleted: false},

			{hasBatchChange: true, hasChangesetSpecs: true, createdAt: underTTL, wantDeleted: false},
			{hasBatchChange: true, hasChangesetSpecs: true, createdAt: overTTL, wantDeleted: false},

			{hasBatchChange: true, hasChangesetSpecs: true, createdAt: underTTL, wantDeleted: false},
			{hasBatchChange: true, hasChangesetSpecs: true, createdAt: overTTL, wantDeleted: false},
		}

		for i, tc := range tests {
			batchSpec := &btypes.BatchSpec{
				UserID:          1,
				NamespaceUserID: 1,
				CreatedAt:       tc.createdAt,
			}

			if err := s.CreateBatchSpec(ctx, batchSpec); err != nil {
				t.Fatal(err)
			}

			if tc.hasBatchChange {
				batchChange := &btypes.BatchChange{
					Name:            fmt.Sprintf("not-blank-%d", i),
					CreatorID:       1,
					NamespaceUserID: 1,
					BatchSpecID:     batchSpec.ID,
					LastApplierID:   1,
					LastAppliedAt:   time.Now(),
				}
				if err := s.CreateBatchChange(ctx, batchChange); err != nil {
					t.Fatal(err)
				}
			}

			if tc.hasChangesetSpecs {
				changesetSpec := &btypes.ChangesetSpec{
					RepoID:      1,
					BatchSpecID: batchSpec.ID,
				}
				if err := s.CreateChangesetSpec(ctx, changesetSpec); err != nil {
					t.Fatal(err)
				}
			}

			if err := s.DeleteExpiredBatchSpecs(ctx); err != nil {
				t.Fatal(err)
			}

			haveBatchSpecs, err := s.GetBatchSpec(ctx, GetBatchSpecOpts{ID: batchSpec.ID})
			if err != nil && err != ErrNoResults {
				t.Fatal(err)
			}

			if tc.wantDeleted && err == nil {
				t.Fatalf("tc=%+v\n\t want batch spec to be deleted. got: %v", tc, haveBatchSpecs)
			}

			if !tc.wantDeleted && err == ErrNoResults {
				t.Fatalf("tc=%+v\n\t want batch spec NOT to be deleted, but got deleted", tc)
			}
		}
	})
}

func TestStoreGetBatchSpecStats(t *testing.T) {
	ctx := context.Background()
	c := &ct.TestClock{Time: timeutil.Now()}
	minAgo := func(m int) time.Time { return c.Now().Add(-time.Duration(m) * time.Minute) }

	db := database.NewDB(dbtest.NewDB(t))
	s := NewWithClock(db, &observation.TestContext, nil, c.Now)

	repo, _ := ct.CreateTestRepo(t, ctx, db)

	admin := ct.CreateTestUser(t, db, true)

	var specIDs []int64
	for _, setup := range []struct {
		jobs                       []*btypes.BatchSpecWorkspaceExecutionJob
		additionalWorkspace        int
		additionalCachedWorkspace  int
		additionalSkippedWorkspace int
	}{
		{
			jobs: []*btypes.BatchSpecWorkspaceExecutionJob{
				{State: btypes.BatchSpecWorkspaceExecutionJobStateProcessing, StartedAt: minAgo(99)},
				{State: btypes.BatchSpecWorkspaceExecutionJobStateCompleted, StartedAt: minAgo(5), FinishedAt: minAgo(2)},
				{State: btypes.BatchSpecWorkspaceExecutionJobStateFailed, StartedAt: minAgo(5), FinishedAt: minAgo(2), Cancel: true},
				{State: btypes.BatchSpecWorkspaceExecutionJobStateProcessing, StartedAt: minAgo(10), Cancel: true},
				{State: btypes.BatchSpecWorkspaceExecutionJobStateQueued},
				{State: btypes.BatchSpecWorkspaceExecutionJobStateFailed, StartedAt: minAgo(5), FinishedAt: minAgo(1)},
			},
			additionalWorkspace:        1,
			additionalCachedWorkspace:  1,
			additionalSkippedWorkspace: 2,
		},
		{
			jobs: []*btypes.BatchSpecWorkspaceExecutionJob{
				{State: btypes.BatchSpecWorkspaceExecutionJobStateProcessing, StartedAt: minAgo(5)},
				{State: btypes.BatchSpecWorkspaceExecutionJobStateProcessing, StartedAt: minAgo(55)},
				{State: btypes.BatchSpecWorkspaceExecutionJobStateCompleted, StartedAt: minAgo(5), FinishedAt: minAgo(2)},
				{State: btypes.BatchSpecWorkspaceExecutionJobStateFailed, StartedAt: minAgo(5), FinishedAt: minAgo(2), Cancel: true},
				{State: btypes.BatchSpecWorkspaceExecutionJobStateProcessing, StartedAt: minAgo(10), Cancel: true},
				{State: btypes.BatchSpecWorkspaceExecutionJobStateProcessing, StartedAt: minAgo(10), Cancel: true},
				{State: btypes.BatchSpecWorkspaceExecutionJobStateQueued},
				{State: btypes.BatchSpecWorkspaceExecutionJobStateFailed, StartedAt: minAgo(5), FinishedAt: minAgo(1)},
			},
			additionalWorkspace:        3,
			additionalSkippedWorkspace: 2,
		},
		{
			jobs:                []*btypes.BatchSpecWorkspaceExecutionJob{},
			additionalWorkspace: 0,
		},
		{
			jobs: []*btypes.BatchSpecWorkspaceExecutionJob{
				{State: btypes.BatchSpecWorkspaceExecutionJobStateProcessing, StartedAt: minAgo(5)},
			},
			additionalWorkspace: 0,
		},
	} {
		spec := &btypes.BatchSpec{
			Spec:            &batcheslib.BatchSpec{},
			UserID:          admin.ID,
			NamespaceUserID: admin.ID,
		}
		if err := s.CreateBatchSpec(ctx, spec); err != nil {
			t.Fatal(err)
		}
		specIDs = append(specIDs, spec.ID)

		job := &btypes.BatchSpecResolutionJob{
			BatchSpecID: spec.ID,
			InitiatorID: admin.ID,
		}
		if err := s.CreateBatchSpecResolutionJob(ctx, job); err != nil {
			t.Fatal(err)
		}

		// Workspaces without execution job
		for i := 0; i < setup.additionalWorkspace; i++ {
			ws := &btypes.BatchSpecWorkspace{BatchSpecID: spec.ID, RepoID: repo.ID}
			if err := s.CreateBatchSpecWorkspace(ctx, ws); err != nil {
				t.Fatal(err)
			}
		}

		// Workspaces with cached result
		for i := 0; i < setup.additionalCachedWorkspace; i++ {
			ws := &btypes.BatchSpecWorkspace{BatchSpecID: spec.ID, RepoID: repo.ID, CachedResultFound: true}
			if err := s.CreateBatchSpecWorkspace(ctx, ws); err != nil {
				t.Fatal(err)
			}
		}

		// Workspaces without execution job and skipped
		if setup.additionalSkippedWorkspace > 0 {
			for i := 0; i < setup.additionalSkippedWorkspace; i++ {
				ws := &btypes.BatchSpecWorkspace{
					BatchSpecID: spec.ID,
					RepoID:      repo.ID,
					Skipped:     true,
				}
				if err := s.CreateBatchSpecWorkspace(ctx, ws); err != nil {
					t.Fatal(err)
				}
			}
		}

		// Workspaces with execution jobs
		for _, job := range setup.jobs {
			ws := &btypes.BatchSpecWorkspace{BatchSpecID: spec.ID, RepoID: repo.ID}
			if err := s.CreateBatchSpecWorkspace(ctx, ws); err != nil {
				t.Fatal(err)
			}

			// We use a clone so that CreateBatchSpecWorkspaceExecutionJob doesn't overwrite the fields we set

			clone := *job
			clone.BatchSpecWorkspaceID = ws.ID
			clone.UserID = spec.UserID
			if err := ct.CreateBatchSpecWorkspaceExecutionJob(ctx, s, ScanBatchSpecWorkspaceExecutionJob, &clone); err != nil {
				t.Fatal(err)
			}

			job.ID = clone.ID
			ct.UpdateJobState(t, ctx, s, job)
		}

	}
	have, err := s.GetBatchSpecStats(ctx, specIDs)
	if err != nil {
		t.Fatal(err)
	}

	want := map[int64]btypes.BatchSpecStats{
		specIDs[0]: {
			StartedAt:         minAgo(99),
			FinishedAt:        minAgo(1),
			Workspaces:        10,
			SkippedWorkspaces: 2,
			Executions:        6,
			Queued:            1,
			Processing:        1,
			Completed:         1,
			Canceling:         1,
			Canceled:          1,
			Failed:            1,
			CachedWorkspaces:  1,
		},
		specIDs[1]: {
			StartedAt:         minAgo(55),
			FinishedAt:        minAgo(1),
			Workspaces:        13,
			SkippedWorkspaces: 2,
			Executions:        8,
			Queued:            1,
			Processing:        2,
			Completed:         1,
			Canceling:         2,
			Canceled:          1,
			Failed:            1,
		},
		specIDs[2]: {
			StartedAt:  time.Time{},
			FinishedAt: time.Time{},
		},
		specIDs[3]: {
			StartedAt:  minAgo(5),
			FinishedAt: time.Time{},
			Workspaces: 1,
			Executions: 1,
			Processing: 1,
		},
	}
	if diff := cmp.Diff(have, want); diff != "" {
		t.Errorf("unexpected batch spec stats:\n%s", diff)
	}
}
