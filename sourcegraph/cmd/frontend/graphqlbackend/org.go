package graphqlbackend

import (
	"context"
	"time"

	graphql "github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/suspiciousnames"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/actor"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/errcode"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

func (r *schemaResolver) Organization(ctx context.Context, args struct{ Name string }) (*OrgResolver, error) {
	org, err := db.Orgs.GetByName(ctx, args.Name)
	if err != nil {
		return nil, err
	}
	return &OrgResolver{org: org}, nil
}

// Org is DEPRECATED (but still in use by sourcegraph/src). Use Node to look up an org by its
// graphql.ID instead.
func (r *schemaResolver) Org(ctx context.Context, args *struct {
	ID graphql.ID
}) (*OrgResolver, error) {
	return orgByID(ctx, args.ID)
}

func orgByID(ctx context.Context, id graphql.ID) (*OrgResolver, error) {
	orgID, err := UnmarshalOrgID(id)
	if err != nil {
		return nil, err
	}
	return OrgByIDInt32(ctx, orgID)
}

func OrgByIDInt32(ctx context.Context, orgID int32) (*OrgResolver, error) {
	org, err := db.Orgs.GetByID(ctx, orgID)
	if err != nil {
		return nil, err
	}
	return &OrgResolver{org}, nil
}

type OrgResolver struct {
	org *types.Org
}

func NewOrg(org *types.Org) *OrgResolver { return &OrgResolver{org: org} }

func (o *OrgResolver) ID() graphql.ID { return marshalOrgID(o.org.ID) }

func marshalOrgID(id int32) graphql.ID { return relay.MarshalID("Org", id) }

func UnmarshalOrgID(id graphql.ID) (orgID int32, err error) {
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

func (r *OrgResolver) URL() string { return "/organizations/" + r.org.Name }

func (r *OrgResolver) SettingsURL() string { return r.URL() + "/settings" }

func (o *OrgResolver) CreatedAt() string { return o.org.CreatedAt.Format(time.RFC3339) }

func (o *OrgResolver) Members(ctx context.Context) (*staticUserConnectionResolver, error) {
	// 🚨 SECURITY: Only org members can list the org members.
	if err := backend.CheckOrgAccess(ctx, o.org.ID); err != nil {
		if err == backend.ErrNotAnOrgMember {
			return nil, errors.New("must be a member of this organization to view members")
		}
		return nil, err
	}

	memberships, err := db.OrgMembers.GetByOrgID(ctx, o.org.ID)
	if err != nil {
		return nil, err
	}
	users := make([]*types.User, len(memberships))
	for i, membership := range memberships {
		user, err := db.Users.GetByID(ctx, membership.UserID)
		if err != nil {
			return nil, err
		}
		users[i] = user
	}
	return &staticUserConnectionResolver{users: users}, nil
}

func (o *OrgResolver) configurationSubject() api.ConfigurationSubject {
	return api.ConfigurationSubject{Org: &o.org.ID}
}

func (o *OrgResolver) LatestSettings(ctx context.Context) (*settingsResolver, error) {
	// 🚨 SECURITY: Only organization members and site admins may access the settings, because they
	// may contains secrets or other sensitive data.
	if err := backend.CheckOrgAccess(ctx, o.org.ID); err != nil {
		return nil, err
	}

	settings, err := db.Settings.GetLatest(ctx, o.configurationSubject())
	if err != nil {
		return nil, err
	}
	if settings == nil {
		return nil, nil
	}
	return &settingsResolver{&configurationSubject{org: o}, settings, nil}, nil
}

func (o *OrgResolver) ConfigurationCascade() *configurationCascadeResolver {
	return &configurationCascadeResolver{subject: &configurationSubject{org: o}}
}

func (o *OrgResolver) ViewerPendingInvitation(ctx context.Context) (*organizationInvitationResolver, error) {
	if actor := actor.FromContext(ctx); actor.IsAuthenticated() {
		orgInvitation, err := db.OrgInvitations.GetPending(ctx, o.org.ID, actor.UID)
		if errcode.IsNotFound(err) {
			return nil, nil
		}
		if err != nil {
			return nil, err
		}
		return &organizationInvitationResolver{orgInvitation}, nil
	}
	return nil, nil
}

func (o *OrgResolver) ViewerCanAdminister(ctx context.Context) (bool, error) {
	if err := backend.CheckOrgAccess(ctx, o.org.ID); err == backend.ErrNotAuthenticated || err == backend.ErrNotAnOrgMember {
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
	if _, err := db.OrgMembers.GetByOrgIDAndUserID(ctx, o.org.ID, actor.UID); err != nil {
		if errcode.IsNotFound(err) {
			err = nil
		}
		return false, err
	}
	return true, nil
}

func (*schemaResolver) CreateOrganization(ctx context.Context, args *struct {
	Name        string
	DisplayName *string
}) (*OrgResolver, error) {
	currentUser, err := CurrentUser(ctx)
	if err != nil {
		return nil, err
	}
	if currentUser == nil {
		return nil, errors.New("no current user")
	}

	if err := suspiciousnames.CheckNameAllowedForUserOrOrganization(args.Name); err != nil {
		return nil, err
	}
	newOrg, err := db.Orgs.Create(ctx, args.Name, args.DisplayName)
	if err != nil {
		return nil, err
	}

	// Add the current user as the first member of the new org.
	_, err = db.OrgMembers.Create(ctx, newOrg.ID, currentUser.SourcegraphID())
	if err != nil {
		return nil, err
	}

	return &OrgResolver{org: newOrg}, nil
}

func (*schemaResolver) UpdateOrganization(ctx context.Context, args *struct {
	ID          graphql.ID
	DisplayName *string
}) (*OrgResolver, error) {
	var orgID int32
	if err := relay.UnmarshalSpec(args.ID, &orgID); err != nil {
		return nil, err
	}

	// 🚨 SECURITY: Check that the current user is a member
	// of the org that is being modified.
	if err := backend.CheckOrgAccess(ctx, orgID); err != nil {
		return nil, err
	}

	updatedOrg, err := db.Orgs.Update(ctx, orgID, args.DisplayName)
	if err != nil {
		return nil, err
	}

	return &OrgResolver{org: updatedOrg}, nil
}

func (*schemaResolver) RemoveUserFromOrganization(ctx context.Context, args *struct {
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
	if err := backend.CheckOrgAccess(ctx, orgID); err != nil {
		return nil, err
	}

	log15.Info("removing user from org", "user", userID, "org", orgID)
	return nil, db.OrgMembers.Remove(ctx, orgID, userID)
}

func (*schemaResolver) AddUserToOrganization(ctx context.Context, args *struct {
	Organization graphql.ID
	Username     string
}) (*EmptyResponse, error) {
	var orgID int32
	if err := relay.UnmarshalSpec(args.Organization, &orgID); err != nil {
		return nil, err
	}
	// 🚨 SECURITY: Must be a site admin to immediately add a user to an organization (bypassing the
	// invitation step).
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	userToInvite, _, err := getUserToInviteToOrganization(ctx, args.Username, orgID)
	if err != nil {
		return nil, err
	}
	if _, err := db.OrgMembers.Create(ctx, orgID, userToInvite.ID); err != nil {
		return nil, err
	}
	return &EmptyResponse{}, nil
}
