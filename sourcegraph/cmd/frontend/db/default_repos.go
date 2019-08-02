package db

import (
	"context"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/db/dbconn"
)

type defaultRepos struct{}

func (s *defaultRepos) List(ctx context.Context) (results []*types.Repo, err error) {
	const q = `
SELECT default_repos.repo_id, repo.name
FROM default_repos
JOIN repo
ON default_repos.repo_id = repo.id
`
	rows, err := dbconn.Global.QueryContext(ctx, q)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var repos []*types.Repo
	for rows.Next() {
		var r types.Repo
		if err := rows.Scan(&r.ID, &r.Name); err != nil {
			return nil, err
		}
		repos = append(repos, &r)
	}
	if err = rows.Err(); err != nil {
		return nil, err
	}
	return repos, nil
}
