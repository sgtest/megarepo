package ui

import (
	"context"
	"html/template"
	"log"
	"net/http"
	"net/http/httputil"
	"net/url"
	"regexp"
	"strings"

	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/env"
	"github.com/sourcegraph/sourcegraph/pkg/errcode"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver"
	"github.com/sourcegraph/sourcegraph/pkg/repoupdater"
	"github.com/sourcegraph/sourcegraph/pkg/routevar"
	"github.com/sourcegraph/sourcegraph/pkg/vcs/git"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"

	"github.com/gorilla/mux"
	"github.com/pkg/errors"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/assetsutil"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/jscontext"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/handlerutil"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/actor"
	"github.com/sourcegraph/sourcegraph/pkg/conf"
	"github.com/sourcegraph/sourcegraph/pkg/vcs"
)

type InjectedHTML struct {
	HeadTop    template.HTML
	HeadBottom template.HTML
	BodyTop    template.HTML
	BodyBottom template.HTML
}

type Metadata struct {
	// Title is the title of the page for Twitter cards, OpenGraph, etc.
	// e.g. "Open in Sourcegraph"
	Title string

	// Description is the description of the page for Twitter cards, OpenGraph,
	// etc. e.g. "View this link in Sourcegraph Editor."
	Description string
}

type Common struct {
	Injected InjectedHTML
	Metadata *Metadata
	Context  jscontext.JSContext
	AssetURL string
	Title    string
	Error    *pageError

	InjectSourcegraphTracker bool

	// The fields below have zero values when not on a repo page.
	Repo         *types.Repo
	Rev          string // unresolved / user-specified revision (e.x.: "@master")
	api.CommitID        // resolved SHA1 revision
}

// repoShortName trims the first path element of the given repo uri if it has
// at least two path components.
func repoShortName(uri api.RepoURI) string {
	split := strings.Split(string(uri), "/")
	if len(split) < 2 {
		return string(uri)
	}
	return strings.Join(split[1:], "/")
}

// newCommon builds a *Common data structure, returning an error if one occurs.
//
// In the event of the repository having been renamed, the request is handled
// by newCommon and nil, nil is returned. Basic usage looks like:
//
// 	common, err := newCommon(w, r, serveError)
// 	if err != nil {
// 		return err
// 	}
// 	if common == nil {
// 		return nil // request was handled
// 	}
//
// In the case of a repository that is cloning, a Common data structure is
// returned but it has an incomplete RevSpec.
func newCommon(w http.ResponseWriter, r *http.Request, title string, serveError func(w http.ResponseWriter, r *http.Request, err error, statusCode int)) (*Common, error) {
	injectTelligentTracker := false
	if envvar.SourcegraphDotComMode() {
		injectTelligentTracker = true
	}

	common := &Common{
		Injected: InjectedHTML{
			HeadTop:    template.HTML(conf.Get().HtmlHeadTop),
			HeadBottom: template.HTML(conf.Get().HtmlHeadBottom),
			BodyTop:    template.HTML(conf.Get().HtmlBodyTop),
			BodyBottom: template.HTML(conf.Get().HtmlBodyBottom),
		},
		Context:  jscontext.NewJSContextFromRequest(r),
		AssetURL: assetsutil.URL("").String(),
		Title:    title,

		InjectSourcegraphTracker: injectTelligentTracker,
	}

	if _, ok := mux.Vars(r)["Repo"]; ok {
		// Common repo pages (blob, tree, etc).
		var err error
		common.Repo, common.CommitID, err = handlerutil.GetRepoAndRev(r.Context(), mux.Vars(r))
		isRepoEmptyError := routevar.ToRepoRev(mux.Vars(r)).Rev == "" && git.IsRevisionNotFound(errors.Cause(err)) // should reply with HTTP 200
		if err != nil && !isRepoEmptyError {
			if e, ok := err.(*handlerutil.URLMovedError); ok {
				// The repository has been renamed, e.g. "github.com/docker/docker"
				// was renamed to "github.com/moby/moby" -> redirect the user now.
				http.Redirect(w, r, "/"+string(e.NewRepo), http.StatusMovedPermanently)
				return nil, nil
			}
			if e, ok := err.(backend.ErrRepoSeeOther); ok {
				// Repo does not exist here, redirect to the recommended location.
				u, err := url.Parse(e.RedirectURL)
				if err != nil {
					return nil, err
				}
				u.Path, u.RawQuery = r.URL.Path, r.URL.RawQuery
				http.Redirect(w, r, u.String(), http.StatusSeeOther)
				return nil, nil
			}
			if errcode.IsNotFound(err) || errors.Cause(err) == repoupdater.ErrNotFound {
				// Repo does not exist.
				serveError(w, r, err, http.StatusNotFound)
				return nil, nil
			}
			if errors.Cause(err) == repoupdater.ErrUnauthorized {
				// Not authorized to access repository.
				serveError(w, r, err, http.StatusUnauthorized)
				return nil, nil
			}
			if git.IsRevisionNotFound(errors.Cause(err)) {
				// Revision does not exist.
				serveError(w, r, err, http.StatusNotFound)
				return nil, nil
			}
			if _, ok := errors.Cause(err).(*gitserver.RepoNotCloneableErr); ok {
				// Repository is not clonable.
				dangerouslyServeError(w, r, errors.New("repository could not be cloned"), http.StatusInternalServerError)
				return nil, nil
			}
			if vcs.IsRepoNotExist(err) {
				if vcs.IsCloneInProgress(err) {
					// Repo is cloning.
					return common, nil
				}
				// Repo does not exist.
				serveError(w, r, err, http.StatusNotFound)
				return nil, nil
			}
			return nil, err
		}
		if common.Repo.URI == "github.com/sourcegraphtest/Always500Test" {
			return nil, errors.New("error caused by Always500Test repo URI")
		}
		common.Rev = mux.Vars(r)["Rev"]
		// Update gitserver contents for a repo whenever it is visited.
		go func() {
			ctx := context.Background()
			if gitserverRepo, err := backend.GitRepo(ctx, common.Repo); err == nil {
				repoupdater.DefaultClient.EnqueueRepoUpdate(ctx, gitserverRepo)
			}
		}()
	}
	return common, nil
}

type handlerFunc func(w http.ResponseWriter, r *http.Request) error

func serveBasicPageString(title string) handlerFunc {
	return serveBasicPage(func(c *Common, r *http.Request) string {
		return title
	})
}

func serveBasicPage(title func(c *Common, r *http.Request) string) handlerFunc {
	return func(w http.ResponseWriter, r *http.Request) error {
		common, err := newCommon(w, r, "", serveError)
		if err != nil {
			return err
		}
		if common == nil {
			return nil // request was handled
		}
		common.Title = title(common, r)
		return renderTemplate(w, "app.html", common)
	}
}

func serveHome(w http.ResponseWriter, r *http.Request) error {
	common, err := newCommon(w, r, "Sourcegraph", serveError)
	if err != nil {
		return err
	}
	if common == nil {
		return nil // request was handled
	}

	if (r.Host == "sourcegraph.com" || r.Host == "www.sourcegraph.com") && !actor.FromContext(r.Context()).IsAuthenticated() {
		// The user is not signed in and tried to access our main site at sourcegraph.com.
		// Redirect to about.sourcegraph.com so they see general info.
		u, err := url.Parse(aboutRedirectScheme + "://" + aboutRedirectHost)
		if err != nil {
			return err
		}
		http.Redirect(w, r, u.String(), http.StatusTemporaryRedirect)
		return nil
	}

	// sourcegraph.com (not about) homepage. There is none, redirect them to /search.
	r.URL.Path = "/search"
	http.Redirect(w, r, r.URL.String(), http.StatusTemporaryRedirect)
	return nil
}

func serveRepoOrBlob(routeName string, title func(c *Common, r *http.Request) string) handlerFunc {
	return func(w http.ResponseWriter, r *http.Request) error {
		common, err := newCommon(w, r, "", serveError)
		if err != nil {
			return err
		}
		if common == nil {
			return nil // request was handled
		}
		common.Title = title(common, r)

		q := r.URL.Query()
		_, isNewQueryUX := q["sq"] // sq URL param is only set by new query UX in SearchNavbarItem.tsx
		if search := q.Get("q"); search != "" && !isNewQueryUX {
			// Redirect old search URLs:
			//
			// 	/github.com/gorilla/mux@24fca303ac6da784b9e8269f724ddeb0b2eea5e7?q=ErrMethodMismatch&utm_source=chrome-extension
			// 	/github.com/gorilla/mux@24fca303ac6da784b9e8269f724ddeb0b2eea5e7/-/blob/mux.go?q=NewRouter
			//
			// To new ones:
			//
			// 	/search?q=repo:^github.com/gorilla/mux$+ErrMethodMismatch
			//
			// It does not apply the file: filter because that was not the behavior of the
			// old blob URLs with a 'q' parameter either.
			r.URL.Path = "/search"
			q.Set("sq", "repo:^"+regexp.QuoteMeta(string(common.Repo.URI))+"$")
			r.URL.RawQuery = q.Encode()
			http.Redirect(w, r, r.URL.String(), http.StatusPermanentRedirect)
			return nil
		}
		return renderTemplate(w, "app.html", common)
	}
}

// searchBadgeHandler serves the search readme badges from the search-badger service
// https://github.com/sourcegraph/search-badger
var searchBadgeHandler = &httputil.ReverseProxy{
	Director: func(r *http.Request) {
		r.URL.Scheme = "http"
		r.URL.Host = "search-badger"
		r.URL.Path = "/"
	},
	ErrorLog: log.New(env.DebugOut, "search-badger proxy: ", log.LstdFlags),
}
