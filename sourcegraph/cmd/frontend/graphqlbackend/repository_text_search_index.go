package graphqlbackend

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/google/zoekt"
	zoektquery "github.com/google/zoekt/query"
)

func (r *repositoryResolver) TextSearchIndex() *repositoryTextSearchIndexResolver {
	if zoektCl == nil {
		return nil
	}
	return &repositoryTextSearchIndexResolver{repo: r}
}

type repositoryTextSearchIndexResolver struct {
	repo *repositoryResolver

	once  sync.Once
	entry *zoekt.RepoListEntry
	err   error
}

func (r *repositoryTextSearchIndexResolver) resolve(ctx context.Context) (*zoekt.RepoListEntry, error) {
	r.once.Do(func() {
		repoList, err := zoektCl.List(ctx, zoektquery.NewRepoSet(string(r.repo.repo.URI)))
		if err != nil {
			r.err = err
			return
		}
		if len(repoList.Repos) > 1 {
			r.err = fmt.Errorf("more than 1 indexed repo found for %q", r.repo.repo.URI)
			return
		}
		if len(repoList.Repos) == 1 {
			r.entry = repoList.Repos[0]
		}
	})
	return r.entry, r.err
}

func (r *repositoryTextSearchIndexResolver) Repository() *repositoryResolver { return r.repo }

func (r *repositoryTextSearchIndexResolver) Status(ctx context.Context) (*repositoryTextSearchIndexStatus, error) {
	entry, err := r.resolve(ctx)
	if err != nil {
		return nil, err
	}
	if entry == nil {
		return nil, nil
	}
	return &repositoryTextSearchIndexStatus{entry: *entry}, nil
}

type repositoryTextSearchIndexStatus struct {
	entry zoekt.RepoListEntry
}

func (r *repositoryTextSearchIndexStatus) UpdatedAt() string {
	return r.entry.IndexMetadata.IndexTime.Format(time.RFC3339)
}
func (r *repositoryTextSearchIndexStatus) ContentByteSize() int32 {
	return int32(r.entry.Stats.ContentBytes)
}
func (r *repositoryTextSearchIndexStatus) ContentFilesCount() int32 {
	return int32(r.entry.Stats.Documents)
}
func (r *repositoryTextSearchIndexStatus) IndexByteSize() int32 {
	return int32(r.entry.Stats.IndexBytes)
}
func (r *repositoryTextSearchIndexStatus) IndexShardsCount() int32 {
	return int32(r.entry.Stats.Shards + 1)
}

func (r *repositoryTextSearchIndexResolver) Refs(ctx context.Context) ([]*repositoryTextSearchIndexedRef, error) {
	// We assume that the default branch for enabled repositories is always configured to be indexed.
	//
	// TODO(sqs): support configuring which branches should be indexed (add'l branches, not default branch, etc.).
	defaultBranchRef, err := r.repo.DefaultBranch(ctx)
	if err != nil {
		return nil, err
	}
	if defaultBranchRef == nil {
		return []*repositoryTextSearchIndexedRef{}, nil
	}
	refNames := []string{defaultBranchRef.name}

	refs := make([]*repositoryTextSearchIndexedRef, len(refNames))
	for i, refName := range refNames {
		refs[i] = &repositoryTextSearchIndexedRef{ref: &gitRefResolver{name: refName, repo: r.repo}}
	}
	refByName := func(refName string) *repositoryTextSearchIndexedRef {
		for _, ref := range refs {
			if ref.ref.name == refName {
				return ref
			}
		}

		// If Zoekt reports it has another indexed branch, include that.
		newRef := &repositoryTextSearchIndexedRef{ref: &gitRefResolver{name: refName, repo: r.repo}}
		refs = append(refs, newRef)
		return newRef
	}

	entry, err := r.resolve(ctx)
	if err != nil {
		return nil, err
	}
	if entry != nil {
		for _, branch := range entry.Repository.Branches {
			name := "refs/heads/" + branch.Name
			if branch.Name == "HEAD" {
				name = defaultBranchRef.name
			}
			ref := refByName(name)
			ref.indexedCommit = gitObjectID(branch.Version)
		}
	}
	return refs, nil
}

type repositoryTextSearchIndexedRef struct {
	ref           *gitRefResolver
	indexedCommit gitObjectID
}

func (r *repositoryTextSearchIndexedRef) Ref() *gitRefResolver { return r.ref }
func (r *repositoryTextSearchIndexedRef) Indexed() bool        { return r.indexedCommit != "" }

func (r *repositoryTextSearchIndexedRef) Current(ctx context.Context) (bool, error) {
	if r.indexedCommit == "" {
		return false, nil
	}

	target, err := r.ref.Target().OID(ctx)
	if err != nil {
		return false, err
	}
	return target == r.indexedCommit, nil
}

func (r *repositoryTextSearchIndexedRef) IndexedCommit() *gitObject {
	if r.indexedCommit == "" {
		return nil
	}
	return &gitObject{repo: r.ref.repo, oid: r.indexedCommit, typ: gitObjectTypeCommit}
}
