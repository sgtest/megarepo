package commitgraph

import (
	"context"
	"time"

	"github.com/sourcegraph/sourcegraph/internal/database/locker"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
)

type DBStore interface {
	DirtyRepositories(ctx context.Context) (map[int]int, error)
	CalculateVisibleUploads(
		ctx context.Context,
		repositoryID int,
		graph *gitdomain.CommitGraph,
		refDescriptions map[string][]gitdomain.RefDescription,
		maxAgeForNonStaleBranches, maxAgeForNonStaleTags time.Duration,
		dirtyToken int,
	) error
	GetOldestCommitDate(ctx context.Context, repositoryID int) (time.Time, bool, error)
}

type Locker interface {
	Lock(ctx context.Context, key int32, blocking bool) (bool, locker.UnlockFunc, error)
}

type GitserverClient interface {
	RefDescriptions(ctx context.Context, repositoryID int) (map[string][]gitdomain.RefDescription, error)
	CommitGraph(ctx context.Context, repositoryID int, options git.CommitGraphOptions) (*gitdomain.CommitGraph, error)
}
