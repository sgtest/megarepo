package connections

import (
	"context"
	"database/sql"

	"github.com/hashicorp/go-multierror"

	"github.com/sourcegraph/sourcegraph/internal/database/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/database/migration/runner"
	"github.com/sourcegraph/sourcegraph/internal/database/migration/schemas"
)

// NewTestDB creates a new connection to the a database and applies the given migration.
func NewTestDB(dsn string, schemas ...*schemas.Schema) (_ *sql.DB, _ func(err error) error, err error) {
	db, close, err := dbconn.ConnectInternal(dsn, "", "", nil)
	if err != nil {
		return nil, nil, err
	}
	defer func() {
		if err != nil {
			if closeErr := db.Close(); closeErr != nil {
				err = multierror.Append(err, closeErr)
			}
		}
	}()

	options := runner.Options{
		Up:          true,
		SchemaNames: schemaNames(schemas),
	}
	if err := runner.NewRunner(newStoreFactoryMap(db, schemas)).Run(context.Background(), options); err != nil {
		return nil, nil, err
	}

	return db, close, nil
}

func newStoreFactoryMap(db *sql.DB, schemas []*schemas.Schema) map[string]runner.StoreFactory {
	storeFactoryMap := make(map[string]runner.StoreFactory, len(schemas))
	for _, schema := range schemas {
		schema := schema

		storeFactoryMap[schema.Name] = func(ctx context.Context) (runner.Store, error) {
			return newMemoryStore(db), nil
		}
	}

	return storeFactoryMap
}

func schemaNames(schemas []*schemas.Schema) []string {
	names := make([]string, 0, len(schemas))
	for _, schema := range schemas {
		names = append(names, schema.Name)
	}

	return names
}
