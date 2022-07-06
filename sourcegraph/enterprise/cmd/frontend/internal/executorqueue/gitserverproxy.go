package executorqueue

import (
	"context"
	"net/http"
	"net/http/httputil"
	"net/url"
	"path"

	"github.com/gorilla/mux"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
)

type GitserverClient interface {
	// AddrForRepo returns the gitserver address to use for the given repo name.
	AddrForRepo(context.Context, api.RepoName) (string, error)
}

// gitserverProxy creates an HTTP handler that will proxy requests to the correct
// gitserver at the given gitPath.
func gitserverProxy(gitserverClient GitserverClient, gitPath string) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		repo := getRepoName(r)

		addrForRepo, err := gitserverClient.AddrForRepo(r.Context(), api.RepoName(repo))
		if err != nil {
			w.WriteHeader(http.StatusInternalServerError)
			return
		}

		p := httputil.ReverseProxy{
			Director: func(r *http.Request) {
				u := &url.URL{
					Scheme:   "http",
					Host:     addrForRepo,
					Path:     path.Join("/git", repo, gitPath),
					RawQuery: r.URL.RawQuery,
				}
				r.URL = u
			},
			Transport: httpcli.InternalClient.Transport,
		}
		p.ServeHTTP(w, r)
		return
	})
}

// getRepoName returns the "RepoName" segment of the request's URL. This is a function variable so
// we can swap it out easily during testing. The gorilla/mux does have a testing function to
// set variables on a request context, but the context gets lost somewhere between construction
// of the request and the default client's handling of the request.
var getRepoName = func(r *http.Request) string {
	return mux.Vars(r)["RepoName"]
}
