package gitlaboauth

import (
	"context"
	"fmt"
	"net/http"
	"net/url"
	"path"
	"strconv"

	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/auth"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/auth/providers"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/auth/oauth"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/db"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitlab"
	"golang.org/x/oauth2"
)

type sessionIssuerHelper struct {
	*extsvc.CodeHost
	clientID string
}

func (s *sessionIssuerHelper) GetOrCreateUser(ctx context.Context, token *oauth2.Token) (actr *actor.Actor, safeErrMsg string, err error) {
	gUser, err := UserFromContext(ctx)
	if err != nil {
		return nil, "Could not read GitLab user from callback request.", errors.Wrap(err, "could not read user from context")
	}

	login, err := auth.NormalizeUsername(gUser.Username)
	if err != nil {
		return nil, fmt.Sprintf("Error normalizing the username %q. See https://docs.sourcegraph.com/admin/auth/#username-normalization.", login), err
	}

	var data extsvc.AccountData
	gitlab.SetExternalAccountData(&data, gUser, token)

	// Unlike with GitHub, we can *only* use the primary email to resolve the user's identity,
	// because the GitLab API does not return whether an email has been verified. The user's primary
	// email on GitLab is always verified, so we use that.
	userID, safeErrMsg, err := auth.GetAndSaveUser(ctx, auth.GetAndSaveUserOp{
		UserProps: db.NewUser{
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
		CreateIfNotExist:    true,
	})
	if err != nil {
		return nil, safeErrMsg, err
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

func SignOutURL(gitlabURL string) (string, error) {
	if gitlabURL == "" {
		gitlabURL = "https://gitlab.com"
	}
	ghURL, err := url.Parse(gitlabURL)
	if err != nil {
		return "", err
	}
	ghURL.Path = path.Join(ghURL.Path, "users/sign_out")
	return ghURL.String(), nil
}
