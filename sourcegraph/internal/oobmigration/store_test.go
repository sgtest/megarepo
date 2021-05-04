package oobmigration

import (
	"context"
	"fmt"
	"sort"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/internal/database/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtesting"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
)

func init() {
	dbtesting.DBNameSuffix = "oobmigration"
}

func TestList(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	store := testStore(t)

	migrations, err := store.List(context.Background())
	if err != nil {
		t.Fatalf("unexpected error getting migrations: %s", err)
	}

	expectedMigrations := make([]Migration, len(testMigrations))
	copy(expectedMigrations, testMigrations)
	sort.Slice(expectedMigrations, func(i, j int) bool {
		return expectedMigrations[i].ID > expectedMigrations[j].ID
	})

	if diff := cmp.Diff(expectedMigrations, migrations); diff != "" {
		t.Errorf("unexpected migrations (-want +got):\n%s", diff)
	}
}

func TestUpdateDirection(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	store := testStore(t)

	if err := store.UpdateDirection(context.Background(), 3, true); err != nil {
		t.Fatalf("unexpected error updating direction: %s", err)
	}

	migration, exists, err := store.GetByID(context.Background(), 3)
	if err != nil {
		t.Fatalf("unexpected error getting migrations: %s", err)
	}
	if !exists {
		t.Fatalf("expected record to exist")
	}

	expectedMigration := testMigrations[2] // ID = 3
	expectedMigration.ApplyReverse = true

	if diff := cmp.Diff(expectedMigration, migration); diff != "" {
		t.Errorf("unexpected migration (-want +got):\n%s", diff)
	}
}

func TestUpdateProgress(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	now := testTime.Add(time.Hour * 7)
	dbtesting.SetupGlobalTestDB(t)
	store := testStore(t)

	if err := store.updateProgress(context.Background(), 3, 0.7, now); err != nil {
		t.Fatalf("unexpected error updating migration: %s", err)
	}

	migration, exists, err := store.GetByID(context.Background(), 3)
	if err != nil {
		t.Fatalf("unexpected error getting migrations: %s", err)
	}
	if !exists {
		t.Fatalf("expected record to exist")
	}

	expectedMigration := testMigrations[2] // ID = 3
	expectedMigration.Progress = 0.7
	expectedMigration.LastUpdated = timePtr(now)

	if diff := cmp.Diff(expectedMigration, migration); diff != "" {
		t.Errorf("unexpected migration (-want +got):\n%s", diff)
	}
}

func TestAddError(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	now := testTime.Add(time.Hour * 8)
	dbtesting.SetupGlobalTestDB(t)
	store := testStore(t)

	if err := store.addError(context.Background(), 2, "oops", now); err != nil {
		t.Fatalf("unexpected error updating migration: %s", err)
	}

	migration, exists, err := store.GetByID(context.Background(), 2)
	if err != nil {
		t.Fatalf("unexpected error getting migrations: %s", err)
	}
	if !exists {
		t.Fatalf("expected record to exist")
	}

	expectedMigration := testMigrations[1] // ID = 2
	expectedMigration.LastUpdated = timePtr(now)
	expectedMigration.Errors = []MigrationError{
		{Message: "oops", Created: now},
		{Message: "uh-oh 1", Created: testTime.Add(time.Hour*5 + time.Second*2)},
		{Message: "uh-oh 2", Created: testTime.Add(time.Hour*5 + time.Second*1)},
	}

	if diff := cmp.Diff(expectedMigration, migration); diff != "" {
		t.Errorf("unexpected migration (-want +got):\n%s", diff)
	}
}

func TestAddErrorBounded(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	now := testTime.Add(time.Hour * 9)
	dbtesting.SetupGlobalTestDB(t)
	store := testStore(t)

	var expectedErrors []MigrationError
	for i := 0; i < MaxMigrationErrors*1.5; i++ {
		now = now.Add(time.Second)

		if err := store.addError(context.Background(), 2, fmt.Sprintf("oops %d", i), now); err != nil {
			t.Fatalf("unexpected error updating migration: %s", err)
		}

		expectedErrors = append(expectedErrors, MigrationError{
			Message: fmt.Sprintf("oops %d", i),
			Created: now,
		})
	}

	migration, exists, err := store.GetByID(context.Background(), 2)
	if err != nil {
		t.Fatalf("unexpected error getting migration: %s", err)
	}
	if !exists {
		t.Fatalf("expected record to exist")
	}

	n := len(expectedErrors) - 1
	for i := 0; i < len(expectedErrors)/2; i++ {
		expectedErrors[i], expectedErrors[n-i] = expectedErrors[n-i], expectedErrors[i]
	}

	expectedMigration := testMigrations[1] // ID = 2
	expectedMigration.LastUpdated = timePtr(now)
	expectedMigration.Errors = expectedErrors[:MaxMigrationErrors]

	if diff := cmp.Diff(expectedMigration, migration); diff != "" {
		t.Errorf("unexpected migrations (-want +got):\n%s", diff)
	}
}

//
//

var testTime = time.Unix(1613414740, 0)

var testMigrations = []Migration{
	{
		ID:             1,
		Team:           "search",
		Component:      "zoekt-index",
		Description:    "rot13 all the indexes for security",
		Introduced:     "3.25.0",
		Deprecated:     nil,
		Progress:       0,
		Created:        testTime,
		LastUpdated:    nil,
		NonDestructive: false,
		ApplyReverse:   false,
		Errors:         []MigrationError{},
	},
	{
		ID:             2,
		Team:           "codeintel",
		Component:      "lsif_data_documents",
		Description:    "denormalize counts",
		Introduced:     "3.26.0",
		Deprecated:     strPtr("3.28.0"),
		Progress:       0.5,
		Created:        testTime.Add(time.Hour * 1),
		LastUpdated:    timePtr(testTime.Add(time.Hour * 2)),
		NonDestructive: true,
		ApplyReverse:   false,
		Errors: []MigrationError{
			{Message: "uh-oh 1", Created: testTime.Add(time.Hour*5 + time.Second*2)},
			{Message: "uh-oh 2", Created: testTime.Add(time.Hour*5 + time.Second*1)},
		},
	},
	{
		ID:             3,
		Team:           "platform",
		Component:      "lsif_data_documents",
		Description:    "gzip payloads",
		Introduced:     "3.24.0",
		Deprecated:     nil,
		Progress:       0.4,
		Created:        testTime.Add(time.Hour * 3),
		LastUpdated:    timePtr(testTime.Add(time.Hour * 4)),
		NonDestructive: false,
		ApplyReverse:   true,
		Errors: []MigrationError{
			{Message: "uh-oh 3", Created: testTime.Add(time.Hour*5 + time.Second*4)},
			{Message: "uh-oh 4", Created: testTime.Add(time.Hour*5 + time.Second*3)},
		},
	},
}

func strPtr(s string) *string        { return &s }
func timePtr(t time.Time) *time.Time { return &t }

func testStore(t *testing.T) *Store {
	store := NewStoreWithDB(dbconn.Global)

	for i := range testMigrations {
		if err := insertMigration(store, testMigrations[i]); err != nil {
			t.Fatalf("unexpected error inserting migration: %s", err)
		}
	}

	return store
}

func insertMigration(store *Store, migration Migration) error {
	query := sqlf.Sprintf(`
		INSERT INTO out_of_band_migrations (
			id,
			team,
			component,
			description,
			introduced,
			deprecated,
			progress,
			created,
			last_updated,
			non_destructive,
			apply_reverse
		) VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s, %s, %s)
	`,
		migration.ID,
		migration.Team,
		migration.Component,
		migration.Description,
		migration.Introduced,
		dbutil.NullString{S: migration.Deprecated},
		migration.Progress,
		migration.Created,
		migration.LastUpdated,
		migration.NonDestructive,
		migration.ApplyReverse,
	)

	if err := store.Store.Exec(context.Background(), query); err != nil {
		return err
	}

	for _, err := range migration.Errors {
		query := sqlf.Sprintf(`
			INSERT INTO out_of_band_migrations_errors (
				migration_id,
				message,
				created
			) VALUES (%s, %s, %s)
		`,
			migration.ID,
			err.Message,
			err.Created,
		)

		if err := store.Store.Exec(context.Background(), query); err != nil {
			return err
		}
	}

	return nil
}
