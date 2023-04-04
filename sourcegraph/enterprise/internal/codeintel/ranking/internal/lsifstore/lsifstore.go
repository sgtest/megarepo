package lsifstore

import (
	"context"

	"github.com/sourcegraph/scip/bindings/go/scip"

	codeintelshared "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/shared"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

type LsifStore interface {
	Transact(ctx context.Context) (LsifStore, error)
	Done(err error) error

	// Stream
	InsertDefinitionsAndReferencesForDocument(ctx context.Context, upload shared.ExportedUpload, rankingGraphKey string, rankingBatchSize int, f func(ctx context.Context, upload shared.ExportedUpload, rankingBatchSize int, rankingGraphKey, path string, document *scip.Document) error) error
}

type SCIPWriter interface {
	InsertDocument(ctx context.Context, path string, scipDocument *scip.Document) error
	Flush(ctx context.Context) (uint32, error)
}

type store struct {
	db         *basestore.Store
	operations *operations
}

func New(observationCtx *observation.Context, db codeintelshared.CodeIntelDB) LsifStore {
	return newStore(observationCtx, db)
}

func newStore(observationCtx *observation.Context, db codeintelshared.CodeIntelDB) *store {
	return &store{
		db:         basestore.NewWithHandle(db.Handle()),
		operations: newOperations(observationCtx),
	}
}

func (s *store) Transact(ctx context.Context) (LsifStore, error) {
	tx, err := s.db.Transact(ctx)
	if err != nil {
		return nil, err
	}

	return &store{
		db:         tx,
		operations: s.operations,
	}, nil
}

func (s *store) Done(err error) error {
	return s.db.Done(err)
}
