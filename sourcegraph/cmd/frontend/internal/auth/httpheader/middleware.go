// Package httpheader implements auth via HTTP Headers.
package httpheader

import (
	"net/http"
	"strings"

	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/auth"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/auth/providers"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
)

const providerType = "http-header"

// Middleware is the same for both the app and API because the HTTP proxy is assumed to wrap
// requests to both the app and API and add headers.
//
// See the "func middleware" docs for more information.
func Middleware(db database.DB) *auth.Middleware {
	return &auth.Middleware{
		API: middleware(db),
		App: middleware(db),
	}
}

// middleware is middleware that checks for an HTTP header from an auth proxy that specifies the
// client's authenticated username. It's for use with auth proxies like
// https://github.com/bitly/oauth2_proxy and is configured with the http-header auth provider in
// site config.
//
// TESTING: Use the testproxy test program to test HTTP auth proxy behavior. For example, run `go
// run cmd/frontend/auth/httpheader/testproxy.go -username=alice` then go to
// http://localhost:4080. See `-h` for flag help.
//
// TESTING: Also see dev/internal/cmd/auth-proxy-http-header for conveniently
// starting up a proxy for multiple users.
//
// 🚨 SECURITY
func middleware(db database.DB) func(next http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			authProvider, ok := providers.GetProviderByConfigID(providers.ConfigID{Type: providerType}).(*provider)
			if !ok {
				next.ServeHTTP(w, r)
				return
			}

			if authProvider.c.UsernameHeader == "" {
				log15.Error("No HTTP header set for username (set the http-header auth provider's usernameHeader property).")
				http.Error(w, "misconfigured http-header auth provider", http.StatusInternalServerError)
				return
			}

			rawUsername := strings.TrimPrefix(r.Header.Get(authProvider.c.UsernameHeader), authProvider.c.StripUsernameHeaderPrefix)
			rawEmail := strings.TrimPrefix(r.Header.Get(authProvider.c.EmailHeader), authProvider.c.StripUsernameHeaderPrefix)

			// Continue onto next auth provider if no header is set (in case the auth proxy allows
			// unauthenticated users to bypass it, which some do). Also respect already authenticated
			// actors (e.g., via access token).
			//
			// It would NOT add any additional security to return an error here, because a user who can
			// access this HTTP endpoint directly can just as easily supply a fake username whose
			// identity to assume.
			if (rawEmail == "" && rawUsername == "") || actor.FromContext(r.Context()).IsAuthenticated() {
				next.ServeHTTP(w, r)
				return
			}

			// Otherwise, get or create the user and proceed with the authenticated request.
			var (
				username string
				err      error
			)
			if rawUsername != "" {
				username, err = auth.NormalizeUsername(rawUsername)
				if err != nil {
					log15.Error("Error normalizing username from HTTP auth proxy.", "username", rawUsername, "err", err)
					http.Error(w, "unable to normalize username", http.StatusInternalServerError)
					return
				}
			} else if rawEmail != "" {
				// if they don't have a username, let's create one from their email
				username, err = auth.NormalizeUsername(rawEmail)
				if err != nil {
					log15.Error("Error normalizing username from email header in HTTP auth proxy.", "email", rawEmail, "err", err)
					http.Error(w, "unable to normalize username", http.StatusInternalServerError)
					return
				}
			}
			userID, safeErrMsg, err := auth.GetAndSaveUser(r.Context(), db, auth.GetAndSaveUserOp{
				UserProps: database.NewUser{
					Username: username,
					Email:    rawEmail,
					// We always only take verified emails from an external source.
					EmailIsVerified: true,
				},
				ExternalAccount: extsvc.AccountSpec{
					ServiceType: providerType,
					// Store rawUsername, not normalized username, to prevent two users with distinct
					// pre-normalization usernames from being merged into the same normalized username
					// (and therefore letting them each impersonate the other).
					AccountID: func() string {
						if rawEmail != "" {
							return rawEmail
						}
						return rawUsername
					}(),
				},
				CreateIfNotExist: true,
				LookUpByUsername: rawEmail == "", // if the email is provided, we should look up by email, otherwise username
			})
			if err != nil {
				log15.Error("unable to get/create user from SSO header", "header", authProvider.c.UsernameHeader, "rawUsername", rawUsername, "err", err, "userErr", safeErrMsg)
				http.Error(w, safeErrMsg, http.StatusInternalServerError)
				return
			}

			r = r.WithContext(actor.WithActor(r.Context(), &actor.Actor{UID: userID}))
			next.ServeHTTP(w, r)
		})
	}
}
