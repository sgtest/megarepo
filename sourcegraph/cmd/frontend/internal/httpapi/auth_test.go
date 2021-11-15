package httpapi

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"net/url"
	"testing"

	"github.com/cockroachdb/errors"
	mockrequire "github.com/derision-test/go-mockgen/testutil/require"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbmock"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestAccessTokenAuthMiddleware(t *testing.T) {
	newHandler := func(db database.DB) http.Handler {
		return AccessTokenAuthMiddleware(db, http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			actor := actor.FromContext(r.Context())
			if actor.IsAuthenticated() {
				fmt.Fprintf(w, "user %v", actor.UID)
			} else {
				fmt.Fprint(w, "no user")
			}
		}))
	}

	checkHTTPResponse := func(t *testing.T, db database.DB, req *http.Request, wantStatusCode int, wantBody string) {
		rr := httptest.NewRecorder()
		newHandler(db).ServeHTTP(rr, req)
		if rr.Code != wantStatusCode {
			t.Errorf("got response status %d, want %d", rr.Code, wantStatusCode)
		}
		if got := rr.Body.String(); got != wantBody {
			t.Errorf("got response body %q, want %q", got, wantBody)
		}
	}

	t.Run("no header", func(t *testing.T) {
		req, _ := http.NewRequest("GET", "/", nil)
		checkHTTPResponse(t, dbmock.NewMockDB(), req, http.StatusOK, "no user")
	})

	// Test that the absence of an Authorization header doesn't unset the actor provided by a prior
	// auth middleware.
	t.Run("no header, actor present", func(t *testing.T) {
		req, _ := http.NewRequest("GET", "/", nil)
		req = req.WithContext(actor.WithActor(context.Background(), &actor.Actor{UID: 123}))
		checkHTTPResponse(t, dbmock.NewMockDB(), req, http.StatusOK, "user 123")
	})

	for _, unrecognizedHeaderValue := range []string{"x", "x y", "Basic abcd"} {
		t.Run("unrecognized header "+unrecognizedHeaderValue, func(t *testing.T) {
			req, _ := http.NewRequest("GET", "/", nil)
			req.Header.Set("Authorization", unrecognizedHeaderValue)
			checkHTTPResponse(t, dbmock.NewMockDB(), req, http.StatusOK, "no user")
		})
	}

	for _, invalidHeaderValue := range []string{"token-sudo abc", `token-sudo token=""`, "token "} {
		t.Run("invalid header "+invalidHeaderValue, func(t *testing.T) {
			req, _ := http.NewRequest("GET", "/", nil)
			req.Header.Set("Authorization", invalidHeaderValue)
			checkHTTPResponse(t, dbmock.NewMockDB(), req, http.StatusUnauthorized, "Invalid Authorization header.\n")
		})
	}

	t.Run("valid header with invalid token", func(t *testing.T) {
		req, _ := http.NewRequest("GET", "/", nil)
		req.Header.Set("Authorization", "token badbad")

		accessTokens := dbmock.NewMockAccessTokenStore()
		accessTokens.LookupFunc.SetDefaultReturn(0, errors.New("x"))
		db := dbmock.NewMockDB()
		db.AccessTokensFunc.SetDefaultReturn(accessTokens)

		checkHTTPResponse(t, db, req, http.StatusUnauthorized, "Invalid access token.\n")
		mockrequire.Called(t, accessTokens.LookupFunc)
	})

	for _, headerValue := range []string{"token abcdef", `token token="abcdef"`} {
		t.Run("valid non-sudo token: "+headerValue, func(t *testing.T) {
			req, _ := http.NewRequest("GET", "/", nil)
			req.Header.Set("Authorization", headerValue)

			accessTokens := dbmock.NewMockAccessTokenStore()
			accessTokens.LookupFunc.SetDefaultHook(func(_ context.Context, tokenHexEncoded, requiredScope string) (subjectUserID int32, err error) {
				if want := "abcdef"; tokenHexEncoded != want {
					t.Errorf("got %q, want %q", tokenHexEncoded, want)
				}
				if want := authz.ScopeUserAll; requiredScope != want {
					t.Errorf("got %q, want %q", requiredScope, want)
				}
				return 123, nil
			})
			db := dbmock.NewMockDB()
			db.AccessTokensFunc.SetDefaultReturn(accessTokens)

			checkHTTPResponse(t, db, req, http.StatusOK, "user 123")
			mockrequire.Called(t, accessTokens.LookupFunc)
		})
	}

	// Test that an access token overwrites the actor set by a prior auth middleware.
	t.Run("actor present, valid non-sudo token", func(t *testing.T) {
		req, _ := http.NewRequest("GET", "/", nil)
		req.Header.Set("Authorization", "token abcdef")
		req = req.WithContext(actor.WithActor(context.Background(), &actor.Actor{UID: 456}))

		accessTokens := dbmock.NewMockAccessTokenStore()
		accessTokens.LookupFunc.SetDefaultHook(func(_ context.Context, tokenHexEncoded, requiredScope string) (subjectUserID int32, err error) {
			if want := "abcdef"; tokenHexEncoded != want {
				t.Errorf("got %q, want %q", tokenHexEncoded, want)
			}
			if want := authz.ScopeUserAll; requiredScope != want {
				t.Errorf("got %q, want %q", requiredScope, want)
			}
			return 123, nil
		})
		db := dbmock.NewMockDB()
		db.AccessTokensFunc.SetDefaultReturn(accessTokens)

		checkHTTPResponse(t, db, req, http.StatusOK, "user 123")
		mockrequire.Called(t, accessTokens.LookupFunc)
	})

	// Test that an access token overwrites the actor set by a prior auth middleware.
	const (
		sourceQueryParam = "query-param"
		sourceBasicAuth  = "basic-auth"
	)
	for _, source := range []string{sourceQueryParam, sourceBasicAuth} {
		t.Run("actor present, valid non-sudo token in "+source, func(t *testing.T) {
			req, _ := http.NewRequest("GET", "/", nil)
			if source == sourceQueryParam {
				q := url.Values{}
				q.Add("token", "abcdef")
				req.URL.RawQuery = q.Encode()
			} else {
				req.SetBasicAuth("abcdef", "")
			}
			req = req.WithContext(actor.WithActor(context.Background(), &actor.Actor{UID: 456}))

			accessTokens := dbmock.NewMockAccessTokenStore()
			accessTokens.LookupFunc.SetDefaultHook(func(_ context.Context, tokenHexEncoded, requiredScope string) (subjectUserID int32, err error) {
				if want := "abcdef"; tokenHexEncoded != want {
					t.Errorf("got %q, want %q", tokenHexEncoded, want)
				}
				if want := authz.ScopeUserAll; requiredScope != want {
					t.Errorf("got %q, want %q", requiredScope, want)
				}
				return 123, nil
			})
			db := dbmock.NewMockDB()
			db.AccessTokensFunc.SetDefaultReturn(accessTokens)

			checkHTTPResponse(t, db, req, http.StatusOK, "user 123")
			mockrequire.Called(t, accessTokens.LookupFunc)
		})
	}

	t.Run("valid sudo token", func(t *testing.T) {
		req, _ := http.NewRequest("GET", "/", nil)
		req.Header.Set("Authorization", `token-sudo token="abcdef",user="alice"`)

		accessTokens := dbmock.NewMockAccessTokenStore()
		accessTokens.LookupFunc.SetDefaultHook(func(_ context.Context, tokenHexEncoded, requiredScope string) (subjectUserID int32, err error) {
			if want := "abcdef"; tokenHexEncoded != want {
				t.Errorf("got %q, want %q", tokenHexEncoded, want)
			}
			if want := authz.ScopeSiteAdminSudo; requiredScope != want {
				t.Errorf("got %q, want %q", requiredScope, want)
			}
			return 123, nil
		})

		users := dbmock.NewMockUserStore()
		users.GetByIDFunc.SetDefaultHook(func(ctx context.Context, userID int32) (*types.User, error) {
			if want := int32(123); userID != want {
				t.Errorf("got %d, want %d", userID, want)
			}
			return &types.User{ID: userID, SiteAdmin: true}, nil
		})
		users.GetByUsernameFunc.SetDefaultHook(func(ctx context.Context, username string) (*types.User, error) {
			if want := "alice"; username != want {
				t.Errorf("got %q, want %q", username, want)
			}
			return &types.User{ID: 456, SiteAdmin: true}, nil
		})

		db := dbmock.NewMockDB()
		db.AccessTokensFunc.SetDefaultReturn(accessTokens)
		db.UsersFunc.SetDefaultReturn(users)

		checkHTTPResponse(t, db, req, http.StatusOK, "user 456")
		mockrequire.Called(t, accessTokens.LookupFunc)
		mockrequire.Called(t, users.GetByIDFunc)
		mockrequire.Called(t, users.GetByUsernameFunc)
	})

	// Test that if a sudo token's subject user is not a site admin (which means they were demoted
	// from site admin AFTER the token was created), then the sudo token is invalid.
	t.Run("valid sudo token, subject is not site admin", func(t *testing.T) {
		req, _ := http.NewRequest("GET", "/", nil)
		req.Header.Set("Authorization", `token-sudo token="abcdef",user="alice"`)

		accessTokens := dbmock.NewMockAccessTokenStore()
		accessTokens.LookupFunc.SetDefaultHook(func(_ context.Context, tokenHexEncoded, requiredScope string) (subjectUserID int32, err error) {
			if want := "abcdef"; tokenHexEncoded != want {
				t.Errorf("got %q, want %q", tokenHexEncoded, want)
			}
			if want := authz.ScopeSiteAdminSudo; requiredScope != want {
				t.Errorf("got %q, want %q", requiredScope, want)
			}
			return 123, nil
		})

		users := dbmock.NewMockUserStore()
		users.GetByIDFunc.SetDefaultHook(func(ctx context.Context, userID int32) (*types.User, error) {
			if want := int32(123); userID != want {
				t.Errorf("got %d, want %d", userID, want)
			}
			return &types.User{ID: userID, SiteAdmin: false}, nil
		})

		db := dbmock.NewMockDB()
		db.AccessTokensFunc.SetDefaultReturn(accessTokens)
		db.UsersFunc.SetDefaultReturn(users)

		checkHTTPResponse(t, db, req, http.StatusForbidden, "The subject user of a sudo access token must be a site admin.\n")
		mockrequire.Called(t, accessTokens.LookupFunc)
		mockrequire.Called(t, users.GetByIDFunc)
	})

	t.Run("valid sudo token, invalid sudo user", func(t *testing.T) {
		req, _ := http.NewRequest("GET", "/", nil)
		req.Header.Set("Authorization", `token-sudo token="abcdef",user="doesntexist"`)

		accessTokens := dbmock.NewMockAccessTokenStore()
		accessTokens.LookupFunc.SetDefaultHook(func(_ context.Context, tokenHexEncoded, requiredScope string) (subjectUserID int32, err error) {
			if want := "abcdef"; tokenHexEncoded != want {
				t.Errorf("got %q, want %q", tokenHexEncoded, want)
			}
			if want := authz.ScopeSiteAdminSudo; requiredScope != want {
				t.Errorf("got %q, want %q", requiredScope, want)
			}
			return 123, nil
		})

		users := dbmock.NewMockUserStore()
		users.GetByIDFunc.SetDefaultHook(func(ctx context.Context, userID int32) (*types.User, error) {
			if want := int32(123); userID != want {
				t.Errorf("got %d, want %d", userID, want)
			}
			return &types.User{ID: userID, SiteAdmin: true}, nil
		})
		users.GetByUsernameFunc.SetDefaultHook(func(ctx context.Context, username string) (*types.User, error) {
			if want := "doesntexist"; username != want {
				t.Errorf("got %q, want %q", username, want)
			}
			return nil, &errcode.Mock{IsNotFound: true}
		})

		db := dbmock.NewMockDB()
		db.AccessTokensFunc.SetDefaultReturn(accessTokens)
		db.UsersFunc.SetDefaultReturn(users)

		checkHTTPResponse(t, db, req, http.StatusForbidden, "Unable to sudo to nonexistent user.\n")
		mockrequire.Called(t, accessTokens.LookupFunc)
		mockrequire.Called(t, users.GetByIDFunc)
		mockrequire.Called(t, users.GetByUsernameFunc)
	})
}
