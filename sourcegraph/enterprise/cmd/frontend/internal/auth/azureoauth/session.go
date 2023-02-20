package azureoauth

import (
	"context"
	"net/http"
	"strings"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/auth"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/auth/providers"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/auth/oauth"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/azuredevops"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"golang.org/x/oauth2"
)

const stateCookie = "azure-state-cookie"

type sessionIssuerHelper struct {
	*extsvc.CodeHost
	db          database.DB
	clientID    string
	allowSignup *bool
}

func (s *sessionIssuerHelper) GetOrCreateUser(ctx context.Context, token *oauth2.Token, anonymousUserID, firstSourceURL, lastSourceURL string) (actr *actor.Actor, safeErrMsg string, err error) {
	user, err := userFromContext(ctx)
	if err != nil {
		return nil, "failed to read Azure DevOps Profile from oauth2 callback request", errors.Wrap(err, "azureoauth.GetOrCreateUser: failed to read user from context of callback request")
	}

	// allowSignup is true by default in the config schema. If it's not set in the provider config,
	// then default to true. Otherwise defer to the value that's set in the config.
	signupAllowed := s.allowSignup == nil || *s.allowSignup

	var data extsvc.AccountData
	if err := azuredevops.SetExternalAccountData(&data, user, token); err != nil {
		return nil, "", errors.Wrapf(err, "failed to set external account data for azure devops user with email %q", user.EmailAddress)
	}

	// The API returned an email address with the first character capitalized during development.
	// Not taking any chances.
	email := strings.ToLower(user.EmailAddress)
	username, err := auth.NormalizeUsername(email)
	if err != nil {
		return nil, "failed to normalize username from email of azure dev ops account", errors.Wrapf(err, "failed to normalize username from email: %q", email)
	}

	userID, safeErrMsg, err := auth.GetAndSaveUser(ctx, s.db, auth.GetAndSaveUserOp{
		UserProps: database.NewUser{
			Username: username,
			Email:    email,
			// TODO: Verify if we can assume this.
			EmailIsVerified: email != "",
			DisplayName:     user.DisplayName,
		},
		ExternalAccount: extsvc.AccountSpec{
			ServiceType: s.ServiceType,
			ServiceID:   s.ServiceID,
			ClientID:    s.clientID,
			AccountID:   user.ID,
		},
		ExternalAccountData: data,
		CreateIfNotExist:    signupAllowed,
	})
	if err != nil {
		return nil, safeErrMsg, err
	}

	return actor.FromUser(userID), "", nil
}

func (s *sessionIssuerHelper) DeleteStateCookie(w http.ResponseWriter) {
	stateConfig := oauth.GetStateConfig(stateCookie)
	stateConfig.MaxAge = -1
	http.SetCookie(w, oauth.NewCookie(stateConfig, ""))
}

func (s *sessionIssuerHelper) SessionData(token *oauth2.Token) oauth.SessionData {
	return oauth.SessionData{
		ID: providers.ConfigID{
			ID:   s.ServiceID,
			Type: s.ServiceType,
		},
		AccessToken: token.AccessToken,
		TokenType:   token.Type(),
	}
}

func (s *sessionIssuerHelper) AuthSucceededEventName() database.SecurityEventName {
	return database.SecurityEventAzureDevOpsAuthSucceeded
}

func (s *sessionIssuerHelper) AuthFailedEventName() database.SecurityEventName {
	return database.SecurityEventAzureDevOpsAuthFailed
}

type key int

const userKey key = iota

func withUser(ctx context.Context, user azuredevops.Profile) context.Context {
	return context.WithValue(ctx, userKey, user)
}

func userFromContext(ctx context.Context) (*azuredevops.Profile, error) {
	user, ok := ctx.Value(userKey).(azuredevops.Profile)
	if !ok {
		return nil, errors.Errorf("azuredevops: Context missing Azure DevOps user")
	}
	return &user, nil
}
