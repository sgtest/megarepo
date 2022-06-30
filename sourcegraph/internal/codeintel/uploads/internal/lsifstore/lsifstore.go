package lsifstore

import (
	"context"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type LsifStore interface {
	Clear(ctx context.Context, bundleIDs ...int) (err error)
}

type store struct {
	db         *basestore.Store
	operations *operations
}

func New(db database.DB, observationContext *observation.Context) LsifStore {
	return &store{
		db:         basestore.NewWithHandle(db.Handle()),
		operations: newOperations(observationContext),
	}
}
