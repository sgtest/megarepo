package database

import (
	"context"
	"database/sql"

	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
)

// GitserverLocalCloneStore is used to migrate repos from one gitserver to another asynchronously.
type GitserverLocalCloneStore interface {
	basestore.ShareableStore
	With(other basestore.ShareableStore) GitserverLocalCloneStore
	Enqueue(ctx context.Context, repoID int, sourceHostname, destHostname string, deleteSource bool) (int, error)
}

type gitserverLocalCloneStore struct {
	*basestore.Store
}

// GitserverLocalClone instantiates and returns a new gitserverRepoStore.
func GitserverLocalClone(db DB) GitserverLocalCloneStore {
	return &gitserverLocalCloneStore{Store: basestore.NewWithDB(db, sql.TxOptions{})}
}

// NewGitserverLocalCloneStoreWith instantiates and returns a new gitserverLocalCloneStore using
// the other store handle.
func NewGitserverLocalCloneStoreWith(other basestore.ShareableStore) GitserverLocalCloneStore {
	return &gitserverLocalCloneStore{Store: basestore.NewWithHandle(other.Handle())}
}

func (s *gitserverLocalCloneStore) With(other basestore.ShareableStore) GitserverLocalCloneStore {
	return &gitserverLocalCloneStore{Store: s.Store.With(other)}
}

func (s *gitserverLocalCloneStore) Transact(ctx context.Context) (GitserverLocalCloneStore, error) {
	txBase, err := s.Store.Transact(ctx)
	return &gitserverLocalCloneStore{Store: txBase}, err
}

// Enqueue a local clone request.
func (s *gitserverLocalCloneStore) Enqueue(ctx context.Context, repoID int, sourceHostname string, destHostname string, deleteSource bool) (int, error) {
	var jobId int
	err := s.QueryRow(ctx, sqlf.Sprintf(`
-- source: internal/database/gitserver_localclone_jobs.go:gitserverLocalCloneStore.Enqueue
INSERT INTO
	gitserver_relocator_jobs (repo_id, source_hostname, dest_hostname, delete_source)
VALUES (%s, %s, %s, %s) RETURNING id
	`, repoID, sourceHostname, destHostname, deleteSource)).Scan(&jobId)
	if err != nil {
		return 0, err
	}

	return jobId, nil
}
