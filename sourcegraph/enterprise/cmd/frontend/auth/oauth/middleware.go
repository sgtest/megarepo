package oauth

import (
	"net/http"
	"net/url"
	"strings"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/external/auth"
	"github.com/sourcegraph/sourcegraph/pkg/actor"
	"github.com/sourcegraph/sourcegraph/schema"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

func NewHandler(serviceType, authPrefix string, isAPIHandler bool, next http.Handler) http.Handler {
	oauthFlowHandler := http.StripPrefix(authPrefix, NewOAuthFlowHandler(serviceType))
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// Delegate to the auth flow handler
		if !isAPIHandler && strings.HasPrefix(r.URL.Path, authPrefix+"/") {
			oauthFlowHandler.ServeHTTP(w, r)
			return
		}

		// If the actor is authenticated and not performing an OAuth flow, then proceed to
		// next.
		if actor.FromContext(r.Context()).IsAuthenticated() {
			next.ServeHTTP(w, r)
			return
		}

		// If there is only one auth provider configured, the single auth provider is a OAuth
		// instance, and it's an app request, redirect to signin immediately. The user wouldn't be
		// able to do anything else anyway; there's no point in showing them a signin screen with
		// just a single signin option.
		if pc := getExactlyOneOAuthProvider(); pc != nil && !isAPIHandler {
			v := make(url.Values)
			v.Set("redirect", auth.SafeRedirectURL(r.URL.String()))
			v.Set("pc", pc.ConfigID().ID)
			http.Redirect(w, r, authPrefix+"/login?"+v.Encode(), http.StatusFound)
			return
		}

		next.ServeHTTP(w, r)
	})
}

func NewOAuthFlowHandler(serviceType string) http.Handler {
	mux := http.NewServeMux()
	mux.Handle("/login", http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		id := req.URL.Query().Get("pc")
		p := getProvider(serviceType, id)
		if p == nil {
			log15.Error("no OAuth provider found with ID and service type", "id", id, "serviceType", serviceType)
			http.Error(w, "Misconfigured GitHub auth provider.", http.StatusInternalServerError)
		}
		p.Login.ServeHTTP(w, req)
	}))
	mux.Handle("/callback", http.HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		state, err := DecodeState(req.URL.Query().Get("state"))
		if err != nil {
			http.Error(w, "Authentication failed. Try signing in again (and clearing cookies for the current site). The error was: could not decode OAuth state from URL parameter.", http.StatusBadRequest)
			return
		}

		p := getProvider(serviceType, state.ProviderID)
		if p == nil {
			log15.Error("OAuth failed: in callback, no auth provider found with ID and service type", "id", state.ProviderID, "serviceType", serviceType)
			http.Error(w, "Authentication failed. Try signing in again (and clearing cookies for the current site). The error was: could not find provider that matches the OAuth state parameter.", http.StatusBadRequest)
			return
		}
		p.Callback.ServeHTTP(w, req)
	}))
	return mux
}

func getExactlyOneOAuthProvider() *Provider {
	ps := auth.Providers()
	if len(ps) != 1 {
		return nil
	}
	p, ok := ps[0].(*Provider)
	if !ok {
		return nil
	}
	if !IsOAuth(p.Config()) {
		return nil
	}
	return p
}

var isOAuths []func(p schema.AuthProviders) bool

func AddIsOAuth(f func(p schema.AuthProviders) bool) {
	isOAuths = append(isOAuths, f)
}

func IsOAuth(p schema.AuthProviders) bool {
	for _, f := range isOAuths {
		if f(p) {
			return true
		}
	}
	return false
}
