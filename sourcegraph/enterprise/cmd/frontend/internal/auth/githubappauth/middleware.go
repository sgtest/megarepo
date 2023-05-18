package githubapp

import (
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"strconv"
	"strings"

	"github.com/google/uuid"
	"github.com/gorilla/mux"
	"github.com/graph-gophers/graphql-go"
	"go.opentelemetry.io/otel/attribute"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/auth"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/github_apps/types"
	authcheck "github.com/sourcegraph/sourcegraph/internal/auth"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/encryption"
	"github.com/sourcegraph/sourcegraph/internal/encryption/keyring"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/rcache"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const authPrefix = auth.AuthURLPrefix + "/githubapp"

func Middleware(db database.DB) *auth.Middleware {
	return &auth.Middleware{
		API: func(next http.Handler) http.Handler {
			return newMiddleware(db, authPrefix, true, next)
		},
		App: func(next http.Handler) http.Handler {
			return newMiddleware(db, authPrefix, false, next)
		},
	}
}

const cacheTTLSeconds = 60 * 60 // 1 hour

func newMiddleware(ossDB database.DB, authPrefix string, isAPIHandler bool, next http.Handler) http.Handler {
	db := edb.NewEnterpriseDB(ossDB)
	ghAppState := rcache.NewWithTTL("github_app_state", cacheTTLSeconds)
	handler := newServeMux(db, authPrefix, ghAppState)
	traceFamily := "githubapp"

	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		// This span should be manually finished before delegating to the next handler or
		// redirecting.
		span, _ := trace.New(r.Context(), traceFamily, "Middleware.Handle")
		span.SetAttributes(attribute.Bool("isAPIHandler", isAPIHandler))
		span.Finish()
		if strings.HasPrefix(r.URL.Path, authPrefix+"/") {
			handler.ServeHTTP(w, r)
			return
		}

		next.ServeHTTP(w, r)
	})
}

// checkSiteAdmin checks if the current user is a site admin and sets http error if not
func checkSiteAdmin(db edb.EnterpriseDB, w http.ResponseWriter, req *http.Request) error {
	err := authcheck.CheckCurrentUserIsSiteAdmin(req.Context(), db)
	if err == nil {
		return nil
	}
	status := http.StatusForbidden
	if err == authcheck.ErrNotAuthenticated {
		status = http.StatusUnauthorized
	}
	http.Error(w, "Bad request, user must be a site admin", status)
	return err
}

// randomState returns a random sha256 hash that can be used as a state parameter
func randomState(n int) (string, error) {
	data := make([]byte, n)
	if _, err := io.ReadFull(rand.Reader, data); err != nil {
		return "", err
	}

	h := sha256.New()
	h.Write(data)
	return hex.EncodeToString(h.Sum(nil)), nil
}

type GitHubAppResponse struct {
	AppID         int               `json:"id"`
	Slug          string            `json:"slug"`
	Name          string            `json:"name"`
	HtmlURL       string            `json:"html_url"`
	ClientID      string            `json:"client_id"`
	ClientSecret  string            `json:"client_secret"`
	PEM           string            `json:"pem"`
	WebhookSecret string            `json:"webhook_secret"`
	Permissions   map[string]string `json:"permissions"`
	Events        []string          `json:"events"`
}

func newServeMux(db edb.EnterpriseDB, prefix string, cache *rcache.Cache) http.Handler {
	r := mux.NewRouter()

	r.Path(prefix + "/state").Methods("GET").HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		// 🚨 SECURITY: only site admins can create github apps
		if err := checkSiteAdmin(db, w, req); err != nil {
			return
		}

		s, err := randomState(128)
		if err != nil {
			http.Error(w, fmt.Sprintf("Unexpected error when creating redirect URL: %s", err.Error()), http.StatusInternalServerError)
			return
		}

		gqlID := req.URL.Query().Get("id")
		if gqlID == "" {
			cache.Set(s, []byte{1})

			_, _ = w.Write([]byte(s))
			return
		}

		id64, err := UnmarshalGitHubAppID(graphql.ID(gqlID))
		if err != nil {
			http.Error(w, fmt.Sprintf("Unexpected error while unmarshalling App ID: %s", err.Error()), http.StatusBadRequest)
			return
		}
		id := int(id64)

		cache.Set(s, []byte(strconv.Itoa(id)))

		_, _ = w.Write([]byte(s))
	})

	r.Path(prefix + "/new-app-state").Methods("GET").HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		// 🚨 SECURITY: only site admins can create github apps
		if err := checkSiteAdmin(db, w, req); err != nil {
			return
		}

		webhookURN := req.URL.Query().Get("webhookURN")
		appName := req.URL.Query().Get("appName")
		var webhookUUID string
		if webhookURN != "" {
			ws := backend.NewWebhookService(db, keyring.Default())
			hook, err := ws.CreateWebhook(req.Context(), appName, extsvc.KindGitHub, webhookURN, nil)
			if err != nil {
				http.Error(w, fmt.Sprintf("Unexpected error while setting up webhook endpiont: %s", err.Error()), http.StatusInternalServerError)
				return
			}
			webhookUUID = hook.UUID.String()
		}

		s, err := randomState(128)
		if err != nil {
			http.Error(w, fmt.Sprintf("Unexpected error when creating redirectURL: %s", err.Error()), http.StatusInternalServerError)
			return
		}

		cache.Set(s, []byte(webhookUUID))

		resp := struct {
			State       string `json:"state"`
			WebhookUUID string `json:"webhookUUID,omitempty"`
		}{
			State:       s,
			WebhookUUID: webhookUUID,
		}

		if err := json.NewEncoder(w).Encode(resp); err != nil {
			http.Error(w, fmt.Sprintf("Unexpected error while writing response: %s", err.Error()), http.StatusInternalServerError)
		}
	})

	r.Path(prefix + "/redirect").Methods("GET").HandlerFunc(func(w http.ResponseWriter, req *http.Request) {
		// 🚨 SECURITY: only site admins can setup github apps
		if err := checkSiteAdmin(db, w, req); err != nil {
			return
		}

		query := req.URL.Query()
		state := query.Get("state")
		code := query.Get("code")
		if state == "" || code == "" {
			http.Error(w, "Bad request, code and state query params must be present", http.StatusBadRequest)
			return
		}

		stateValue, ok := cache.Get(state)
		if !ok {
			http.Error(w, "Bad request, state query param does not match", http.StatusBadRequest)
			return
		}
		cache.Delete(state)

		webhookUUID, err := uuid.Parse(string(stateValue))
		if err != nil {
			http.Error(w, fmt.Sprintf("Bad request, could not parse webhook UUID: %s", err.Error()), http.StatusBadRequest)
			return
		}

		u, err := getAPIUrl(req, code)
		if err != nil {
			http.Error(w, fmt.Sprintf("Unexpected error when creating github API url: %s", err.Error()), http.StatusInternalServerError)
			return
		}
		app, err := createGitHubApp(u)
		if err != nil {
			http.Error(w, fmt.Sprintf("Unexpected error while converting github app: %s", err.Error()), http.StatusInternalServerError)
			return
		}

		id, err := db.GitHubApps().Create(req.Context(), app)
		if err != nil {
			http.Error(w, fmt.Sprintf("Unexpected error while storing github app in DB: %s", err.Error()), http.StatusInternalServerError)
			return
		}

		webhookDB := db.Webhooks(keyring.Default().WebhookKey)
		hook, err := webhookDB.GetByUUID(req.Context(), webhookUUID)
		if err != nil {
			http.Error(w, fmt.Sprintf("Error while fetching webhook: %s", err.Error()), http.StatusInternalServerError)
			return
		}
		hook.Secret = encryption.NewUnencrypted(app.WebhookSecret)
		hook.Name = app.Name
		if _, err := webhookDB.Update(req.Context(), hook); err != nil {
			http.Error(w, fmt.Sprintf("Error while updating webhook secret: %s", err.Error()), http.StatusInternalServerError)
			return
		}

		state, err = randomState(128)
		if err != nil {
			http.Error(w, fmt.Sprintf("Unexpected error when creating state param: %s", err.Error()), http.StatusInternalServerError)
			return
		}
		cache.Set(state, []byte(strconv.Itoa(id)))

		redirectURL, err := url.JoinPath(app.AppURL, "installations/new")
		if err != nil {
			// if there is an error, try to redirect to app url, which should show Install button as well
			redirectURL = app.AppURL
		}
		http.Redirect(w, req, redirectURL+fmt.Sprintf("?state=%s", state), http.StatusSeeOther)
	})

	r.HandleFunc(prefix+"/setup", func(w http.ResponseWriter, req *http.Request) {
		// 🚨 SECURITY: only site admins can setup github apps
		if err := checkSiteAdmin(db, w, req); err != nil {
			return
		}

		query := req.URL.Query()
		state := query.Get("state")
		instID := query.Get("installation_id")
		if state == "" || instID == "" {
			// If neither state or installation ID is set, we redirect to the GitHub Apps page.
			// This can happen when someone installs the App directly from GitHub, instead of
			// following the link from within Sourcegraph.
			http.Redirect(w, req, "/site-admin/github-apps", http.StatusFound)
			return
		}
		idBytes, ok := cache.Get(state)
		if !ok {
			http.Error(w, "Bad request, state query param does not match", http.StatusBadRequest)
			return
		}
		cache.Delete(state)

		id, err := strconv.Atoi(string(idBytes))
		if err != nil {
			http.Error(w, "Bad request, cannot parse appID", http.StatusBadRequest)
		}

		installationID, err := strconv.Atoi(instID)
		if err != nil {
			http.Error(w, "Bad request, cannot parse installation_id", http.StatusBadRequest)
			return
		}

		action := query.Get("setup_action")
		if action == "install" {
			ctx := req.Context()
			app, err := db.GitHubApps().GetByID(ctx, id)
			if err != nil {
				http.Error(w, fmt.Sprintf("Unexpected error while fetching github app data: %s", err.Error()), http.StatusInternalServerError)
				return
			}

			http.Redirect(w, req, fmt.Sprintf("/site-admin/github-apps/%s?installation_id=%d", MarshalGitHubAppID(int64(app.ID)), installationID), http.StatusFound)
			return
		} else {
			http.Error(w, fmt.Sprintf("Bad request; unsupported setup action: %s", action), http.StatusBadRequest)
			return
		}
	})

	return r
}

func getAPIUrl(req *http.Request, code string) (string, error) {
	referer, err := url.Parse(req.Referer())
	if err != nil {
		return "", err
	}
	api := referer.Scheme + "://api." + referer.Host
	u, err := url.JoinPath(api, "/app-manifests", code, "conversions")
	if err != nil {
		return "", err
	}
	return u, nil
}

func createGitHubApp(conversionURL string) (*types.GitHubApp, error) {
	r, err := http.NewRequest("POST", conversionURL, http.NoBody)
	if err != nil {
		return nil, err
	}
	resp, err := http.DefaultClient.Do(r)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode != 201 {
		return nil, errors.Newf("expected 201 statusCode, got: %d", resp.StatusCode)
	}

	defer resp.Body.Close()

	var response GitHubAppResponse
	if err := json.NewDecoder(resp.Body).Decode(&response); err != nil {
		return nil, err
	}

	htmlURL, err := url.Parse(response.HtmlURL)
	if err != nil {
		return nil, err
	}

	return &types.GitHubApp{
		AppID:         response.AppID,
		Name:          response.Name,
		Slug:          response.Slug,
		ClientID:      response.ClientID,
		ClientSecret:  response.ClientSecret,
		WebhookSecret: response.WebhookSecret,
		PrivateKey:    response.PEM,
		BaseURL:       htmlURL.Scheme + "://" + htmlURL.Host,
		AppURL:        htmlURL.String(),
		Logo:          fmt.Sprintf("%s://%s/identicons/app/app/%s", htmlURL.Scheme, htmlURL.Host, response.Slug),
	}, nil
}
