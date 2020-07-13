package store

import (
	"github.com/sourcegraph/sourcegraph/internal/db/basestore"
	"github.com/sourcegraph/sourcegraph/internal/db/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/db/dbtesting"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func init() {
	dbtesting.DBNameSuffix = "codeintel"
}

func rawTestStore() *store {
	return &store{Store: basestore.NewWithHandle(basestore.NewHandleWithDB(dbconn.Global))}
}

func testStore() Store {
	// Wrap in observed, as that's how it's used in production
	return NewObserved(rawTestStore(), &observation.TestContext)
}
