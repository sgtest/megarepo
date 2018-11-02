package cli

import (
	"context"
	"fmt"
	"net/http"
	"strings"

	"github.com/NYTimes/gziphandler"
	gcontext "github.com/gorilla/context"
	"github.com/gorilla/mux"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/globals"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/hooks"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/assetsutil"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/auth"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/cli/middleware"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/httpapi"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/httpapi/router"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/handlerutil"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/session"
	"github.com/sourcegraph/sourcegraph/pkg/actor"
	"github.com/sourcegraph/sourcegraph/pkg/conf"
	"github.com/sourcegraph/sourcegraph/pkg/env"
	tracepkg "github.com/sourcegraph/sourcegraph/pkg/trace"
)

// newExternalHTTPHandler creates and returns the HTTP handler that serves the app and API pages to
// external clients.
func newExternalHTTPHandler(ctx context.Context) (http.Handler, error) {
	// Each auth middleware determines on a per-request basis whether it should be enabled (if not, it
	// immediately delegates the request to the next middleware in the chain).
	authMiddlewares := auth.AuthMiddleware()

	// HTTP API handler.
	apiHandler := httpapi.NewHandler(router.New(mux.NewRouter().PathPrefix("/.api/").Subrouter()))
	apiHandler = authMiddlewares.API(apiHandler) // 🚨 SECURITY: auth middleware
	// 🚨 SECURITY: The HTTP API should not accept cookies as authentication (except those with the
	// X-Requested-With header). Doing so would open it up to CSRF attacks.
	apiHandler = session.CookieMiddlewareWithCSRFSafety(apiHandler, corsAllowHeader, isTrustedOrigin) // API accepts cookies with special header
	apiHandler = httpapi.AccessTokenAuthMiddleware(apiHandler)                                        // API accepts access tokens
	apiHandler = gziphandler.GzipHandler(apiHandler)

	// App handler (HTML pages).
	appHandler := app.NewHandler()
	appHandler = handlerutil.CSRFMiddleware(appHandler, globals.AppURL.Scheme == "https") // after appAuthMiddleware because SAML IdP posts data to us w/o a CSRF token
	appHandler = authMiddlewares.App(appHandler)                                          // 🚨 SECURITY: auth middleware
	appHandler = session.CookieMiddleware(appHandler)                                     // app accepts cookies

	// Mount handlers and assets.
	sm := http.NewServeMux()
	sm.Handle("/.api/", apiHandler)
	sm.Handle("/", appHandler)
	assetsutil.Mount(sm)

	var h http.Handler = sm

	// Wrap in middleware.
	//
	// 🚨 SECURITY: Auth middleware that must run before other auth middlewares.
	h = auth.OverrideAuthMiddleware(h)
	h = auth.ForbidAllRequestsMiddleware(h)
	// 🚨 SECURITY: These all run before the auth handler, so the client is not yet authenticated.
	if hooks.PreAuthMiddleware != nil {
		h = hooks.PreAuthMiddleware(h)
	}
	h = healthCheckMiddleware(h)
	h = tracepkg.Middleware(h)
	h = middleware.SourcegraphComGoGetHandler(h)
	h = middleware.BlackHole(h)
	h = secureHeadersMiddleware(h)
	h = middleware.CanonicalURL(h)
	h = gcontext.ClearHandler(h)
	h = middleware.Trace(h)
	return h, nil
}

func healthCheckMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		switch r.URL.Path {
		case "/healthz", "/__version":
			fmt.Fprintf(w, env.Version)
		default:
			next.ServeHTTP(w, r)
		}
	})
}

// newInternalHTTPHandler creates and returns the HTTP handler for the internal API (accessible to
// other internal services).
func newInternalHTTPHandler() http.Handler {
	internalMux := http.NewServeMux()
	internalMux.Handle("/.internal/", gziphandler.GzipHandler(
		withInternalActor(
			httpapi.NewInternalHandler(
				router.NewInternal(mux.NewRouter().PathPrefix("/.internal/").Subrouter()),
			),
		),
	))
	return gcontext.ClearHandler(internalMux)
}

// withInternalActor wraps an existing HTTP handler by setting an internal actor in the HTTP request
// context.
//
// 🚨 SECURITY: This should *never* be called to wrap externally accessible handlers (i.e., only use
// for the internal endpoint), because internal requests will bypass repository permissions checks.
func withInternalActor(h http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		rWithActor := r.WithContext(actor.WithActor(r.Context(), &actor.Actor{Internal: true}))
		h.ServeHTTP(w, rWithActor)
	})
}

// corsAllowHeader is the HTTP header that, if present (and assuming secureHeadersMiddleware is
// used), indicates that the incoming HTTP request is either same-origin or is from an allowed
// origin. See
// https://www.owasp.org/index.php/Cross-Site_Request_Forgery_(CSRF)_Prevention_Cheat_Sheet#Protecting_REST_Services:_Use_of_Custom_Request_Headers
// for more information on this technique.
const corsAllowHeader = "X-Requested-With"

// secureHeadersMiddleware adds and checks for HTTP security-related headers.
//
// 🚨 SECURITY: This handler is served to all clients, even on private servers to clients who have
// not authenticated. It must not reveal any sensitive information.
func secureHeadersMiddleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// headers for security
		w.Header().Set("X-Content-Type-Options", "nosniff")
		w.Header().Set("X-XSS-Protection", "1; mode=block")
		w.Header().Set("X-Frame-Options", "DENY")
		if hsts := conf.HTTPStrictTransportSecurity(); hsts != "" {
			w.Header().Set("Strict-Transport-Security", hsts)
		}
		// no cache by default
		w.Header().Set("Cache-Control", "no-cache, max-age=0")

		// CORS
		// If the headerOrigin is the development or production Chrome Extension explicitly set the Allow-Control-Allow-Origin
		// to the incoming header URL. Otherwise use the configured CORS origin.
		headerOrigin := r.Header.Get("Origin")
		isExtensionRequest := (headerOrigin == devExtension || headerOrigin == prodExtension) && !disableBrowserExtension
		if corsOrigin := conf.Get().CorsOrigin; corsOrigin != "" || isExtensionRequest {
			w.Header().Set("Access-Control-Allow-Credentials", "true")

			allowOrigin := corsOrigin
			if isExtensionRequest || isAllowedOrigin(headerOrigin, strings.Fields(corsOrigin)) {
				allowOrigin = headerOrigin
			}

			w.Header().Set("Access-Control-Allow-Origin", allowOrigin)
			if r.Method == "OPTIONS" {
				w.Header().Set("Access-Control-Allow-Methods", "GET, POST, OPTIONS")
				w.Header().Set("Access-Control-Allow-Headers", corsAllowHeader+", X-Sourcegraph-Client, Content-Type")
				w.WriteHeader(http.StatusOK)
				return // do not invoke next handler
			}
		}

		next.ServeHTTP(w, r)
	})
}

// isTrustedOrigin returns whether the HTTP request's Origin is trusted to initiate authenticated
// cross-origin requests.
func isTrustedOrigin(r *http.Request) bool {
	requestOrigin := r.Header.Get("Origin")

	var isExtensionRequest bool
	if !disableBrowserExtension {
		isExtensionRequest = requestOrigin == devExtension || requestOrigin == prodExtension
	}

	var isCORSAllowedRequest bool
	if corsOrigin := conf.Get().CorsOrigin; corsOrigin != "" {
		isCORSAllowedRequest = isAllowedOrigin(requestOrigin, strings.Fields(corsOrigin))
	}

	if appURL := strings.TrimSuffix(conf.Get().AppURL, "/"); appURL != "" && requestOrigin == appURL {
		isCORSAllowedRequest = true
	}

	return isExtensionRequest || isCORSAllowedRequest
}
