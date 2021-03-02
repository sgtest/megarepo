package database

import (
	"context"
	"database/sql"

	"github.com/keegancsmith/sqlf"
	"github.com/pkg/errors"

	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

// GitserverReposStore is responsible for data stored in the gitserver_repos table.
type GitserverRepoStore struct {
	*basestore.Store
}

// GitserverRepos instantiates and returns a new GitserverRepoStore.
func GitserverRepos(db dbutil.DB) *GitserverRepoStore {
	return &GitserverRepoStore{Store: basestore.NewWithDB(db, sql.TxOptions{})}
}

// NewGitserverReposWith instantiates and returns a new GitserverRepoStore using
// the other store handle.
func NewGitserverReposWith(other basestore.ShareableStore) *GitserverRepoStore {
	return &GitserverRepoStore{Store: basestore.NewWithHandle(other.Handle())}
}

func (s *GitserverRepoStore) With(other basestore.ShareableStore) *GitserverRepoStore {
	return &GitserverRepoStore{Store: s.Store.With(other)}
}

func (s *GitserverRepoStore) Transact(ctx context.Context) (*GitserverRepoStore, error) {
	txBase, err := s.Store.Transact(ctx)
	return &GitserverRepoStore{Store: txBase}, err
}

// Create adds a row representing the GitServer status of a repo
func (s *GitserverRepoStore) Create(ctx context.Context, gr *types.GitserverRepo) error {
	err := s.Exec(ctx, sqlf.Sprintf(`
INSERT INTO gitserver_repos(repo_id, clone_status, shard_id, last_external_service, last_error) VALUES (%s,%s,%s,%s,%s)
`, gr.RepoID, gr.CloneStatus, dbutil.NewNullString(gr.ShardID), dbutil.NewNullInt64(gr.LastExternalService), dbutil.NewNullString(gr.LastError)))

	return errors.Wrap(err, "creating GitserverRepo")
}

// IterateRepoGitserverStatus iterates over the status of all repos by joining
// our repo and gitserver_repos table. It is possible for us not to have a
// corresponding row in gitserver_repos yet. repoFn will be called once for each
// row. If it returns an error we'll abort iteration.
func (s *GitserverRepoStore) IterateRepoGitserverStatus(ctx context.Context, repoFn func(repo types.RepoGitserverStatus) error) error {
	if repoFn == nil {
		return errors.New("nil repoFn")
	}

	q := `
SELECT repo.id,
       repo.name,
       gr.clone_status,
       gr.shard_id,
       gr.last_external_service,
       gr.last_error,
       gr.updated_at
FROM repo 
    LEFT JOIN gitserver_repos gr ON gr.repo_id = repo.id
ORDER BY repo.id ASC
`

	rows, err := s.Query(ctx, sqlf.Sprintf(q))
	if err != nil {
		return errors.Wrap(err, "fetching gitserver status")
	}
	defer rows.Close()

	for rows.Next() {
		var rgs types.RepoGitserverStatus
		var gr types.GitserverRepo
		var cloneStatus string

		if err := rows.Scan(
			&rgs.ID,
			&rgs.Name,
			&dbutil.NullString{S: &cloneStatus},
			&dbutil.NullString{S: &gr.ShardID},
			&dbutil.NullInt64{N: &gr.LastExternalService},
			&dbutil.NullString{S: &gr.LastError},
			&dbutil.NullTime{Time: &gr.UpdatedAt},
		); err != nil {
			return errors.Wrap(err, "scanning row")
		}

		// Clone status will only be null if we don't have a corresponding row in
		// gitserver_repos
		if cloneStatus != "" {
			gr.CloneStatus = types.ParseCloneStatus(cloneStatus)
			rgs.GitserverRepo = &gr
		}

		err := repoFn(rgs)
		if err != nil {
			// Abort
			return errors.Wrap(err, "calling repoFn")
		}
	}

	if rows.Err() != nil {
		return errors.Wrap(rows.Err(), "iterating rows")
	}

	return nil
}
