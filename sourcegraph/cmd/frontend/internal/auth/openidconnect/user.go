package openidconnect

import (
	"context"
	"encoding/json"
	"fmt"

	"github.com/coreos/go-oidc"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/auth"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/encryption"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type ExternalAccountData struct {
	IDToken    oidc.IDToken  `json:"idToken"`
	UserInfo   oidc.UserInfo `json:"userInfo"`
	UserClaims userClaims    `json:"userClaims"`
}

// getOrCreateUser gets or creates a user account based on the OpenID Connect token. It returns the
// authenticated actor if successful; otherwise it returns a friendly error message (safeErrMsg)
// that is safe to display to users, and a non-nil err with lower-level error details.
func getOrCreateUser(ctx context.Context, db database.DB, p *Provider, idToken *oidc.IDToken, userInfo *oidc.UserInfo, claims *userClaims, usernamePrefix string) (_ *actor.Actor, safeErrMsg string, err error) {
	if userInfo.Email == "" {
		return nil, "Only users with an email address may authenticate to Sourcegraph.", errors.New("no email address in claims")
	}
	if unverifiedEmail := claims.EmailVerified != nil && !*claims.EmailVerified; unverifiedEmail {
		// If the OP explicitly reports `"email_verified": false`, then reject the authentication
		// attempt. If undefined or true, then it will be allowed.
		return nil, fmt.Sprintf("Only users with verified email addresses may authenticate to Sourcegraph. The email address %q is not verified on the external authentication provider.", userInfo.Email), errors.Errorf("refusing unverified user email address %q", userInfo.Email)
	}

	pi, err := p.getCachedInfoAndError()
	if err != nil {
		return nil, "", err
	}

	login := claims.PreferredUsername
	if login == "" {
		login = userInfo.Email
	}
	email := userInfo.Email
	displayName := claims.GivenName
	if displayName == "" {
		if claims.Name == "" {
			displayName = claims.Name
		} else {
			displayName = login
		}
	}

	if usernamePrefix != "" {
		login = usernamePrefix + login
	}
	login, err = auth.NormalizeUsername(login)
	if err != nil {
		return nil,
			fmt.Sprintf("Error normalizing the username %q. See https://docs.sourcegraph.com/admin/auth/#username-normalization.", login),
			errors.Wrap(err, "normalize username")
	}

	serialized, err := json.Marshal(ExternalAccountData{
		IDToken:    *idToken,
		UserInfo:   *userInfo,
		UserClaims: *claims,
	})
	if err != nil {
		return nil, "", err
	}
	data := extsvc.AccountData{
		Data: extsvc.NewUnencryptedData(serialized),
	}

	userID, safeErrMsg, err := auth.GetAndSaveUser(ctx, db, auth.GetAndSaveUserOp{
		UserProps: database.NewUser{
			Username:        login,
			Email:           email,
			EmailIsVerified: email != "", // verified email check is at the top of the function
			DisplayName:     displayName,
			AvatarURL:       claims.Picture,
		},
		ExternalAccount: extsvc.AccountSpec{
			ServiceType: p.config.Type,
			ServiceID:   pi.ServiceID,
			ClientID:    pi.ClientID,
			AccountID:   idToken.Subject,
		},
		ExternalAccountData: data,
		CreateIfNotExist:    p.config.AllowSignup == nil || *p.config.AllowSignup,
	})
	if err != nil {
		return nil, safeErrMsg, err
	}
	return actor.FromUser(userID), "", nil
}

// GetExternalAccountData returns the deserialized JSON blob from user external accounts table
func GetExternalAccountData(ctx context.Context, data *extsvc.AccountData) (val *ExternalAccountData, err error) {
	if data.Data != nil {
		val, err = encryption.DecryptJSON[ExternalAccountData](ctx, data.Data)
		if err != nil {
			return nil, err
		}
	}

	return val, nil
}

func GetPublicExternalAccountData(ctx context.Context, accountData *extsvc.AccountData) (*extsvc.PublicAccountData, error) {
	data, err := GetExternalAccountData(ctx, accountData)
	if err != nil {
		return nil, err
	}

	login := data.UserClaims.PreferredUsername
	if login == "" {
		login = data.UserInfo.Email
	}
	displayName := data.UserClaims.GivenName
	if displayName == "" {
		if data.UserClaims.Name == "" {
			displayName = data.UserClaims.Name
		} else {
			displayName = login
		}
	}

	return &extsvc.PublicAccountData{
		Login:       login,
		DisplayName: displayName,
		URL:         data.UserInfo.Profile,
	}, nil
}
