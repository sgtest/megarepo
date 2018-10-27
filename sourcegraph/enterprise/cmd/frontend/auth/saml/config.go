package saml

import (
	"context"
	"crypto/sha256"
	"encoding/base64"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"path"
	"strconv"
	"strings"

	"github.com/sourcegraph/enterprise/cmd/frontend/internal/licensing"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/external/auth"
	"github.com/sourcegraph/sourcegraph/pkg/conf"
	"github.com/sourcegraph/sourcegraph/pkg/env"
	"github.com/sourcegraph/sourcegraph/schema"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

var mockGetProviderValue *provider

// getProvider looks up the registered saml auth provider with the given ID.
func getProvider(pcID string) *provider {
	if mockGetProviderValue != nil {
		return mockGetProviderValue
	}

	p, _ := auth.GetProviderByConfigID(auth.ProviderConfigID{Type: providerType, ID: pcID}).(*provider)
	if p != nil {
		return p
	}

	// Special case: if there is only a single SAML auth provider, return it regardless of the pcID.
	for _, ap := range auth.Providers() {
		if ap.Config().Saml != nil {
			if p != nil {
				return nil // multiple SAML providers, can't use this special case
			}
			p = ap.(*provider)
		}
	}

	return p
}

func handleGetProvider(ctx context.Context, w http.ResponseWriter, pcID string) (p *provider, handled bool) {
	handled = true // safer default

	// License check.
	if !licensing.IsFeatureEnabledLenient(licensing.FeatureExternalAuthProvider) {
		licensing.WriteSubscriptionErrorResponseForFeature(w, "SAML user authentication (SSO)")
		return nil, true
	}

	p = getProvider(pcID)
	if p == nil {
		log15.Error("No SAML auth provider found with ID.", "id", pcID)
		http.Error(w, "Misconfigured SAML auth provider.", http.StatusInternalServerError)
		return nil, true
	}
	if err := p.Refresh(ctx); err != nil {
		log15.Error("Error refreshing SAML auth provider.", "id", p.ConfigID(), "error", err)
		http.Error(w, "Unexpected error refreshing SAML authentication provider.", http.StatusInternalServerError)
		return nil, true
	}
	return p, false
}

func init() {
	conf.ContributeValidator(validateConfig)
}

func validateConfig(c schema.SiteConfiguration) (problems []string) {
	var loggedNeedsAppURL bool
	for _, p := range c.AuthProviders {
		if p.Saml != nil && c.AppURL == "" && !loggedNeedsAppURL {
			problems = append(problems, `saml auth provider requires appURL to be set to the external URL of your site (example: https://sourcegraph.example.com)`)
			loggedNeedsAppURL = true
		}
	}

	seen := map[schema.SAMLAuthProvider]int{}
	for i, p := range c.AuthProviders {
		if p.Saml != nil {
			if j, ok := seen[*p.Saml]; ok {
				problems = append(problems, fmt.Sprintf("SAML auth provider at index %d is duplicate of index %d, ignoring", i, j))
			} else {
				seen[*p.Saml] = i
			}
		}
	}

	return problems
}

func withConfigDefaults(pc *schema.SAMLAuthProvider) *schema.SAMLAuthProvider {
	if pc.ServiceProviderIssuer == "" {
		appURL := conf.Get().AppURL
		if appURL == "" {
			// An empty issuer will be detected as an error later.
			return pc
		}

		// Derive default issuer from appURL.
		tmp := *pc
		tmp.ServiceProviderIssuer = strings.TrimSuffix(conf.Get().AppURL, "/") + path.Join(authPrefix, "metadata")
		return &tmp
	}
	return pc
}

func getNameIDFormat(pc *schema.SAMLAuthProvider) string {
	// Persistent is best because users will reuse their user_external_accounts row instead of (as
	// with transient) creating a new one each time they authenticate.
	const defaultNameIDFormat = "urn:oasis:names:tc:SAML:2.0:nameid-format:persistent"
	if pc.NameIDFormat != "" {
		return pc.NameIDFormat
	}
	return defaultNameIDFormat
}

// providerConfigID produces a semi-stable identifier for a saml auth provider config object. It is
// used to distinguish between multiple auth providers of the same type when in multi-step auth
// flows. Its value is never persisted, and it must be deterministic.
//
// If there is only a single saml auth provider, it returns the empty string because that satisfies
// the requirements above.
func providerConfigID(pc *schema.SAMLAuthProvider, multiple bool) string {
	if !multiple {
		return ""
	}
	data, err := json.Marshal(pc)
	if err != nil {
		panic(err)
	}
	b := sha256.Sum256(data)
	return base64.RawURLEncoding.EncodeToString(b[:16])
}

var traceLogEnabled, _ = strconv.ParseBool(env.Get("INSECURE_SAML_LOG_TRACES", "false", "Log all SAML requests and responses. Only use during testing because the log messages will contain sensitive data."))

func traceLog(description, body string) {
	if traceLogEnabled {
		const n = 40
		log.Printf("%s SAML trace: %s\n%s\n%s", strings.Repeat("=", n), description, body, strings.Repeat("=", n+len(description)+1))
	}
}
