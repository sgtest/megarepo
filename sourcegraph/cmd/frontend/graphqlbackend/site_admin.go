package graphqlbackend

import (
	"context"
	"encoding/json"
	"time"

	"github.com/graph-gophers/graphql-go"
	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/external/session"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func (r *schemaResolver) DeleteUser(ctx context.Context, args *struct {
	User graphql.ID
	Hard *bool
}) (*EmptyResponse, error) {
	// 🚨 SECURITY: Only site admins can delete users.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}

	userID, err := UnmarshalUserID(args.User)
	if err != nil {
		return nil, err
	}

	// a must be authenticated at this point, CheckCurrentUserIsSiteAdmin enforces it.
	a := actor.FromContext(ctx)
	if a.UID == userID {
		return nil, errors.New("unable to delete current user")
	}

	// Collect username, verified email addresses, and external accounts to be used
	// for revoking user permissions later, otherwise they will be removed from database
	// if it's a hard delete.
	user, err := r.db.Users().GetByID(ctx, userID)
	if err != nil {
		return nil, errors.Wrap(err, "get user by ID")
	}

	var accounts []*extsvc.Accounts

	extAccounts, err := r.db.UserExternalAccounts().List(ctx, database.ExternalAccountsListOptions{UserID: userID})
	if err != nil {
		return nil, errors.Wrap(err, "list external accounts")
	}
	for _, acct := range extAccounts {
		accounts = append(accounts, &extsvc.Accounts{
			ServiceType: acct.ServiceType,
			ServiceID:   acct.ServiceID,
			AccountIDs:  []string{acct.AccountID},
		})
	}

	verifiedEmails, err := r.db.UserEmails().ListByUser(ctx, database.UserEmailsListOptions{
		UserID:       user.ID,
		OnlyVerified: true,
	})
	if err != nil {
		return nil, err
	}
	emailStrs := make([]string, len(verifiedEmails))
	for i := range verifiedEmails {
		emailStrs[i] = verifiedEmails[i].Email
	}
	accounts = append(accounts, &extsvc.Accounts{
		ServiceType: authz.SourcegraphServiceType,
		ServiceID:   authz.SourcegraphServiceID,
		AccountIDs:  append(emailStrs, user.Username),
	})

	if args.Hard != nil && *args.Hard {
		if err := r.db.Users().HardDelete(ctx, user.ID); err != nil {
			return nil, err
		}
	} else {
		if err := r.db.Users().Delete(ctx, user.ID); err != nil {
			return nil, err
		}
	}

	// NOTE: Practically, we don't reuse the ID for any new users, and the situation of left-over pending permissions
	// is possible but highly unlikely. Therefore, there is no need to roll back user deletion even if this step failed.
	// This call is purely for the purpose of cleanup.
	if err := r.db.Authz().RevokeUserPermissions(ctx, &database.RevokeUserPermissionsArgs{
		UserID:   user.ID,
		Accounts: accounts,
	}); err != nil {
		return nil, err
	}

	return &EmptyResponse{}, nil
}

func (r *schemaResolver) DeleteOrganization(ctx context.Context, args *struct {
	Organization graphql.ID
	Hard         *bool
}) (*EmptyResponse, error) {

	if args.Hard != nil && *args.Hard {
		return r.hardDelete(ctx, args.Organization)
	} else {
		return r.softDelete(ctx, args.Organization)
	}
}

func (r *schemaResolver) hardDelete(ctx context.Context, org graphql.ID) (*EmptyResponse, error) {
	if !envvar.SourcegraphDotComMode() {
		return nil, errors.New("hard deleting organization is only supported on Sourcegraph.com")
	}

	orgID, err := UnmarshalOrgID(org)
	if err != nil {
		return nil, err
	}

	//🚨 SECURITY: Only org members can hard delete orgs.
	if err := backend.CheckOrgAccess(ctx, r.db, orgID); err != nil {
		return nil, err
	}

	orgDeletionFlag, err := r.db.FeatureFlags().GetFeatureFlag(ctx, "org-deletion")
	if err != nil {
		return nil, err
	}

	if orgDeletionFlag == nil || !orgDeletionFlag.Bool.Value {
		return nil, errors.New("hard deleting organization is not supported")
	}

	if err := r.db.Orgs().HardDelete(ctx, orgID); err != nil {
		return nil, err
	}

	return &EmptyResponse{}, nil
}

func (r *schemaResolver) softDelete(ctx context.Context, org graphql.ID) (*EmptyResponse, error) {
	// For Cloud, orgs can only be hard deleted.
	if envvar.SourcegraphDotComMode() {
		return nil, errors.New("soft deleting organization in not supported on Sourcegraph.com")
	}

	// 🚨 SECURITY: For On-premise, only site admins can soft delete orgs.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}

	orgID, err := UnmarshalOrgID(org)
	if err != nil {
		return nil, err
	}

	if err := r.db.Orgs().Delete(ctx, orgID); err != nil {
		return nil, err
	}

	return &EmptyResponse{}, nil
}

type roleChangeEventArgs struct {
	By   int32  `json:"by"`
	For  int32  `json:"for"`
	From string `json:"from"`
	To   string `json:"to"`

	// Reason will be present only if the RoleChangeDenied event is logged, but will be set to an
	// empty string in other cases for a consistent experience of the clients that consume this
	// data.
	Reason string `json:"reason"`
}

func (r *schemaResolver) SetUserIsSiteAdmin(ctx context.Context, args *struct {
	UserID    graphql.ID
	SiteAdmin bool
}) (response *EmptyResponse, err error) {
	// 🚨 SECURITY: Only site admins can promote other users to site admin (or demote from site
	// admin).

	// Set default values for event args.
	eventArgs := roleChangeEventArgs{
		From: "role_user",
		To:   "role_site_admin",
	}

	// Correct the values based on the value of SiteAdmin in the GraphQL mutation.
	if !args.SiteAdmin {
		eventArgs.From = "role_site_admin"
		eventArgs.To = "role_user"
	}

	affectedUserID, err := UnmarshalUserID(args.UserID)
	if err != nil {
		return nil, err
	}

	eventArgs.For = affectedUserID

	userResolver, err := CurrentUser(ctx, r.db)
	if err != nil {
		return nil, err
	}

	eventArgs.By = userResolver.user.ID

	// At the moment, we log only two types of events:
	// - RoleChangeDenied
	// - RoleChangeGranted
	//
	// Unless we want to log another event for RoleChangeAttempted as well, invoking
	// logRoleChangeAttempt before this point does not make sense since this is the first time in
	// the lifetime of this function when we have all the details required for eventArgs, especially
	// eventArgs.By which is used as the UserID in database.SecurityEvent - a required argument to
	// write an entry into the database.
	eventName := database.SecurityEventNameRoleChangeDenied
	defer logRoleChangeAttempt(ctx, r.db, &eventName, &eventArgs, &err)

	if err = backend.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}

	if userResolver.ID() == args.UserID {
		return nil, errors.New("refusing to set current user site admin status")
	}

	if err = database.Users(r.db).SetIsSiteAdmin(ctx, affectedUserID, args.SiteAdmin); err != nil {
		return nil, err
	}

	eventName = database.SecurityEventNameRoleChangeGranted
	return &EmptyResponse{}, nil
}

func (r *schemaResolver) InvalidateSessionsByID(ctx context.Context, args *struct {
	UserID graphql.ID
}) (*EmptyResponse, error) {
	// 🚨 SECURITY: Only the site admin can invalidate the sessions of a user
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}
	userID, err := UnmarshalUserID(args.UserID)
	if err != nil {
		return nil, err
	}
	if err := session.InvalidateSessionsByID(ctx, r.db, userID); err != nil {
		return nil, err
	}
	return &EmptyResponse{}, nil
}

func logRoleChangeAttempt(ctx context.Context, db database.DB, name *database.SecurityEventName, eventArgs *roleChangeEventArgs, parentErr *error) {
	// To avoid a panic, it's important to check for a nil parentErr before we dereference it.
	if parentErr != nil && *parentErr != nil {
		eventArgs.Reason = (*parentErr).Error()
	}

	args, err := json.Marshal(eventArgs)
	if err != nil {
		log15.Error("logRoleChangeAttempt: failed to marshal JSON", "eventArgs", eventArgs)
	}

	event := &database.SecurityEvent{
		Name:            *name,
		URL:             "",
		UserID:          uint32(eventArgs.By),
		AnonymousUserID: "",
		Argument:        args,
		Source:          "BACKEND",
		Timestamp:       time.Now(),
	}

	db.SecurityEventLogs().LogEvent(ctx, event)
}
