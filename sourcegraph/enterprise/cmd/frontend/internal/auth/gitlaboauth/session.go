package gitlaboauth

import (
	"context"
	"fmt"
	"net/http"
	"strconv"

	"golang.org/x/oauth2"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/auth"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/hubspot"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/hubspot/hubspotutil"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/auth/oauth"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/auth/providers"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitlab"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type sessionIssuerHelper struct {
	*extsvc.CodeHost
	clientID    string
	db          database.DB
	allowSignup *bool
	allowGroups []string
}

func (s *sessionIssuerHelper) AuthSucceededEventName() database.SecurityEventName {
	return database.SecurityEventGitLabAuthSucceeded
}

func (s *sessionIssuerHelper) AuthFailedEventName() database.SecurityEventName {
	return database.SecurityEventGitLabAuthFailed
}

func (s *sessionIssuerHelper) GetOrCreateUser(ctx context.Context, token *oauth2.Token, anonymousUserID, firstSourceURL, lastSourceURL string) (actr *actor.Actor, safeErrMsg string, err error) {
	gUser, err := UserFromContext(ctx)
	if err != nil {
		return nil, "Could not read GitLab user from callback request.", errors.Wrap(err, "could not read user from context")
	}

	login, err := auth.NormalizeUsername(gUser.Username)
	if err != nil {
		return nil, fmt.Sprintf("Error normalizing the username %q. See https://docs.sourcegraph.com/admin/auth/#username-normalization.", login), err
	}

	provider := gitlab.NewClientProvider(extsvc.URNGitLabOAuth, s.BaseURL, nil)
	glClient := provider.GetOAuthClient(token.AccessToken)

	// 🚨 SECURITY: Ensure that the user is part of one of the allowed groups or subgroups when the allowGroups option is set.
	userBelongsToAllowedGroups, err := s.verifyUserGroups(ctx, glClient)
	if err != nil {
		message := "Error verifying user groups."
		return nil, message, err
	}

	if !userBelongsToAllowedGroups {
		message := "User does not belong to allowed GitLab groups or subgroups."
		return nil, message, errors.New(message)
	}

	// AllowSignup defaults to true when not set to preserve the existing behavior.
	signupAllowed := s.allowSignup == nil || *s.allowSignup

	var data extsvc.AccountData
	if err := gitlab.SetExternalAccountData(&data, gUser, token); err != nil {
		return nil, "", err
	}

	// Unlike with GitHub, we can *only* use the primary email to resolve the user's identity,
	// because the GitLab API does not return whether an email has been verified. The user's primary
	// email on GitLab is always verified, so we use that.
	userID, safeErrMsg, err := auth.GetAndSaveUser(ctx, s.db, auth.GetAndSaveUserOp{
		UserProps: database.NewUser{
			Username:        login,
			Email:           gUser.Email,
			EmailIsVerified: gUser.Email != "",
			DisplayName:     gUser.Name,
			AvatarURL:       gUser.AvatarURL,
		},
		ExternalAccount: extsvc.AccountSpec{
			ServiceType: s.ServiceType,
			ServiceID:   s.ServiceID,
			ClientID:    s.clientID,
			AccountID:   strconv.FormatInt(int64(gUser.ID), 10),
		},
		ExternalAccountData: data,
		CreateIfNotExist:    signupAllowed,
	})
	if err != nil {
		return nil, safeErrMsg, err
	}

	// There is no need to send record if we know email is empty as it's a primary property
	if gUser.Email != "" {
		go hubspotutil.SyncUser(gUser.Email, hubspotutil.SignupEventID, &hubspot.ContactProperties{
			AnonymousUserID: anonymousUserID,
			FirstSourceURL:  firstSourceURL,
			LastSourceURL:   lastSourceURL,
		})
	}

	return actor.FromUser(userID), "", nil
}

func (s *sessionIssuerHelper) DeleteStateCookie(w http.ResponseWriter) {
	stateConfig := getStateConfig()
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
		// TODO(beyang): store and use refresh token to auto-refresh sessions
	}
}

// verifyUserGroups checks whether the authenticated user belongs to one of the GitLab groups when the allowGroups option is set.
func (s *sessionIssuerHelper) verifyUserGroups(ctx context.Context, glClient *gitlab.Client) (bool, error) {
	if len(s.allowGroups) == 0 {
		return true, nil
	}

	allowed := make(map[string]bool, len(s.allowGroups))
	for _, group := range s.allowGroups {
		allowed[group] = true
	}

	var err error
	var gitlabGroups []*gitlab.Group
	hasNextPage := true

	for page := 1; hasNextPage; page++ {
		gitlabGroups, hasNextPage, err = glClient.ListGroups(ctx, page)
		if err != nil {
			return false, err
		}

		// Check the full path instead of name so we can better handle subgroups.
		for _, glGroup := range gitlabGroups {
			if allowed[glGroup.FullPath] {
				return true, nil
			}
		}
	}

	return false, nil
}
