package codenav

import (
	"sync"

	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/codenav/shared"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

type RequestState struct {
	// Local Caches
	dataLoader        *UploadsDataLoader
	GitTreeTranslator GitTreeTranslator
	commitCache       CommitCache
	// maximumIndexesPerMonikerSearch configures the maximum number of reference upload identifiers
	// that can be passed to a single moniker search query. Previously this limit was meant to keep
	// the number of SQLite files we'd have to open within a single call relatively low. Since we've
	// migrated to Postgres this limit is not a concern. Now we only want to limit these values
	// based on the number of elements we can pass to an IN () clause in the codeintel-db, as well
	// as the size required to encode them in a user-facing pagination cursor.
	maximumIndexesPerMonikerSearch int

	authChecker authz.SubRepoPermissionChecker
}

func NewRequestState(
	uploads []shared.Dump,
	authChecker authz.SubRepoPermissionChecker,
	gitclient shared.GitserverClient, repo *types.Repo, commit, path string,
	maxIndexes int,
	hunkCacheSize int,
) RequestState {
	r := &RequestState{}
	r.SetUploadsDataLoader(uploads)
	r.SetAuthChecker(authChecker)
	r.SetLocalGitTreeTranslator(gitclient, repo, commit, path, hunkCacheSize)
	r.SetLocalCommitCache(gitclient)
	r.SetMaximumIndexesPerMonikerSearch(maxIndexes)

	return *r
}

func (r RequestState) GetCacheUploads() []shared.Dump {
	return r.dataLoader.uploads
}

func (r RequestState) GetCacheUploadsAtIndex(index int) shared.Dump {
	if index < 0 || index >= len(r.dataLoader.uploads) {
		return shared.Dump{}
	}

	return r.dataLoader.uploads[index]
}

func (r *RequestState) SetAuthChecker(authChecker authz.SubRepoPermissionChecker) {
	r.authChecker = authChecker
}

func (r *RequestState) SetUploadsDataLoader(uploads []shared.Dump) {
	r.dataLoader = NewUploadsDataLoader()
	for _, upload := range uploads {
		r.dataLoader.AddUpload(upload)
	}
}

func (r *RequestState) SetLocalGitTreeTranslator(client shared.GitserverClient, repo *types.Repo, commit, path string, hunkCacheSize int) error {
	hunkCache, err := NewHunkCache(hunkCacheSize)
	if err != nil {
		return err
	}

	args := &requestArgs{
		repo:   repo,
		commit: commit,
		path:   path,
	}

	r.GitTreeTranslator = NewGitTreeTranslator(client, args, hunkCache)

	return nil
}

func (r *RequestState) SetLocalCommitCache(client shared.GitserverClient) {
	r.commitCache = NewCommitCache(client)
}

func (r *RequestState) SetMaximumIndexesPerMonikerSearch(maxNumber int) {
	r.maximumIndexesPerMonikerSearch = maxNumber
}

type UploadsDataLoader struct {
	uploads     []shared.Dump
	uploadsByID map[int]shared.Dump
	cacheMutex  sync.RWMutex
}

func NewUploadsDataLoader() *UploadsDataLoader {
	return &UploadsDataLoader{
		uploadsByID: make(map[int]shared.Dump),
	}
}

func (l *UploadsDataLoader) GetUploadFromCacheMap(id int) (shared.Dump, bool) {
	l.cacheMutex.RLock()
	defer l.cacheMutex.RUnlock()

	upload, ok := l.uploadsByID[id]
	return upload, ok
}

func (l *UploadsDataLoader) SetUploadInCacheMap(uploads []shared.Dump) {
	l.cacheMutex.Lock()
	defer l.cacheMutex.Unlock()

	for i := range uploads {
		l.uploadsByID[uploads[i].ID] = uploads[i]
	}
}

func (l *UploadsDataLoader) AddUpload(dump shared.Dump) {
	l.cacheMutex.Lock()
	defer l.cacheMutex.Unlock()

	l.uploads = append(l.uploads, dump)
	l.uploadsByID[dump.ID] = dump
}
