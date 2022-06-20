package githuboauth

import (
	"testing"

	"github.com/davecgh/go-spew/spew"
	"github.com/google/go-cmp/cmp"
	"golang.org/x/oauth2"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/auth/oauth"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestParseConfig(t *testing.T) {
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	spew.Config.DisablePointerAddresses = true
	spew.Config.SortKeys = true
	spew.Config.SpewKeys = true

	type args struct {
		cfg *conf.Unified
	}
	tests := []struct {
		name          string
		args          args
		wantProviders []Provider
		wantProblems  []string
	}{
		{
			name:          "No configs",
			args:          args{cfg: &conf.Unified{}},
			wantProviders: []Provider(nil),
		},
		{
			name: "1 GitHub.com config",
			args: args{cfg: &conf.Unified{SiteConfiguration: schema.SiteConfiguration{
				AuthProviders: []schema.AuthProviders{{
					Github: &schema.GitHubAuthProvider{
						ClientID:     "myclientid",
						ClientSecret: "myclientsecret",
						DisplayName:  "GitHub",
						Type:         extsvc.TypeGitHub,
						Url:          "https://github.com",
						AllowOrgs:    []string{"myorg"},
					},
				}},
			}}},
			wantProviders: []Provider{
				{
					GitHubAuthProvider: &schema.GitHubAuthProvider{
						ClientID:     "myclientid",
						ClientSecret: "myclientsecret",
						DisplayName:  "GitHub",
						Type:         extsvc.TypeGitHub,
						Url:          "https://github.com",
						AllowOrgs:    []string{"myorg"},
					},
					Provider: provider("https://github.com/", oauth2.Config{
						ClientID:     "myclientid",
						ClientSecret: "myclientsecret",
						Endpoint: oauth2.Endpoint{
							AuthURL:  "https://github.com/login/oauth/authorize",
							TokenURL: "https://github.com/login/oauth/access_token",
						},
						Scopes: []string{"user:email", "repo", "read:org"},
					}),
				},
			},
		},
		{
			name: "2 GitHub configs",
			args: args{cfg: &conf.Unified{SiteConfiguration: schema.SiteConfiguration{
				AuthProviders: []schema.AuthProviders{{
					Github: &schema.GitHubAuthProvider{
						ClientID:     "myclientid",
						ClientSecret: "myclientsecret",
						DisplayName:  "GitHub",
						Type:         extsvc.TypeGitHub,
						Url:          "https://github.com",
						AllowOrgs:    []string{"myorg"},
					},
				}, {
					Github: &schema.GitHubAuthProvider{
						ClientID:     "myclientid2",
						ClientSecret: "myclientsecret2",
						DisplayName:  "GitHub Enterprise",
						Type:         extsvc.TypeGitHub,
						Url:          "https://mycompany.com",
					},
				}},
			}}},
			wantProviders: []Provider{
				{
					GitHubAuthProvider: &schema.GitHubAuthProvider{
						ClientID:     "myclientid",
						ClientSecret: "myclientsecret",
						DisplayName:  "GitHub",
						Type:         extsvc.TypeGitHub,
						Url:          "https://github.com",
						AllowOrgs:    []string{"myorg"},
					},
					Provider: provider("https://github.com/", oauth2.Config{
						ClientID:     "myclientid",
						ClientSecret: "myclientsecret",
						Endpoint: oauth2.Endpoint{
							AuthURL:  "https://github.com/login/oauth/authorize",
							TokenURL: "https://github.com/login/oauth/access_token",
						},
						Scopes: []string{"user:email", "repo", "read:org"},
					}),
				},
				{
					GitHubAuthProvider: &schema.GitHubAuthProvider{
						ClientID:     "myclientid2",
						ClientSecret: "myclientsecret2",
						DisplayName:  "GitHub Enterprise",
						Type:         extsvc.TypeGitHub,
						Url:          "https://mycompany.com",
					},
					Provider: provider("https://mycompany.com/", oauth2.Config{
						ClientID:     "myclientid2",
						ClientSecret: "myclientsecret2",
						Endpoint: oauth2.Endpoint{
							AuthURL:  "https://mycompany.com/login/oauth/authorize",
							TokenURL: "https://mycompany.com/login/oauth/access_token",
						},
						Scopes: []string{"user:email", "repo"},
					}),
				},
			},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			gotProviders, gotProblems := parseConfig(tt.args.cfg, db)
			gotConfigs := make([]oauth2.Config, len(gotProviders))
			for k, p := range gotProviders {
				if p, ok := p.Provider.(*oauth.Provider); ok {
					p.Login, p.Callback = nil, nil
					gotConfigs[k] = p.OAuth2Config()
					p.OAuth2Config = nil
					p.ProviderOp.Login, p.ProviderOp.Callback = nil, nil
				}
			}
			wantConfigs := make([]oauth2.Config, len(tt.wantProviders))
			for k, p := range tt.wantProviders {
				k := k
				if q, ok := p.Provider.(*oauth.Provider); ok {
					q.SourceConfig = schema.AuthProviders{Github: p.GitHubAuthProvider}
					wantConfigs[k] = q.OAuth2Config()
					q.OAuth2Config = nil
				}
			}
			if diff := cmp.Diff(tt.wantProviders, gotProviders); diff != "" {
				t.Errorf("providers: %s", diff)
			}
			if diff := cmp.Diff(tt.wantProblems, gotProblems.Messages()); diff != "" {
				t.Errorf("problems: %s", diff)
			}
			if diff := cmp.Diff(wantConfigs, gotConfigs); diff != "" {
				t.Errorf("problems: %s", diff)
			}
		})
	}
}

func provider(serviceID string, oauth2Config oauth2.Config) *oauth.Provider {
	op := oauth.ProviderOp{
		AuthPrefix:   authPrefix,
		OAuth2Config: func(_ ...string) oauth2.Config { return oauth2Config },
		StateConfig:  getStateConfig(),
		ServiceID:    serviceID,
		ServiceType:  extsvc.TypeGitHub,
	}
	return &oauth.Provider{ProviderOp: op}
}
