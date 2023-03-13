package authz

import (
	"fmt"
	"time"

	otlog "github.com/opentracing/opentracing-go/log"
	"golang.org/x/exp/maps"
	"golang.org/x/exp/slices"

	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var ErrPermsNotFound = errors.New("permissions not found")

// RepoPerms contains a repo and the permissions a given user
// has associated with it.
type RepoPerms struct {
	Repo  *types.Repo
	Perms Perms
}

// Perms is a permission set represented as bitset.
type Perms uint32

// Perm constants.
const (
	None Perms = 0
	Read Perms = 1 << iota
	Write
)

// Include is a convenience method to test if Perms
// includes all the other Perms.
func (p Perms) Include(other Perms) bool {
	return p&other == other
}

// String implements the fmt.Stringer interface.
func (p Perms) String() string {
	switch p {
	case Read:
		return "read"
	case Write:
		return "write"
	case Read | Write:
		return "read,write"
	default:
		return "none"
	}
}

// PermType is the object type of the user permissions.
type PermType string

// PermRepos is the list of available user permission types.
const (
	PermRepos PermType = "repos"
)

// RepoPermsSort sorts a slice of RepoPerms to guarantee a stable ordering.
type RepoPermsSort []RepoPerms

func (s RepoPermsSort) Len() int      { return len(s) }
func (s RepoPermsSort) Swap(i, j int) { s[i], s[j] = s[j], s[i] }
func (s RepoPermsSort) Less(i, j int) bool {
	if s[i].Repo.ID != s[j].Repo.ID {
		return s[i].Repo.ID < s[j].Repo.ID
	}
	if s[i].Repo.ExternalRepo.ID != s[j].Repo.ExternalRepo.ID {
		return s[i].Repo.ExternalRepo.ID < s[j].Repo.ExternalRepo.ID
	}
	return s[i].Repo.Name < s[j].Repo.Name
}

// ErrStalePermissions is returned by LoadPermissions when the stored
// permissions are stale (e.g. the first time a user needs them and they haven't
// been fetched yet). Callers should pass this error up to the user and show a
// more friendly prompt message in the UI.
type ErrStalePermissions struct {
	UserID int32
	Perm   Perms
	Type   PermType
}

// Error implements the error interface.
func (e ErrStalePermissions) Error() string {
	return fmt.Sprintf("%s:%s permissions for user=%d are stale and being updated", e.Perm, e.Type, e.UserID)
}

// Permission determines if a user with a specific id
// can read a repository with a specific id
type Permission struct {
	UserID            int32     // The internal database ID of a user
	ExternalAccountID int32     // The internal database ID of a user external account
	RepoID            int32     // The internal database ID of a repo
	CreatedAt         time.Time // The creation time
	UpdatedAt         time.Time // The last updated time
	Source            string    // source of the permission
}

// A struct that holds the entity we are updating the permissions for
// It can be either a user or a repository.
type PermissionEntity struct {
	UserID            int32 // The internal database ID of a user
	ExternalAccountID int32 // The internal database ID of a user external account
	RepoID            int32 // The internal database ID of a repo
}

type UserIDWithExternalAccountID struct {
	UserID            int32
	ExternalAccountID int32
}

const SourceRepoSync = "repo_sync"
const SourceUserSync = "user_sync"
const SourceAPI = "api"

// TracingFields returns tracing fields for the opentracing log.
func (p *Permission) TracingFields() []otlog.Field {
	fs := []otlog.Field{
		otlog.Int32("SrcPermissions.UserID", p.UserID),
		otlog.Int32("SrcPermissions.RepoID", p.RepoID),
		otlog.Int32("SrcPermissions.ExternalAccountID", p.ExternalAccountID),
		otlog.String("SrcPermissions.CreatedAt", p.CreatedAt.String()),
		otlog.String("SrcPermissions.UpdatedAt", p.UpdatedAt.String()),
		otlog.String("SrcPermissions.UpdatedAt", p.Source),
	}
	return fs
}

// UserPermissions are the permissions of a user to perform an action
// on the given set of object IDs of the defined type.
type UserPermissions struct {
	UserID    int32              // The internal database ID of a user
	Perm      Perms              // The permission set
	Type      PermType           // The type of the permissions
	IDs       map[int32]struct{} // The object IDs
	UpdatedAt time.Time          // The last updated time
	SyncedAt  time.Time          // The last user-centric synced time
}

// Expired returns true if these UserPermissions have elapsed the given ttl.
func (p *UserPermissions) Expired(ttl time.Duration, now time.Time) bool {
	return !now.Before(p.UpdatedAt.Add(ttl))
}

// GenerateSortedIDsSlice returns a sorted slice of the IDs set.
func (p *UserPermissions) GenerateSortedIDsSlice() []int32 {
	return convertMapSetToSortedSlice(p.IDs)
}

// TracingFields returns tracing fields for the opentracing log.
func (p *UserPermissions) TracingFields() []otlog.Field {
	fs := []otlog.Field{
		otlog.Int32("UserPermissions.UserID", p.UserID),
		trace.Stringer("UserPermissions.Perm", p.Perm),
		otlog.String("UserPermissions.Type", string(p.Type)),
	}

	if p.IDs != nil {
		fs = append(fs,
			otlog.Int("UserPermissions.IDs.Count", len(p.IDs)),
			otlog.String("UserPermissions.UpdatedAt", p.UpdatedAt.String()),
			otlog.String("UserPermissions.SyncedAt", p.SyncedAt.String()),
		)
	}

	return fs
}

// RepoPermissions declares which users have access to a given repository
type RepoPermissions struct {
	RepoID         int32              // The internal database ID of a repository
	Perm           Perms              // The permission set
	UserIDs        map[int32]struct{} // The user IDs
	PendingUserIDs map[int64]struct{} // The pending user IDs
	UpdatedAt      time.Time          // The last updated time
	SyncedAt       time.Time          // The last repo-centric synced time
	Unrestricted   bool               // Anyone can see the repo, overrides all other permissions
}

// Expired returns true if these RepoPermissions have elapsed the given ttl.
func (p *RepoPermissions) Expired(ttl time.Duration, now time.Time) bool {
	return !now.Before(p.UpdatedAt.Add(ttl))
}

// GenerateSortedIDsSlice returns a sorted slice of the IDs set.
func (p *RepoPermissions) GenerateSortedIDsSlice() []int32 {
	return convertMapSetToSortedSlice(p.UserIDs)
}

// TracingFields returns tracing fields for the opentracing log.
func (p *RepoPermissions) TracingFields() []otlog.Field {
	fs := []otlog.Field{
		otlog.Int32("RepoPermissions.RepoID", p.RepoID),
		trace.Stringer("RepoPermissions.Perm", p.Perm),
	}

	if p.UserIDs != nil {
		fs = append(fs,
			otlog.Int("RepoPermissions.UserIDs.Count", len(p.UserIDs)),
			otlog.Int("RepoPermissions.PendingUserIDs.Count", len(p.PendingUserIDs)),
			otlog.String("RepoPermissions.UpdatedAt", p.UpdatedAt.String()),
			otlog.String("RepoPermissions.SyncedAt", p.SyncedAt.String()),
		)
	}

	return fs
}

// UserGrantPermissions defines the structure to grant pending permissions to a user.
// See also UserPendingPermissions.
type UserGrantPermissions struct {
	// UserID of the user to grant permissions to.
	UserID int32
	// ID of the user external account that the permissions are from.
	UserExternalAccountID int32
	// The type of the code host as if it would be used as extsvc.AccountSpec.ServiceType
	ServiceType string
	// The ID of the code host as if it would be used as extsvc.AccountSpec.ServiceID
	ServiceID string
	// The account ID of the user external account, that the permissions are from
	AccountID string
}

// TracingFields returns tracing fields for the opentracing log.
func (p *UserGrantPermissions) TracingFields() []otlog.Field {
	fs := []otlog.Field{
		otlog.Int32("UserGrantPermissions.UserID", p.UserID),
		otlog.Int32("UserGrantPermissions.UserExternalAccountID", p.UserExternalAccountID),
		otlog.String("UserPendingPermissions.ServiceType", p.ServiceType),
		otlog.String("UserPendingPermissions.ServiceID", p.ServiceID),
		otlog.String("UserPendingPermissions.AccountID", p.AccountID),
	}

	return fs
}

// UserPendingPermissions defines permissions that a not-yet-created user has to
// perform on a given set of object IDs. Not-yet-created users may exist on the
// code host but not yet in Sourcegraph. "ServiceType", "ServiceID" and "BindID"
// are used to map this stub user to an actual user when the user is created.
type UserPendingPermissions struct {
	// The auto-generated internal database ID.
	ID int64
	// The type of the code host as if it would be used as extsvc.AccountSpec.ServiceType,
	// e.g. "github", "gitlab", "bitbucketServer" and "sourcegraph".
	ServiceType string
	// The ID of the code host as if it would be used as extsvc.AccountSpec.ServiceID,
	// e.g. "https://github.com/", "https://gitlab.com/" and "https://sourcegraph.com/".
	ServiceID string
	// The account ID that a code host (and its authz provider) uses to identify a user,
	// e.g. a username (for Bitbucket Server), a GraphID ( for GitHub), or a user ID
	// (for GitLab).
	//
	// When use the Sourcegraph authz provider, "BindID" can be either a username or
	// an email based on site configuration.
	BindID string
	// The permissions this user has to the "IDs" of the "Type".
	Perm Perms
	// The type of permissions this user has.
	Type PermType
	// The object IDs with the "Type".
	IDs map[int32]struct{}
	// The last updated time.
	UpdatedAt time.Time
}

// GenerateSortedIDsSlice returns a sorted slice of the IDs set.
func (p *UserPendingPermissions) GenerateSortedIDsSlice() []int32 {
	return convertMapSetToSortedSlice(p.IDs)
}

// TracingFields returns tracing fields for the opentracing log.
func (p *UserPendingPermissions) TracingFields() []otlog.Field {
	fs := []otlog.Field{
		otlog.Int64("UserPendingPermissions.ID", p.ID),
		otlog.String("UserPendingPermissions.ServiceType", p.ServiceType),
		otlog.String("UserPendingPermissions.ServiceID", p.ServiceID),
		otlog.String("UserPendingPermissions.BindID", p.BindID),
		trace.Stringer("UserPendingPermissions.Perm", p.Perm),
		otlog.String("UserPendingPermissions.Type", string(p.Type)),
	}

	if p.IDs != nil {
		fs = append(fs,
			otlog.Int("UserPendingPermissions.IDs.Count", len(p.IDs)),
			otlog.String("UserPendingPermissions.UpdatedAt", p.UpdatedAt.String()),
		)
	}

	return fs
}

// convertMapSetToSortedSlice converts a map set into a slice of sorted integers
func convertMapSetToSortedSlice(mapSet map[int32]struct{}) []int32 {
	slice := maps.Keys(mapSet)
	slices.Sort(slice)
	return slice
}
