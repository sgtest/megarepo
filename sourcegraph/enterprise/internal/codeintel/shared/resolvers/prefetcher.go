package sharedresolvers

import (
	"context"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/shared"
	uploadsshared "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/shared"
)

// Prefetcher is a batch query utility and cache used to reduce the amount of database
// queries made by a tree of upload and index resolvers. A single prefetcher instance
// is shared by all sibling resolvers resulting from an upload or index connection, as
// well as index records resulting from an upload resolver (and vice versa).
type Prefetcher struct {
	uploadLoader *DataLoader[int, shared.Upload]
	indexLoader  *DataLoader[int, uploadsshared.Index]
}

type PrefetcherFactory struct {
	uploadSvc UploadsService
}

func NewPrefetcherFactory(uploadSvc UploadsService) *PrefetcherFactory {
	return &PrefetcherFactory{
		uploadSvc: uploadSvc,
	}
}

func (f *PrefetcherFactory) Create() *Prefetcher {
	return NewPrefetcher(f.uploadSvc)
}

// NewPrefetcher returns a prefetcher with an empty cache.
func NewPrefetcher(uploadSvc UploadsService) *Prefetcher {
	return &Prefetcher{
		uploadLoader: NewDataLoader[int, shared.Upload](DataLoaderBackingServiceFunc[int, shared.Upload](func(ctx context.Context, ids ...int) ([]shared.Upload, error) {
			return uploadSvc.GetUploadsByIDs(ctx, ids...)
		})),
		indexLoader: NewDataLoader[int, uploadsshared.Index](DataLoaderBackingServiceFunc[int, uploadsshared.Index](func(ctx context.Context, ids ...int) ([]uploadsshared.Index, error) {
			return uploadSvc.GetIndexesByIDs(ctx, ids...)
		})),
	}
}

// MarkUpload adds the given identifier to the next batch of uploads to fetch.
func (p *Prefetcher) MarkUpload(id int) {
	p.uploadLoader.Presubmit(id)
}

// GetUploadByID will return an upload with the given identifier as well as a boolean
// flag indicating such a record's existence. If the given ID has already been fetched
// by another call to GetUploadByID, that record is returned immediately. Otherwise,
// the given identifier will be added to the current batch of identifiers constructed
// via calls to MarkUpload. All uploads will in the current batch are requested at once
// and the upload with the given identifier is returned from that result set.
func (p *Prefetcher) GetUploadByID(ctx context.Context, id int) (shared.Upload, bool, error) {
	return p.uploadLoader.GetByID(ctx, id)
}

// MarkIndex adds the given identifier to the next batch of indexes to fetch.
func (p *Prefetcher) MarkIndex(id int) {
	p.indexLoader.Presubmit(id)
}

// GetIndexByID will return an index with the given identifier as well as a boolean
// flag indicating such a record's existence. If the given ID has already been fetched
// by another call to GetIndexByID, that record is returned immediately. Otherwise,
// the given identifier will be added to the current batch of identifiers constructed
// via calls to MarkIndex. All indexes will in the current batch are requested at once
// and the index with the given identifier is returned from that result set.
func (p *Prefetcher) GetIndexByID(ctx context.Context, id int) (uploadsshared.Index, bool, error) {
	return p.indexLoader.GetByID(ctx, id)
}
