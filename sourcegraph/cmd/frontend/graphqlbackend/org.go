package graphqlbackend

import (
	"context"

	"github.com/cockroachdb/errors"
	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/suspiciousnames"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/repoupdater/protocol"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func (r *schemaResolver) Organization(ctx context.Context, args struct{ Name string }) (*OrgResolver, error) {
	org, err := r.db.Orgs().GetByName(ctx, args.Name)
	if err != nil {
		return nil, err
	}
	// 🚨 SECURITY: Only org members can get org details on Cloud
	if envvar.SourcegraphDotComMode() {
		err := backend.CheckOrgAccess(ctx, r.db, org.ID)
		if err != nil {
			return nil, errors.Newf("org not found: %s", args.Name)
		}
	}
	return &OrgResolver{db: r.db, org: org}, nil
}

// Deprecated: Org is only in use by sourcegraph/src. Use Node to look up an
// org by its graphql.ID instead.
func (r *schemaResolver) Org(ctx context.Context, args *struct {
	ID graphql.ID
}) (*OrgResolver, error) {
	return OrgByID(ctx, r.db, args.ID)
}

func OrgByID(ctx context.Context, db database.DB, id graphql.ID) (*OrgResolver, error) {
	orgID, err := UnmarshalOrgID(id)
	if err != nil {
		return nil, err
	}
	return OrgByIDInt32(ctx, db, orgID)
}

func OrgByIDInt32(ctx context.Context, db database.DB, orgID int32) (*OrgResolver, error) {
	// 🚨 SECURITY: Only org members can get org details on Cloud
	if envvar.SourcegraphDotComMode() {
		err := backend.CheckOrgAccess(ctx, db, orgID)
		if err != nil {
			return nil, errors.Newf("org not found: %d", orgID)
		}
	}
	org, err := db.Orgs().GetByID(ctx, orgID)
	if err != nil {
		return nil, err
	}
	return &OrgResolver{db, org}, nil
}

type OrgResolver struct {
	db  database.DB
	org *types.Org
}

func NewOrg(db database.DB, org *types.Org) *OrgResolver { return &OrgResolver{db: db, org: org} }

func (o *OrgResolver) ID() graphql.ID { return MarshalOrgID(o.org.ID) }

func MarshalOrgID(id int32) graphql.ID { return relay.MarshalID("Org", id) }

func UnmarshalOrgID(id graphql.ID) (orgID int32, err error) {
	if kind := relay.UnmarshalKind(id); kind != "Org" {
		return 0, errors.Newf("invalid org id of kind %q", kind)
	}
	err = relay.UnmarshalSpec(id, &orgID)
	return
}

func (o *OrgResolver) OrgID() int32 {
	return o.org.ID
}

func (o *OrgResolver) Name() string {
	return o.org.Name
}

func (o *OrgResolver) DisplayName() *string {
	return o.org.DisplayName
}

func (o *OrgResolver) URL() string { return "/organizations/" + o.org.Name }

func (o *OrgResolver) SettingsURL() *string { return strptr(o.URL() + "/settings") }

func (o *OrgResolver) CreatedAt() DateTime { return DateTime{Time: o.org.CreatedAt} }

func (o *OrgResolver) Members(ctx context.Context) (*staticUserConnectionResolver, error) {
	// 🚨 SECURITY: Only org members can list other org members.
	if err := backend.CheckOrgAccessOrSiteAdmin(ctx, o.db, o.org.ID); err != nil {
		if err == backend.ErrNotAnOrgMember {
			return nil, errors.New("must be a member of this organization to view members")
		}
		return nil, err
	}

	memberships, err := o.db.OrgMembers().GetByOrgID(ctx, o.org.ID)
	if err != nil {
		return nil, err
	}
	users := make([]*types.User, len(memberships))
	for i, membership := range memberships {
		user, err := o.db.Users().GetByID(ctx, membership.UserID)
		if err != nil {
			return nil, err
		}
		users[i] = user
	}
	return &staticUserConnectionResolver{db: o.db, users: users}, nil
}

func (o *OrgResolver) settingsSubject() api.SettingsSubject {
	return api.SettingsSubject{Org: &o.org.ID}
}

func (o *OrgResolver) LatestSettings(ctx context.Context) (*settingsResolver, error) {
	// 🚨 SECURITY: Only organization members and site admins (not on cloud) may access the settings,
	// because they may contain secrets or other sensitive data.
	if err := backend.CheckOrgAccessOrSiteAdmin(ctx, o.db, o.org.ID); err != nil {
		return nil, err
	}

	settings, err := o.db.Settings().GetLatest(ctx, o.settingsSubject())
	if err != nil {
		return nil, err
	}
	if settings == nil {
		return nil, nil
	}
	return &settingsResolver{o.db, &settingsSubject{org: o}, settings, nil}, nil
}

func (o *OrgResolver) SettingsCascade() *settingsCascade {
	return &settingsCascade{db: o.db, subject: &settingsSubject{org: o}}
}

func (o *OrgResolver) ConfigurationCascade() *settingsCascade { return o.SettingsCascade() }

func (o *OrgResolver) ViewerPendingInvitation(ctx context.Context) (*organizationInvitationResolver, error) {
	if actor := actor.FromContext(ctx); actor.IsAuthenticated() {
		orgInvitation, err := o.db.OrgInvitations().GetPending(ctx, o.org.ID, actor.UID)
		if errcode.IsNotFound(err) {
			return nil, nil
		}
		if err != nil {
			return nil, err
		}
		return &organizationInvitationResolver{o.db, orgInvitation}, nil
	}
	return nil, nil
}

func (o *OrgResolver) ViewerCanAdminister(ctx context.Context) (bool, error) {
	if err := backend.CheckOrgAccessOrSiteAdmin(ctx, o.db, o.org.ID); err == backend.ErrNotAuthenticated || err == backend.ErrNotAnOrgMember {
		return false, nil
	} else if err != nil {
		return false, err
	}
	return true, nil
}

func (o *OrgResolver) ViewerIsMember(ctx context.Context) (bool, error) {
	actor := actor.FromContext(ctx)
	if !actor.IsAuthenticated() {
		return false, nil
	}
	if _, err := o.db.OrgMembers().GetByOrgIDAndUserID(ctx, o.org.ID, actor.UID); err != nil {
		if errcode.IsNotFound(err) {
			err = nil
		}
		return false, err
	}
	return true, nil
}

func (o *OrgResolver) NamespaceName() string { return o.org.Name }

func (o *OrgResolver) BatchChanges(ctx context.Context, args *ListBatchChangesArgs) (BatchChangesConnectionResolver, error) {
	id := o.ID()
	args.Namespace = &id
	return EnterpriseResolvers.batchChangesResolver.BatchChanges(ctx, args)
}

func (r *schemaResolver) CreateOrganization(ctx context.Context, args *struct {
	Name        string
	DisplayName *string
}) (*OrgResolver, error) {
	a := actor.FromContext(ctx)
	if !a.IsAuthenticated() {
		return nil, errors.New("no current user")
	}

	if err := suspiciousnames.CheckNameAllowedForUserOrOrganization(args.Name); err != nil {
		return nil, err
	}
	newOrg, err := database.Orgs(r.db).Create(ctx, args.Name, args.DisplayName)
	if err != nil {
		return nil, err
	}

	// Add the current user as the first member of the new org.
	_, err = database.OrgMembers(r.db).Create(ctx, newOrg.ID, a.UID)
	if err != nil {
		return nil, err
	}

	return &OrgResolver{db: r.db, org: newOrg}, nil
}

func (r *schemaResolver) UpdateOrganization(ctx context.Context, args *struct {
	ID          graphql.ID
	DisplayName *string
}) (*OrgResolver, error) {
	var orgID int32
	if err := relay.UnmarshalSpec(args.ID, &orgID); err != nil {
		return nil, err
	}

	// 🚨 SECURITY: Check that the current user is a member
	// of the org that is being modified.
	if err := backend.CheckOrgAccessOrSiteAdmin(ctx, r.db, orgID); err != nil {
		return nil, err
	}

	updatedOrg, err := database.Orgs(r.db).Update(ctx, orgID, args.DisplayName)
	if err != nil {
		return nil, err
	}

	return &OrgResolver{db: r.db, org: updatedOrg}, nil
}

func (r *schemaResolver) RemoveUserFromOrganization(ctx context.Context, args *struct {
	User         graphql.ID
	Organization graphql.ID
}) (*EmptyResponse, error) {
	orgID, err := UnmarshalOrgID(args.Organization)
	if err != nil {
		return nil, err
	}
	userID, err := UnmarshalUserID(args.User)
	if err != nil {
		return nil, err
	}

	// 🚨 SECURITY: Check that the current user is a member of the org that is being modified, or a
	// site admin.
	if err := backend.CheckOrgAccessOrSiteAdmin(ctx, r.db, orgID); err != nil {
		return nil, err
	}
	memberCount, err := database.OrgMembers(r.db).MemberCount(ctx, orgID)
	if err != nil {
		return nil, err
	}
	if memberCount == 1 {
		return nil, errors.New("you can’t remove the only member of an organization")
	}
	log15.Info("removing user from org", "user", userID, "org", orgID)
	if err := database.OrgMembers(r.db).Remove(ctx, orgID, userID); err != nil {
		return nil, err
	}
	r.repoupdaterClient.SchedulePermsSync(ctx, protocol.PermsSyncRequest{UserIDs: []int32{userID}})
	return nil, nil
}

func (r *schemaResolver) AddUserToOrganization(ctx context.Context, args *struct {
	Organization graphql.ID
	Username     string
}) (*EmptyResponse, error) {
	// 🚨 SECURITY: Do not allow direct add on cloud.
	if envvar.SourcegraphDotComMode() {
		return nil, errors.New("adding users to organization directly is not allowed")
	}
	// 🚨 SECURITY: Must be a site admin to immediately add a user to an organization (bypassing the
	// invitation step).
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}

	var orgID int32
	if err := relay.UnmarshalSpec(args.Organization, &orgID); err != nil {
		return nil, err
	}

	userToInvite, _, err := getUserToInviteToOrganization(ctx, r.db, args.Username, orgID)
	if err != nil {
		return nil, err
	}
	if _, err := database.OrgMembers(r.db).Create(ctx, orgID, userToInvite.ID); err != nil {
		return nil, err
	}
	// Schedule permission sync for newly added user
	r.repoupdaterClient.SchedulePermsSync(ctx, protocol.PermsSyncRequest{UserIDs: []int32{userToInvite.ID}})
	return &EmptyResponse{}, nil
}

type ListOrgRepositoriesArgs struct {
	First              *int32
	Query              *string
	After              *string
	Cloned             bool
	NotCloned          bool
	Indexed            bool
	NotIndexed         bool
	ExternalServiceIDs *[]*graphql.ID
	OrderBy            *string
	Descending         bool
}

func (o *OrgResolver) Repositories(ctx context.Context, args *ListOrgRepositoriesArgs) (RepositoryConnectionResolver, error) {
	if err := backend.CheckOrgExternalServices(ctx, o.db, o.org.ID); err != nil {
		return nil, err
	}
	// 🚨 SECURITY: Only org members can list the org repositories.
	if err := backend.CheckOrgAccess(ctx, o.db, o.org.ID); err != nil {
		if err == backend.ErrNotAnOrgMember {
			return nil, errors.New("must be a member of this organization to view its repositories")
		}
		return nil, err
	}

	opt := database.ReposListOptions{}
	if args.Query != nil {
		opt.Query = *args.Query
	}
	if args.First != nil {
		opt.LimitOffset = &database.LimitOffset{Limit: int(*args.First)}
	}
	if args.After != nil {
		cursor, err := unmarshalRepositoryCursor(args.After)
		if err != nil {
			return nil, err
		}
		opt.Cursors = append(opt.Cursors, cursor)
	} else {
		opt.Cursors = append(opt.Cursors, &types.Cursor{Direction: "next"})
	}
	if args.OrderBy == nil {
		opt.OrderBy = database.RepoListOrderBy{{
			Field:      "name",
			Descending: false,
		}}
	} else {
		opt.OrderBy = database.RepoListOrderBy{{
			Field:      toDBRepoListColumn(*args.OrderBy),
			Descending: args.Descending,
		}}
	}

	if args.ExternalServiceIDs == nil || len(*args.ExternalServiceIDs) == 0 {
		opt.OrgID = o.org.ID
	} else {
		var idArray []int64
		for i, externalServiceID := range *args.ExternalServiceIDs {
			id, err := unmarshalExternalServiceID(*externalServiceID)
			if err != nil {
				return nil, err
			}
			idArray[i] = id
		}
		opt.ExternalServiceIDs = idArray
	}

	return &repositoryConnectionResolver{
		db:         o.db,
		opt:        opt,
		cloned:     args.Cloned,
		notCloned:  args.NotCloned,
		indexed:    args.Indexed,
		notIndexed: args.NotIndexed,
	}, nil
}
