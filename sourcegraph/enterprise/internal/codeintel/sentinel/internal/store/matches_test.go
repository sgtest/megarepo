package store

import (
	"context"
	"fmt"
	"math"
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"
	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/sentinel/shared"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared/types"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func TestVulnerabilityMatchByID(t *testing.T) {
	ctx := context.Background()
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	store := New(&observation.TestContext, db)

	setupReferences(t, db)

	if _, err := store.InsertVulnerabilities(ctx, testVulnerabilities); err != nil {
		t.Fatalf("unexpected error inserting vulnerabilities: %s", err)
	}

	if _, _, err := store.ScanMatches(ctx, 100); err != nil {
		t.Fatalf("unexpected error inserting vulnerabilities: %s", err)
	}

	match, ok, err := store.VulnerabilityMatchByID(ctx, 3)
	if err != nil {
		t.Fatalf("unexpected error getting vulnerability match: %s", err)
	}
	if !ok {
		t.Fatalf("expected match to exist")
	}

	expectedMatch := shared.VulnerabilityMatch{
		ID:              3,
		UploadID:        52,
		VulnerabilityID: 1,
		AffectedPackage: badConfig,
	}
	if diff := cmp.Diff(expectedMatch, match); diff != "" {
		t.Errorf("unexpected vulnerability match (-want +got):\n%s", diff)
	}
}

func TestGetVulnerabilityMatches(t *testing.T) {
	ctx := context.Background()
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	store := New(&observation.TestContext, db)

	setupReferences(t, db)

	if _, err := store.InsertVulnerabilities(ctx, testVulnerabilities); err != nil {
		t.Fatalf("unexpected error inserting vulnerabilities: %s", err)
	}

	if _, _, err := store.ScanMatches(ctx, 100); err != nil {
		t.Fatalf("unexpected error inserting vulnerabilities: %s", err)
	}

	type testCase struct {
		name            string
		expectedMatches []shared.VulnerabilityMatch
	}
	testCases := []testCase{
		{
			name: "all",
			expectedMatches: []shared.VulnerabilityMatch{
				{
					ID:              1,
					UploadID:        50,
					VulnerabilityID: 1,
					AffectedPackage: badConfig,
				}, {
					ID:              2,
					UploadID:        51,
					VulnerabilityID: 1,
					AffectedPackage: badConfig,
				}, {
					ID:              3,
					UploadID:        52,
					VulnerabilityID: 1,
					AffectedPackage: badConfig,
				},
			},
		},
	}

	runTest := func(testCase testCase, lo, hi int) (errors int) {
		t.Run(testCase.name, func(t *testing.T) {
			matches, totalCount, err := store.GetVulnerabilityMatches(ctx, shared.GetVulnerabilityMatchesArgs{
				Limit:  3,
				Offset: lo,
			})
			if err != nil {
				t.Fatalf("unexpected error getting vulnerability matches: %s", err)
			}
			if totalCount != len(testCase.expectedMatches) {
				t.Errorf("unexpected total count. want=%d have=%d", len(testCase.expectedMatches), totalCount)
			}

			if totalCount != 0 {
				if diff := cmp.Diff(testCase.expectedMatches[lo:hi], matches); diff != "" {
					t.Errorf("unexpected vulnerability matches at offset %d-%d (-want +got):\n%s", lo, hi, diff)
					errors++
				}
			}
		})

		return
	}

	for _, testCase := range testCases {
		if n := len(testCase.expectedMatches); n == 0 {
			runTest(testCase, 0, 0)
		} else {
			for lo := 0; lo < n; lo++ {
				if numErrors := runTest(testCase, lo, int(math.Min(float64(lo)+3, float64(n)))); numErrors > 0 {
					break
				}
			}
		}
	}
}

func setupReferences(t *testing.T, db database.DB) {
	store := basestore.NewWithHandle(db.Handle())

	insertUploads(t, db,
		types.Upload{ID: 50},
		types.Upload{ID: 51},
		types.Upload{ID: 52},
		types.Upload{ID: 53},
		types.Upload{ID: 54},
		types.Upload{ID: 55},
	)

	if err := store.Exec(context.Background(), sqlf.Sprintf(`
		INSERT INTO lsif_references (scheme, name, version, dump_id)
		VALUES
			('gomod', 'github.com/go-nacelle/config', 'v1.2.3', 50),
			('gomod', 'github.com/go-nacelle/config', 'v1.2.4', 51),
			('gomod', 'github.com/go-nacelle/config', 'v1.2.5', 52),
			('gomod', 'github.com/go-nacelle/config', 'v1.2.6', 53)
	`)); err != nil {
		t.Fatalf("failed to insert references: %s", err)
	}
}

// insertUploads populates the lsif_uploads table with the given upload models.
func insertUploads(t testing.TB, db database.DB, uploads ...types.Upload) {
	for _, upload := range uploads {
		if upload.Commit == "" {
			upload.Commit = makeCommit(upload.ID)
		}
		if upload.State == "" {
			upload.State = "completed"
		}
		if upload.RepositoryID == 0 {
			upload.RepositoryID = 50
		}
		if upload.Indexer == "" {
			upload.Indexer = "lsif-go"
		}
		if upload.IndexerVersion == "" {
			upload.IndexerVersion = "latest"
		}
		if upload.UploadedParts == nil {
			upload.UploadedParts = []int{}
		}

		// Ensure we have a repo for the inner join in select queries
		insertRepo(t, db, upload.RepositoryID, upload.RepositoryName)

		query := sqlf.Sprintf(`
			INSERT INTO lsif_uploads (
				id,
				commit,
				root,
				uploaded_at,
				state,
				failure_message,
				started_at,
				finished_at,
				process_after,
				num_resets,
				num_failures,
				repository_id,
				indexer,
				indexer_version,
				num_parts,
				uploaded_parts,
				upload_size,
				associated_index_id,
				content_type,
				should_reindex
			) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
		`,
			upload.ID,
			upload.Commit,
			upload.Root,
			upload.UploadedAt,
			upload.State,
			upload.FailureMessage,
			upload.StartedAt,
			upload.FinishedAt,
			upload.ProcessAfter,
			upload.NumResets,
			upload.NumFailures,
			upload.RepositoryID,
			upload.Indexer,
			upload.IndexerVersion,
			upload.NumParts,
			pq.Array(upload.UploadedParts),
			upload.UploadSize,
			upload.AssociatedIndexID,
			upload.ContentType,
			upload.ShouldReindex,
		)

		if _, err := db.ExecContext(context.Background(), query.Query(sqlf.PostgresBindVar), query.Args()...); err != nil {
			t.Fatalf("unexpected error while inserting upload: %s", err)
		}
	}
}

// makeCommit formats an integer as a 40-character git commit hash.
func makeCommit(i int) string {
	return fmt.Sprintf("%040d", i)
}

// insertRepo creates a repository record with the given id and name. If there is already a repository
// with the given identifier, nothing happens
func insertRepo(t testing.TB, db database.DB, id int, name string) {
	if name == "" {
		name = fmt.Sprintf("n-%d", id)
	}

	deletedAt := sqlf.Sprintf("NULL")
	if strings.HasPrefix(name, "DELETED-") {
		deletedAt = sqlf.Sprintf("%s", time.Unix(1587396557, 0).UTC())
	}
	insertRepoQuery := sqlf.Sprintf(
		`INSERT INTO repo (id, name, deleted_at) VALUES (%s, %s, %s) ON CONFLICT (id) DO NOTHING`,
		id,
		name,
		deletedAt,
	)
	if _, err := db.ExecContext(context.Background(), insertRepoQuery.Query(sqlf.PostgresBindVar), insertRepoQuery.Args()...); err != nil {
		t.Fatalf("unexpected error while upserting repository: %s", err)
	}

	status := "cloned"
	if strings.HasPrefix(name, "DELETED-") {
		status = "not_cloned"
	}
	updateGitserverRepoQuery := sqlf.Sprintf(
		`UPDATE gitserver_repos SET clone_status = %s WHERE repo_id = %s`,
		status,
		id,
	)
	if _, err := db.ExecContext(context.Background(), updateGitserverRepoQuery.Query(sqlf.PostgresBindVar), updateGitserverRepoQuery.Args()...); err != nil {
		t.Fatalf("unexpected error while upserting gitserver repository: %s", err)
	}
}
