package dbstore

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/internal/database/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtesting"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
)

func TestGetIndexByID(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	store := testStore()

	// Index does not exist initially
	if _, exists, err := store.GetIndexByID(context.Background(), 1); err != nil {
		t.Fatalf("unexpected error getting index: %s", err)
	} else if exists {
		t.Fatal("unexpected record")
	}

	queuedAt := time.Unix(1587396557, 0).UTC()
	startedAt := queuedAt.Add(time.Minute)
	expected := Index{
		ID:             1,
		Commit:         makeCommit(1),
		QueuedAt:       queuedAt,
		State:          "processing",
		FailureMessage: nil,
		StartedAt:      &startedAt,
		FinishedAt:     nil,
		RepositoryID:   123,
		RepositoryName: "n-123",
		DockerSteps: []DockerStep{
			{
				Image:    "cimg/node:12.16",
				Commands: []string{"yarn install --frozen-lockfile --no-progress"},
			},
		},
		LocalSteps:  []string{"echo hello"},
		Root:        "/foo/bar",
		Indexer:     "sourcegraph/lsif-tsc:latest",
		IndexerArgs: []string{"lib/**/*.js", "test/**/*.js", "--allowJs", "--checkJs"},
		Outfile:     "dump.lsif",
		ExecutionLogs: []workerutil.ExecutionLogEntry{
			{Command: []string{"op", "1"}, Out: "Indexing\nUploading\nDone with 1.\n"},
			{Command: []string{"op", "2"}, Out: "Indexing\nUploading\nDone with 2.\n"},
		},
		Rank: nil,
	}

	insertIndexes(t, dbconn.Global, expected)

	if index, exists, err := store.GetIndexByID(context.Background(), 1); err != nil {
		t.Fatalf("unexpected error getting index: %s", err)
	} else if !exists {
		t.Fatal("expected record to exist")
	} else if diff := cmp.Diff(expected, index); diff != "" {
		t.Errorf("unexpected index (-want +got):\n%s", diff)
	}
}

func TestGetQueuedIndexRank(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	store := testStore()

	t1 := time.Unix(1587396557, 0).UTC()
	t2 := t1.Add(+time.Minute * 6)
	t3 := t1.Add(+time.Minute * 3)
	t4 := t1.Add(+time.Minute * 1)
	t5 := t1.Add(+time.Minute * 4)
	t6 := t1.Add(+time.Minute * 2)
	t7 := t1.Add(+time.Minute * 5)

	insertIndexes(t, dbconn.Global,
		Index{ID: 1, QueuedAt: t1, State: "queued"},
		Index{ID: 2, QueuedAt: t2, State: "queued"},
		Index{ID: 3, QueuedAt: t3, State: "queued"},
		Index{ID: 4, QueuedAt: t4, State: "queued"},
		Index{ID: 5, QueuedAt: t5, State: "queued"},
		Index{ID: 6, QueuedAt: t6, State: "processing"},
		Index{ID: 7, QueuedAt: t1, State: "queued", ProcessAfter: &t7},
	)

	if index, _, _ := store.GetIndexByID(context.Background(), 1); index.Rank == nil || *index.Rank != 1 {
		t.Errorf("unexpected rank. want=%d have=%s", 1, printableRank{index.Rank})
	}
	if index, _, _ := store.GetIndexByID(context.Background(), 2); index.Rank == nil || *index.Rank != 6 {
		t.Errorf("unexpected rank. want=%d have=%s", 5, printableRank{index.Rank})
	}
	if index, _, _ := store.GetIndexByID(context.Background(), 3); index.Rank == nil || *index.Rank != 3 {
		t.Errorf("unexpected rank. want=%d have=%s", 3, printableRank{index.Rank})
	}
	if index, _, _ := store.GetIndexByID(context.Background(), 4); index.Rank == nil || *index.Rank != 2 {
		t.Errorf("unexpected rank. want=%d have=%s", 2, printableRank{index.Rank})
	}
	if index, _, _ := store.GetIndexByID(context.Background(), 5); index.Rank == nil || *index.Rank != 4 {
		t.Errorf("unexpected rank. want=%d have=%s", 4, printableRank{index.Rank})
	}

	// Only considers queued indexes to determine rank
	if index, _, _ := store.GetIndexByID(context.Background(), 6); index.Rank != nil {
		t.Errorf("unexpected rank. want=%s have=%s", "nil", printableRank{index.Rank})
	}

	// Process after takes priority over upload time
	if upload, _, _ := store.GetIndexByID(context.Background(), 7); upload.Rank == nil || *upload.Rank != 5 {
		t.Errorf("unexpected rank. want=%d have=%s", 4, printableRank{upload.Rank})
	}
}

func TestGetIndexes(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	store := testStore()

	t1 := time.Unix(1587396557, 0).UTC()
	t2 := t1.Add(-time.Minute * 1)
	t3 := t1.Add(-time.Minute * 2)
	t4 := t1.Add(-time.Minute * 3)
	t5 := t1.Add(-time.Minute * 4)
	t6 := t1.Add(-time.Minute * 5)
	t7 := t1.Add(-time.Minute * 6)
	t8 := t1.Add(-time.Minute * 7)
	t9 := t1.Add(-time.Minute * 8)
	t10 := t1.Add(-time.Minute * 9)
	failureMessage := "unlucky 333"

	insertIndexes(t, dbconn.Global,
		Index{ID: 1, Commit: makeCommit(3331), QueuedAt: t1, State: "queued"},
		Index{ID: 2, QueuedAt: t2, State: "errored", FailureMessage: &failureMessage},
		Index{ID: 3, Commit: makeCommit(3333), QueuedAt: t3, State: "queued"},
		Index{ID: 4, QueuedAt: t4, State: "queued", RepositoryID: 51, RepositoryName: "foo bar x"},
		Index{ID: 5, Commit: makeCommit(3333), QueuedAt: t5, State: "processing"},
		Index{ID: 6, QueuedAt: t6, State: "processing", RepositoryID: 52, RepositoryName: "foo bar y"},
		Index{ID: 7, QueuedAt: t7},
		Index{ID: 8, QueuedAt: t8},
		Index{ID: 9, QueuedAt: t9, State: "queued"},
		Index{ID: 10, QueuedAt: t10},
	)

	testCases := []struct {
		repositoryID int
		state        string
		term         string
		expectedIDs  []int
	}{
		{expectedIDs: []int{1, 2, 3, 4, 5, 6, 7, 8, 9, 10}},
		{repositoryID: 50, expectedIDs: []int{1, 2, 3, 5, 7, 8, 9, 10}},
		{state: "completed", expectedIDs: []int{7, 8, 10}},
		{term: "003", expectedIDs: []int{1, 3, 5}},       // searches commits
		{term: "333", expectedIDs: []int{1, 2, 3, 5}},    // searches commits and failure message
		{term: "QuEuEd", expectedIDs: []int{1, 3, 4, 9}}, // searches text status
		{term: "bAr", expectedIDs: []int{4, 6}},          // search repo names
	}

	for _, testCase := range testCases {
		for lo := 0; lo < len(testCase.expectedIDs); lo++ {
			hi := lo + 3
			if hi > len(testCase.expectedIDs) {
				hi = len(testCase.expectedIDs)
			}

			name := fmt.Sprintf(
				"repositoryID=%d state=%s term=%s offset=%d",
				testCase.repositoryID,
				testCase.state,
				testCase.term,
				lo,
			)

			t.Run(name, func(t *testing.T) {
				indexes, totalCount, err := store.GetIndexes(context.Background(), GetIndexesOptions{
					RepositoryID: testCase.repositoryID,
					State:        testCase.state,
					Term:         testCase.term,
					Limit:        3,
					Offset:       lo,
				})
				if err != nil {
					t.Fatalf("unexpected error getting indexes for repo: %s", err)
				}
				if totalCount != len(testCase.expectedIDs) {
					t.Errorf("unexpected total count. want=%d have=%d", len(testCase.expectedIDs), totalCount)
				}

				var ids []int
				for _, index := range indexes {
					ids = append(ids, index.ID)
				}

				if diff := cmp.Diff(testCase.expectedIDs[lo:hi], ids); diff != "" {
					t.Errorf("unexpected index ids at offset %d (-want +got):\n%s", lo, diff)
				}
			})
		}
	}
}

func TestIsQueued(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	store := testStore()

	insertIndexes(t, dbconn.Global, Index{ID: 1, RepositoryID: 1, Commit: makeCommit(1)})
	insertUploads(t, dbconn.Global, Upload{ID: 2, RepositoryID: 2, Commit: makeCommit(2)})
	insertUploads(t, dbconn.Global, Upload{ID: 3, RepositoryID: 3, Commit: makeCommit(3), State: "deleted"})

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
	}

	for _, testCase := range testCases {
		name := fmt.Sprintf("repositoryId=%d commit=%s", testCase.repositoryID, testCase.commit)

		t.Run(name, func(t *testing.T) {
			queued, err := store.IsQueued(context.Background(), testCase.repositoryID, testCase.commit)
			if err != nil {
				t.Fatalf("unexpected error checking if commit is queued: %s", err)
			}
			if queued != testCase.expected {
				t.Errorf("unexpected state. want=%v have=%v", testCase.expected, queued)
			}
		})
	}
}

func TestInsertIndex(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	store := testStore()

	insertRepo(t, dbconn.Global, 50, "")

	id, err := store.InsertIndex(context.Background(), Index{
		State:        "queued",
		Commit:       makeCommit(1),
		RepositoryID: 50,
		DockerSteps: []DockerStep{
			{
				Image:    "cimg/node:12.16",
				Commands: []string{"yarn install --frozen-lockfile --no-progress"},
			},
		},
		LocalSteps:  []string{"echo hello"},
		Root:        "/foo/bar",
		Indexer:     "sourcegraph/lsif-tsc:latest",
		IndexerArgs: []string{"lib/**/*.js", "test/**/*.js", "--allowJs", "--checkJs"},
		Outfile:     "dump.lsif",
		ExecutionLogs: []workerutil.ExecutionLogEntry{
			{Command: []string{"op", "1"}, Out: "Indexing\nUploading\nDone with 1.\n"},
			{Command: []string{"op", "2"}, Out: "Indexing\nUploading\nDone with 2.\n"},
		},
	})
	if err != nil {
		t.Fatalf("unexpected error enqueueing index: %s", err)
	}

	rank := 1
	expected := Index{
		ID:             id,
		Commit:         makeCommit(1),
		QueuedAt:       time.Time{},
		State:          "queued",
		FailureMessage: nil,
		StartedAt:      nil,
		FinishedAt:     nil,
		RepositoryID:   50,
		RepositoryName: "n-50",
		DockerSteps: []DockerStep{
			{
				Image:    "cimg/node:12.16",
				Commands: []string{"yarn install --frozen-lockfile --no-progress"},
			},
		},
		LocalSteps:  []string{"echo hello"},
		Root:        "/foo/bar",
		Indexer:     "sourcegraph/lsif-tsc:latest",
		IndexerArgs: []string{"lib/**/*.js", "test/**/*.js", "--allowJs", "--checkJs"},
		Outfile:     "dump.lsif",
		ExecutionLogs: []workerutil.ExecutionLogEntry{
			{Command: []string{"op", "1"}, Out: "Indexing\nUploading\nDone with 1.\n"},
			{Command: []string{"op", "2"}, Out: "Indexing\nUploading\nDone with 2.\n"},
		},
		Rank: &rank,
	}

	if index, exists, err := store.GetIndexByID(context.Background(), id); err != nil {
		t.Fatalf("unexpected error getting index: %s", err)
	} else if !exists {
		t.Fatal("expected record to exist")
	} else {
		// Update auto-generated timestamp
		expected.QueuedAt = index.QueuedAt

		if diff := cmp.Diff(expected, index); diff != "" {
			t.Errorf("unexpected index (-want +got):\n%s", diff)
		}
	}
}

func TestDeleteIndexByID(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	store := testStore()

	insertIndexes(t, dbconn.Global,
		Index{ID: 1},
	)

	if found, err := store.DeleteIndexByID(context.Background(), 1); err != nil {
		t.Fatalf("unexpected error deleting index: %s", err)
	} else if !found {
		t.Fatalf("expected record to exist")
	}

	// Index no longer exists
	if _, exists, err := store.GetIndexByID(context.Background(), 1); err != nil {
		t.Fatalf("unexpected error getting index: %s", err)
	} else if exists {
		t.Fatal("unexpected record")
	}
}

func TestDeleteIndexByIDMissingRow(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	store := testStore()

	if found, err := store.DeleteIndexByID(context.Background(), 1); err != nil {
		t.Fatalf("unexpected error deleting index: %s", err)
	} else if found {
		t.Fatalf("unexpected record")
	}
}

func TestDeleteIndexesWithoutRepository(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	store := testStore()

	var indexes []Index
	for i := 0; i < 25; i++ {
		for j := 0; j < 10+i; j++ {
			indexes = append(indexes, Index{ID: len(indexes) + 1, RepositoryID: 50 + i})
		}
	}
	insertIndexes(t, dbconn.Global, indexes...)

	t1 := time.Unix(1587396557, 0).UTC()
	t2 := t1.Add(-DeletedRepositoryGracePeriod + time.Minute)
	t3 := t1.Add(-DeletedRepositoryGracePeriod - time.Minute)

	deletions := map[int]time.Time{
		52: t2, 54: t2, 56: t2, // deleted too recently
		61: t3, 63: t3, 65: t3, // deleted
	}

	for repositoryID, deletedAt := range deletions {
		query := sqlf.Sprintf(`UPDATE repo SET deleted_at=%s WHERE id=%s`, deletedAt, repositoryID)

		if _, err := dbconn.Global.Query(query.Query(sqlf.PostgresBindVar), query.Args()...); err != nil {
			t.Fatalf("Failed to update repository: %s", err)
		}
	}

	ids, err := store.DeleteIndexesWithoutRepository(context.Background(), t1)
	if err != nil {
		t.Fatalf("unexpected error deleting indexes: %s", err)
	}

	expected := map[int]int{
		61: 21,
		63: 23,
		65: 25,
	}
	if diff := cmp.Diff(expected, ids); diff != "" {
		t.Errorf("unexpected ids (-want +got):\n%s", diff)
	}
}

func TestDeleteOldIndexes(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	store := testStore()

	t1 := time.Unix(1587396557, 0).UTC()
	t2 := t1.Add(time.Minute)
	t3 := t1.Add(time.Minute * 4)
	t4 := t1.Add(time.Minute * 6)

	insertIndexes(t, dbconn.Global,
		Index{ID: 1, State: "completed", QueuedAt: t1},
		Index{ID: 2, State: "errored", QueuedAt: t2},
		Index{ID: 3, State: "completed", QueuedAt: t3},
		Index{ID: 4, State: "completed", QueuedAt: t4}, // too new
		Index{ID: 5, State: "queued", QueuedAt: t4},    // too new
		Index{ID: 6, State: "queued", QueuedAt: t3},
		Index{ID: 7, State: "queued", QueuedAt: t4}, // too new
	)

	if count, err := store.DeleteOldIndexes(context.Background(), time.Minute, t1.Add(time.Minute*6)); err != nil {
		t.Fatalf("unexpected error pruning indexes: %s", err)
	} else if count != 4 {
		t.Fatalf("unexpected number of indexes deleted: want=%d have=%d", 4, count)
	}

	existence := map[int]bool{
		1: false,
		2: false,
		3: false,
		4: true,
		5: true,
		6: false,
		7: true,
	}
	for id, expectedExists := range existence {
		if _, exists, err := store.GetIndexByID(context.Background(), id); err != nil {
			t.Fatalf("unexpected error getting index: %s", err)
		} else if exists != expectedExists {
			t.Fatalf("unexpected record %d. want=%v have=%v", id, expectedExists, exists)
		}
	}
}
