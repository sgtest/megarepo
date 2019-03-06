// Package repoupdater implements the repo-updater service HTTP handler.
package repoupdater

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strings"
	"time"

	"github.com/sourcegraph/sourcegraph/cmd/repo-updater/repos"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/errcode"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/awscodecommit"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/github"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/gitlab"
	"github.com/sourcegraph/sourcegraph/pkg/repoupdater/protocol"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

// Server is a repoupdater server.
type Server struct {
	repos.Store
	*repos.Syncer
	*repos.OtherReposSyncer
}

// Handler returns the http.Handler that should be used to serve requests.
func (s *Server) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("/repo-update-scheduler-info", s.handleRepoUpdateSchedulerInfo)
	mux.HandleFunc("/repo-lookup", s.handleRepoLookup)
	mux.HandleFunc("/enqueue-repo-update", s.handleEnqueueRepoUpdate)
	mux.HandleFunc("/sync-external-service", s.handleExternalServiceSync)
	return mux
}

func (s *Server) handleRepoUpdateSchedulerInfo(w http.ResponseWriter, r *http.Request) {
	var args protocol.RepoUpdateSchedulerInfoArgs
	if err := json.NewDecoder(r.Body).Decode(&args); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	result := repos.Scheduler.ScheduleInfo(args.RepoName)
	if err := json.NewEncoder(w).Encode(result); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
}

func (s *Server) handleRepoLookup(w http.ResponseWriter, r *http.Request) {
	var args protocol.RepoLookupArgs
	if err := json.NewDecoder(r.Body).Decode(&args); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	t := time.Now()
	result, err := s.repoLookup(r.Context(), args)
	if err != nil {
		if err == context.Canceled {
			http.Error(w, "request canceled", http.StatusGatewayTimeout)
			return
		}
		log15.Error("repoLookup failed", "args", &args, "error", err)
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	log15.Debug("TRACE repoLookup", "args", &args, "result", result, "duration", time.Since(t))

	if err := json.NewEncoder(w).Encode(result); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
}

func (s *Server) handleEnqueueRepoUpdate(w http.ResponseWriter, r *http.Request) {
	var req protocol.RepoUpdateRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	repos.Scheduler.UpdateOnce(req.Repo, req.URL)
}

func (s *Server) handleExternalServiceSync(w http.ResponseWriter, r *http.Request) {
	var req protocol.ExternalServiceSyncRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	if req.ExternalService.Kind == "OTHER" {
		res := s.OtherReposSyncer.Sync(r.Context(), &req.ExternalService)
		if len(res.Errors) > 0 {
			log15.Error("server.external-service-sync", "kind", req.ExternalService.Kind, "error", res.Errors)
			http.Error(w, res.Errors.Error(), http.StatusInternalServerError)
		}
		return
	}

	if s.Syncer == nil {
		log15.Debug("server.external-service-sync", "syncer", "disabled")
		return
	}

	switch req.ExternalService.Kind {
	case "GITHUB":
		_, err := s.Syncer.Sync(r.Context(), req.ExternalService.Kind)
		switch {
		case err == nil:
			log15.Info("server.external-service-sync", "synced", req.ExternalService.Kind)
			_ = json.NewEncoder(w).Encode(&protocol.ExternalServiceSyncResult{
				ExternalService: req.ExternalService,
				Error:           err,
			})
		default:
			log15.Error("server.external-service-sync", "kind", req.ExternalService.Kind, "error", err)
			http.Error(w, err.Error(), http.StatusInternalServerError)
		}
	default:
		// TODO(tsenart): Handle other external service kinds.
	}
}

var mockRepoLookup func(protocol.RepoLookupArgs) (*protocol.RepoLookupResult, error)

func (s *Server) repoLookup(ctx context.Context, args protocol.RepoLookupArgs) (*protocol.RepoLookupResult, error) {
	if args.Repo == "" && args.ExternalRepo == nil {
		return nil, errors.New("at least one of Repo and ExternalRepo must be set (both are empty)")
	}

	if mockRepoLookup != nil {
		return mockRepoLookup(args)
	}

	type getfn struct {
		kind string
		fn   func(context.Context, protocol.RepoLookupArgs) (*protocol.RepoInfo, bool, error)
	}

	fns := []getfn{
		// We begin by searching the "OTHER" external service kind repos because lookups
		// are fast in-memory only operations, as opposed to the other external service kinds which
		// don't *always* have enough metadata cached to answer this request without performing network
		// requests to their respective code host APIs
		{"OTHER", func(ctx context.Context, args protocol.RepoLookupArgs) (*protocol.RepoInfo, bool, error) {
			r := s.OtherReposSyncer.GetRepoInfoByName(ctx, string(args.Repo))
			return r, r != nil, nil
		}},
	}

	if s.Store != nil && s.Syncer != nil {
		fns = append(fns, getfn{"SYNCER", func(ctx context.Context, args protocol.RepoLookupArgs) (*protocol.RepoInfo, bool, error) {
			repo, err := s.Store.GetRepoByName(ctx, string(args.Repo))
			if err != nil {
				return nil, false, err
			}

			info, err := newRepoInfo(repo)
			if err != nil {
				return nil, false, err
			}

			return info, true, nil
		}})
	} else {
		fns = append(fns, getfn{"GITHUB", repos.GetGitHubRepository})
	}

	fns = append(fns,
		getfn{"GITLAB", repos.GetGitLabRepository},
		getfn{"BITBUCKETSERVER", repos.GetBitbucketServerRepository},
		getfn{"AWSCODECOMMIT", repos.GetAWSCodeCommitRepository},
		getfn{"GITOLITE", repos.GetGitoliteRepository},
	)

	var (
		result        protocol.RepoLookupResult
		repo          *protocol.RepoInfo
		authoritative bool
		err           error
	)

	// Find the authoritative source of the repository being looked up.
	for _, get := range fns {
		if repo, authoritative, err = get.fn(ctx, args); authoritative {
			log15.Debug("repoupdater.lookup-repo", "source", get.kind)
			break
		}
	}

	if authoritative {
		if isNotFound(err) {
			result.ErrorNotFound = true
			err = nil
		} else if isUnauthorized(err) {
			result.ErrorUnauthorized = true
			err = nil
		} else if isTemporarilyUnavailable(err) {
			result.ErrorTemporarilyUnavailable = true
			err = nil
		}
		if err != nil {
			return nil, err
		}
		if repo != nil {
			go func() {
				err := api.InternalClient.ReposUpdateMetadata(context.Background(), repo.Name, repo.Description, repo.Fork, repo.Archived)
				if err != nil {
					log15.Warn("Error updating repo metadata", "repo", repo.Name, "err", err)
				}
			}()
		}
		if err != nil {
			return nil, err
		}
		result.Repo = repo
		return &result, nil
	}

	// No configured code hosts are authoritative for this repository.
	result.ErrorNotFound = true
	return &result, nil
}

func newRepoInfo(r *repos.Repo) (*protocol.RepoInfo, error) {
	urls := r.CloneURLs()
	if len(urls) == 0 {
		return nil, fmt.Errorf("no clone urls for repo id=%q name=%q", r.ID, r.Name)
	}

	info := protocol.RepoInfo{
		Name:         api.RepoName(r.Name),
		Description:  r.Description,
		Fork:         r.Fork,
		Archived:     r.Archived,
		VCS:          protocol.VCSInfo{URL: urls[0]},
		ExternalRepo: &r.ExternalRepo,
	}

	switch strings.ToLower(r.ExternalRepo.ServiceType) {
	case "github":
		baseURL := r.ExternalRepo.ServiceID
		info.Links = &protocol.RepoLinks{
			Root:   baseURL,
			Tree:   baseURL + "/tree/{rev}/{path}",
			Blob:   baseURL + "/blob/{rev}/{path}",
			Commit: baseURL + "/commit/{commit}",
		}
	}

	return &info, nil
}

func isNotFound(err error) bool {
	// TODO(sqs): reduce duplication
	return github.IsNotFound(err) || gitlab.IsNotFound(err) || awscodecommit.IsNotFound(err) || errcode.IsNotFound(err)
}

func isUnauthorized(err error) bool {
	// TODO(sqs): reduce duplication
	if awscodecommit.IsUnauthorized(err) || errcode.IsUnauthorized(err) {
		return true
	}
	code := github.HTTPErrorCode(err)
	if code == 0 {
		code = gitlab.HTTPErrorCode(err)
	}
	return code == http.StatusUnauthorized || code == http.StatusForbidden
}

func isTemporarilyUnavailable(err error) bool {
	return err == repos.ErrGitHubAPITemporarilyUnavailable || github.IsRateLimitExceeded(err)
}
