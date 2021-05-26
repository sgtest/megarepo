package backend

import (
	"context"
	"errors"
	"fmt"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

var ErrMustBeSiteAdmin = errors.New("must be site admin")

// CheckCurrentUserIsSiteAdmin returns an error if the current user is NOT a site admin.
func CheckCurrentUserIsSiteAdmin(ctx context.Context, db dbutil.DB) error {
	if hasAuthzBypass(ctx) {
		return nil
	}
	user, err := CurrentUser(ctx, db)
	if err != nil {
		return err
	}
	if user == nil {
		return ErrNotAuthenticated
	}
	if !user.SiteAdmin {
		return ErrMustBeSiteAdmin
	}
	return nil
}

// CheckUserIsSiteAdmin returns an error if the user is NOT a site admin.
func CheckUserIsSiteAdmin(ctx context.Context, db dbutil.DB, userID int32) error {
	if hasAuthzBypass(ctx) {
		return nil
	}
	user, err := database.Users(db).GetByID(ctx, userID)
	if err != nil {
		return err
	}
	if user == nil {
		return ErrNotAuthenticated
	}
	if !user.SiteAdmin {
		return ErrMustBeSiteAdmin
	}
	return nil
}

// InsufficientAuthorizationError is an error that occurs when the authentication is technically valid
// (e.g., the token is not expired) but does not yield a user with privileges to perform a certain
// action.
type InsufficientAuthorizationError struct {
	Message string
}

func (e *InsufficientAuthorizationError) Error() string      { return e.Message }
func (e *InsufficientAuthorizationError) Unauthorized() bool { return true }

// CheckSiteAdminOrSameUser returns an error if the user is NEITHER (1) a
// site admin NOR (2) the user specified by subjectUserID.
//
// It is used when an action on a user can be performed by site admins and the
// user themselves, but nobody else.
//
// Returns an error containing the name of the given user.
func CheckSiteAdminOrSameUser(ctx context.Context, db dbutil.DB, subjectUserID int32) error {
	if hasAuthzBypass(ctx) {
		return nil
	}
	a := actor.FromContext(ctx)
	if a.IsAuthenticated() && a.UID == subjectUserID {
		return nil
	}
	isSiteAdminErr := CheckCurrentUserIsSiteAdmin(ctx, db)
	if isSiteAdminErr == nil {
		return nil
	}
	subjectUser, err := database.Users(db).GetByID(ctx, subjectUserID)
	if err != nil {
		return &InsufficientAuthorizationError{fmt.Sprintf("must be authenticated as an admin (%s)", isSiteAdminErr.Error())}
	}
	return &InsufficientAuthorizationError{fmt.Sprintf("must be authenticated as %s or as an admin (%s)", subjectUser.Username, isSiteAdminErr.Error())}
}

// CheckSameUser returns an error if the user is not the user specified by
// subjectUserID.
func CheckSameUser(ctx context.Context, subjectUserID int32) error {
	if hasAuthzBypass(ctx) {
		return nil
	}
	a := actor.FromContext(ctx)
	if a.IsAuthenticated() && a.UID == subjectUserID {
		return nil
	}
	return &InsufficientAuthorizationError{Message: fmt.Sprintf("Must be authenticated as user with id %d", subjectUserID)}
}

// CurrentUser gets the current authenticated user
// It returns nil, nil if no user is found
func CurrentUser(ctx context.Context, db dbutil.DB) (*types.User, error) {
	user, err := database.Users(db).GetByCurrentAuthUser(ctx)
	if err != nil {
		if errcode.IsNotFound(err) || err == database.ErrNoCurrentUser {
			return nil, nil
		}
		return nil, err
	}
	return user, nil
}

// WithAuthzBypass returns a context that backend.CheckXyz funcs report as being a site admin. It
// is used to bypass the backend.CheckXyz access control funcs when needed.
//
// 🚨 SECURITY: The caller MUST ensure that it performs its own access controls or removal of
// sensitive data.
func WithAuthzBypass(ctx context.Context) context.Context {
	return context.WithValue(ctx, authzBypass, struct{}{})
}

func hasAuthzBypass(ctx context.Context) bool {
	return ctx.Value(authzBypass) != nil
}

type contextKey int

const authzBypass contextKey = iota
