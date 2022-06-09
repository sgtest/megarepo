package migration

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/internal/codeintel/stores/dbstore"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func TestCommittedAtMigrator(t *testing.T) {
	db := database.NewDB(dbtest.NewDB(t))
	store := dbstore.NewWithDB(db, &observation.TestContext)
	gitserverClient := NewMockGitserverClient()
	migrator := NewCommittedAtMigrator(store, gitserverClient, 250)

	n := 500
	t0 := time.Unix(1587396557, 0).UTC()
	expectedCommitDates := make([]time.Time, 0, n)
	for i := 0; i < n; i++ {
		expectedCommitDates = append(expectedCommitDates, t0.Add(time.Second*time.Duration(i)))
	}

	gitserverClient.CommitDateFunc.SetDefaultHook(func(ctx context.Context, repositoryID int, commit string) (string, time.Time, bool, error) {
		if i := len(gitserverClient.CommitDateFunc.History()); i < n {
			return commit, expectedCommitDates[i], true, nil
		}

		return "", time.Time{}, false, errors.Errorf("too many calls")
	})

	assertProgress := func(expectedProgress float64) {
		if progress, err := migrator.Progress(context.Background()); err != nil {
			t.Fatalf("unexpected error querying progress: %s", err)
		} else if progress != expectedProgress {
			t.Errorf("unexpected progress. want=%.2f have=%.2f", expectedProgress, progress)
		}
	}

	assertDirty := func(expectedDirty []int) {
		query := sqlf.Sprintf(`SELECT repository_id FROM lsif_dirty_repositories WHERE dirty_token != update_token ORDER BY repository_id`)

		if dirty, err := basestore.ScanInts(store.Query(context.Background(), query)); err != nil {
			t.Fatalf("unexpected error querying num diagnostics: %s", err)
		} else if diff := cmp.Diff(expectedDirty, dirty); diff != "" {
			t.Errorf("unexpected counts (-want +got):\n%s", diff)
		}
	}

	assertCommitDates := func(expectedCommitDates []time.Time) {
		query := sqlf.Sprintf(`SELECT committed_at FROM lsif_uploads WHERE committed_at IS NOT NULL AND committed_at != '-infinity' ORDER BY committed_at`)

		if commitDates, err := basestore.ScanTimes(store.Query(context.Background(), query)); err != nil {
			t.Fatalf("unexpected error querying uploads: %s", err)
		} else if diff := cmp.Diff(expectedCommitDates, commitDates); diff != "" {
			t.Errorf("unexpected commit dates (-want +got):\n%s", diff)
		}
	}

	if err := store.Exec(context.Background(), sqlf.Sprintf("INSERT INTO repo (id, name) VALUES (42, 'foo'), (43, 'bar')")); err != nil {
		t.Fatalf("unexpected error inserting repo: %s", err)
	}

	for i := 0; i < n; i++ {
		if err := store.Exec(context.Background(), sqlf.Sprintf(
			"INSERT INTO lsif_uploads (repository_id, commit, state, indexer, num_parts, uploaded_parts) VALUES (%s, %s, 'completed', 'lsif-go', 0, '{}')",
			42+i/(n/2), // 50% id=42, 50% id=43
			fmt.Sprintf("%040d", i),
		)); err != nil {
			t.Fatalf("unexpected error inserting upload: %s", err)
		}
	}

	assertProgress(0)

	if err := migrator.Up(context.Background()); err != nil {
		t.Fatalf("unexpected error performing up migration: %s", err)
	}
	assertProgress(0.5)
	assertDirty([]int{42})
	assertCommitDates(expectedCommitDates[:n/2])

	if err := migrator.Up(context.Background()); err != nil {
		t.Fatalf("unexpected error performing up migration: %s", err)
	}
	assertProgress(1)
	assertDirty([]int{42, 43})
	assertCommitDates(expectedCommitDates)

	if err := migrator.Down(context.Background()); err != nil {
		t.Fatalf("unexpected error performing down migration: %s", err)
	}
	assertProgress(0.5)

	if err := migrator.Down(context.Background()); err != nil {
		t.Fatalf("unexpected error performing down migration: %s", err)
	}
	assertProgress(0)
}

func TestCommittedAtMigratorUnknownRepository(t *testing.T) {
	db := database.NewDB(dbtest.NewDB(t))
	store := dbstore.NewWithDB(db, &observation.TestContext)
	gitserverClient := NewMockGitserverClient()
	migrator := NewCommittedAtMigrator(store, gitserverClient, 250)

	n := 500
	t0 := time.Unix(1587396557, 0).UTC()
	allDates := make([]time.Time, 0, n)
	expectedCommitDates := make([]time.Time, 0, n)
	for i := 0; i < n; i++ {
		date := t0.Add(time.Second * time.Duration(i))
		allDates = append(allDates, date)

		if i%3 != 0 {
			expectedCommitDates = append(expectedCommitDates, date)
		}
	}

	gitserverClient.CommitDateFunc.SetDefaultHook(func(ctx context.Context, repositoryID int, commit string) (string, time.Time, bool, error) {
		if i := len(gitserverClient.CommitDateFunc.History()); i < n {
			if i%3 == 0 {
				return "", time.Time{}, false, &gitdomain.RepoNotExistError{}
			}

			return commit, allDates[i], true, nil
		}

		return "", time.Time{}, false, errors.Errorf("too many calls")
	})

	assertProgress := func(expectedProgress float64) {
		if progress, err := migrator.Progress(context.Background()); err != nil {
			t.Fatalf("unexpected error querying progress: %s", err)
		} else if progress != expectedProgress {
			t.Errorf("unexpected progress. want=%.2f have=%.2f", expectedProgress, progress)
		}
	}

	assertDirty := func(expectedDirty []int) {
		query := sqlf.Sprintf(`SELECT repository_id FROM lsif_dirty_repositories WHERE dirty_token != update_token ORDER BY repository_id`)

		if dirty, err := basestore.ScanInts(store.Query(context.Background(), query)); err != nil {
			t.Fatalf("unexpected error querying num diagnostics: %s", err)
		} else if diff := cmp.Diff(expectedDirty, dirty); diff != "" {
			t.Errorf("unexpected counts (-want +got):\n%s", diff)
		}
	}

	assertCommitDates := func(expectedCommitDates []time.Time) {
		query := sqlf.Sprintf(`SELECT committed_at FROM lsif_uploads WHERE committed_at IS NOT NULL AND committed_at != '-infinity' ORDER BY committed_at`)

		if commitDates, err := basestore.ScanTimes(store.Query(context.Background(), query)); err != nil {
			t.Fatalf("unexpected error querying uploads: %s", err)
		} else if diff := cmp.Diff(expectedCommitDates, commitDates); diff != "" {
			t.Errorf("unexpected commit dates (-want +got):\n%s", diff)
		}
	}

	if err := store.Exec(context.Background(), sqlf.Sprintf("INSERT INTO repo (id, name) VALUES (42, 'foo'), (43, 'bar')")); err != nil {
		t.Fatalf("unexpected error inserting repo: %s", err)
	}

	for i := 0; i < n; i++ {
		if err := store.Exec(context.Background(), sqlf.Sprintf(
			"INSERT INTO lsif_uploads (repository_id, commit, state, indexer, num_parts, uploaded_parts) VALUES (%s, %s, 'completed', 'lsif-go', 0, '{}')",
			42+i/(n/2), // 50% id=42, 50% id=43
			fmt.Sprintf("%040d", i),
		)); err != nil {
			t.Fatalf("unexpected error inserting upload: %s", err)
		}
	}

	assertProgress(0)

	if err := migrator.Up(context.Background()); err != nil {
		t.Fatalf("unexpected error performing up migration: %s", err)
	}
	assertProgress(0.5)
	assertDirty([]int{42})
	assertCommitDates(expectedCommitDates[:n/3]) // (2/3*n)/2 = n/3

	if err := migrator.Up(context.Background()); err != nil {
		t.Fatalf("unexpected error performing up migration: %s", err)
	}
	assertProgress(1)
	assertDirty([]int{42, 43})
	assertCommitDates(expectedCommitDates)

	if err := migrator.Down(context.Background()); err != nil {
		t.Fatalf("unexpected error performing down migration: %s", err)
	}
	assertProgress(0.5)

	if err := migrator.Down(context.Background()); err != nil {
		t.Fatalf("unexpected error performing down migration: %s", err)
	}
	assertProgress(0)
}

func TestCommittedAtMigratorUnknownCommits(t *testing.T) {
	db := database.NewDB(dbtest.NewDB(t))
	store := dbstore.NewWithDB(db, &observation.TestContext)
	gitserverClient := NewMockGitserverClient()
	migrator := NewCommittedAtMigrator(store, gitserverClient, 250)

	n := 500
	t0 := time.Unix(1587396557, 0).UTC()
	allDates := make([]time.Time, 0, n)
	expectedCommitDates := make([]time.Time, 0, n)
	for i := 0; i < n; i++ {
		date := t0.Add(time.Second * time.Duration(i))
		allDates = append(allDates, date)

		if i%3 != 0 {
			expectedCommitDates = append(expectedCommitDates, date)
		}
	}

	gitserverClient.CommitDateFunc.SetDefaultHook(func(ctx context.Context, repositoryID int, commit string) (string, time.Time, bool, error) {
		if i := len(gitserverClient.CommitDateFunc.History()); i < n {
			if i%3 == 0 {
				return "", time.Time{}, false, nil
			}

			return commit, allDates[i], true, nil
		}

		return "", time.Time{}, false, errors.Errorf("too many calls")
	})

	assertProgress := func(expectedProgress float64) {
		if progress, err := migrator.Progress(context.Background()); err != nil {
			t.Fatalf("unexpected error querying progress: %s", err)
		} else if progress != expectedProgress {
			t.Errorf("unexpected progress. want=%.2f have=%.2f", expectedProgress, progress)
		}
	}

	assertDirty := func(expectedDirty []int) {
		query := sqlf.Sprintf(`SELECT repository_id FROM lsif_dirty_repositories WHERE dirty_token != update_token ORDER BY repository_id`)

		if dirty, err := basestore.ScanInts(store.Query(context.Background(), query)); err != nil {
			t.Fatalf("unexpected error querying num diagnostics: %s", err)
		} else if diff := cmp.Diff(expectedDirty, dirty); diff != "" {
			t.Errorf("unexpected counts (-want +got):\n%s", diff)
		}
	}

	assertCommitDates := func(expectedCommitDates []time.Time) {
		query := sqlf.Sprintf(`SELECT committed_at FROM lsif_uploads WHERE committed_at IS NOT NULL AND committed_at != '-infinity' ORDER BY committed_at`)

		if commitDates, err := basestore.ScanTimes(store.Query(context.Background(), query)); err != nil {
			t.Fatalf("unexpected error querying uploads: %s", err)
		} else if diff := cmp.Diff(expectedCommitDates, commitDates); diff != "" {
			t.Errorf("unexpected commit dates (-want +got):\n%s", diff)
		}
	}

	if err := store.Exec(context.Background(), sqlf.Sprintf("INSERT INTO repo (id, name) VALUES (42, 'foo'), (43, 'bar')")); err != nil {
		t.Fatalf("unexpected error inserting repo: %s", err)
	}

	for i := 0; i < n; i++ {
		if err := store.Exec(context.Background(), sqlf.Sprintf(
			"INSERT INTO lsif_uploads (repository_id, commit, state, indexer, num_parts, uploaded_parts) VALUES (%s, %s, 'completed', 'lsif-go', 0, '{}')",
			42+i/(n/2), // 50% id=42, 50% id=43
			fmt.Sprintf("%040d", i),
		)); err != nil {
			t.Fatalf("unexpected error inserting upload: %s", err)
		}
	}

	assertProgress(0)

	if err := migrator.Up(context.Background()); err != nil {
		t.Fatalf("unexpected error performing up migration: %s", err)
	}
	assertProgress(0.5)
	assertCommitDates(expectedCommitDates[:n/3]) // (2/3*n)/2 = n/3

	if err := migrator.Up(context.Background()); err != nil {
		t.Fatalf("unexpected error performing up migration: %s", err)
	}
	assertProgress(1)
	assertCommitDates(expectedCommitDates)
	assertDirty([]int{42, 43})

	if err := migrator.Down(context.Background()); err != nil {
		t.Fatalf("unexpected error performing down migration: %s", err)
	}
	assertProgress(0.5)

	if err := migrator.Down(context.Background()); err != nil {
		t.Fatalf("unexpected error performing down migration: %s", err)
	}
	assertProgress(0)
}
