package openidconnect

import (
	"net/http"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/external/session"
	"github.com/sourcegraph/sourcegraph/internal/auth/providers"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// SessionKey is the key of the key-value pair in a user session for the OpenID
// Connect authentication provider.
const SessionKey = "oidc@0"

// SessionData is the data format to be stored in a user session.
type SessionData struct {
	ID providers.ConfigID

	// Store only the oauth2.Token fields we need, to avoid hitting the ~4096-byte session data
	// limit.
	AccessToken string
	TokenType   string
}

// SignOut clears OpenID Connect-related data from the session. If possible, it revokes the token
// from the OP. If there is an end-session endpoint, it returns that for the caller to present to
// the user.
func SignOut(w http.ResponseWriter, r *http.Request, sessionKey string, getProvider func(id string) *Provider) (endSessionEndpoint string, err error) {
	defer func() {
		if err != nil {
			_ = session.SetData(w, r, sessionKey, nil) // clear the bad data
		}
	}()

	var data *SessionData
	if err := session.GetData(r, sessionKey, &data); err != nil {
		return "", errors.WithMessage(err, "reading OpenID Connect session data")
	}
	if err := session.SetData(w, r, sessionKey, nil); err != nil {
		return "", errors.WithMessage(err, "clearing OpenID Connect session data")
	}
	if data != nil {
		p := getProvider(data.ID.ID)
		if p == nil {
			return "", errors.Errorf("unable to revoke token or end session for OpenID Connect because no provider %q exists", data.ID)
		}

		endSessionEndpoint = p.oidc.EndSessionEndpoint

		if p.oidc.RevocationEndpoint != "" {
			if err := revokeToken(r.Context(), p, data.AccessToken, data.TokenType); err != nil {
				return endSessionEndpoint, errors.WithMessage(err, "revoking OpenID Connect token")
			}
		}
	}

	return endSessionEndpoint, nil
}
