package worker

import (
	"context"
	"time"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/gitserver"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/dbstore"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/lsifstore"
	"github.com/sourcegraph/sourcegraph/internal/db/basestore"
)

type DBStore interface {
	basestore.ShareableStore
	gitserver.DBStore

	With(other basestore.ShareableStore) DBStore
	Transact(ctx context.Context) (DBStore, error)
	Done(err error) error

	UpdatePackages(ctx context.Context, packages []lsifstore.Package) error
	UpdatePackageReferences(ctx context.Context, packageReferences []lsifstore.PackageReference) error
	MarkRepositoryAsDirty(ctx context.Context, repositoryID int) error
	MarkComplete(ctx context.Context, id int) error
	Requeue(ctx context.Context, id int, after time.Time) error
	DeleteOverlappingDumps(ctx context.Context, repositoryID int, commit, root, indexer string) error
}

type DBStoreShim struct {
	*dbstore.Store
}

func (s *DBStoreShim) With(other basestore.ShareableStore) DBStore {
	return &DBStoreShim{s.Store.With(other)}
}

func (s *DBStoreShim) Transact(ctx context.Context) (DBStore, error) {
	tx, err := s.Store.Transact(ctx)
	if err != nil {
		return nil, err
	}

	return &DBStoreShim{tx}, nil
}

type LSIFStore interface {
	Transact(ctx context.Context) (LSIFStore, error)
	Done(err error) error

	WriteMeta(ctx context.Context, bundleID int, meta lsifstore.MetaData) error
	WriteDocuments(ctx context.Context, bundleID int, documents chan lsifstore.KeyedDocumentData) error
	WriteResultChunks(ctx context.Context, bundleID int, resultChunks chan lsifstore.IndexedResultChunkData) error
	WriteDefinitions(ctx context.Context, bundleID int, monikerLocations chan lsifstore.MonikerLocations) error
	WriteReferences(ctx context.Context, bundleID int, monikerLocations chan lsifstore.MonikerLocations) error
}

type LSIFStoreShim struct {
	*lsifstore.Store
}

func (s *LSIFStoreShim) Transact(ctx context.Context) (LSIFStore, error) {
	tx, err := s.Store.Transact(ctx)
	if err != nil {
		return nil, err
	}

	return &LSIFStoreShim{tx}, nil
}

type GitserverClient interface {
	DirectoryChildren(ctx context.Context, store gitserver.DBStore, repositoryID int, commit string, dirnames []string) (map[string][]string, error)
}
