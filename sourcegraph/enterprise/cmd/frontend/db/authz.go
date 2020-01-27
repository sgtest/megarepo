package db

import (
	"context"
	"fmt"
	"time"

	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/globals"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	iauthz "github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/db/dbutil"
)

// NewAuthzStore returns an OSS db.AuthzStore set with enterprise implementation.
func NewAuthzStore(db dbutil.DB, clock func() time.Time) db.AuthzStore {
	return &authzStore{
		store: NewPermsStore(db, clock),
	}
}

type authzStore struct {
	store *PermsStore
}

// GrantPendingPermissions grants pending permissions for a user, which implements the db.AuthzStore interface.
// It uses provided arguments to retrieve information directly from the database to offload security concerns
// from the caller.
func (s *authzStore) GrantPendingPermissions(ctx context.Context, args *db.GrantPendingPermissionsArgs) error {
	if args.UserID <= 0 {
		return nil
	}

	// Note: It's possible that there are more than one verified emails associated to the user and all of them
	// have pending permissions due to any previous grant failures, we can safely grant all of them whenever
	// possible because permissions are unioned.
	var bindIDs []string

	// Note: we purposely don't check cfg.PermissionsUserMapping.Enabled here because admin could disable the
	// feature by mistake while a user has valid pending permissions.
	cfg := globals.PermissionsUserMapping()
	switch cfg.BindID {
	case "email":
		// 🚨 SECURITY: It is critical to ensure only grant emails that are verified.
		emails, err := db.UserEmails.ListByUser(ctx, db.UserEmailsListOptions{
			UserID:       args.UserID,
			OnlyVerified: true,
		})
		if err != nil {
			return errors.Wrap(err, "list verified emails")
		}
		bindIDs = make([]string, len(emails))
		for i := range emails {
			bindIDs[i] = emails[i].Email
		}

	case "username":
		user, err := db.Users.GetByID(ctx, args.UserID)
		if err != nil {
			return errors.Wrap(err, "get user")
		}
		bindIDs = append(bindIDs, user.Username)

	default:
		return fmt.Errorf("unrecognized user mapping bind ID type %q", cfg.BindID)
	}

	txs, err := s.store.Transact(ctx)
	if err != nil {
		return errors.Wrap(err, "start transaction")
	}
	defer txs.Done(&err)

	for _, bindID := range bindIDs {
		err = txs.GrantPendingPermissions(ctx, args.UserID, &iauthz.UserPendingPermissions{
			BindID: bindID,
			Perm:   args.Perm,
			Type:   args.Type,
		})
		if err != nil {
			return errors.Wrap(err, "grant pending permissions")
		}
	}

	return nil
}

// AuthorizedRepos checks if a user is authorized to access repositories in the candidate list,
// which implements the db.AuthzStore interface.
func (s *authzStore) AuthorizedRepos(ctx context.Context, args *db.AuthorizedReposArgs) ([]*types.Repo, error) {
	if len(args.Repos) == 0 {
		return args.Repos, nil
	}

	p := &iauthz.UserPermissions{
		UserID:   args.UserID,
		Perm:     args.Perm,
		Type:     args.Type,
		Provider: args.Provider,
	}
	if err := s.store.LoadUserPermissions(ctx, p); err != nil {
		if err == ErrPermsNotFound {
			return []*types.Repo{}, nil
		}
		return nil, err
	}

	perms := p.AuthorizedRepos(args.Repos)
	filtered := make([]*types.Repo, len(perms))
	for i, r := range perms {
		filtered[i] = r.Repo
	}
	return filtered, nil
}

// RevokeUserPermissions deletes both effective and pending permissions that could be related to a user,
// which implements the db.AuthzStore interface. It proactively clean up left-over pending permissions to
// prevent accidental reuse (i.e. another user with same username or email address(es) but not the same person).
func (s *authzStore) RevokeUserPermissions(ctx context.Context, args *db.RevokeUserPermissionsArgs) error {
	txs, err := s.store.Transact(ctx)
	if err != nil {
		return errors.Wrap(err, "start transaction")
	}
	defer txs.Done(&err)

	if err = txs.DeleteAllUserPermissions(ctx, args.UserID); err != nil {
		return err
	}

	bindIDs := append([]string{args.Username}, args.VerifiedEmails...)
	if err := txs.DeleteAllUserPendingPermissions(ctx, bindIDs); err != nil {
		return err
	}
	return nil
}
