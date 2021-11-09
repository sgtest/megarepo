package app

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/cockroachdb/errors"
	mockrequire "github.com/derision-test/go-mockgen/testutil/require"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/database/dbmock"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestServeVerifyEmail(t *testing.T) {
	t.Run("primary email is already set", func(t *testing.T) {
		users := dbmock.NewMockUserStore()
		users.GetByCurrentAuthUserFunc.SetDefaultReturn(&types.User{ID: 1}, nil)

		userEmails := dbmock.NewMockUserEmailsStore()
		userEmails.GetFunc.SetDefaultReturn("alice@example.com", false, nil)
		userEmails.VerifyFunc.SetDefaultReturn(true, nil)
		userEmails.GetPrimaryEmailFunc.SetDefaultReturn("alice@example.com", true, nil)
		userEmails.SetPrimaryEmailFunc.SetDefaultReturn(nil)

		authz := dbmock.NewMockAuthzStore()
		authz.GrantPendingPermissionsFunc.SetDefaultReturn(nil)

		db := dbmock.NewMockDB()
		db.UsersFunc.SetDefaultReturn(users)
		db.UserEmailsFunc.SetDefaultReturn(userEmails)
		db.AuthzFunc.SetDefaultReturn(authz)

		ctx := context.Background()
		ctx = actor.WithActor(ctx, &actor.Actor{UID: 1})

		req := httptest.NewRequest(http.MethodGet, "/", nil)
		req = req.WithContext(ctx)
		resp := httptest.NewRecorder()

		handler := serveVerifyEmail(db)
		handler(resp, req)

		mockrequire.NotCalled(t, userEmails.SetPrimaryEmailFunc)
	})

	t.Run("primary email is not set", func(t *testing.T) {
		users := dbmock.NewMockUserStore()
		users.GetByCurrentAuthUserFunc.SetDefaultReturn(&types.User{ID: 1}, nil)

		userEmails := dbmock.NewMockUserEmailsStore()
		userEmails.GetFunc.SetDefaultReturn("alice@example.com", false, nil)
		userEmails.VerifyFunc.SetDefaultReturn(true, nil)
		userEmails.GetPrimaryEmailFunc.SetDefaultReturn("", false, errors.New("primary email not found"))
		userEmails.SetPrimaryEmailFunc.SetDefaultReturn(nil)

		authz := dbmock.NewMockAuthzStore()
		authz.GrantPendingPermissionsFunc.SetDefaultReturn(nil)

		db := dbmock.NewMockDB()
		db.UsersFunc.SetDefaultReturn(users)
		db.UserEmailsFunc.SetDefaultReturn(userEmails)
		db.AuthzFunc.SetDefaultReturn(authz)

		ctx := context.Background()
		ctx = actor.WithActor(ctx, &actor.Actor{UID: 1})

		req := httptest.NewRequest(http.MethodGet, "/", nil)
		req = req.WithContext(ctx)
		resp := httptest.NewRecorder()

		handler := serveVerifyEmail(db)
		handler(resp, req)

		mockrequire.Called(t, userEmails.SetPrimaryEmailFunc)
	})
}
