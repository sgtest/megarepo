package graphqlbackend

import (
	"context"
	"errors"
	"time"

	graphql "github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/suspiciousnames"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/errcode"
)

func (r *schemaResolver) User(ctx context.Context, args struct{ Username string }) (*UserResolver, error) {
	user, err := db.Users.GetByUsername(ctx, args.Username)
	if err != nil {
		return nil, err
	}
	return &UserResolver{user: user}, nil
}

// UserResolver implements the GraphQL User type.
type UserResolver struct {
	user *types.User
}

// UserByID looks up and returns the user with the given GraphQL ID. If no such user exists, it returns a
// non-nil error.
func UserByID(ctx context.Context, id graphql.ID) (*UserResolver, error) {
	userID, err := UnmarshalUserID(id)
	if err != nil {
		return nil, err
	}
	return UserByIDInt32(ctx, userID)
}

// UserByIDInt32 looks up and returns the user with the given database ID. If no such user exists,
// it returns a non-nil error.
func UserByIDInt32(ctx context.Context, id int32) (*UserResolver, error) {
	user, err := db.Users.GetByID(ctx, id)
	if err != nil {
		return nil, err
	}
	return &UserResolver{user: user}, nil
}

func (r *UserResolver) ID() graphql.ID { return marshalUserID(r.user.ID) }

func marshalUserID(id int32) graphql.ID { return relay.MarshalID("User", id) }

func UnmarshalUserID(id graphql.ID) (userID int32, err error) {
	err = relay.UnmarshalSpec(id, &userID)
	return
}

func (r *UserResolver) SourcegraphID() int32 { return r.user.ID }

func (r *UserResolver) Email(ctx context.Context) (string, error) {
	// 🚨 SECURITY: Only the user and admins are allowed to access the email address.
	if err := backend.CheckSiteAdminOrSameUser(ctx, r.user.ID); err != nil {
		return "", err
	}

	email, _, err := db.UserEmails.GetPrimaryEmail(ctx, r.user.ID)
	if err != nil {
		return "", err
	}

	return email, nil
}

func (r *UserResolver) Username() string { return r.user.Username }

func (r *UserResolver) DisplayName() *string {
	if r.user.DisplayName == "" {
		return nil
	}
	return &r.user.DisplayName
}

func (r *UserResolver) AvatarURL() *string {
	if r.user.AvatarURL == "" {
		return nil
	}
	return &r.user.AvatarURL
}

func (r *UserResolver) URL() string {
	return "/users/" + r.user.Username
}

func (r *UserResolver) SettingsURL() string { return r.URL() + "/settings" }

func (r *UserResolver) CreatedAt() string {
	return r.user.CreatedAt.Format(time.RFC3339)
}

func (r *UserResolver) UpdatedAt() *string {
	t := r.user.UpdatedAt.Format(time.RFC3339) // ISO
	return &t
}

func (r *UserResolver) configurationSubject() api.ConfigurationSubject {
	return api.ConfigurationSubject{User: &r.user.ID}
}

func (r *UserResolver) LatestSettings(ctx context.Context) (*settingsResolver, error) {
	// 🚨 SECURITY: Only the user and admins are allowed to access the user's settings, because they
	// may contain secrets or other sensitive data.
	if err := backend.CheckSiteAdminOrSameUser(ctx, r.user.ID); err != nil {
		return nil, err
	}

	settings, err := db.Settings.GetLatest(ctx, r.configurationSubject())
	if err != nil {
		return nil, err
	}
	if settings == nil {
		return nil, nil
	}
	return &settingsResolver{&configurationSubject{user: r}, settings, nil}, nil
}

func (r *UserResolver) ConfigurationCascade() *configurationCascadeResolver {
	return &configurationCascadeResolver{subject: &configurationSubject{user: r}}
}

func (r *UserResolver) SiteAdmin(ctx context.Context) (bool, error) {
	// 🚨 SECURITY: Only the user and admins are allowed to determine if the user is a site admin.
	if err := backend.CheckSiteAdminOrSameUser(ctx, r.user.ID); err != nil {
		return false, err
	}

	return r.user.SiteAdmin, nil
}

func (*schemaResolver) UpdateUser(ctx context.Context, args *struct {
	User        graphql.ID
	Username    *string
	DisplayName *string
	AvatarURL   *string
}) (*EmptyResponse, error) {
	userID, err := UnmarshalUserID(args.User)
	if err != nil {
		return nil, err
	}

	// 🚨 SECURITY: Only the user and site admins are allowed to update the user.
	if err := backend.CheckSiteAdminOrSameUser(ctx, userID); err != nil {
		return nil, err
	}

	if args.Username != nil {
		if err := suspiciousnames.CheckNameAllowedForUserOrOrganization(*args.Username); err != nil {
			return nil, err
		}
	}

	update := db.UserUpdate{
		DisplayName: args.DisplayName,
		AvatarURL:   args.AvatarURL,
	}
	if args.Username != nil {
		update.Username = *args.Username
	}
	if err := db.Users.Update(ctx, userID, update); err != nil {
		return nil, err
	}
	return &EmptyResponse{}, nil
}

// CurrentUser returns the authenticated user if any. If there is no authenticated user, it returns
// (nil, nil). If some other error occurs, then the error is returned.
func CurrentUser(ctx context.Context) (*UserResolver, error) {
	user, err := db.Users.GetByCurrentAuthUser(ctx)
	if err != nil {
		if errcode.IsNotFound(err) || err == db.ErrNoCurrentUser {
			return nil, nil
		}
		return nil, err
	}
	return &UserResolver{user: user}, nil
}

func (r *UserResolver) Organizations(ctx context.Context) (*orgConnectionStaticResolver, error) {
	orgs, err := db.Orgs.GetByUserID(ctx, r.user.ID)
	if err != nil {
		return nil, err
	}
	c := orgConnectionStaticResolver{nodes: make([]*OrgResolver, len(orgs))}
	for i, org := range orgs {
		c.nodes[i] = &OrgResolver{org}
	}
	return &c, nil
}

func (r *UserResolver) Tags(ctx context.Context) ([]string, error) {
	// 🚨 SECURITY: Only the user and admins are allowed to access the user's tags.
	if err := backend.CheckSiteAdminOrSameUser(ctx, r.user.ID); err != nil {
		return nil, err
	}
	return r.user.Tags, nil
}

func (r *UserResolver) SurveyResponses(ctx context.Context) ([]*surveyResponseResolver, error) {
	// 🚨 SECURITY: Only the user and admins are allowed to access the user's survey responses.
	if err := backend.CheckSiteAdminOrSameUser(ctx, r.user.ID); err != nil {
		return nil, err
	}

	responses, err := db.SurveyResponses.GetByUserID(ctx, r.user.ID)
	if err != nil {
		return nil, err
	}
	surveyResponseResolvers := []*surveyResponseResolver{}
	for _, response := range responses {
		surveyResponseResolvers = append(surveyResponseResolvers, &surveyResponseResolver{response})
	}
	return surveyResponseResolvers, nil
}

func (r *UserResolver) ViewerCanAdminister(ctx context.Context) (bool, error) {
	if err := backend.CheckSiteAdminOrSameUser(ctx, r.user.ID); err == backend.ErrNotAuthenticated || err == backend.ErrMustBeSiteAdmin {
		return false, nil
	} else if err != nil {
		return false, err
	}
	return true, nil
}

// UserURLForSiteAdminBilling is called to obtain the GraphQL User.urlForSiteAdminBilling value. It
// is only set if billing is implemented.
var UserURLForSiteAdminBilling func(ctx context.Context, userID int32) (*string, error)

func (r *UserResolver) URLForSiteAdminBilling(ctx context.Context) (*string, error) {
	if UserURLForSiteAdminBilling == nil {
		return nil, nil
	}
	return UserURLForSiteAdminBilling(ctx, r.user.ID)
}

func (r *schemaResolver) UpdatePassword(ctx context.Context, args *struct {
	OldPassword string
	NewPassword string
}) (*EmptyResponse, error) {
	// 🚨 SECURITY: A user can only change their own password.
	user, err := db.Users.GetByCurrentAuthUser(ctx)
	if err != nil {
		return nil, err
	}
	if user == nil {
		return nil, errors.New("no authenticated user")
	}

	if err := db.Users.UpdatePassword(ctx, user.ID, args.OldPassword, args.NewPassword); err != nil {
		return nil, err
	}
	return &EmptyResponse{}, nil
}
