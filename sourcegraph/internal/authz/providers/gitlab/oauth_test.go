package gitlab

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/stretchr/testify/require"
	"golang.org/x/oauth2"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitlab"
	"github.com/sourcegraph/sourcegraph/internal/oauthutil"
	"github.com/sourcegraph/sourcegraph/internal/rcache"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type mockDoer struct {
	do func(*http.Request) (*http.Response, error)
}

func (c *mockDoer) Do(r *http.Request) (*http.Response, error) {
	return c.do(r)
}

func TestOAuthProvider_FetchUserPerms(t *testing.T) {
	t.Run("nil account", func(t *testing.T) {
		p := newOAuthProvider(OAuthProviderOp{
			BaseURL: mustURL(t, "https://gitlab.com"),
		}, nil)
		_, err := p.FetchUserPerms(context.Background(), nil, authz.FetchPermsOptions{})
		want := "no account provided"
		got := fmt.Sprintf("%v", err)
		if got != want {
			t.Fatalf("err: want %q but got %q", want, got)
		}
	})

	t.Run("not the code host of the account", func(t *testing.T) {
		p := newOAuthProvider(OAuthProviderOp{
			BaseURL: mustURL(t, "https://gitlab.com"),
		}, nil)
		_, err := p.FetchUserPerms(context.Background(),
			&extsvc.Account{
				AccountSpec: extsvc.AccountSpec{
					ServiceType: extsvc.TypeGitHub,
					ServiceID:   "https://github.com/",
				},
			},
			authz.FetchPermsOptions{},
		)
		want := `not a code host of the account: want "https://github.com/" but have "https://gitlab.com/"`
		got := fmt.Sprintf("%v", err)
		if got != want {
			t.Fatalf("err: want %q but got %q", want, got)
		}
	})

	// The OAuthProvider uses the gitlab.Client under the hood,
	// which uses rcache, a caching layer that uses Redis.
	// We need to clear the cache before we run the tests
	rcache.SetupForTest(t)

	p := newOAuthProvider(
		OAuthProviderOp{
			BaseURL: mustURL(t, "https://gitlab.com"),
			Token:   "admin_token",
			DB:      database.NewMockDB(),
		},
		&mockDoer{
			do: func(r *http.Request) (*http.Response, error) {
				visibility := r.URL.Query().Get("visibility")
				if visibility != "private" && visibility != "internal" {
					return nil, errors.Errorf("URL visibility: want private or internal, got %s", visibility)
				}
				want := fmt.Sprintf("https://gitlab.com/api/v4/projects?min_access_level=20&per_page=100&visibility=%s", visibility)
				if r.URL.String() != want {
					return nil, errors.Errorf("URL: want %q but got %q", want, r.URL)
				}

				want = "Bearer my_access_token"
				got := r.Header.Get("Authorization")
				if got != want {
					return nil, errors.Errorf("HTTP Authorization: want %q but got %q", want, got)
				}

				body := `[{"id": 1}, {"id": 2}]`
				if visibility == "internal" {
					body = `[{"id": 3}]`
				}
				return &http.Response{
					Status:     http.StatusText(http.StatusOK),
					StatusCode: http.StatusOK,
					Body:       io.NopCloser(bytes.NewReader([]byte(body))),
				}, nil
			},
		},
	)

	gitlab.MockGetOAuthContext = func() *oauthutil.OAuthContext {
		return &oauthutil.OAuthContext{
			ClientID:     "client",
			ClientSecret: "client_sec",
			Endpoint: oauth2.Endpoint{
				AuthURL:  "url/oauth/authorize",
				TokenURL: "url/oauth/token",
			},
			Scopes: []string{"read_user"},
		}
	}
	defer func() { gitlab.MockGetOAuthContext = nil }()

	authData := json.RawMessage(`{"access_token": "my_access_token"}`)
	repoIDs, err := p.FetchUserPerms(context.Background(),
		&extsvc.Account{
			AccountSpec: extsvc.AccountSpec{
				ServiceType: extsvc.TypeGitLab,
				ServiceID:   "https://gitlab.com/",
			},
			AccountData: extsvc.AccountData{
				AuthData: extsvc.NewUnencryptedData(authData),
			},
		},
		authz.FetchPermsOptions{},
	)
	if err != nil {
		t.Fatal(err)
	}

	expRepoIDs := []extsvc.RepoID{"1", "2", "3"}
	if diff := cmp.Diff(expRepoIDs, repoIDs.Exacts); diff != "" {
		t.Fatal(diff)
	}
}

func TestOAuthProvider_FetchRepoPerms(t *testing.T) {
	t.Run("token type PAT", func(t *testing.T) {
		p := newOAuthProvider(
			OAuthProviderOp{
				BaseURL:   mustURL(t, "https://gitlab.com"),
				Token:     "admin_token",
				TokenType: gitlab.TokenTypePAT,
			},
			nil,
		)

		_, err := p.FetchRepoPerms(context.Background(),
			&extsvc.Repository{
				URI: "gitlab.com/user/repo",
				ExternalRepoSpec: api.ExternalRepoSpec{
					ServiceType: "gitlab",
					ServiceID:   "https://gitlab.com/",
					ID:          "gitlab_project_id",
				},
			},
			authz.FetchPermsOptions{},
		)
		require.ErrorIs(t, err, &authz.ErrUnimplemented{})
	})
}
