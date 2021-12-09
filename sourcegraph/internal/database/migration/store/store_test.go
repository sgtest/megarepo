package store

import (
	"context"
	"strings"
	"testing"

	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/database/migration/definition"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func TestEnsureSchemaTable(t *testing.T) {
	db := dbtest.NewDB(t)
	store := testStore(db)
	ctx := context.Background()

	if err := store.Exec(ctx, sqlf.Sprintf("SELECT * FROM test_migrations_table")); err == nil {
		t.Fatalf("expected query to fail due to missing schema table")
	}

	if err := store.EnsureSchemaTable(ctx); err != nil {
		t.Fatalf("unexpected error ensuring schema table exists: %s", err)
	}

	if err := store.Exec(ctx, sqlf.Sprintf("SELECT * FROM test_migrations_table")); err != nil {
		t.Fatalf("unexpected error querying version table: %s", err)
	}

	if err := store.EnsureSchemaTable(ctx); err != nil {
		t.Fatalf("expected method to be idempotent, got error: %s", err)
	}
}

func TestVersion(t *testing.T) {
	db := dbtest.NewDB(t)
	store := testStore(db)
	ctx := context.Background()

	if err := store.EnsureSchemaTable(ctx); err != nil {
		t.Fatalf("unexpected error ensuring schema table exists: %s", err)
	}

	t.Run("empty", func(*testing.T) {
		if _, _, ok, err := store.Version(ctx); err != nil {
			t.Fatalf("unexpected error querying version: %s", err)
		} else if ok {
			t.Fatalf("unexpected version")
		}
	})

	testCases := []struct {
		name    string
		version int
		dirty   bool
	}{
		{"clean", 25, false},
		{"dirty", 32, true},
	}

	for _, testCase := range testCases {
		t.Run(testCase.name, func(t *testing.T) {
			if err := store.Exec(ctx, sqlf.Sprintf(`DELETE FROM test_migrations_table`)); err != nil {
				t.Fatalf("unexpected error clearing data: %s", err)
			}
			if err := store.Exec(ctx, sqlf.Sprintf(`INSERT INTO test_migrations_table VALUES (%s, %s)`, testCase.version, testCase.dirty)); err != nil {
				t.Fatalf("unexpected error inserting data: %s", err)
			}

			if version, dirty, ok, err := store.Version(ctx); err != nil {
				t.Fatalf("unexpected error querying version: %s", err)
			} else if !ok {
				t.Fatalf("expected a version to be found")
			} else if version != testCase.version {
				t.Fatalf("unexpected version. want=%d have=%d", testCase.version, version)
			} else if dirty != testCase.dirty {
				t.Fatalf("unexpected dirty flag. want=%v have=%v", testCase.dirty, dirty)
			}
		})
	}
}

func TestLock(t *testing.T) {
	db := dbtest.NewDB(t)
	store := testStore(db)
	ctx := context.Background()

	t.Run("sanity test", func(t *testing.T) {
		acquired, close, err := store.Lock(ctx)
		if err != nil {
			t.Fatalf("unexpected error acquiring lock: %s", err)
		}
		if !acquired {
			t.Fatalf("expected lock to be acquired")
		}

		if err := close(nil); err != nil {
			t.Fatalf("unexpected error releasing lock: %s", err)
		}
	})
}

func TestUp(t *testing.T) {
	db := dbtest.NewDB(t)
	store := testStore(db)
	ctx := context.Background()

	if err := store.EnsureSchemaTable(ctx); err != nil {
		t.Fatalf("unexpected error ensuring schema table exists: %s", err)
	}
	if err := store.Exec(ctx, sqlf.Sprintf(`INSERT INTO test_migrations_table VALUES (15, false)`)); err != nil {
		t.Fatalf("unexpected error setting initial version: %s", err)
	}

	t.Run("success", func(t *testing.T) {
		if err := store.Up(ctx, definition.Definition{
			ID: 16,
			UpQuery: sqlf.Sprintf(`
				CREATE TABLE test_trees (
					name text,
					leaf_type text,
					seed_type text,
					bark_type text
				);

				INSERT INTO test_trees VALUES
					('oak', 'broad', 'regular', 'strong'),
					('birch', 'narrow', 'regular', 'flaky'),
					('pine', 'needle', 'pine cone', 'soft');
			`),
		}); err != nil {
			t.Fatalf("unexpected error running migration: %s", err)
		}

		if barkType, _, err := basestore.ScanFirstString(store.Query(ctx, sqlf.Sprintf(`SELECT bark_type FROM test_trees WHERE name = 'birch'`))); err != nil {
			t.Fatalf("migration query did not succeed; unexpected error querying test table: %s", err)
		} else if barkType != "flaky" {
			t.Fatalf("migration query did not succeed; unexpected bark type. want=%s have=%s", "flaky", barkType)
		}

		// Version set to migration ID; not dirty
		if version, dirty, ok, err := store.Version(ctx); err != nil || !ok || dirty || version != 16 {
			t.Fatalf("unexpected version. want=(version=%d, dirty=%v), have=(version=%d, dirty=%v, ok=%v, error=%q)", 16, false, version, dirty, ok, err)
		}
	})

	t.Run("unexpected version", func(t *testing.T) {
		expectedErrorMessage := "wrong expected version"

		if err := store.Up(ctx, definition.Definition{
			ID: 18,
			UpQuery: sqlf.Sprintf(`
				-- Does not actually run
			`),
		}); err == nil || !strings.HasPrefix(err.Error(), expectedErrorMessage) {
			t.Fatalf("unexpected error want=%q have=%q", expectedErrorMessage, err)
		}

		// Version, dirty status unchanged
		if version, dirty, ok, err := store.Version(ctx); err != nil || !ok || dirty || version != 16 {
			t.Fatalf("unexpected version. want=(version=%d, dirty=%v), have=(version=%d, dirty=%v, ok=%v, error=%q)", 16, false, version, dirty, ok, err)
		}
	})

	t.Run("query failure", func(t *testing.T) {
		expectedErrorMessage := "SQL Error"

		if err := store.Up(ctx, definition.Definition{
			ID: 17,
			UpQuery: sqlf.Sprintf(`
				-- Note: table already exists
				CREATE TABLE test_trees (
					name text,
					leaf_type text,
					seed_type text,
					bark_type text
				);
			`),
		}); err == nil || !strings.HasPrefix(err.Error(), expectedErrorMessage) {
			t.Fatalf("unexpected error want=%q have=%q", expectedErrorMessage, err)
		}

		// Version set to migration ID; dirty
		if version, dirty, ok, err := store.Version(ctx); err != nil || !ok || !dirty || version != 17 {
			t.Fatalf("unexpected version. want=(version=%d, dirty=%v), have=(version=%d, dirty=%v, ok=%v, error=%q)", 17, true, version, dirty, ok, err)
		}
	})

	t.Run("dirty", func(t *testing.T) {
		expectedErrorMessage := "dirty database"

		if err := store.Up(ctx, definition.Definition{
			ID: 17,
			UpQuery: sqlf.Sprintf(`
				-- Does not actually run
			`),
		}); err == nil || !strings.HasPrefix(err.Error(), expectedErrorMessage) {
			t.Fatalf("unexpected error want=%q have=%q", expectedErrorMessage, err)
		}

		// Version, dirty status unchanged
		if version, dirty, ok, err := store.Version(ctx); err != nil || !ok || !dirty || version != 17 {
			t.Fatalf("unexpected version. want=(version=%d, dirty=%v), have=(version=%d, dirty=%v, ok=%v, error=%q)", 17, true, version, dirty, ok, err)
		}
	})
}

func TestDown(t *testing.T) {
	db := dbtest.NewDB(t)
	store := testStore(db)
	ctx := context.Background()

	if err := store.EnsureSchemaTable(ctx); err != nil {
		t.Fatalf("unexpected error ensuring schema table exists: %s", err)
	}
	if err := store.Exec(ctx, sqlf.Sprintf(`INSERT INTO test_migrations_table VALUES (14, false)`)); err != nil {
		t.Fatalf("unexpected error setting initial version: %s", err)
	}
	if err := store.Exec(ctx, sqlf.Sprintf(`
		CREATE TABLE test_trees (
			name text,
			leaf_type text,
			seed_type text,
			bark_type text
		);
	`)); err != nil {
		t.Fatalf("unexpected error creating test table: %s", err)
	}

	testQuery := sqlf.Sprintf(`
		INSERT INTO test_trees VALUES
			('oak', 'broad', 'regular', 'strong'),
			('birch', 'narrow', 'regular', 'flaky'),
			('pine', 'needle', 'pine cone', 'soft');
	`)

	// run twice to ensure the error post-migration is not due to an index constraint
	if err := store.Exec(ctx, testQuery); err != nil {
		t.Fatalf("unexpected error inserting into test table: %s", err)
	}
	if err := store.Exec(ctx, testQuery); err != nil {
		t.Fatalf("unexpected error inserting into test table: %s", err)
	}

	t.Run("success", func(t *testing.T) {
		if err := store.Down(ctx, definition.Definition{
			ID: 14,
			DownQuery: sqlf.Sprintf(`
				DROP TABLE test_trees;
			`),
		}); err != nil {
			t.Fatalf("unexpected error running migration: %s", err)
		}

		// note: this query succeeded twice earlier
		if err := store.Exec(ctx, testQuery); err == nil || !strings.HasPrefix(err.Error(), "SQL Error") {
			t.Fatalf("migration query did not succeed; expected missing table. want=%q have=%q", "SQL Error", err)
		}

		// Version set to migration ID; not dirty
		if version, dirty, ok, err := store.Version(ctx); err != nil || !ok || dirty || version != 13 {
			t.Fatalf("unexpected version. want=(version=%d, dirty=%v), have=(version=%d, dirty=%v, ok=%v, error=%q)", 13, false, version, dirty, ok, err)
		}
	})

	t.Run("unexpected version", func(t *testing.T) {
		expectedErrorMessage := "wrong expected version"

		if err := store.Down(ctx, definition.Definition{
			ID: 12,
			DownQuery: sqlf.Sprintf(`
				-- Does not actually run
			`),
		}); err == nil || !strings.HasPrefix(err.Error(), expectedErrorMessage) {
			t.Fatalf("unexpected error want=%q have=%q", expectedErrorMessage, err)
		}

		// Version, dirty status unchanged
		if version, dirty, ok, err := store.Version(ctx); err != nil || !ok || dirty || version != 13 {
			t.Fatalf("unexpected version. want=(version=%d, dirty=%v), have=(version=%d, dirty=%v, ok=%v, error=%q)", 13, false, version, dirty, ok, err)
		}
	})

	t.Run("query failure", func(t *testing.T) {
		expectedErrorMessage := "SQL Error"

		if err := store.Down(ctx, definition.Definition{
			ID: 13,
			DownQuery: sqlf.Sprintf(`
				-- Note: table does not exist
				DROP TABLE TABLE test_trees;
			`),
		}); err == nil || !strings.HasPrefix(err.Error(), expectedErrorMessage) {
			t.Fatalf("unexpected error want=%q have=%q", expectedErrorMessage, err)
		}

		// Version set to migration ID; dirty
		if version, dirty, ok, err := store.Version(ctx); err != nil || !ok || !dirty || version != 12 {
			t.Fatalf("unexpected version. want=(version=%d, dirty=%v), have=(version=%d, dirty=%v, ok=%v, error=%q)", 12, true, version, dirty, ok, err)
		}
	})

	t.Run("dirty", func(t *testing.T) {
		expectedErrorMessage := "dirty database"

		if err := store.Down(ctx, definition.Definition{
			ID: 12,
			DownQuery: sqlf.Sprintf(`
				-- Does not actually run
			`),
		}); err == nil || !strings.HasPrefix(err.Error(), expectedErrorMessage) {
			t.Fatalf("unexpected error want=%q have=%q", expectedErrorMessage, err)
		}

		// Version, dirty status unchanged
		if version, dirty, ok, err := store.Version(ctx); err != nil || !ok || !dirty || version != 12 {
			t.Fatalf("unexpected version. want=(version=%d, dirty=%v), have=(version=%d, dirty=%v, ok=%v, error=%q)", 12, true, version, dirty, ok, err)
		}
	})
}

func testStore(db dbutil.DB) *Store {
	return NewWithDB(db, "test_migrations_table", NewOperations(&observation.TestContext))
}
