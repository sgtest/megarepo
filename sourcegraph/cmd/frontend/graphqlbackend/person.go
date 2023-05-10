package graphqlbackend

import (
	"context"
	"strings"
	"sync"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

type PersonResolver struct {
	db    database.DB
	name  string
	email string

	// fetch + serve sourcegraph stored user information
	includeUserInfo bool

	// cache result because it is used by multiple fields
	once sync.Once
	user *types.User
	err  error
}

func NewPersonResolver(db database.DB, name, email string, includeUserInfo bool) *PersonResolver {
	return &PersonResolver{
		db:              db,
		name:            name,
		email:           email,
		includeUserInfo: includeUserInfo,
	}
}

func NewPersonResolverFromUser(db database.DB, email string, user *types.User) *PersonResolver {
	return &PersonResolver{
		db:    db,
		user:  user,
		email: email,
		// We don't need to query for user.
		includeUserInfo: false,
	}
}

// resolveUser resolves the person to a user (using the email address). Not all persons can be
// resolved to a user.
func (r *PersonResolver) resolveUser(ctx context.Context) (*types.User, error) {
	r.once.Do(func() {
		if r.includeUserInfo && r.email != "" {
			r.user, r.err = r.db.Users().GetByVerifiedEmail(ctx, r.email)
			if errcode.IsNotFound(r.err) {
				r.err = nil
			}
		}
	})
	return r.user, r.err
}

func (r *PersonResolver) Name(ctx context.Context) (string, error) {
	user, err := r.resolveUser(ctx)
	if err != nil {
		return "", err
	}
	if user != nil && user.Username != "" {
		return user.Username, nil
	}

	// Fall back to provided username.
	return r.name, nil
}

func (r *PersonResolver) Email() string {
	return r.email
}

func (r *PersonResolver) DisplayName(ctx context.Context) (string, error) {
	user, err := r.resolveUser(ctx)
	if err != nil {
		return "", err
	}
	if user != nil && user.DisplayName != "" {
		return user.DisplayName, nil
	}

	if name := strings.TrimSpace(r.name); name != "" {
		return name, nil
	}
	if r.email != "" {
		return r.email, nil
	}
	return "unknown", nil
}

func (r *PersonResolver) AvatarURL(ctx context.Context) (*string, error) {
	user, err := r.resolveUser(ctx)
	if err != nil {
		return nil, err
	}
	if user != nil && user.AvatarURL != "" {
		return &user.AvatarURL, nil
	}
	return nil, nil
}

func (r *PersonResolver) User(ctx context.Context) (*UserResolver, error) {
	user, err := r.resolveUser(ctx)
	if user == nil || err != nil {
		return nil, err
	}
	return NewUserResolver(ctx, r.db, user), nil
}

func (r *PersonResolver) OwnerField() string {
	return EnterpriseResolvers.ownResolver.PersonOwnerField(r)
}
