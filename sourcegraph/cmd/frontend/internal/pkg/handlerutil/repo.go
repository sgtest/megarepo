package handlerutil

import (
	"net/http"

	"context"

	"github.com/gorilla/mux"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/routevar"
)

// GetRepo gets the repo (from the reposSvc) specified in the URL's
// Repo route param. Callers should ideally check for a return error of type
// URLMovedError and handle this scenario by warning or redirecting the user.
func GetRepo(ctx context.Context, vars map[string]string) (*types.Repo, error) {
	origRepo := routevar.ToRepo(vars)

	repo, err := backend.Repos.GetByURI(ctx, origRepo)
	if err != nil {
		return nil, err
	}

	if origRepo != repo.URI {
		return nil, &URLMovedError{repo.URI}
	}

	return repo, nil
}

// getRepoRev resolves the repository and commit specified in the route vars.
func getRepoRev(ctx context.Context, vars map[string]string, repoID api.RepoID) (api.RepoID, api.CommitID, error) {
	repoRev := routevar.ToRepoRev(vars)
	repo, err := backend.Repos.Get(ctx, repoID)
	if err != nil {
		return repoID, "", err
	}
	commitID, err := backend.Repos.ResolveRev(ctx, repo, repoRev.Rev)
	if err != nil {
		return repoID, "", err
	}

	return repoID, commitID, nil
}

// GetRepoAndRev returns the repo object and the commit ID for a repository. It may
// also return custom error URLMovedError to allow special handling of this case,
// such as for example redirecting the user.
func GetRepoAndRev(ctx context.Context, vars map[string]string) (*types.Repo, api.CommitID, error) {
	repo, err := GetRepo(ctx, vars)
	if err != nil {
		return repo, "", err
	}

	_, commitID, err := getRepoRev(ctx, vars, repo.ID)
	return repo, commitID, err
}

// RedirectToNewRepoURI writes an HTTP redirect response with a
// Location that matches the request's location except with the
// Repo route var updated to refer to newRepoURI (instead of the
// originally requested repo URI).
func RedirectToNewRepoURI(w http.ResponseWriter, r *http.Request, newRepoURI api.RepoURI) error {
	origVars := mux.Vars(r)
	origVars["Repo"] = string(newRepoURI)

	var pairs []string
	for k, v := range origVars {
		pairs = append(pairs, k, v)
	}
	destURL, err := mux.CurrentRoute(r).URLPath(pairs...)
	if err != nil {
		return err
	}

	http.Redirect(w, r, destURL.String(), http.StatusMovedPermanently)
	return nil
}
