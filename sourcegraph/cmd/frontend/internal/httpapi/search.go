package httpapi

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"

	"github.com/google/zoekt"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	searchbackend "github.com/sourcegraph/sourcegraph/internal/search/backend"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
	"github.com/sourcegraph/sourcegraph/schema"
)

func repoRankFromConfig(siteConfig schema.SiteConfiguration, repoName string) float64 {
	val := 0.0
	if siteConfig.ExperimentalFeatures == nil || siteConfig.ExperimentalFeatures.Ranking == nil {
		return val
	}
	scores := siteConfig.ExperimentalFeatures.Ranking.RepoScores
	if len(scores) == 0 {
		return val
	}
	// try every "directory" in the repo name to assign it a value, so a repoName like
	// "github.com/sourcegraph/zoekt" will have "github.com", "github.com/sourcegraph",
	// and "github.com/sourcegraph/zoekt" tested.
	for i := 0; i < len(repoName); i++ {
		if repoName[i] == '/' {
			val += scores[repoName[:i]]
		}
	}
	val += scores[repoName]
	return val
}

// serveSearchConfiguration is _only_ used by the zoekt index server. Zoekt does
// not depend on frontend and therefore does not have access to `conf.Watch`.
// Additionally, it only cares about certain search specific settings so this
// search specific endpoint is used rather than serving the entire site settings
// from /.internal/configuration.
//
// This endpoint also supports batch requests to avoid managing concurrency in
// zoekt. On vertically scaled instances we have observed zoekt requesting
// this endpoint concurrently leading to socket starvation.
//
// A repo can be specified via name ("repo") or id ("repoID").
func serveSearchConfiguration(db dbutil.DB) func(http.ResponseWriter, *http.Request) error {
	return func(w http.ResponseWriter, r *http.Request) error {
		ctx := r.Context()
		siteConfig := conf.Get().SiteConfiguration

		if err := r.ParseForm(); err != nil {
			return err
		}

		indexedIDs := make([]api.RepoID, 0, len(r.Form["repoID"]))
		for _, idStr := range r.Form["repoID"] {
			id, err := strconv.Atoi(idStr)
			if err != nil {
				http.Error(w, fmt.Sprintf("invalid repo id %s: %s", idStr, err), http.StatusBadRequest)
				return nil
			}
			indexedIDs = append(indexedIDs, api.RepoID(id))
		}

		if len(indexedIDs) == 0 {
			http.Error(w, "atleast one repoID required", http.StatusBadRequest)
			return nil
		}

		// Preload repos to support fast lookups by repo ID.
		repos, loadReposErr := database.Repos(db).List(ctx, database.ReposListOptions{
			IDs: indexedIDs,
		})
		reposMap := make(map[api.RepoID]*types.Repo, len(repos))
		for _, repo := range repos {
			reposMap[repo.ID] = repo
		}

		getRepoIndexOptions := func(repoID int32) (*searchbackend.RepoIndexOptions, error) {
			if loadReposErr != nil {
				return nil, loadReposErr
			}
			// Replicate what database.Repos.GetByName would do here:
			repo, ok := reposMap[api.RepoID(repoID)]
			if !ok {
				return nil, &database.RepoNotFoundErr{ID: api.RepoID(repoID)}
			}

			getVersion := func(branch string) (string, error) {
				// Do not to trigger a repo-updater lookup since this is a batch job.
				commitID, err := git.ResolveRevision(ctx, repo.Name, branch, git.ResolveRevisionOptions{})
				if err != nil && errcode.HTTP(err) == http.StatusNotFound {
					// GetIndexOptions wants an empty rev for a missing rev or empty
					// repo.
					return "", nil
				}
				return string(commitID), err
			}

			priority := float64(repo.Stars) + repoRankFromConfig(siteConfig, string(repo.Name))

			return &searchbackend.RepoIndexOptions{
				Name:       string(repo.Name),
				RepoID:     int32(repo.ID),
				Public:     !repo.Private,
				Priority:   priority,
				Fork:       repo.Fork,
				Archived:   repo.Archived,
				GetVersion: getVersion,
			}, nil
		}

		revisionsForRepo, revisionsForRepoErr := database.SearchContexts(db).GetAllRevisionsForRepos(ctx, indexedIDs)
		getSearchContextRevisions := func(repoID int32) ([]string, error) {
			if revisionsForRepoErr != nil {
				return nil, revisionsForRepoErr
			}
			return revisionsForRepo[api.RepoID(repoID)], nil
		}

		// searchbackend uses int32 instead of api.RepoID currently, so build
		// up a slice of that.
		repoIDs := make([]int32, len(indexedIDs))
		for i := range indexedIDs {
			repoIDs[i] = int32(indexedIDs[i])
		}

		b := searchbackend.GetIndexOptions(&siteConfig, getRepoIndexOptions, getSearchContextRevisions, repoIDs...)
		_, _ = w.Write(b)
		return nil
	}
}

type reposListServer struct {
	// ListIndexable returns the repositories to index.
	ListIndexable func(context.Context) ([]types.MinimalRepo, error)

	StreamMinimalRepos func(context.Context, database.ReposListOptions, func(*types.MinimalRepo)) error

	// Indexers is the subset of searchbackend.Indexers methods we
	// use. reposListServer is used by indexed-search to get the list of
	// repositories to index. These methods are used to return the correct
	// subset for horizontal indexed search. Declared as an interface for
	// testing.
	Indexers interface {
		// ReposSubset returns the subset of repoNames that hostname should
		// index.
		ReposSubset(ctx context.Context, hostname string, indexed map[uint32]*zoekt.MinimalRepoListEntry, indexable []types.MinimalRepo) ([]types.MinimalRepo, error)
		// Enabled is true if horizontal indexed search is enabled.
		Enabled() bool
	}
}

// serveIndex is used by zoekt to get the list of repositories for it to
// index.
func (h *reposListServer) serveIndex(w http.ResponseWriter, r *http.Request) error {
	var opt struct {
		// Hostname is used to determine the subset of repos to return
		Hostname string
		// IndexedIDs are the repository IDs of indexed repos by Hostname.
		IndexedIDs []api.RepoID
	}

	err := json.NewDecoder(r.Body).Decode(&opt)
	if err != nil {
		return err
	}

	indexable, err := h.ListIndexable(r.Context())
	if err != nil {
		return err
	}

	if h.Indexers.Enabled() {
		indexed := make(map[uint32]*zoekt.MinimalRepoListEntry, len(opt.IndexedIDs))
		err = h.StreamMinimalRepos(r.Context(), database.ReposListOptions{
			IDs: opt.IndexedIDs,
		}, func(r *types.MinimalRepo) { indexed[uint32(r.ID)] = nil })

		if err != nil {
			return err
		}

		indexable, err = h.Indexers.ReposSubset(r.Context(), opt.Hostname, indexed, indexable)
		if err != nil {
			return err
		}
	}

	// TODO: Avoid batching up so much in memory by:
	// 1. Changing the schema from object of arrays to array of objects.
	// 2. Stream out each object marshalled rather than marshall the full list in memory.

	names := make([]string, 0, len(indexable))
	ids := make([]api.RepoID, 0, len(indexable))

	for _, r := range indexable {
		names = append(names, string(r.Name))
		ids = append(ids, r.ID)
	}

	data := struct {
		RepoNames []string
		RepoIDs   []api.RepoID
	}{
		RepoNames: names,
		RepoIDs:   ids,
	}

	return json.NewEncoder(w).Encode(&data)
}
