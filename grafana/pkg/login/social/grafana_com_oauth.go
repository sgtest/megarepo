package social

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"

	"golang.org/x/oauth2"

	"github.com/grafana/grafana/pkg/models/roletype"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/services/org"
	"github.com/grafana/grafana/pkg/setting"
	"github.com/grafana/grafana/pkg/util"
)

const grafanaComProviderName = "grafana_com"

type SocialGrafanaCom struct {
	*SocialBase
	url                  string
	allowedOrganizations []string
	skipOrgRoleSync      bool
}

type OrgRecord struct {
	Login string `json:"login"`
}

func NewGrafanaComProvider(settings map[string]any, cfg *setting.Cfg, features *featuremgmt.FeatureManager) (*SocialGrafanaCom, error) {
	info, err := createOAuthInfoFromKeyValues(settings)
	if err != nil {
		return nil, err
	}

	// Override necessary settings
	info.AuthUrl = cfg.GrafanaComURL + "/oauth2/authorize"
	info.TokenUrl = cfg.GrafanaComURL + "/api/oauth2/token"
	info.AuthStyle = "inheader"

	config := createOAuthConfig(info, cfg, grafanaComProviderName)
	provider := &SocialGrafanaCom{
		SocialBase:           newSocialBase(grafanaComProviderName, config, info, cfg.AutoAssignOrgRole, cfg.OAuthSkipOrgRoleUpdateSync, *features),
		url:                  cfg.GrafanaComURL,
		allowedOrganizations: util.SplitString(info.Extra["allowed_organizations"]),
		skipOrgRoleSync:      cfg.GrafanaComSkipOrgRoleSync,
		// FIXME: Move skipOrgRoleSync to OAuthInfo
		// skipOrgRoleSync: info.SkipOrgRoleSync
	}

	return provider, nil
}

func (s *SocialGrafanaCom) IsEmailAllowed(email string) bool {
	return true
}

func (s *SocialGrafanaCom) IsOrganizationMember(organizations []OrgRecord) bool {
	if len(s.allowedOrganizations) == 0 {
		return true
	}

	for _, allowedOrganization := range s.allowedOrganizations {
		for _, organization := range organizations {
			if organization.Login == allowedOrganization {
				return true
			}
		}
	}

	return false
}

// UserInfo is used for login credentials for the user
func (s *SocialGrafanaCom) UserInfo(ctx context.Context, client *http.Client, _ *oauth2.Token) (*BasicUserInfo, error) {
	var data struct {
		Id    int         `json:"id"`
		Name  string      `json:"name"`
		Login string      `json:"username"`
		Email string      `json:"email"`
		Role  string      `json:"role"`
		Orgs  []OrgRecord `json:"orgs"`
	}

	response, err := s.httpGet(ctx, client, s.url+"/api/oauth2/user")

	if err != nil {
		return nil, fmt.Errorf("Error getting user info: %s", err)
	}

	err = json.Unmarshal(response.Body, &data)
	if err != nil {
		return nil, fmt.Errorf("Error getting user info: %s", err)
	}

	// on login we do not want to display the role from the external provider
	var role roletype.RoleType
	if !s.skipOrgRoleSync {
		role = org.RoleType(data.Role)
	}
	userInfo := &BasicUserInfo{
		Id:    fmt.Sprintf("%d", data.Id),
		Name:  data.Name,
		Login: data.Login,
		Email: data.Email,
		Role:  role,
	}

	if !s.IsOrganizationMember(data.Orgs) {
		return nil, ErrMissingOrganizationMembership.Errorf(
			"User is not a member of any of the allowed organizations: %v. Returned Organizations: %v",
			s.allowedOrganizations, data.Orgs)
	}

	return userInfo, nil
}

func (s *SocialGrafanaCom) GetOAuthInfo() *OAuthInfo {
	return s.info
}
