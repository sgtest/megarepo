package dbstore

import (
	"context"
	"testing"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func testStore(db database.DB) *Store {
	return NewWithDB(db, &observation.TestContext)
}

// removes default configuration policies
func testStoreWithoutConfigurationPolicies(t *testing.T, db database.DB) *Store {
	if _, err := db.ExecContext(context.Background(), `TRUNCATE lsif_configuration_policies`); err != nil {
		t.Fatalf("unexpected error while inserting configuration policies: %s", err)
	}

	return testStore(db)
}
