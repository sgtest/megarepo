package graphqlbackend

import (
	"context"

	"github.com/graph-gophers/graphql-go"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
)

type ListLockfileIndexesArgs struct {
	First int32
	After *string
}

type DependenciesResolver interface {
	LockfileIndexes(ctx context.Context, args *ListLockfileIndexesArgs) (LockfileIndexConnectionResolver, error)

	NodeResolvers() map[string]NodeByIDFunc
}

type LockfileIndexConnectionResolver interface {
	Nodes(ctx context.Context) []LockfileIndexResolver
	TotalCount(ctx context.Context) int32
	PageInfo(ctx context.Context) *graphqlutil.PageInfo
}

type LockfileIndexResolver interface {
	ID() graphql.ID
	Lockfile() string
	Repository() *RepositoryResolver
	Commit() *GitCommitResolver
	Fidelity() string
}
