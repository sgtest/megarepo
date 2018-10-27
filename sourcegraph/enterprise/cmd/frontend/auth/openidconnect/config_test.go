package openidconnect

import (
	"testing"

	"github.com/sourcegraph/sourcegraph/pkg/conf"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestValidateCustom(t *testing.T) {
	tests := map[string]struct {
		input        schema.SiteConfiguration
		wantProblems []string
	}{
		"duplicates": {
			input: schema.SiteConfiguration{
				AppURL: "x",
				AuthProviders: []schema.AuthProviders{
					{Openidconnect: &schema.OpenIDConnectAuthProvider{Type: "openidconnect", Issuer: "x"}},
					{Openidconnect: &schema.OpenIDConnectAuthProvider{Type: "openidconnect", Issuer: "x"}},
				},
			},
			wantProblems: []string{"OpenID Connect auth provider at index 1 is duplicate of index 0"},
		},
	}
	for name, test := range tests {
		t.Run(name, func(t *testing.T) {
			conf.TestValidator(t, test.input, validateConfig, test.wantProblems)
		})
	}
}

func TestProviderConfigID(t *testing.T) {
	p := schema.OpenIDConnectAuthProvider{Issuer: "x"}
	id1 := providerConfigID(&p)
	id2 := providerConfigID(&p)
	if id1 != id2 {
		t.Errorf("id1 (%q) != id2 (%q)", id1, id2)
	}
}
