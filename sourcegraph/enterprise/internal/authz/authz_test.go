package authz

import (
	"context"
	"net/url"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/google/go-cmp/cmp/cmpopts"
	jsoniter "github.com/json-iterator/go"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/globals"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/authz/gitlab"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/licensing"
	"github.com/sourcegraph/sourcegraph/internal/auth/providers"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

const bogusKey = `LS0tLS1CRUdJTiBSU0EgUFJJVkFURSBLRVktLS0tLQpNSUlCUEFJQkFBSkJBUEpIaWprdG1UMUlLYUd0YTVFZXAzQVo5Q2VPZUw4alBESUZUN3dRZ0tabXQzRUZxRGhCCk93bitRVUhKdUs5Zm92UkROSmVWTDJvWTVCT0l6NHJ3L0cwQ0F3RUFBUUpCQU1BK0o5Mks0d2NQVllsbWMrM28KcHU5NmlKTkNwMmp5Nm5hK1pEQlQzK0VvSUo1VFJGdnN3R2kvTHUzZThYUWwxTDNTM21ub0xPSlZNcTF0bUxOMgpIY0VDSVFEK3daeS83RlYxUEFtdmlXeWlYVklETzJnNWJOaUJlbmdKQ3hFa3Nia1VtUUloQVBOMlZaczN6UFFwCk1EVG9vTlJXcnl0RW1URERkamdiOFpzTldYL1JPRGIxQWlCZWNKblNVQ05TQllLMXJ5VTFmNURTbitoQU9ZaDkKWDFBMlVnTDE3bWhsS1FJaEFPK2JMNmRDWktpTGZORWxmVnRkTUtxQnFjNlBIK01heFU2VzlkVlFvR1dkQWlFQQptdGZ5cE9zYTFiS2hFTDg0blovaXZFYkJyaVJHalAya3lERHYzUlg0V0JrPQotLS0tLUVORCBSU0EgUFJJVkFURSBLRVktLS0tLQo=`

type gitlabAuthzProviderParams struct {
	OAuthOp gitlab.OAuthProviderOp
	SudoOp  gitlab.SudoProviderOp
}

func (m gitlabAuthzProviderParams) Repos(ctx context.Context, repos []*types.Repo) (mine []*types.Repo, others []*types.Repo) {
	panic("should never be called")
}

func (m gitlabAuthzProviderParams) FetchAccount(ctx context.Context, user *types.User, current []*extsvc.Account, verifiedEmails []string) (mine *extsvc.Account, err error) {
	panic("should never be called")
}

func (m gitlabAuthzProviderParams) ServiceID() string {
	panic("should never be called")
}

func (m gitlabAuthzProviderParams) ServiceType() string {
	return extsvc.TypeGitLab
}

func (m gitlabAuthzProviderParams) URN() string {
	panic("should never be called")
}

func (m gitlabAuthzProviderParams) ValidateConnection(context.Context) error { return nil }

func (m gitlabAuthzProviderParams) FetchUserPerms(context.Context, *extsvc.Account, authz.FetchPermsOptions) (*authz.ExternalUserPermissions, error) {
	panic("should never be called")
}

func (m gitlabAuthzProviderParams) FetchUserPermsByToken(context.Context, string, authz.FetchPermsOptions) (*authz.ExternalUserPermissions, error) {
	panic("should never be called")
}

func (m gitlabAuthzProviderParams) FetchRepoPerms(context.Context, *extsvc.Repository, authz.FetchPermsOptions) ([]extsvc.AccountID, error) {
	panic("should never be called")
}

var errPermissionsUserMappingConflict = errors.New("The explicit permissions API (site configuration `permissions.userMapping`) cannot be enabled when bitbucketServer authorization provider is in use. Blocking access to all repositories until the conflict is resolved.")

func TestAuthzProvidersFromConfig(t *testing.T) {
	t.Cleanup(licensing.TestingSkipFeatureChecks())
	gitlab.NewOAuthProvider = func(op gitlab.OAuthProviderOp) authz.Provider {
		return gitlabAuthzProviderParams{OAuthOp: op}
	}
	gitlab.NewSudoProvider = func(op gitlab.SudoProviderOp) authz.Provider {
		return gitlabAuthzProviderParams{SudoOp: op}
	}

	providersEqual := func(want ...authz.Provider) func(*testing.T, []authz.Provider) {
		return func(t *testing.T, have []authz.Provider) {
			if diff := cmp.Diff(want, have, cmpopts.IgnoreInterfaces(struct{ database.DB }{})); diff != "" {
				t.Errorf("authzProviders mismatch (-want +got):\n%s", diff)
			}
		}
	}

	tests := []struct {
		description                  string
		cfg                          conf.Unified
		gitlabConnections            []*schema.GitLabConnection
		bitbucketServerConnections   []*schema.BitbucketServerConnection
		expAuthzAllowAccessByDefault bool
		expAuthzProviders            func(*testing.T, []authz.Provider)
		expSeriousProblems           []string
	}{
		{
			description: "1 GitLab connection with authz enabled, 1 GitLab matching auth provider",
			cfg: conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					AuthProviders: []schema.AuthProviders{{
						Gitlab: &schema.GitLabAuthProvider{
							ClientID:     "clientID",
							ClientSecret: "clientSecret",
							DisplayName:  "GitLab",
							Type:         extsvc.TypeGitLab,
							Url:          "https://gitlab.mine",
						},
					}},
				},
			},
			gitlabConnections: []*schema.GitLabConnection{
				{
					Authorization: &schema.GitLabAuthorization{
						IdentityProvider: schema.IdentityProvider{Oauth: &schema.OAuthIdentity{Type: "oauth"}},
					},
					Url:   "https://gitlab.mine",
					Token: "asdf",
				},
			},
			expAuthzAllowAccessByDefault: true,
			expAuthzProviders: providersEqual(
				gitlabAuthzProviderParams{
					OAuthOp: gitlab.OAuthProviderOp{
						URN:     "extsvc:gitlab:0",
						BaseURL: mustURLParse(t, "https://gitlab.mine"),
						Token:   "asdf",
					},
				},
			),
		},
		{
			description: "1 GitLab connection with authz enabled, 1 GitLab auth provider but doesn't match",
			cfg: conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					AuthProviders: []schema.AuthProviders{{
						Gitlab: &schema.GitLabAuthProvider{
							ClientID:     "clientID",
							ClientSecret: "clientSecret",
							DisplayName:  "GitLab",
							Type:         extsvc.TypeGitLab,
							Url:          "https://gitlab.com",
						},
					}},
				},
			},
			gitlabConnections: []*schema.GitLabConnection{
				{
					Authorization: &schema.GitLabAuthorization{
						IdentityProvider: schema.IdentityProvider{Oauth: &schema.OAuthIdentity{Type: "oauth"}},
					},
					Url:   "https://gitlab.mine",
					Token: "asdf",
				},
			},
			expAuthzAllowAccessByDefault: false,
			expSeriousProblems:           []string{"Did not find authentication provider matching \"https://gitlab.mine\". Check the [**site configuration**](/site-admin/configuration) to verify an entry in [`auth.providers`](https://docs.sourcegraph.com/admin/auth) exists for https://gitlab.mine."},
		},
		{
			description: "1 GitLab connection with authz enabled, no GitLab auth provider",
			cfg: conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					AuthProviders: []schema.AuthProviders{{
						Builtin: &schema.BuiltinAuthProvider{Type: "builtin"},
					}},
				},
			},
			gitlabConnections: []*schema.GitLabConnection{
				{
					Authorization: &schema.GitLabAuthorization{
						IdentityProvider: schema.IdentityProvider{Oauth: &schema.OAuthIdentity{Type: "oauth"}},
					},
					Url:   "https://gitlab.mine",
					Token: "asdf",
				},
			},
			expAuthzAllowAccessByDefault: false,
			expSeriousProblems:           []string{"Did not find authentication provider matching \"https://gitlab.mine\". Check the [**site configuration**](/site-admin/configuration) to verify an entry in [`auth.providers`](https://docs.sourcegraph.com/admin/auth) exists for https://gitlab.mine."},
		},
		{
			description: "Two GitLab connections with authz enabled, two matching GitLab auth providers",
			cfg: conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					AuthProviders: []schema.AuthProviders{
						{
							Gitlab: &schema.GitLabAuthProvider{
								ClientID:     "clientID",
								ClientSecret: "clientSecret",
								DisplayName:  "GitLab.com",
								Type:         extsvc.TypeGitLab,
								Url:          "https://gitlab.com",
							},
						}, {
							Gitlab: &schema.GitLabAuthProvider{
								ClientID:     "clientID",
								ClientSecret: "clientSecret",
								DisplayName:  "GitLab.mine",
								Type:         extsvc.TypeGitLab,
								Url:          "https://gitlab.mine",
							},
						},
					},
				},
			},
			gitlabConnections: []*schema.GitLabConnection{
				{
					Authorization: &schema.GitLabAuthorization{
						IdentityProvider: schema.IdentityProvider{Oauth: &schema.OAuthIdentity{Type: "oauth"}},
					},
					Url:   "https://gitlab.mine",
					Token: "asdf",
				},
				{
					Authorization: &schema.GitLabAuthorization{
						IdentityProvider: schema.IdentityProvider{Oauth: &schema.OAuthIdentity{Type: "oauth"}},
					},
					Url:   "https://gitlab.com",
					Token: "asdf",
				},
			},
			expAuthzAllowAccessByDefault: true,
			expAuthzProviders: providersEqual(
				gitlabAuthzProviderParams{
					OAuthOp: gitlab.OAuthProviderOp{
						URN:     "extsvc:gitlab:0",
						BaseURL: mustURLParse(t, "https://gitlab.mine"),
						Token:   "asdf",
					},
				},
				gitlabAuthzProviderParams{
					OAuthOp: gitlab.OAuthProviderOp{
						URN:     "extsvc:gitlab:0",
						BaseURL: mustURLParse(t, "https://gitlab.com"),
						Token:   "asdf",
					},
				},
			),
		},
		{
			description: "1 GitLab connection with authz disabled",
			cfg: conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					AuthProviders: []schema.AuthProviders{{
						Gitlab: &schema.GitLabAuthProvider{
							ClientID:     "clientID",
							ClientSecret: "clientSecret",
							DisplayName:  "GitLab",
							Type:         extsvc.TypeGitLab,
							Url:          "https://gitlab.mine",
						},
					}},
				},
			},
			gitlabConnections: []*schema.GitLabConnection{
				{
					Authorization: nil,
					Url:           "https://gitlab.mine",
					Token:         "asdf",
				},
			},
			expAuthzAllowAccessByDefault: true,
			expAuthzProviders:            nil,
		},
		{
			description: "external auth provider",
			cfg: conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					AuthProviders: []schema.AuthProviders{{
						Saml: &schema.SAMLAuthProvider{
							ConfigID: "okta",
							Type:     "saml",
						},
					}},
				},
			},
			gitlabConnections: []*schema.GitLabConnection{
				{
					Authorization: &schema.GitLabAuthorization{
						IdentityProvider: schema.IdentityProvider{External: &schema.ExternalIdentity{
							Type:             "external",
							AuthProviderID:   "okta",
							AuthProviderType: "saml",
							GitlabProvider:   "my-external",
						}},
					},
					Url:   "https://gitlab.mine",
					Token: "asdf",
				},
			},
			expAuthzAllowAccessByDefault: true,
			expAuthzProviders: providersEqual(
				gitlabAuthzProviderParams{
					SudoOp: gitlab.SudoProviderOp{
						URN:     "extsvc:gitlab:0",
						BaseURL: mustURLParse(t, "https://gitlab.mine"),
						AuthnConfigID: providers.ConfigID{
							Type: "saml",
							ID:   "okta",
						},
						GitLabProvider:    "my-external",
						SudoToken:         "asdf",
						UseNativeUsername: false,
					},
				},
			),
		},
		{
			description: "exact username matching",
			cfg: conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					AuthProviders: []schema.AuthProviders{},
				},
			},
			gitlabConnections: []*schema.GitLabConnection{
				{
					Authorization: &schema.GitLabAuthorization{
						IdentityProvider: schema.IdentityProvider{Username: &schema.UsernameIdentity{Type: "username"}},
					},
					Url:   "https://gitlab.mine",
					Token: "asdf",
				},
			},
			expAuthzAllowAccessByDefault: true,
			expAuthzProviders: providersEqual(
				gitlabAuthzProviderParams{
					SudoOp: gitlab.SudoProviderOp{
						URN:               "extsvc:gitlab:0",
						BaseURL:           mustURLParse(t, "https://gitlab.mine"),
						SudoToken:         "asdf",
						UseNativeUsername: true,
					},
				},
			),
		},
		{
			description: "1 BitbucketServer connection with authz disabled",
			bitbucketServerConnections: []*schema.BitbucketServerConnection{
				{
					Authorization: nil,
					Url:           "https://bitbucket.mycorp.org",
					Username:      "admin",
					Token:         "secret-token",
				},
			},
			expAuthzAllowAccessByDefault: true,
			expAuthzProviders:            providersEqual(),
		},
		{
			description: "Bitbucket Server Oauth config error",
			cfg:         conf.Unified{},
			bitbucketServerConnections: []*schema.BitbucketServerConnection{
				{
					Authorization: &schema.BitbucketServerAuthorization{
						IdentityProvider: schema.BitbucketServerIdentityProvider{
							Username: &schema.BitbucketServerUsernameIdentity{
								Type: "username",
							},
						},
						Oauth: schema.BitbucketServerOAuth{
							ConsumerKey: "sourcegraph",
							SigningKey:  "Invalid Key",
						},
					},
					Url:      "https://bitbucketserver.mycorp.org",
					Username: "admin",
					Token:    "secret-token",
				},
			},
			expAuthzAllowAccessByDefault: false,
			expSeriousProblems:           []string{"authorization.oauth.signingKey: illegal base64 data at input byte 7"},
		},
		{
			description: "Bitbucket Server exact username matching",
			cfg:         conf.Unified{},
			bitbucketServerConnections: []*schema.BitbucketServerConnection{
				{
					Authorization: &schema.BitbucketServerAuthorization{
						IdentityProvider: schema.BitbucketServerIdentityProvider{
							Username: &schema.BitbucketServerUsernameIdentity{
								Type: "username",
							},
						},
						Oauth: schema.BitbucketServerOAuth{
							ConsumerKey: "sourcegraph",
							SigningKey:  bogusKey,
						},
					},
					Url:      "https://bitbucketserver.mycorp.org",
					Username: "admin",
					Token:    "secret-token",
				},
			},
			expAuthzAllowAccessByDefault: true,
			expAuthzProviders: func(t *testing.T, have []authz.Provider) {
				if len(have) == 0 {
					t.Fatalf("no providers")
				}

				if have[0].ServiceType() != extsvc.TypeBitbucketServer {
					t.Fatalf("no Bitbucket Server authz provider returned")
				}
			},
		},

		// For Sourcegraph authz provider
		{
			description: "Explicit permissions can be enabled alongside synced permissions",
			cfg: conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					PermissionsUserMapping: &schema.PermissionsUserMapping{
						Enabled: true,
						BindID:  "email",
					},
					AuthProviders: []schema.AuthProviders{{
						Gitlab: &schema.GitLabAuthProvider{
							ClientID:     "clientID",
							ClientSecret: "clientSecret",
							DisplayName:  "GitLab",
							Type:         extsvc.TypeGitLab,
							Url:          "https://gitlab.mine",
						},
					}},
				},
			},
			gitlabConnections: []*schema.GitLabConnection{
				{
					Authorization: &schema.GitLabAuthorization{
						IdentityProvider: schema.IdentityProvider{Oauth: &schema.OAuthIdentity{Type: "oauth"}},
					},
					Url:   "https://gitlab.mine",
					Token: "asdf",
				},
			},
			expAuthzAllowAccessByDefault: true,
			expAuthzProviders: providersEqual(
				gitlabAuthzProviderParams{
					OAuthOp: gitlab.OAuthProviderOp{
						URN:     "extsvc:gitlab:0",
						BaseURL: mustURLParse(t, "https://gitlab.mine"),
						Token:   "asdf",
					},
				},
			),
		},
	}

	for _, test := range tests {
		t.Run(test.description, func(t *testing.T) {
			externalServices := database.NewMockExternalServiceStore()
			externalServices.ListFunc.SetDefaultHook(func(ctx context.Context, opt database.ExternalServicesListOptions) ([]*types.ExternalService, error) {
				mustMarshalJSONString := func(v any) string {
					str, err := jsoniter.MarshalToString(v)
					require.NoError(t, err)
					return str
				}

				var svcs []*types.ExternalService
				for _, kind := range opt.Kinds {
					switch kind {
					case extsvc.KindGitLab:
						for _, gl := range test.gitlabConnections {
							svcs = append(svcs, &types.ExternalService{
								Kind:   kind,
								Config: extsvc.NewUnencryptedConfig(mustMarshalJSONString(gl)),
							})
						}
					case extsvc.KindBitbucketServer:
						for _, bbs := range test.bitbucketServerConnections {
							svcs = append(svcs, &types.ExternalService{
								Kind:   kind,
								Config: extsvc.NewUnencryptedConfig(mustMarshalJSONString(bbs)),
							})
						}
					case extsvc.KindGitHub, extsvc.KindPerforce, extsvc.KindBitbucketCloud, extsvc.KindGerrit, extsvc.KindAzureDevOps:
					default:
						return nil, errors.Errorf("unexpected kind: %s", kind)
					}
				}
				return svcs, nil
			})
			allowAccessByDefault, authzProviders, seriousProblems, _, _ := ProvidersFromConfig(
				context.Background(),
				staticConfig(test.cfg.SiteConfiguration),
				externalServices,
				database.NewMockDB(),
			)
			assert.Equal(t, test.expAuthzAllowAccessByDefault, allowAccessByDefault)
			if test.expAuthzProviders != nil {
				test.expAuthzProviders(t, authzProviders)
			}

			assert.Equal(t, test.expSeriousProblems, seriousProblems)
		})
	}
}

func TestAuthzProvidersEnabledACLsDisabled(t *testing.T) {
	t.Cleanup(licensing.MockCheckFeatureError("failed"))
	tests := []struct {
		description                string
		cfg                        conf.Unified
		azureDevOpsConnections     []*schema.AzureDevOpsConnection
		gitlabConnections          []*schema.GitLabConnection
		bitbucketServerConnections []*schema.BitbucketServerConnection
		githubConnections          []*schema.GitHubConnection
		perforceConnections        []*schema.PerforceConnection
		bitbucketCloudConnections  []*schema.BitbucketCloudConnection
		gerritConnections          []*schema.GerritConnection

		expInvalidConnections []string
		expSeriousProblems    []string
	}{
		{
			description: "Azure DevOps connection with enforce permissions enabled but missing license for ACLs",
			cfg: conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					AuthProviders: []schema.AuthProviders{{
						AzureDevOps: &schema.AzureDevOpsAuthProvider{
							ClientID:     "clientID",
							ClientSecret: "clientSecret",
							DisplayName:  "Azure DevOps",
							Type:         extsvc.TypeAzureDevOps,
						},
					}},
				},
			},
			azureDevOpsConnections: []*schema.AzureDevOpsConnection{
				{
					EnforcePermissions: true,
					Url:                "https://dev.azure.com",
				},
			},
			expSeriousProblems:    []string{"failed"},
			expInvalidConnections: []string{"azuredevops"},
		},
		{
			description: "GitHub connection with authz enabled but missing license for ACLs",
			cfg: conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					AuthProviders: []schema.AuthProviders{{
						Github: &schema.GitHubAuthProvider{
							ClientID:     "clientID",
							ClientSecret: "clientSecret",
							DisplayName:  "GitHub",
							Type:         extsvc.TypeGitHub,
							Url:          "https://github.mine",
						},
					}},
				},
			},
			githubConnections: []*schema.GitHubConnection{
				{
					Authorization: &schema.GitHubAuthorization{},
					Url:           "https://github.com/my-org",
				},
			},
			expSeriousProblems:    []string{"failed"},
			expInvalidConnections: []string{"github"},
		},
		{
			description: "GitLab connection with authz enabled but missing license for ACLs",
			cfg: conf.Unified{
				SiteConfiguration: schema.SiteConfiguration{
					AuthProviders: []schema.AuthProviders{{
						Gitlab: &schema.GitLabAuthProvider{
							ClientID:     "clientID",
							ClientSecret: "clientSecret",
							DisplayName:  "GitLab",
							Type:         extsvc.TypeGitLab,
							Url:          "https://gitlab.mine",
						},
					}},
				},
			},
			gitlabConnections: []*schema.GitLabConnection{
				{
					Authorization: &schema.GitLabAuthorization{
						IdentityProvider: schema.IdentityProvider{Oauth: &schema.OAuthIdentity{Type: "oauth"}},
					},
					Url:   "https://gitlab.mine",
					Token: "asdf",
				},
			},
			expSeriousProblems:    []string{"failed"},
			expInvalidConnections: []string{"gitlab"},
		},
		{
			description: "Bitbucket Server connection with authz enabled but missing license for ACLs",
			cfg:         conf.Unified{},
			bitbucketServerConnections: []*schema.BitbucketServerConnection{
				{
					Authorization: &schema.BitbucketServerAuthorization{
						IdentityProvider: schema.BitbucketServerIdentityProvider{
							Username: &schema.BitbucketServerUsernameIdentity{
								Type: "username",
							},
						},
						Oauth: schema.BitbucketServerOAuth{
							ConsumerKey: "sourcegraph",
							SigningKey:  bogusKey,
						},
					},
					Url:      "https://bitbucketserver.mycorp.org",
					Username: "admin",
					Token:    "secret-token",
				},
			},
			expSeriousProblems:    []string{"failed"},
			expInvalidConnections: []string{"bitbucketServer"},
		},
		{
			description: "Bitbucket Cloud connection with authz enabled but missing license for ACLs",
			cfg:         conf.Unified{},
			bitbucketCloudConnections: []*schema.BitbucketCloudConnection{
				{
					Authorization: &schema.BitbucketCloudAuthorization{},
					Url:           "https://bitbucket.org",
					Username:      "admin",
					AppPassword:   "secret-password",
				},
			},
			expSeriousProblems:    []string{"failed"},
			expInvalidConnections: []string{"bitbucketCloud"},
		},
		{
			description: "Gerrit connection with authz enabled but missing license for ACLs",
			cfg:         conf.Unified{},
			gerritConnections: []*schema.GerritConnection{
				{
					Authorization: &schema.GerritAuthorization{},
					Url:           "https://gerrit.sgdev.org",
					Username:      "admin",
					Password:      "secret-password",
				},
			},
			expSeriousProblems:    []string{"failed"},
			expInvalidConnections: []string{"gerrit"},
		},
		{
			description: "Perforce connection with authz enabled but missing license for ACLs",
			cfg:         conf.Unified{},
			perforceConnections: []*schema.PerforceConnection{
				{
					Authorization: &schema.PerforceAuthorization{},
					P4Port:        "ssl:111.222.333.444:1666",
					P4User:        "admin",
					P4Passwd:      "pa$$word",
					Depots: []string{
						"//Sourcegraph",
						"//Engineering/Cloud",
					},
				},
			},
			expSeriousProblems:    []string{"failed"},
			expInvalidConnections: []string{"perforce"},
		},
	}

	for _, test := range tests {
		t.Run(test.description, func(t *testing.T) {
			externalServices := database.NewMockExternalServiceStore()
			externalServices.ListFunc.SetDefaultHook(func(ctx context.Context, opt database.ExternalServicesListOptions) ([]*types.ExternalService, error) {
				mustMarshalJSONString := func(v any) string {
					str, err := jsoniter.MarshalToString(v)
					require.NoError(t, err)
					return str
				}

				var svcs []*types.ExternalService
				for _, kind := range opt.Kinds {
					switch kind {
					case extsvc.KindAzureDevOps:
						for _, ado := range test.azureDevOpsConnections {
							svcs = append(svcs, &types.ExternalService{
								Kind:   kind,
								Config: extsvc.NewUnencryptedConfig(mustMarshalJSONString(ado)),
							})
						}
					case extsvc.KindGitLab:
						for _, gl := range test.gitlabConnections {
							svcs = append(svcs, &types.ExternalService{
								Kind:   kind,
								Config: extsvc.NewUnencryptedConfig(mustMarshalJSONString(gl)),
							})
						}
					case extsvc.KindBitbucketServer:
						for _, bbs := range test.bitbucketServerConnections {
							svcs = append(svcs, &types.ExternalService{
								Kind:   kind,
								Config: extsvc.NewUnencryptedConfig(mustMarshalJSONString(bbs)),
							})
						}
					case extsvc.KindGitHub:
						for _, gh := range test.githubConnections {
							svcs = append(svcs, &types.ExternalService{
								Kind:   kind,
								Config: extsvc.NewUnencryptedConfig(mustMarshalJSONString(gh)),
							})
						}
					case extsvc.KindBitbucketCloud:
						for _, bbcloud := range test.bitbucketCloudConnections {
							svcs = append(svcs, &types.ExternalService{
								Kind:   kind,
								Config: extsvc.NewUnencryptedConfig(mustMarshalJSONString(bbcloud)),
							})
						}
					case extsvc.KindGerrit:
						for _, g := range test.gerritConnections {
							svcs = append(svcs, &types.ExternalService{
								Kind:   kind,
								Config: extsvc.NewUnencryptedConfig(mustMarshalJSONString(g)),
							})
						}
					case extsvc.KindPerforce:
						for _, pf := range test.perforceConnections {
							svcs = append(svcs, &types.ExternalService{
								Kind:   kind,
								Config: extsvc.NewUnencryptedConfig(mustMarshalJSONString(pf)),
							})
						}
					}
				}
				return svcs, nil
			})

			_, _, seriousProblems, _, invalidConnections := ProvidersFromConfig(
				context.Background(),
				staticConfig(test.cfg.SiteConfiguration),
				externalServices,
				database.NewMockDB(),
			)

			assert.Equal(t, test.expSeriousProblems, seriousProblems)
			assert.Equal(t, test.expInvalidConnections, invalidConnections)
		})
	}
}

type staticConfig schema.SiteConfiguration

func (s staticConfig) SiteConfig() schema.SiteConfiguration {
	return schema.SiteConfiguration(s)
}

func mustURLParse(t *testing.T, u string) *url.URL {
	parsed, err := url.Parse(u)
	if err != nil {
		t.Fatal(err)
	}
	return parsed
}

type mockProvider struct {
	codeHost *extsvc.CodeHost
	extAcct  *extsvc.Account
}

func (p *mockProvider) FetchAccount(context.Context, *types.User, []*extsvc.Account, []string) (mine *extsvc.Account, err error) {
	return p.extAcct, nil
}

func (p *mockProvider) ServiceType() string { return p.codeHost.ServiceType }
func (p *mockProvider) ServiceID() string   { return p.codeHost.ServiceID }
func (p *mockProvider) URN() string         { return extsvc.URN(p.codeHost.ServiceType, 0) }

func (p *mockProvider) ValidateConnection(context.Context) error { return nil }

func (p *mockProvider) FetchUserPerms(context.Context, *extsvc.Account, authz.FetchPermsOptions) (*authz.ExternalUserPermissions, error) {
	return nil, nil
}

func (p *mockProvider) FetchUserPermsByToken(context.Context, string, authz.FetchPermsOptions) (*authz.ExternalUserPermissions, error) {
	return nil, nil
}

func (p *mockProvider) FetchRepoPerms(context.Context, *extsvc.Repository, authz.FetchPermsOptions) ([]extsvc.AccountID, error) {
	return nil, nil
}

func mockExplicitPermissions(enabled bool) func() {
	orig := globals.PermissionsUserMapping()
	globals.SetPermissionsUserMapping(&schema.PermissionsUserMapping{Enabled: enabled})
	return func() {
		globals.SetPermissionsUserMapping(orig)
	}
}

func TestPermissionSyncingDisabled(t *testing.T) {
	authz.SetProviders(true, []authz.Provider{&mockProvider{}})
	cleanupLicense := licensing.MockCheckFeatureError("")

	t.Cleanup(func() {
		authz.SetProviders(true, nil)
		cleanupLicense()
	})

	t.Run("no authz providers", func(t *testing.T) {
		authz.SetProviders(true, nil)
		t.Cleanup(func() {
			authz.SetProviders(true, []authz.Provider{&mockProvider{}})
		})

		assert.True(t, PermissionSyncingDisabled())
	})

	t.Run("permissions user mapping enabled", func(t *testing.T) {
		cleanup := mockExplicitPermissions(true)
		t.Cleanup(func() {
			cleanup()
			conf.Mock(nil)
		})

		assert.False(t, PermissionSyncingDisabled())
	})

	t.Run("license does not have acls feature", func(t *testing.T) {
		licensing.MockCheckFeatureError("failed")
		t.Cleanup(func() {
			licensing.MockCheckFeatureError("")
		})
		assert.True(t, PermissionSyncingDisabled())
	})

	t.Run("Auto code host syncs disabled", func(t *testing.T) {
		conf.Mock(&conf.Unified{SiteConfiguration: schema.SiteConfiguration{DisableAutoCodeHostSyncs: true}})
		t.Cleanup(func() {
			conf.Mock(nil)
		})
		assert.True(t, PermissionSyncingDisabled())
	})

	t.Run("Auto code host syncs enabled", func(t *testing.T) {
		conf.Mock(&conf.Unified{SiteConfiguration: schema.SiteConfiguration{DisableAutoCodeHostSyncs: false}})
		t.Cleanup(func() {
			conf.Mock(nil)
		})
		assert.False(t, PermissionSyncingDisabled())
	})
}
