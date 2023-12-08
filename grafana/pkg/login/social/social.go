package social

import (
	"bytes"
	"context"
	"fmt"
	"net/http"

	"github.com/grafana/grafana/pkg/services/org"
	"golang.org/x/oauth2"
)

const (
	OfflineAccessScope = "offline_access"
	RoleGrafanaAdmin   = "GrafanaAdmin" // For AzureAD for example this value cannot contain spaces

	AzureADProviderName      = "azuread"
	GenericOAuthProviderName = "generic_oauth"
	GitHubProviderName       = "github"
	GitlabProviderName       = "gitlab"
	GoogleProviderName       = "google"
	GrafanaComProviderName   = "grafana_com"
	// legacy/old settings for the provider
	GrafanaNetProviderName = "grafananet"
	OktaProviderName       = "okta"
)

var (
	SocialBaseUrl = "/login/"
)

type Service interface {
	GetOAuthProviders() map[string]bool
	GetOAuthHttpClient(string) (*http.Client, error)
	GetConnector(string) (SocialConnector, error)
	GetOAuthInfoProvider(string) *OAuthInfo
	GetOAuthInfoProviders() map[string]*OAuthInfo
}

//go:generate mockery --name SocialConnector --structname MockSocialConnector --outpkg socialtest --filename social_connector_mock.go --output ./socialtest/
type SocialConnector interface {
	UserInfo(ctx context.Context, client *http.Client, token *oauth2.Token) (*BasicUserInfo, error)
	IsEmailAllowed(email string) bool
	IsSignupAllowed() bool

	GetOAuthInfo() *OAuthInfo

	AuthCodeURL(state string, opts ...oauth2.AuthCodeOption) string
	Exchange(ctx context.Context, code string, authOptions ...oauth2.AuthCodeOption) (*oauth2.Token, error)
	Client(ctx context.Context, t *oauth2.Token) *http.Client
	TokenSource(ctx context.Context, t *oauth2.Token) oauth2.TokenSource
	SupportBundleContent(*bytes.Buffer) error
}

type OAuthInfo struct {
	AllowAssignGrafanaAdmin bool              `mapstructure:"allow_assign_grafana_admin" toml:"allow_assign_grafana_admin" json:"allowAssignGrafanaAdmin"`
	AllowSignup             bool              `mapstructure:"allow_sign_up" toml:"allow_sign_up" json:"allowSignup"`
	AllowedDomains          []string          `mapstructure:"allowed_domains" toml:"allowed_domains" json:"allowedDomains"`
	AllowedGroups           []string          `mapstructure:"allowed_groups" toml:"allowed_groups" json:"allowedGroups"`
	ApiUrl                  string            `mapstructure:"api_url" toml:"api_url" json:"apiUrl"`
	AuthStyle               string            `mapstructure:"auth_style" toml:"auth_style" json:"authStyle"`
	AuthUrl                 string            `mapstructure:"auth_url" toml:"auth_url" json:"authUrl"`
	AutoLogin               bool              `mapstructure:"auto_login" toml:"auto_login" json:"autoLogin"`
	ClientId                string            `mapstructure:"client_id" toml:"client_id" json:"clientId"`
	ClientSecret            string            `mapstructure:"client_secret" toml:"-" json:"clientSecret"`
	EmailAttributeName      string            `mapstructure:"email_attribute_name" toml:"email_attribute_name" json:"emailAttributeName"`
	EmailAttributePath      string            `mapstructure:"email_attribute_path" toml:"email_attribute_path" json:"emailAttributePath"`
	EmptyScopes             bool              `mapstructure:"empty_scopes" toml:"empty_scopes" json:"emptyScopes"`
	Enabled                 bool              `mapstructure:"enabled" toml:"enabled" json:"enabled"`
	GroupsAttributePath     string            `mapstructure:"groups_attribute_path" toml:"groups_attribute_path" json:"groupsAttributePath"`
	HostedDomain            string            `mapstructure:"hosted_domain" toml:"hosted_domain" json:"hostedDomain"`
	Icon                    string            `mapstructure:"icon" toml:"icon" json:"icon"`
	Name                    string            `mapstructure:"name" toml:"name" json:"name"`
	RoleAttributePath       string            `mapstructure:"role_attribute_path" toml:"role_attribute_path" json:"roleAttributePath"`
	RoleAttributeStrict     bool              `mapstructure:"role_attribute_strict" toml:"role_attribute_strict" json:"roleAttributeStrict"`
	Scopes                  []string          `mapstructure:"scopes" toml:"scopes" json:"scopes"`
	SignoutRedirectUrl      string            `mapstructure:"signout_redirect_url" toml:"signout_redirect_url" json:"signoutRedirectUrl"`
	SkipOrgRoleSync         bool              `mapstructure:"skip_org_role_sync" toml:"skip_org_role_sync" json:"skipOrgRoleSync"`
	TeamIdsAttributePath    string            `mapstructure:"team_ids_attribute_path" toml:"team_ids_attribute_path" json:"teamIdsAttributePath"`
	TeamsUrl                string            `mapstructure:"teams_url" toml:"teams_url" json:"teamsUrl"`
	TlsClientCa             string            `mapstructure:"tls_client_ca" toml:"tls_client_ca" json:"tlsClientCa"`
	TlsClientCert           string            `mapstructure:"tls_client_cert" toml:"tls_client_cert" json:"tlsClientCert"`
	TlsClientKey            string            `mapstructure:"tls_client_key" toml:"tls_client_key" json:"tlsClientKey"`
	TlsSkipVerify           bool              `mapstructure:"tls_skip_verify_insecure" toml:"tls_skip_verify_insecure" json:"tlsSkipVerify"`
	TokenUrl                string            `mapstructure:"token_url" toml:"token_url" json:"tokenUrl"`
	UsePKCE                 bool              `mapstructure:"use_pkce" toml:"use_pkce" json:"usePKCE"`
	UseRefreshToken         bool              `mapstructure:"use_refresh_token" toml:"use_refresh_token" json:"useRefreshToken"`
	Extra                   map[string]string `mapstructure:",remain" toml:"extra,omitempty" json:"extra"`
}

func NewOAuthInfo() *OAuthInfo {
	return &OAuthInfo{
		Scopes:         []string{},
		AllowedDomains: []string{},
		AllowedGroups:  []string{},
		Extra:          map[string]string{},
	}
}

type BasicUserInfo struct {
	Id             string
	Name           string
	Email          string
	Login          string
	Role           org.RoleType
	IsGrafanaAdmin *bool // nil will avoid overriding user's set server admin setting
	Groups         []string
}

func (b *BasicUserInfo) String() string {
	return fmt.Sprintf("Id: %s, Name: %s, Email: %s, Login: %s, Role: %s, Groups: %v",
		b.Id, b.Name, b.Email, b.Login, b.Role, b.Groups)
}
