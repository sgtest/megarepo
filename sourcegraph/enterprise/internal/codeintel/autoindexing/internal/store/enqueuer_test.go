package store

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/log/logtest"

	uploadsshared "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/shared"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/executor"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func TestIsQueued(t *testing.T) {
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	store := New(&observation.TestContext, db)

	insertIndexes(t, db, uploadsshared.Index{ID: 1, RepositoryID: 1, Commit: makeCommit(1)})
	insertIndexes(t, db, uploadsshared.Index{ID: 2, RepositoryID: 1, Commit: makeCommit(1), ShouldReindex: true})
	insertIndexes(t, db, uploadsshared.Index{ID: 3, RepositoryID: 4, Commit: makeCommit(1), ShouldReindex: true})
	insertIndexes(t, db, uploadsshared.Index{ID: 4, RepositoryID: 5, Commit: makeCommit(4), ShouldReindex: true})
	insertUploads(t, db, upload{ID: 2, RepositoryID: 2, Commit: makeCommit(2)})
	insertUploads(t, db, upload{ID: 3, RepositoryID: 3, Commit: makeCommit(3), State: "deleted"})
	insertUploads(t, db, upload{ID: 4, RepositoryID: 5, Commit: makeCommit(4), ShouldReindex: true})

	testCases := []struct {
		repositoryID int
		commit       string
		expected     bool
	}{
		{1, makeCommit(1), true},
		{1, makeCommit(2), false},
		{2, makeCommit(1), false},
		{2, makeCommit(2), true},
		{3, makeCommit(1), false},
		{3, makeCommit(2), false},
		{3, makeCommit(3), false},
		{4, makeCommit(1), false},
		{5, makeCommit(4), false},
	}

	for _, testCase := range testCases {
		name := fmt.Sprintf("repositoryId=%d commit=%s", testCase.repositoryID, testCase.commit)

		t.Run(name, func(t *testing.T) {
			queued, err := store.IsQueued(context.Background(), testCase.repositoryID, testCase.commit)
			if err != nil {
				t.Fatalf("unexpected error checking if commit is queued: %s", err)
			}
			if queued != testCase.expected {
				t.Errorf("unexpected state. repo=%v commit=%v want=%v have=%v", testCase.repositoryID, testCase.commit, testCase.expected, queued)
			}
		})
	}
}

func TestIsQueuedRootIndexer(t *testing.T) {
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	store := New(&observation.TestContext, db)

	now := time.Now()
	insertIndexes(t, db, uploadsshared.Index{ID: 1, RepositoryID: 1, Commit: makeCommit(1), Root: "/foo", Indexer: "i1", QueuedAt: now.Add(-time.Hour * 1)})
	insertIndexes(t, db, uploadsshared.Index{ID: 2, RepositoryID: 1, Commit: makeCommit(1), Root: "/foo", Indexer: "i1", QueuedAt: now.Add(-time.Hour * 2)})
	insertIndexes(t, db, uploadsshared.Index{ID: 3, RepositoryID: 2, Commit: makeCommit(2), Root: "/foo", Indexer: "i1", QueuedAt: now.Add(-time.Hour * 1), ShouldReindex: true})
	insertIndexes(t, db, uploadsshared.Index{ID: 4, RepositoryID: 2, Commit: makeCommit(2), Root: "/foo", Indexer: "i1", QueuedAt: now.Add(-time.Hour * 2)})
	insertIndexes(t, db, uploadsshared.Index{ID: 5, RepositoryID: 3, Commit: makeCommit(3), Root: "/foo", Indexer: "i1", QueuedAt: now.Add(-time.Hour * 1)})
	insertIndexes(t, db, uploadsshared.Index{ID: 6, RepositoryID: 3, Commit: makeCommit(3), Root: "/foo", Indexer: "i1", QueuedAt: now.Add(-time.Hour * 2), ShouldReindex: true})

	testCases := []struct {
		repositoryID int
		commit       string
		root         string
		indexer      string
		expected     bool
	}{
		{1, makeCommit(1), "/foo", "i1", true},
		{1, makeCommit(1), "/bar", "i1", false}, // no index for root
		{2, makeCommit(2), "/foo", "i1", false}, // reindex (live)
		{3, makeCommit(3), "/foo", "i1", true},  // reindex (done)
	}

	for _, testCase := range testCases {
		name := fmt.Sprintf("repositoryId=%d commit=%s", testCase.repositoryID, testCase.commit)

		t.Run(name, func(t *testing.T) {
			queued, err := store.IsQueuedRootIndexer(context.Background(), testCase.repositoryID, testCase.commit, testCase.root, testCase.indexer)
			if err != nil {
				t.Fatalf("unexpected error checking if commit/root/indexer is queued: %s", err)
			}
			if queued != testCase.expected {
				t.Errorf("unexpected state. repo=%v commit=%v root=%v indexer=%v want=%v have=%v", testCase.repositoryID, testCase.commit, testCase.root, testCase.indexer, testCase.expected, queued)
			}
		})
	}
}

func TestInsertIndexes(t *testing.T) {
	ctx := context.Background()
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	store := New(&observation.TestContext, db)

	insertRepo(t, db, 50, "")

	indexes, err := store.InsertIndexes(ctx, []uploadsshared.Index{
		{
			State:        "queued",
			Commit:       makeCommit(1),
			RepositoryID: 50,
			DockerSteps: []uploadsshared.DockerStep{
				{
					Image:    "cimg/node:12.16",
					Commands: []string{"yarn install --frozen-lockfile --no-progress"},
				},
			},
			LocalSteps:  []string{"echo hello"},
			Root:        "/foo/bar",
			Indexer:     "sourcegraph/scip-typescript:latest",
			IndexerArgs: []string{"index", "--yarn-workspaces"},
			Outfile:     "dump.lsif",
			ExecutionLogs: []executor.ExecutionLogEntry{
				{Command: []string{"op", "1"}, Out: "Indexing\nUploading\nDone with 1.\n"},
				{Command: []string{"op", "2"}, Out: "Indexing\nUploading\nDone with 2.\n"},
			},
		},
		{
			State:        "queued",
			Commit:       makeCommit(2),
			RepositoryID: 50,
			DockerSteps: []uploadsshared.DockerStep{
				{
					Image:    "cimg/rust:nightly",
					Commands: []string{"cargo install"},
				},
			},
			LocalSteps:  nil,
			Root:        "/baz",
			Indexer:     "sourcegraph/lsif-rust:15",
			IndexerArgs: []string{"-v"},
			Outfile:     "dump.lsif",
			ExecutionLogs: []executor.ExecutionLogEntry{
				{Command: []string{"op", "1"}, Out: "Done with 1.\n"},
				{Command: []string{"op", "2"}, Out: "Done with 2.\n"},
			},
		},
	})
	if err != nil {
		t.Fatalf("unexpected error enqueueing index: %s", err)
	}
	if len(indexes) == 0 {
		t.Fatalf("expected records to be inserted")
	}

	rank1 := 1
	rank2 := 2
	expected := []uploadsshared.Index{
		{
			ID:             1,
			Commit:         makeCommit(1),
			QueuedAt:       time.Time{},
			State:          "queued",
			FailureMessage: nil,
			StartedAt:      nil,
			FinishedAt:     nil,
			RepositoryID:   50,
			RepositoryName: "n-50",
			DockerSteps: []uploadsshared.DockerStep{
				{
					Image:    "cimg/node:12.16",
					Commands: []string{"yarn install --frozen-lockfile --no-progress"},
				},
			},
			LocalSteps:  []string{"echo hello"},
			Root:        "/foo/bar",
			Indexer:     "sourcegraph/scip-typescript:latest",
			IndexerArgs: []string{"index", "--yarn-workspaces"},
			Outfile:     "dump.lsif",
			ExecutionLogs: []executor.ExecutionLogEntry{
				{Command: []string{"op", "1"}, Out: "Indexing\nUploading\nDone with 1.\n"},
				{Command: []string{"op", "2"}, Out: "Indexing\nUploading\nDone with 2.\n"},
			},
			Rank: &rank1,
		},
		{
			ID:             2,
			Commit:         makeCommit(2),
			QueuedAt:       time.Time{},
			State:          "queued",
			FailureMessage: nil,
			StartedAt:      nil,
			FinishedAt:     nil,
			RepositoryID:   50,
			RepositoryName: "n-50",
			DockerSteps: []uploadsshared.DockerStep{
				{
					Image:    "cimg/rust:nightly",
					Commands: []string{"cargo install"},
				},
			},
			LocalSteps:  []string{},
			Root:        "/baz",
			Indexer:     "sourcegraph/lsif-rust:15",
			IndexerArgs: []string{"-v"},
			Outfile:     "dump.lsif",
			ExecutionLogs: []executor.ExecutionLogEntry{
				{Command: []string{"op", "1"}, Out: "Done with 1.\n"},
				{Command: []string{"op", "2"}, Out: "Done with 2.\n"},
			},
			Rank: &rank2,
		},
	}

	for i := range expected {
		// Update auto-generated timestamp
		expected[i].QueuedAt = indexes[0].QueuedAt
	}

	if diff := cmp.Diff(expected, indexes); diff != "" {
		t.Errorf("unexpected indexes (-want +got):\n%s", diff)
	}
}
