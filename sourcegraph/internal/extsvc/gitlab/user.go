package gitlab

import (
	"context"
	"encoding/json"

	"golang.org/x/oauth2"

	"github.com/sourcegraph/sourcegraph/internal/encryption"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
)

// GetExternalAccountData returns the deserialized user and token from the external account data
// JSON blob in a typesafe way.
func GetExternalAccountData(ctx context.Context, data *extsvc.AccountData) (usr *AuthUser, tok *oauth2.Token, err error) {
	if data.Data != nil {
		usr, err = encryption.DecryptJSON[AuthUser](ctx, data.Data)
		if err != nil {
			return nil, nil, err
		}
	}

	if data.AuthData != nil {
		tok, err = encryption.DecryptJSON[oauth2.Token](ctx, data.AuthData)
		if err != nil {
			return nil, nil, err
		}
	}

	return usr, tok, nil
}

func GetPublicExternalAccountData(ctx context.Context, accountData *extsvc.AccountData) (*extsvc.PublicAccountData, error) {
	data, _, err := GetExternalAccountData(ctx, accountData)
	if err != nil {
		return nil, err
	}
	return &extsvc.PublicAccountData{
		DisplayName: data.Name,
		Login:       data.Username,
		URL:         data.WebURL,
	}, nil
}

// SetExternalAccountData sets the user and token into the external account data blob.
func SetExternalAccountData(data *extsvc.AccountData, user *AuthUser, token *oauth2.Token) error {
	serializedUser, err := json.Marshal(user)
	if err != nil {
		return err
	}
	serializedToken, err := json.Marshal(token)
	if err != nil {
		return err
	}

	data.Data = extsvc.NewUnencryptedData(serializedUser)
	data.AuthData = extsvc.NewUnencryptedData(serializedToken)
	return nil
}
