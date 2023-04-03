package resolvers

import (
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/graph-gophers/graphql-go"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/actor"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/gqlutil"
)

type permissionsInfoResolver struct {
	db           edb.EnterpriseDB
	ossDB        database.DB
	userID       int32
	repoID       api.RepoID
	perms        authz.Perms
	syncedAt     time.Time
	updatedAt    time.Time
	unrestricted bool
}

func (r *permissionsInfoResolver) Permissions() []string {
	return strings.Split(strings.ToUpper(r.perms.String()), ",")
}

func (r *permissionsInfoResolver) SyncedAt() *gqlutil.DateTime {
	if r.syncedAt.IsZero() {
		return nil
	}
	return &gqlutil.DateTime{Time: r.syncedAt}
}

func (r *permissionsInfoResolver) UpdatedAt() *gqlutil.DateTime {
	return gqlutil.FromTime(r.updatedAt)
}

func (r *permissionsInfoResolver) Unrestricted(_ context.Context) bool {
	return r.unrestricted
}

var permissionsInfoRepositoryConnectionMaxPageSize = 100

var permissionsInfoRepositoryConnectionOptions = &graphqlutil.ConnectionResolverOptions{
	OrderBy:     database.OrderBy{{Field: "repo.name"}},
	Ascending:   true,
	MaxPageSize: &permissionsInfoRepositoryConnectionMaxPageSize,
}

func (r *permissionsInfoResolver) Repositories(_ context.Context, args graphqlbackend.PermissionsInfoRepositoriesArgs) (*graphqlutil.ConnectionResolver[graphqlbackend.PermissionsInfoRepositoryResolver], error) {
	if r.userID == 0 {
		return nil, nil
	}

	query := ""
	if args.Query != nil {
		query = *args.Query
	}

	connectionStore := &permissionsInfoRepositoriesStore{
		userID: r.userID,
		db:     r.db,
		ossDB:  r.ossDB,
		query:  query,
	}

	return graphqlutil.NewConnectionResolver[graphqlbackend.PermissionsInfoRepositoryResolver](connectionStore, &args.ConnectionResolverArgs, permissionsInfoRepositoryConnectionOptions)
}

type permissionsInfoRepositoriesStore struct {
	userID int32
	db     edb.EnterpriseDB
	ossDB  database.DB
	query  string
}

func (s *permissionsInfoRepositoriesStore) MarshalCursor(node graphqlbackend.PermissionsInfoRepositoryResolver, _ database.OrderBy) (*string, error) {
	cursor := node.Repository().Name()

	return &cursor, nil
}

func (s *permissionsInfoRepositoriesStore) UnmarshalCursor(cursor string, _ database.OrderBy) (*string, error) {
	cursorSQL := fmt.Sprintf("'%s'", cursor)

	return &cursorSQL, nil
}

func (s *permissionsInfoRepositoriesStore) ComputeTotal(ctx context.Context) (*int32, error) {
	count, err := s.ossDB.Repos().Count(actor.WithActor(ctx, actor.FromUser(s.userID)), database.ReposListOptions{Query: s.query})
	if err != nil {
		return nil, err
	}

	total := int32(count)
	return &total, nil
}

func (s *permissionsInfoRepositoriesStore) ComputeNodes(ctx context.Context, args *database.PaginationArgs) ([]graphqlbackend.PermissionsInfoRepositoryResolver, error) {
	permissions, err := s.db.Perms().ListUserPermissions(ctx, s.userID, &edb.ListUserPermissionsArgs{Query: s.query, PaginationArgs: args})
	if err != nil {
		return nil, err
	}

	var permissionResolvers []graphqlbackend.PermissionsInfoRepositoryResolver
	for _, perm := range permissions {
		permissionResolvers = append(permissionResolvers, permissionsInfoRepositoryResolver{perm: perm, db: s.ossDB})
	}

	return permissionResolvers, nil
}

type permissionsInfoRepositoryResolver struct {
	db   database.DB
	perm *edb.UserPermission
}

func (r permissionsInfoRepositoryResolver) ID() graphql.ID {
	return graphqlbackend.MarshalRepositoryID(r.perm.Repo.ID)
}

func (r permissionsInfoRepositoryResolver) Repository() *graphqlbackend.RepositoryResolver {
	return graphqlbackend.NewRepositoryResolver(r.db, gitserver.NewClient(), r.perm.Repo)
}

func (r permissionsInfoRepositoryResolver) Reason() string {
	return string(r.perm.Reason)
}

func (r permissionsInfoRepositoryResolver) UpdatedAt() *gqlutil.DateTime {
	return gqlutil.FromTime(r.perm.UpdatedAt)
}

var permissionsInfoUserConnectionMaxPageSize = 100

var permissionsInfoUserConnectionOptions = &graphqlutil.ConnectionResolverOptions{
	OrderBy:     database.OrderBy{{Field: "users.username"}},
	Ascending:   true,
	MaxPageSize: &permissionsInfoUserConnectionMaxPageSize,
}

func (r *permissionsInfoResolver) Users(ctx context.Context, args graphqlbackend.PermissionsInfoUsersArgs) (*graphqlutil.ConnectionResolver[graphqlbackend.PermissionsInfoUserResolver], error) {
	if r.repoID == 0 {
		return nil, nil
	}

	query := ""
	if args.Query != nil {
		query = *args.Query
	}

	connectionStore := &permissionsInfoUsersStore{
		ctx:    ctx,
		repoID: r.repoID,
		db:     r.db,
		ossDB:  r.ossDB,
		query:  query,
	}

	return graphqlutil.NewConnectionResolver[graphqlbackend.PermissionsInfoUserResolver](connectionStore, &args.ConnectionResolverArgs, permissionsInfoUserConnectionOptions)
}

type permissionsInfoUsersStore struct {
	ctx    context.Context
	repoID api.RepoID
	db     edb.EnterpriseDB
	ossDB  database.DB
	query  string
}

func (s *permissionsInfoUsersStore) MarshalCursor(node graphqlbackend.PermissionsInfoUserResolver, _ database.OrderBy) (*string, error) {
	cursor := node.User(s.ctx).Username()

	return &cursor, nil
}

func (s *permissionsInfoUsersStore) UnmarshalCursor(cursor string, _ database.OrderBy) (*string, error) {
	cursorSQL := fmt.Sprintf("'%s'", cursor)

	return &cursorSQL, nil
}

// TODO(naman): implement total count
func (s *permissionsInfoUsersStore) ComputeTotal(ctx context.Context) (*int32, error) {
	return nil, nil
}

func (s *permissionsInfoUsersStore) ComputeNodes(ctx context.Context, args *database.PaginationArgs) ([]graphqlbackend.PermissionsInfoUserResolver, error) {
	permissions, err := s.db.Perms().ListRepoPermissions(ctx, s.repoID, &edb.ListRepoPermissionsArgs{Query: s.query, PaginationArgs: args})
	if err != nil {
		return nil, err
	}

	permissionResolvers := make([]graphqlbackend.PermissionsInfoUserResolver, 0, len(permissions))
	for _, perm := range permissions {
		permissionResolvers = append(permissionResolvers, permissionsInfoUserResolver{perm: perm, db: s.ossDB})
	}

	return permissionResolvers, nil
}

type permissionsInfoUserResolver struct {
	db   database.DB
	perm *edb.RepoPermission
}

func (r permissionsInfoUserResolver) ID() graphql.ID {
	return graphqlbackend.MarshalUserID(r.perm.User.ID)
}

func (r permissionsInfoUserResolver) User(ctx context.Context) *graphqlbackend.UserResolver {
	return graphqlbackend.NewUserResolver(ctx, r.db, r.perm.User)
}

func (r permissionsInfoUserResolver) Reason() string {
	return string(r.perm.Reason)
}

func (r permissionsInfoUserResolver) UpdatedAt() *gqlutil.DateTime {
	return gqlutil.FromTime(r.perm.UpdatedAt)
}
