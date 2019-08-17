package graphqlbackend

import (
	"context"
	"sort"
	"strings"
	"time"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/pkg/vcs/git"
)

func (r *RepositoryResolver) Branches(ctx context.Context, args *struct {
	graphqlutil.ConnectionArgs
	Query   *string
	OrderBy *string
}) (*gitRefConnectionResolver, error) {
	gitRefTypeBranch := gitRefTypeBranch
	return r.GitRefs(ctx, &struct {
		graphqlutil.ConnectionArgs
		Query   *string
		Type    *string
		OrderBy *string
	}{ConnectionArgs: args.ConnectionArgs, Query: args.Query, Type: &gitRefTypeBranch, OrderBy: args.OrderBy})
}

func (r *RepositoryResolver) Tags(ctx context.Context, args *struct {
	graphqlutil.ConnectionArgs
	Query *string
}) (*gitRefConnectionResolver, error) {
	gitRefTypeTag := gitRefTypeTag
	return r.GitRefs(ctx, &struct {
		graphqlutil.ConnectionArgs
		Query   *string
		Type    *string
		OrderBy *string
	}{ConnectionArgs: args.ConnectionArgs, Query: args.Query, Type: &gitRefTypeTag})
}

func (r *RepositoryResolver) GitRefs(ctx context.Context, args *struct {
	graphqlutil.ConnectionArgs
	Query   *string
	Type    *string
	OrderBy *string
}) (*gitRefConnectionResolver, error) {
	var branches []*git.Branch
	if args.Type == nil || *args.Type == gitRefTypeBranch {
		cachedRepo, err := backend.CachedGitRepo(ctx, r.repo)
		if err != nil {
			return nil, err
		}
		branches, err = git.ListBranches(ctx, *cachedRepo, git.BranchesOptions{IncludeCommit: true})
		if err != nil {
			return nil, err
		}

		// Sort branches by most recently committed.
		if args.OrderBy != nil && *args.OrderBy == gitRefOrderAuthoredOrCommittedAt {
			date := func(c *git.Commit) time.Time {
				if c.Committer == nil {
					return c.Author.Date
				}
				if c.Committer.Date.After(c.Author.Date) {
					return c.Committer.Date
				}
				return c.Author.Date
			}
			sort.Slice(branches, func(i, j int) bool {
				bi, bj := branches[i], branches[j]
				if bi.Commit == nil {
					return false
				}
				if bj.Commit == nil {
					return true
				}
				di, dj := date(bi.Commit), date(bj.Commit)
				if di.Equal(dj) {
					return bi.Name < bj.Name
				}
				if di.After(dj) {
					return true
				}
				return false
			})
		}
	}

	var tags []*git.Tag
	if args.Type == nil || *args.Type == gitRefTypeTag {
		cachedRepo, err := backend.CachedGitRepo(ctx, r.repo)
		if err != nil {
			return nil, err
		}
		tags, err = git.ListTags(ctx, *cachedRepo)
		if err != nil {
			return nil, err
		}
		if args.OrderBy != nil && *args.OrderBy == gitRefOrderAuthoredOrCommittedAt {
			// Tags are already sorted by creatordate.
		} else {
			// Sort tags by reverse alpha.
			sort.Slice(tags, func(i, j int) bool {
				return tags[i].Name > tags[j].Name
			})
		}
	}

	// Combine branches and tags.
	refs := make([]*GitRefResolver, len(branches)+len(tags))
	for i, b := range branches {
		refs[i] = &GitRefResolver{name: "refs/heads/" + b.Name, repo: r, target: GitObjectID(b.Head)}
	}
	for i, t := range tags {
		refs[i+len(branches)] = &GitRefResolver{name: "refs/tags/" + t.Name, repo: r, target: GitObjectID(t.CommitID)}
	}

	if args.Query != nil {
		query := strings.ToLower(*args.Query)

		// Filter using query.
		filtered := refs[:0]
		for _, ref := range refs {
			if strings.Contains(strings.ToLower(strings.TrimPrefix(ref.name, gitRefPrefix(ref.name))), query) {
				filtered = append(filtered, ref)
			}
		}
		refs = filtered
	}

	return &gitRefConnectionResolver{
		first: args.First,
		refs:  refs,
		repo:  r,
	}, nil
}

type gitRefConnectionResolver struct {
	first *int32
	refs  []*GitRefResolver

	repo *RepositoryResolver
}

func (r *gitRefConnectionResolver) Nodes() []*GitRefResolver {
	var nodes []*GitRefResolver

	// Paginate.
	if r.first != nil && len(r.refs) > int(*r.first) {
		nodes = r.refs[:int(*r.first)]
	} else {
		nodes = r.refs
	}

	return nodes
}

func (r *gitRefConnectionResolver) TotalCount() int32 {
	return int32(len(r.refs))
}

func (r *gitRefConnectionResolver) PageInfo() *graphqlutil.PageInfo {
	return graphqlutil.HasNextPage(r.first != nil && int(*r.first) < len(r.refs))
}
