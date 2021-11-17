package graphqlbackend

import (
	"context"
	"fmt"
	"sync"

	"github.com/graph-gophers/graphql-go"

	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/database"
)

type AuthzResolver interface {
	// Mutations
	SetRepositoryPermissionsForUsers(ctx context.Context, args *RepoPermsArgs) (*EmptyResponse, error)
	ScheduleRepositoryPermissionsSync(ctx context.Context, args *RepositoryIDArgs) (*EmptyResponse, error)
	ScheduleUserPermissionsSync(ctx context.Context, args *UserPermissionsSyncArgs) (*EmptyResponse, error)

	// Queries
	AuthorizedUserRepositories(ctx context.Context, args *AuthorizedRepoArgs) (RepositoryConnectionResolver, error)
	UsersWithPendingPermissions(ctx context.Context) ([]string, error)
	AuthorizedUsers(ctx context.Context, args *RepoAuthorizedUserArgs) (UserConnectionResolver, error)

	// Helpers
	RepositoryPermissionsInfo(ctx context.Context, repoID graphql.ID) (PermissionsInfoResolver, error)
	UserPermissionsInfo(ctx context.Context, userID graphql.ID) (PermissionsInfoResolver, error)
}

type RepositoryIDArgs struct {
	Repository graphql.ID
}

type UserPermissionsSyncArgs struct {
	User    graphql.ID
	Options *struct {
		InvalidateCaches *bool
	}
}

type RepoPermsArgs struct {
	Repository      graphql.ID
	UserPermissions []struct {
		BindID     string
		Permission string
	}
}

type AuthorizedRepoArgs struct {
	Username *string
	Email    *string
	Perm     string
	First    int32
	After    *string
}

type PermissionsInfoResolver interface {
	Permissions() []string
	SyncedAt() *DateTime
	UpdatedAt() DateTime
}

var subRepoOnce sync.Once
var subRepoClient *authz.SubRepoPermsClient

// subRepoPermsClient returns a reusable instance of the
// authz.SubRepoPermissionChecker that maintains a shared cache.
func subRepoPermsClient(db database.DB) authz.SubRepoPermissionChecker {
	subRepoOnce.Do(func() {
		var err error
		subRepoClient, err = authz.NewSubRepoPermsClient(database.SubRepoPerms(db))
		if err != nil {
			// We expect creating a client to always succeed. If not, it is due to an error
			// in our code when instantiating it.
			panic(fmt.Sprintf("creating SubRepoPermsClient: %v", err))
		}
	})
	return subRepoClient.WithGetter(db.SubRepoPerms())
}
