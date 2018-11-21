package oauth

import (
	"context"
	"crypto/rand"
	"encoding/base64"
	"encoding/json"
	"net/http"
	"net/url"
	"path"

	"github.com/dghubble/gologin"
	goauth2 "github.com/dghubble/gologin/oauth2"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/external/auth"
	"github.com/sourcegraph/sourcegraph/schema"
	"golang.org/x/oauth2"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

type Provider struct {
	ProviderOp

	Login    http.Handler
	Callback http.Handler
}

var _ auth.Provider = (*Provider)(nil)

func getProvider(serviceType, id string) *Provider {
	p, ok := auth.GetProviderByConfigID(auth.ProviderConfigID{Type: serviceType, ID: id}).(*Provider)
	if !ok {
		return nil
	}
	return p
}

func (p *Provider) ConfigID() auth.ProviderConfigID {
	return auth.ProviderConfigID{
		ID:   p.ServiceID,
		Type: p.ServiceType,
	}
}

func (p *Provider) Config() schema.AuthProviders {
	return p.SourceConfig
}

func (p *Provider) CachedInfo() *auth.ProviderInfo {
	displayName := p.ServiceID
	switch {
	case p.SourceConfig.Github != nil && p.SourceConfig.Github.DisplayName != "":
		displayName = p.SourceConfig.Github.DisplayName
	case p.SourceConfig.Gitlab != nil && p.SourceConfig.Gitlab.DisplayName != "":
		displayName = p.SourceConfig.Gitlab.DisplayName
	}
	return &auth.ProviderInfo{
		ServiceID:   p.ServiceID,
		ClientID:    p.OAuth2Config.ClientID,
		DisplayName: displayName,
		AuthenticationURL: (&url.URL{
			Path:     path.Join(p.AuthPrefix, "login"),
			RawQuery: (url.Values{"pc": []string{p.ConfigID().ID}}).Encode(),
		}).String(),
	}
}

func (p *Provider) Refresh(ctx context.Context) error {
	return nil
}

type ProviderOp struct {
	AuthPrefix   string
	OAuth2Config oauth2.Config
	SourceConfig schema.AuthProviders
	StateConfig  gologin.CookieConfig
	ServiceID    string
	ServiceType  string
	Login        http.Handler
	Callback     http.Handler
}

func NewProvider(op ProviderOp) *Provider {
	return &Provider{
		ProviderOp: op,
		Login:      stateHandler(true, op.ServiceID, op.StateConfig, op.Login),
		Callback:   stateHandler(false, op.ServiceID, op.StateConfig, op.Callback),
	}
}

// stateHandler decodes the state from the gologin cookie and sets it in the context. It checked by
// some downstream handler to ensure equality with the value of the state URL param.
//
// This is very similar to gologin's default StateHandler function, but we define our own, because
// we encode the returnTo URL in the state. We could use the `redirect_uri` parameter to do this,
// but doing so would require using Sourcegraph's external hostname and making sure it is consistent
// with what is specified in the OAuth app config as the "callback URL."
func stateHandler(isLogin bool, providerID string, config gologin.CookieConfig, success http.Handler) http.Handler {
	fn := func(w http.ResponseWriter, req *http.Request) {
		ctx := req.Context()
		csrf, err := randomState()
		if err != nil {
			log15.Error("Failed to generated random state", "error", err)
			http.Error(w, "Failed to generate random state", http.StatusInternalServerError)
			return
		}
		if isLogin {
			// add Cookie with a random state + redirect
			stateVal, err := LoginState{
				Redirect:   req.URL.Query().Get("redirect"),
				CSRF:       csrf,
				ProviderID: providerID,
			}.Encode()
			if err != nil {
				log15.Error("Could not encode OAuth state", "error", err)
				http.Error(w, "Could not encode OAuth state.", http.StatusInternalServerError)
				return
			}
			http.SetCookie(w, NewCookie(config, stateVal))
			ctx = goauth2.WithState(ctx, stateVal)
		} else if cookie, err := req.Cookie(config.Name); err == nil { // not login and cookie exists
			// add the cookie state to the ctx
			ctx = goauth2.WithState(ctx, cookie.Value)
		}
		success.ServeHTTP(w, req.WithContext(ctx))
	}
	return http.HandlerFunc(fn)
}

type LoginState struct {
	// Redirect is the URL path to redirect to after login.
	Redirect string

	// ProviderID is the service ID of the provider that is handling the auth flow.
	ProviderID string

	// CSRF is the random string that ensures the encoded state is sufficiently random to be checked
	// for CSRF purposes.
	CSRF string
}

func (s LoginState) Encode() (string, error) {
	sb, err := json.Marshal(s)
	if err != nil {
		return "", err
	}
	return base64.RawURLEncoding.EncodeToString(sb), nil
}

func DecodeState(encoded string) (*LoginState, error) {
	var s LoginState
	decoded, err := base64.RawURLEncoding.DecodeString(encoded)
	if err != nil {
		return nil, err
	}
	if err := json.Unmarshal(decoded, &s); err != nil {
		return nil, err
	}
	return &s, nil
}

// Returns a base64 encoded random 32 byte string.
func randomState() (string, error) {
	b := make([]byte, 32)
	_, err := rand.Read(b)
	if err != nil {
		return "", err
	}
	return base64.RawURLEncoding.EncodeToString(b), nil
}
