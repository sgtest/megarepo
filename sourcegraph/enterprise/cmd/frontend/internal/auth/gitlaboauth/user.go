package gitlaboauth

import (
	"context"

	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitlab"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// unexported key type prevents collisions
type key int

const userKey key = iota

// WithUser returns a copy of ctx that stores the GitLab User.
func WithUser(ctx context.Context, user *gitlab.AuthUser) context.Context {
	return context.WithValue(ctx, userKey, user)
}

// UserFromContext returns the GitLab User from the ctx.
func UserFromContext(ctx context.Context) (*gitlab.AuthUser, error) {
	user, ok := ctx.Value(userKey).(*gitlab.AuthUser)
	if !ok {
		return nil, errors.Errorf("gitlab: Context missing GitLab User")
	}
	return user, nil
}
