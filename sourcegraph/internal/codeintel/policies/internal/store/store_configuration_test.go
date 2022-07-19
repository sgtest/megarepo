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
	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/internal/codeintel/policies/shared"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func TestGetConfigurationPolicies(t *testing.T) {
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	store := testStoreWithoutConfigurationPolicies(t, db)
	ctx := context.Background()

	query := `
		INSERT INTO lsif_configuration_policies (
			id,
			repository_id,
			name,
			type,
			pattern,
			repository_patterns,
			retention_enabled,
			retention_duration_hours,
			retain_intermediate_commits,
			indexing_enabled,
			index_commit_max_age_hours,
			index_intermediate_commits,
			lockfile_indexing_enabled
		) VALUES
			(101, 42,   'policy  1 abc', 'GIT_TREE', '', null,              false, 0, false, true,  0, false, false),
			(102, 42,   'policy  2 def', 'GIT_TREE', '', null,              true , 0, false, false, 0, false, false),
			(103, 43,   'policy  3 bcd', 'GIT_TREE', '', null,              false, 0, false, true,  0, false, false),
			(104, NULL, 'policy  4 abc', 'GIT_TREE', '', null,              true , 0, false, false, 0, false, false),
			(105, NULL, 'policy  5 bcd', 'GIT_TREE', '', null,              false, 0, false, true,  0, false, false),
			(106, NULL, 'policy  6 bcd', 'GIT_TREE', '', '{gitlab.com/*}',  true , 0, false, false, 0, false, false),
			(107, NULL, 'policy  7 def', 'GIT_TREE', '', '{gitlab.com/*1}', false, 0, false, true,  0, false, false),
			(108, NULL, 'policy  8 abc', 'GIT_TREE', '', '{gitlab.com/*2}', true , 0, false, false, 0, false, false),
			(109, NULL, 'policy  9 def', 'GIT_TREE', '', '{github.com/*}',  false, 0, false, true,  0, false, false),
			(110, NULL, 'policy 10 def', 'GIT_TREE', '', '{github.com/*}',  false, 0, false, false, 0, false, true)
	`
	if _, err := db.ExecContext(ctx, query); err != nil {
		t.Fatalf("unexpected error while inserting configuration policies: %s", err)
	}

	insertRepo(t, db, 41, "gitlab.com/test1")
	insertRepo(t, db, 42, "github.com/test2")
	insertRepo(t, db, 43, "bitbucket.com/test3")
	insertRepo(t, db, 44, "localhost/secret-repo")

	for policyID, patterns := range map[int][]string{
		106: {"gitlab.com/*"},
		107: {"gitlab.com/*1"},
		108: {"gitlab.com/*2"},
		109: {"github.com/*"},
		110: {"github.com/*"},
	} {
		if err := store.UpdateReposMatchingPatterns(ctx, patterns, policyID, nil); err != nil {
			t.Fatalf("unexpected error while updating repositories matching patterns: %s", err)
		}
	}

	type testCase struct {
		repositoryID        int
		term                string
		forDataRetention    bool
		forIndexing         bool
		forLockfileIndexing bool
		expectedIDs         []int
	}
	testCases := []testCase{
		{expectedIDs: []int{101, 102, 103, 104, 105, 106, 107, 108, 109, 110}},   // Any flags; all policies
		{repositoryID: 41, expectedIDs: []int{104, 105, 106, 107}},               // Any flags; matches repo by patterns
		{repositoryID: 42, expectedIDs: []int{101, 102, 104, 105, 109, 110}},     // Any flags; matches repo by assignment and pattern
		{repositoryID: 43, expectedIDs: []int{103, 104, 105}},                    // Any flags; matches repo by assignment
		{repositoryID: 44, expectedIDs: []int{104, 105}},                         // Any flags; no matches by repo
		{forDataRetention: true, expectedIDs: []int{102, 104, 106, 108}},         // For data retention; all policies
		{forDataRetention: true, repositoryID: 41, expectedIDs: []int{104, 106}}, // For data retention; matches repo by patterns
		{forDataRetention: true, repositoryID: 42, expectedIDs: []int{102, 104}}, // For data retention; matches repo by assignment and pattern
		{forDataRetention: true, repositoryID: 43, expectedIDs: []int{104}},      // For data retention; matches repo by assignment
		{forDataRetention: true, repositoryID: 44, expectedIDs: []int{104}},      // For data retention; no matches by repo
		{forIndexing: true, expectedIDs: []int{101, 103, 105, 107, 109}},         // For indexing; all policies
		{forIndexing: true, repositoryID: 41, expectedIDs: []int{105, 107}},      // For indexing; matches repo by patterns
		{forIndexing: true, repositoryID: 42, expectedIDs: []int{101, 105, 109}}, // For indexing; matches repo by assignment and pattern
		{forIndexing: true, repositoryID: 43, expectedIDs: []int{103, 105}},      // For indexing; matches repo by assignment
		{forIndexing: true, repositoryID: 44, expectedIDs: []int{105}},           // For indexing; no matches by repo
		{forLockfileIndexing: true, expectedIDs: []int{110}},                     // For lockfile indexing; all policies

		{term: "bc", expectedIDs: []int{101, 103, 104, 105, 106, 108}}, // Searches by name (multiple substring matches)
		{term: "abcd", expectedIDs: []int{}},                           // Searches by name (no matches)
	}

	runTest := func(testCase testCase, lo, hi int) (errors int) {
		name := fmt.Sprintf(
			"repositoryID=%d term=%q forDataRetention=%v forIndexing=%v forLockfileIndexing=%v offset=%d",
			testCase.repositoryID,
			testCase.term,
			testCase.forDataRetention,
			testCase.forIndexing,
			testCase.forLockfileIndexing,
			lo,
		)

		t.Run(name, func(t *testing.T) {
			policies, totalCount, err := store.GetConfigurationPolicies(ctx, shared.GetConfigurationPoliciesOptions{
				RepositoryID:        testCase.repositoryID,
				Term:                testCase.term,
				ForDataRetention:    testCase.forDataRetention,
				ForIndexing:         testCase.forIndexing,
				ForLockfileIndexing: testCase.forLockfileIndexing,
				Limit:               3,
				Offset:              lo,
			})
			if err != nil {
				t.Fatalf("unexpected error fetching configuration policies: %s", err)
			}
			if totalCount != len(testCase.expectedIDs) {
				t.Errorf("unexpected total count. want=%d have=%d", len(testCase.expectedIDs), totalCount)
				errors++
			}
			if totalCount != 0 {
				var ids []int
				for _, policy := range policies {
					ids = append(ids, policy.ID)
				}
				if diff := cmp.Diff(testCase.expectedIDs[lo:hi], ids); diff != "" {
					t.Errorf("unexpected configuration policy ids at offset %d (-want +got):\n%s", lo, diff)
					errors++
				}
			}
		})

		return errors
	}

	for _, testCase := range testCases {
		if n := len(testCase.expectedIDs); n == 0 {
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

// removes default configuration policies
func testStoreWithoutConfigurationPolicies(t *testing.T, db database.DB) Store {
	if _, err := db.ExecContext(context.Background(), `TRUNCATE lsif_configuration_policies`); err != nil {
		t.Fatalf("unexpected error while inserting configuration policies: %s", err)
	}

	return New(db, &observation.TestContext)
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

	query := sqlf.Sprintf(
		`INSERT INTO repo (id, name, deleted_at) VALUES (%s, %s, %s) ON CONFLICT (id) DO NOTHING`,
		id,
		name,
		deletedAt,
	)
	if _, err := db.ExecContext(context.Background(), query.Query(sqlf.PostgresBindVar), query.Args()...); err != nil {
		t.Fatalf("unexpected error while upserting repository: %s", err)
	}
}
