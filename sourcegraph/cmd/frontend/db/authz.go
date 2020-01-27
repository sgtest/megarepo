package db

import (
	"context"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/authz"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
)

// GrantPendingPermissionsArgs contains required arguments to grant pending permissions for a user
// by username or verified email address(es) according to the site configuration.
type GrantPendingPermissionsArgs struct {
	// The user ID that will be used to bind pending permissions.
	UserID int32
	// The permission level to be granted.
	Perm authz.Perms
	// The type of permissions to be granted.
	Type authz.PermType
}

// AuthorizedReposArgs contains required arguments to verify if a user is authorized to access some
// or all of the repositories from the candidate list with the given level and type of permissions.
type AuthorizedReposArgs struct {
	// The candidate list of repositories to be verified.
	Repos []*types.Repo
	// The user whose authorization to access the repos is being checked.
	UserID int32
	// The permission level to be verified.
	Perm authz.Perms
	// The type of permissions to be verified.
	Type authz.PermType
	// The type of authz provider to load user permissions.
	Provider authz.ProviderType
}

// RevokeUserPermissionsArgs contains required arguments to revoke user permissions, it includes all
// possible leads to grant or authorize access for a user.
type RevokeUserPermissionsArgs struct {
	// The user ID that will be used to revoke effective permissions.
	UserID int32
	// The username that will be used to revoke pending permissions.
	Username string
	// The list of verified emails that will be used to revoke pending permissions.
	// Only provide verified emails because that is the definitive way to ensure the email ownership.
	VerifiedEmails []string
}

// AuthzStore contains methods for manipulating user permissions.
type AuthzStore interface {
	// GrantPendingPermissions grants pending permissions for a user. It is a no-op in the OSS version.
	GrantPendingPermissions(ctx context.Context, args *GrantPendingPermissionsArgs) error
	// AuthorizedRepos checks if a user is authorized to access repositories in the candidate list.
	// The returned list must be a list of repositories that are authorized to the given user.
	// It is a no-op in the OSS version.
	AuthorizedRepos(ctx context.Context, args *AuthorizedReposArgs) ([]*types.Repo, error)
	// RevokeUserPermissions deletes both effective and pending permissions that could be related to a user.
	// It is a no-op in the OSS version.
	RevokeUserPermissions(ctx context.Context, args *RevokeUserPermissionsArgs) error
}

// authzStore is a no-op placeholder for the OSS version.
type authzStore struct{}

func (*authzStore) GrantPendingPermissions(ctx context.Context, args *GrantPendingPermissionsArgs) error {
	if Mocks.Authz.GrantPendingPermissions != nil {
		return Mocks.Authz.GrantPendingPermissions(ctx, args)
	}
	return nil
}

func (*authzStore) AuthorizedRepos(ctx context.Context, args *AuthorizedReposArgs) ([]*types.Repo, error) {
	if Mocks.Authz.AuthorizedRepos != nil {
		return Mocks.Authz.AuthorizedRepos(ctx, args)
	}
	return []*types.Repo{}, nil
}

func (*authzStore) RevokeUserPermissions(ctx context.Context, args *RevokeUserPermissionsArgs) error {
	if Mocks.Authz.RevokeUserPermissions != nil {
		return Mocks.Authz.RevokeUserPermissions(ctx, args)
	}
	return nil
}
