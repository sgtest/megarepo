package httpapi

import (
	"context"
	"encoding/json"
	"net/http"

	"github.com/gorilla/mux"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/globals"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	"github.com/sourcegraph/sourcegraph/internal/txemail"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
	"gopkg.in/inconshreveable/log15.v2"
)

func serveReposGetByName(w http.ResponseWriter, r *http.Request) error {
	repoName := api.RepoName(mux.Vars(r)["RepoName"])
	repo, err := backend.Repos.GetByName(r.Context(), repoName)
	if err != nil {
		return err
	}
	data, err := json.Marshal(repo)
	if err != nil {
		return err
	}
	w.WriteHeader(http.StatusOK)
	w.Write(data)
	return nil
}

func serveReposCreateIfNotExists(w http.ResponseWriter, r *http.Request) error {
	var repo api.RepoCreateOrUpdateRequest
	err := json.NewDecoder(r.Body).Decode(&repo)
	if err != nil {
		return err
	}
	err = backend.Repos.Upsert(r.Context(), api.InsertRepoOp{
		Name:         repo.RepoName,
		Description:  repo.Description,
		Fork:         repo.Fork,
		Archived:     repo.Archived,
		Enabled:      repo.Enabled,
		ExternalRepo: repo.ExternalRepo,
	})
	if err != nil {
		return err
	}
	sgRepo, err := backend.Repos.GetByName(r.Context(), repo.RepoName)
	if err != nil {
		return err
	}
	data, err := json.Marshal(sgRepo)
	if err != nil {
		return err
	}
	w.WriteHeader(http.StatusOK)
	w.Write(data)
	return nil
}

func serveReposUpdateMetadata(w http.ResponseWriter, r *http.Request) error {
	var repo api.ReposUpdateMetadataRequest
	err := json.NewDecoder(r.Body).Decode(&repo)
	if err != nil {
		return err
	}
	if err := db.Repos.UpdateRepositoryMetadata(r.Context(), repo.RepoName, repo.Description, repo.Fork, repo.Archived); err != nil {
		return errors.Wrap(err, "Repos.UpdateRepositoryMetadata failed")
	}
	return nil
}

func servePhabricatorRepoCreate(w http.ResponseWriter, r *http.Request) error {
	var repo api.PhabricatorRepoCreateRequest
	err := json.NewDecoder(r.Body).Decode(&repo)
	if err != nil {
		return err
	}
	phabRepo, err := db.Phabricator.CreateOrUpdate(r.Context(), repo.Callsign, repo.RepoName, repo.URL)
	if err != nil {
		return err
	}
	data, err := json.Marshal(phabRepo)
	if err != nil {
		return err
	}
	w.WriteHeader(http.StatusOK)
	w.Write(data)
	return nil
}

// serveExternalServiceConfigs serves a JSON response that is an array of all
// external service configs that match the requested kind.
func serveExternalServiceConfigs(w http.ResponseWriter, r *http.Request) error {
	var req api.ExternalServiceConfigsRequest
	err := json.NewDecoder(r.Body).Decode(&req)
	if err != nil {
		return err
	}
	services, err := db.ExternalServices.List(r.Context(), db.ExternalServicesListOptions{
		Kinds: []string{req.Kind},
	})
	if err != nil {
		return err
	}

	// Instead of returning an intermediate response type, we directly return
	// the array of configs (which are themselves JSON objects).
	// This makes it possible for the caller to directly unmarshal the response into
	// a slice of connection configurations for this external service kind.
	configs := make([]map[string]interface{}, 0, len(services))
	for _, service := range services {
		var config map[string]interface{}
		// Raw configs may have comments in them so we have to use a json parser
		// that supports comments in json.
		if err := jsonc.Unmarshal(service.Config, &config); err != nil {
			log15.Error(
				"ignoring external service config that has invalid json",
				"id", service.ID,
				"displayName", service.DisplayName,
				"config", service.Config,
				"err", err,
			)
			continue
		}
		configs = append(configs, config)
	}
	return json.NewEncoder(w).Encode(configs)
}

// serveExternalServicesList serves a JSON response that is an array of all external services
// of the given kind
func serveExternalServicesList(w http.ResponseWriter, r *http.Request) error {
	var req api.ExternalServicesListRequest
	err := json.NewDecoder(r.Body).Decode(&req)
	if err != nil {
		return err
	}

	if len(req.Kinds) == 0 {
		req.Kinds = append(req.Kinds, req.Kind)
	}

	services, err := db.ExternalServices.List(r.Context(), db.ExternalServicesListOptions{
		Kinds: req.Kinds,
	})
	if err != nil {
		return err
	}
	return json.NewEncoder(w).Encode(services)
}

func serveConfiguration(w http.ResponseWriter, r *http.Request) error {
	raw, err := globals.ConfigurationServerFrontendOnly.Source.Read(r.Context())
	if err != nil {
		return err
	}
	err = json.NewEncoder(w).Encode(raw)
	if err != nil {
		return errors.Wrap(err, "Encode")
	}
	return nil
}

// serveSearchConfiguration is _only_ used by the zoekt index server. Zoekt does
// not depend on frontend and therefore does not have access to `conf.Watch`.
// Additionally, it only cares about certain search specific settings so this
// search specific endpoint is used rather than serving the entire site settings
// from /.internal/configuration.
func serveSearchConfiguration(w http.ResponseWriter, r *http.Request) error {
	opts := struct {
		LargeFiles []string
		Symbols    bool
	}{
		LargeFiles: conf.Get().SearchLargeFiles,
		Symbols:    conf.SymbolIndexEnabled(),
	}
	err := json.NewEncoder(w).Encode(opts)
	if err != nil {
		return errors.Wrap(err, "encode")
	}
	return nil
}

type reposListServer struct {
	SourcegraphDotComMode bool

	Repos interface {
		ListDefault(context.Context) ([]*types.Repo, error)
		List(context.Context, db.ReposListOptions) ([]*types.Repo, error)
	}
}

func (h *reposListServer) serve(w http.ResponseWriter, r *http.Request) error {
	var names []string
	if h.SourcegraphDotComMode {
		res, err := h.Repos.ListDefault(r.Context())
		if err != nil {
			return errors.Wrap(err, "listing repos")
		}
		names = make([]string, len(res))
		for i, r := range res {
			names[i] = string(r.Name)
		}
	} else {
		var opt db.ReposListOptions
		if err := json.NewDecoder(r.Body).Decode(&opt); err != nil {
			return err
		}
		res, err := h.Repos.List(r.Context(), opt)
		if err != nil {
			return errors.Wrap(err, "listing repos")
		}
		names = make([]string, len(res))
		for i, r := range res {
			names[i] = string(r.Name)
		}
	}

	// BACKCOMPAT: Add a Name field that serializes to `URI` because
	// zoekt-sourcegraph-indexserver expects one to exist (with the
	// repository name). This is a legacy of the rename from "repo URI" to
	// "repo name".
	type repoWithBackcompatURIField struct {
		Name string `json:"URI"`
	}
	res := make([]repoWithBackcompatURIField, len(names))
	for i, name := range names {
		res[i].Name = name
	}

	data, err := json.Marshal(res)
	if err != nil {
		return err
	}
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write(data)
	return nil
}

func serveReposListEnabled(w http.ResponseWriter, r *http.Request) error {
	names, err := db.Repos.ListEnabledNames(r.Context())
	if err != nil {
		return err
	}
	return json.NewEncoder(w).Encode(names)
}

func serveSavedQueriesListAll(w http.ResponseWriter, r *http.Request) error {
	// List settings for all users, orgs, etc.
	settings, err := db.SavedSearches.ListAll(r.Context())
	if err != nil {
		return errors.Wrap(err, "db.SavedSearches.ListAll")
	}

	queries := make([]api.SavedQuerySpecAndConfig, 0, len(settings))
	for _, s := range settings {
		var spec api.SavedQueryIDSpec
		if s.Config.UserID != nil {
			spec = api.SavedQueryIDSpec{Subject: api.SettingsSubject{User: s.Config.UserID}, Key: s.Config.Key}
		} else if s.Config.OrgID != nil {
			spec = api.SavedQueryIDSpec{Subject: api.SettingsSubject{Org: s.Config.OrgID}, Key: s.Config.Key}
		}

		queries = append(queries, api.SavedQuerySpecAndConfig{
			Spec:   spec,
			Config: s.Config,
		})
	}

	if err := json.NewEncoder(w).Encode(queries); err != nil {
		return errors.Wrap(err, "Encode")
	}

	return nil
}

func serveSavedQueriesGetInfo(w http.ResponseWriter, r *http.Request) error {
	var query string
	err := json.NewDecoder(r.Body).Decode(&query)
	if err != nil {
		return errors.Wrap(err, "Decode")
	}
	info, err := db.QueryRunnerState.Get(r.Context(), query)
	if err != nil {
		return errors.Wrap(err, "SavedQueries.Get")
	}
	if err := json.NewEncoder(w).Encode(info); err != nil {
		return errors.Wrap(err, "Encode")
	}
	return nil
}

func serveSavedQueriesSetInfo(w http.ResponseWriter, r *http.Request) error {
	var info *api.SavedQueryInfo
	err := json.NewDecoder(r.Body).Decode(&info)
	if err != nil {
		return errors.Wrap(err, "Decode")
	}
	err = db.QueryRunnerState.Set(r.Context(), &db.SavedQueryInfo{
		Query:        info.Query,
		LastExecuted: info.LastExecuted,
		LatestResult: info.LatestResult,
		ExecDuration: info.ExecDuration,
	})
	if err != nil {
		return errors.Wrap(err, "SavedQueries.Set")
	}
	w.WriteHeader(http.StatusOK)
	w.Write([]byte("OK"))
	return nil
}

func serveSavedQueriesDeleteInfo(w http.ResponseWriter, r *http.Request) error {
	var query string
	err := json.NewDecoder(r.Body).Decode(&query)
	if err != nil {
		return errors.Wrap(err, "Decode")
	}
	err = db.QueryRunnerState.Delete(r.Context(), query)
	if err != nil {
		return errors.Wrap(err, "SavedQueries.Delete")
	}
	w.WriteHeader(http.StatusOK)
	w.Write([]byte("OK"))
	return nil
}

func serveSettingsGetForSubject(w http.ResponseWriter, r *http.Request) error {
	var subject api.SettingsSubject
	if err := json.NewDecoder(r.Body).Decode(&subject); err != nil {
		return errors.Wrap(err, "Decode")
	}
	settings, err := db.Settings.GetLatest(r.Context(), subject)
	if err != nil {
		return errors.Wrap(err, "Settings.GetLatest")
	}
	if err := json.NewEncoder(w).Encode(settings); err != nil {
		return errors.Wrap(err, "Encode")
	}
	return nil
}

func serveOrgsListUsers(w http.ResponseWriter, r *http.Request) error {
	var orgID int32
	err := json.NewDecoder(r.Body).Decode(&orgID)
	if err != nil {
		return errors.Wrap(err, "Decode")
	}
	orgMembers, err := db.OrgMembers.GetByOrgID(r.Context(), orgID)
	if err != nil {
		return errors.Wrap(err, "OrgMembers.GetByOrgID")
	}
	users := make([]int32, 0, len(orgMembers))
	for _, member := range orgMembers {
		users = append(users, member.UserID)
	}
	if err := json.NewEncoder(w).Encode(users); err != nil {
		return errors.Wrap(err, "Encode")
	}
	return nil
}

func serveOrgsGetByName(w http.ResponseWriter, r *http.Request) error {
	var orgName string
	err := json.NewDecoder(r.Body).Decode(&orgName)
	if err != nil {
		return errors.Wrap(err, "Decode")
	}
	org, err := db.Orgs.GetByName(r.Context(), orgName)
	if err != nil {
		return errors.Wrap(err, "Orgs.GetByName")
	}
	if err := json.NewEncoder(w).Encode(org.ID); err != nil {
		return errors.Wrap(err, "Encode")
	}
	return nil
}

func serveUsersGetByUsername(w http.ResponseWriter, r *http.Request) error {
	var username string
	err := json.NewDecoder(r.Body).Decode(&username)
	if err != nil {
		return errors.Wrap(err, "Decode")
	}
	user, err := db.Users.GetByUsername(r.Context(), username)
	if err != nil {
		return errors.Wrap(err, "Users.GetByUsername")
	}
	if err := json.NewEncoder(w).Encode(user.ID); err != nil {
		return errors.Wrap(err, "Encode")
	}
	return nil
}

func serveUserEmailsGetEmail(w http.ResponseWriter, r *http.Request) error {
	var userID int32
	err := json.NewDecoder(r.Body).Decode(&userID)
	if err != nil {
		return errors.Wrap(err, "Decode")
	}
	email, _, err := db.UserEmails.GetPrimaryEmail(r.Context(), userID)
	if err != nil {
		return errors.Wrap(err, "UserEmails.GetEmail")
	}
	if err := json.NewEncoder(w).Encode(email); err != nil {
		return errors.Wrap(err, "Encode")
	}
	return nil
}

func serveExternalURL(w http.ResponseWriter, r *http.Request) error {
	if err := json.NewEncoder(w).Encode(globals.ExternalURL().String()); err != nil {
		return errors.Wrap(err, "Encode")
	}
	return nil
}

func serveCanSendEmail(w http.ResponseWriter, r *http.Request) error {
	if err := json.NewEncoder(w).Encode(conf.CanSendEmail()); err != nil {
		return errors.Wrap(err, "Encode")
	}
	return nil
}

func serveSendEmail(w http.ResponseWriter, r *http.Request) error {
	var msg txemail.Message
	err := json.NewDecoder(r.Body).Decode(&msg)
	if err != nil {
		return err
	}
	return txemail.Send(r.Context(), msg)
}

func serveGitResolveRevision(w http.ResponseWriter, r *http.Request) error {
	// used by zoekt-sourcegraph-mirror
	vars := mux.Vars(r)
	name := api.RepoName(vars["RepoName"])
	spec := vars["Spec"]

	// Do not to trigger a repo-updater lookup since this is a batch job.
	commitID, err := git.ResolveRevision(r.Context(), gitserver.Repo{Name: name}, nil, spec, nil)
	if err != nil {
		return err
	}

	w.WriteHeader(http.StatusOK)
	w.Write([]byte(commitID))
	return nil
}

func serveGitTar(w http.ResponseWriter, r *http.Request) error {
	// used by zoekt-sourcegraph-mirror
	vars := mux.Vars(r)
	name := api.RepoName(vars["RepoName"])
	spec := vars["Commit"]

	// Ensure commit exists. Do not want to trigger a repo-updater lookup since this is a batch job.
	repo := gitserver.Repo{Name: name}
	commit, err := git.ResolveRevision(r.Context(), repo, nil, spec, nil)
	if err != nil {
		return err
	}

	opts := gitserver.ArchiveOptions{
		Treeish: string(commit),
		Format:  "tar",
	}

	location := gitserver.DefaultClient.ArchiveURL(r.Context(), repo, opts)

	w.Header().Set("Location", location.String())
	w.WriteHeader(http.StatusFound)

	return nil
}

func handlePing(w http.ResponseWriter, r *http.Request) {
	if err := r.ParseForm(); err != nil {
		http.Error(w, "could not parse form: "+err.Error(), http.StatusBadRequest)
		return
	}

	for _, service := range r.Form["service"] {
		switch service {
		case "gitserver":
			if err := gitserver.DefaultClient.WaitForGitServers(r.Context()); err != nil {
				http.Error(w, "wait for gitservers failed: "+err.Error(), http.StatusBadGateway)
				return
			}

		default:
			http.Error(w, "unknown service: "+service, http.StatusBadRequest)
			return
		}
	}

	_, _ = w.Write([]byte("pong"))
}
