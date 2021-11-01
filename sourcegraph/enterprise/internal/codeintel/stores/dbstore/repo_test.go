package dbstore

import (
	"context"
	"database/sql"
	"sort"
	"testing"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/globals"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtesting"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestRepoIDsByGlobPattern(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	store := testStore(db)
	ctx := context.Background()

	insertRepo(t, db, 50, "Darth Vader")
	insertRepo(t, db, 51, "Darth Venamis")
	insertRepo(t, db, 52, "Darth Maul")
	insertRepo(t, db, 53, "Anakin Skywalker")
	insertRepo(t, db, 54, "Luke Skywalker")
	insertRepo(t, db, 55, "7th Sky Corps")

	testCases := []struct {
		pattern               string
		expectedRepositoryIDs []int
	}{
		{pattern: "Darth*", expectedRepositoryIDs: []int{50, 51, 52}},  // Prefix
		{pattern: "Darth V*", expectedRepositoryIDs: []int{50, 51}},    // Prefix
		{pattern: "* Skywalker", expectedRepositoryIDs: []int{53, 54}}, // Suffix
		{pattern: "*er", expectedRepositoryIDs: []int{50, 53, 54}},     // Suffix
		{pattern: "*Sky*", expectedRepositoryIDs: []int{53, 54, 55}},   // Infix
		{pattern: "Rey Skywalker", expectedRepositoryIDs: nil},         // No match, never happened
	}

	for _, testCase := range testCases {
		repositoryIDs, err := store.RepoIDsByGlobPattern(ctx, testCase.pattern)
		if err != nil {
			t.Fatalf("unexpected error fetching repository ids by glob pattern: %s", err)
		}

		if diff := cmp.Diff(testCase.expectedRepositoryIDs, repositoryIDs); diff != "" {
			t.Errorf("unexpected repository ids (-want +got):\n%s", diff)
		}
	}

	t.Run("enforce repository permissions", func(t *testing.T) {
		// Enable permissions user mapping forces checking repository permissions
		// against permissions tables in the database, which should effectively block
		// all access because permissions tables are empty.
		before := globals.PermissionsUserMapping()
		globals.SetPermissionsUserMapping(&schema.PermissionsUserMapping{Enabled: true})
		defer globals.SetPermissionsUserMapping(before)

		repoIDs, err := store.RepoIDsByGlobPattern(ctx, "*")
		if err != nil {
			t.Fatal(err)
		}
		if len(repoIDs) > 0 {
			t.Fatalf("Want no repositories but got %d repositories", len(repoIDs))
		}
	})
}

func TestUpdateReposMatchingPatterns(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	store := testStore(db)
	ctx := context.Background()

	insertRepo(t, db, 50, "r1")
	insertRepo(t, db, 51, "r2")
	insertRepo(t, db, 52, "r3")
	insertRepo(t, db, 53, "r4")
	insertRepo(t, db, 54, "r5")

	updates := []struct {
		policyID int
		pattern  []string
	}{
		// multiple matches
		{100, []string{"r*"}},

		// exact identifiers
		{101, []string{"r1"}},

		// multiple exact identifiers
		{102, []string{"r2", "r3"}},

		// updated patterns (disjoint)
		{103, []string{"r4"}},
		{103, []string{"r5"}},

		// updated patterns (intersecting)
		{104, []string{"r1", "r2", "r3"}},
		{104, []string{"r2", "r3", "r4"}},

		// deleted matches
		{105, []string{"r5"}},
		{105, []string{}},
	}
	for _, update := range updates {
		if err := store.UpdateReposMatchingPatterns(ctx, update.pattern, update.policyID); err != nil {
			t.Fatalf("unexpected error updating repositories matching patterns: %s", err)
		}
	}

	policies, err := scanPolicyRepositories(db.QueryContext(context.Background(), `
		SELECT policy_id, repo_id
		FROM lsif_configuration_policies_repository_pattern_lookup
	`))
	if err != nil {
		t.Fatalf("unexpected error while scanning policies: %s", err)
	}

	for _, repositoryIDs := range policies {
		sort.Ints(repositoryIDs)
	}

	expectedPolicies := map[int][]int{
		100: {50, 51, 52, 53, 54}, // multiple matches
		101: {50},                 // exact identifiers
		102: {51, 52},             // multiple exact identifiers
		103: {54},                 // updated patterns (disjoint)
		104: {51, 52, 53},         // updated patterns (intersecting)
	}
	if diff := cmp.Diff(expectedPolicies, policies); diff != "" {
		t.Errorf("unexpected repository identifiers for policies (-want +got):\n%s", diff)
	}
}

// scanPolicyRepositories returns a map of policyIDs that have a slice of their correspondent repoIDs (repoIDs associated with that policyIDs).
func scanPolicyRepositories(rows *sql.Rows, queryErr error) (_ map[int][]int, err error) {
	if queryErr != nil {
		return nil, queryErr
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	policies := map[int][]int{}
	for rows.Next() {
		var policyID int
		var repoID int
		if err := rows.Scan(&policyID, &repoID); err != nil {
			return nil, err
		}

		policies[policyID] = append(policies[policyID], repoID)
	}

	return policies, nil
}
