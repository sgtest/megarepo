package sharedresolvers

import (
	"context"
	"sort"
	"sync"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/shared/types"
)

// Prefetcher is a batch query utility and cache used to reduce the amount of database
// queries made by a tree of upload and index resolvers. A single prefetcher instance
// is shared by all sibling resolvers resulting from an upload or index connection, as
// well as index records resulting from an upload resolver (and vice versa).
type Prefetcher struct {
	sync.RWMutex
	autoindexingSvc AutoIndexingService
	uploadSvc       UploadsService
	uploadIDs       []int
	indexIDs        []int
	uploadCache     map[int]types.Upload
	indexCache      map[int]types.Index
}

// NewPrefetcher returns a prefetcher with an empty cache.
func NewPrefetcher(autoindexingSvc AutoIndexingService, uploadSvc UploadsService) *Prefetcher {
	return &Prefetcher{
		autoindexingSvc: autoindexingSvc,
		uploadSvc:       uploadSvc,
		uploadCache:     map[int]types.Upload{},
		indexCache:      map[int]types.Index{},
	}
}

// MarkUpload adds the given identifier to the next batch of uploads to fetch.
func (p *Prefetcher) MarkUpload(id int) {
	p.Lock()
	p.uploadIDs = append(p.uploadIDs, id)
	p.Unlock()
}

// GetUploadByID will return an upload with the given identifier as well as a boolean
// flag indicating such a record's existence. If the given ID has already been fetched
// by another call to GetUploadByID, that record is returned immediately. Otherwise,
// the given identifier will be added to the current batch of identifiers constructed
// via calls to MarkUpload. All uploads will in the current batch are requested at once
// and the upload with the given identifier is returned from that result set.
func (p *Prefetcher) GetUploadByID(ctx context.Context, id int) (types.Upload, bool, error) {
	p.RLock()
	upload, ok := p.uploadCache[id]
	p.RUnlock()
	if ok {
		return upload, true, nil
	}

	p.Lock()
	defer p.Unlock()

	if upload, ok := p.uploadCache[id]; ok {
		return upload, true, nil
	}

	m := map[int]struct{}{}
	for _, x := range append(p.uploadIDs, id) {
		if _, ok := p.uploadCache[x]; !ok {
			m[x] = struct{}{}
		}
	}
	ids := make([]int, 0, len(m))
	for x := range m {
		ids = append(ids, x)
	}
	sort.Ints(ids)

	uploads, err := p.uploadSvc.GetUploadsByIDs(ctx, ids...)
	if err != nil {
		return types.Upload{}, false, err
	}

	for _, upload := range uploads {
		p.uploadCache[upload.ID] = upload
	}
	p.uploadIDs = nil

	upload, ok = p.uploadCache[id]
	return upload, ok, nil
}

// MarkIndex adds the given identifier to the next batch of indexes to fetch.
func (p *Prefetcher) MarkIndex(id int) {
	p.Lock()
	p.indexIDs = append(p.indexIDs, id)
	p.Unlock()
}

// GetIndexByID will return an index with the given identifier as well as a boolean
// flag indicating such a record's existence. If the given ID has already been fetched
// by another call to GetIndexByID, that record is returned immediately. Otherwise,
// the given identifier will be added to the current batch of identifiers constructed
// via calls to MarkIndex. All indexes will in the current batch are requested at once
// and the index with the given identifier is returned from that result set.
func (p *Prefetcher) GetIndexByID(ctx context.Context, id int) (types.Index, bool, error) {
	p.RLock()
	index, ok := p.indexCache[id]
	p.RUnlock()
	if ok {
		return index, true, nil
	}

	p.Lock()
	defer p.Unlock()

	if index, ok := p.indexCache[id]; ok {
		return index, true, nil
	}

	m := map[int]struct{}{}
	for _, x := range append(p.indexIDs, id) {
		if _, ok := p.indexCache[x]; !ok {
			m[x] = struct{}{}
		}
	}
	ids := make([]int, 0, len(m))
	for x := range m {
		ids = append(ids, x)
	}
	sort.Ints(ids)

	indexes, err := p.autoindexingSvc.GetIndexesByIDs(ctx, ids...)
	if err != nil {
		return types.Index{}, false, err
	}
	for _, index := range indexes {
		p.indexCache[index.ID] = index
	}
	p.indexIDs = nil

	index, ok = p.indexCache[id]
	return index, ok, nil
}
