package graphqlbackend

import (
	"context"
	"math"
	"strconv"
	"sync"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type repositoryContributorsArgs struct {
	RevisionRange *string
	AfterDate     *string
	Path          *string
}

func (r *RepositoryResolver) Contributors(args *struct {
	repositoryContributorsArgs
	graphqlutil.ConnectionResolverArgs
}) (*graphqlutil.ConnectionResolver[*repositoryContributorResolver], error) {
	connectionStore := &repositoryContributorConnectionStore{
		db:   r.db,
		args: &args.repositoryContributorsArgs,
		repo: r,
	}
	reverse := false
	connectionOptions := graphqlutil.ConnectionResolverOptions{
		Reverse: &reverse,
	}
	return graphqlutil.NewConnectionResolver[*repositoryContributorResolver](connectionStore, &args.ConnectionResolverArgs, &connectionOptions)
}

type repositoryContributorConnectionStore struct {
	db   database.DB
	args *repositoryContributorsArgs

	repo *RepositoryResolver

	// cache result because it is used by multiple fields
	once    sync.Once
	results []*gitdomain.ContributorCount
	err     error
}

func (s *repositoryContributorConnectionStore) MarshalCursor(node *repositoryContributorResolver, _ database.OrderBy) (*string, error) {
	position := strconv.Itoa(node.index)
	return &position, nil
}

func (s *repositoryContributorConnectionStore) UnmarshalCursor(cursor string, _ database.OrderBy) (*string, error) {
	return &cursor, nil
}

func (s *repositoryContributorConnectionStore) ComputeTotal(ctx context.Context) (*int32, error) {
	results, err := s.compute(ctx)
	num := int32(len(results))
	return &num, err
}

func (s *repositoryContributorConnectionStore) ComputeNodes(ctx context.Context, args *database.PaginationArgs) ([]*repositoryContributorResolver, error) {
	results, err := s.compute(ctx)
	if err != nil {
		return nil, err
	}

	var start int
	results, start, err = OffsetBasedCursorSlice(results, args)
	if err != nil {
		return nil, err
	}

	resolvers := make([]*repositoryContributorResolver, len(results))
	for i, contributor := range results {
		resolvers[i] = &repositoryContributorResolver{
			db:    s.db,
			name:  contributor.Name,
			email: contributor.Email,
			count: contributor.Count,
			repo:  s.repo,
			args:  *s.args,
			index: start + i,
		}
	}

	return resolvers, nil
}

func (s *repositoryContributorConnectionStore) compute(ctx context.Context) ([]*gitdomain.ContributorCount, error) {
	s.once.Do(func() {
		client := gitserver.NewClient()
		var opt gitserver.ContributorOptions
		if s.args.RevisionRange != nil {
			opt.Range = *s.args.RevisionRange
		}
		if s.args.Path != nil {
			opt.Path = *s.args.Path
		}
		if s.args.AfterDate != nil {
			opt.After = *s.args.AfterDate
		}
		s.results, s.err = client.ContributorCount(ctx, s.repo.RepoName(), opt)
	})
	return s.results, s.err
}

func OffsetBasedCursorSlice[T any](nodes []T, args *database.PaginationArgs) ([]T, int, error) {
	start := 0
	end := 0
	totalFloat := float64(len(nodes))
	if args.First != nil {
		if args.After != nil {
			after, err := strconv.Atoi(*args.After)
			if err != nil {
				return nil, 0, err
			}
			start = int(math.Min(float64(after)+1, totalFloat))
		}
		end = int(math.Min(float64(start+*args.First), totalFloat))
	} else if args.Last != nil {
		end = int(totalFloat)
		if args.Before != nil {
			before, err := strconv.Atoi(*args.Before)
			if err != nil {
				return nil, 0, err
			}
			end = int(math.Max(float64(before), 0))
		}
		start = int(math.Max(float64(end-*args.Last), 0))
	} else {
		return nil, 0, errors.New(`args.First and args.Last are nil`)
	}

	nodes = nodes[start:end]

	return nodes, start, nil
}
