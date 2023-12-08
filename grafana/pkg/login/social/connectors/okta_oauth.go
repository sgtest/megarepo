package connectors

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"

	"github.com/go-jose/go-jose/v3/jwt"
	"golang.org/x/oauth2"

	"github.com/grafana/grafana/pkg/login/social"
	"github.com/grafana/grafana/pkg/models/roletype"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/services/ssosettings"
	ssoModels "github.com/grafana/grafana/pkg/services/ssosettings/models"
	"github.com/grafana/grafana/pkg/setting"
)

var _ social.SocialConnector = (*SocialOkta)(nil)
var _ ssosettings.Reloadable = (*SocialOkta)(nil)

type SocialOkta struct {
	*SocialBase
	apiUrl          string
	allowedGroups   []string
	skipOrgRoleSync bool
}

type OktaUserInfoJson struct {
	Name        string              `json:"name"`
	DisplayName string              `json:"display_name"`
	Login       string              `json:"login"`
	Username    string              `json:"username"`
	Email       string              `json:"email"`
	Upn         string              `json:"upn"`
	Attributes  map[string][]string `json:"attributes"`
	Groups      []string            `json:"groups"`
	rawJSON     []byte
}

type OktaClaims struct {
	ID                string `json:"sub"`
	Email             string `json:"email"`
	PreferredUsername string `json:"preferred_username"`
	Name              string `json:"name"`
}

func NewOktaProvider(info *social.OAuthInfo, cfg *setting.Cfg, ssoSettings ssosettings.Service, features *featuremgmt.FeatureManager) *SocialOkta {
	config := createOAuthConfig(info, cfg, social.OktaProviderName)
	provider := &SocialOkta{
		SocialBase:    newSocialBase(social.OktaProviderName, config, info, cfg.AutoAssignOrgRole, cfg.OAuthSkipOrgRoleUpdateSync, *features),
		apiUrl:        info.ApiUrl,
		allowedGroups: info.AllowedGroups,
		// FIXME: Move skipOrgRoleSync to OAuthInfo
		// skipOrgRoleSync: info.SkipOrgRoleSync
		skipOrgRoleSync: cfg.OktaSkipOrgRoleSync,
	}

	if info.UseRefreshToken && features.IsEnabledGlobally(featuremgmt.FlagAccessTokenExpirationCheck) {
		appendUniqueScope(config, social.OfflineAccessScope)
	}

	if features.IsEnabledGlobally(featuremgmt.FlagSsoSettingsApi) {
		ssoSettings.RegisterReloadable(social.OktaProviderName, provider)
	}

	return provider
}

func (s *SocialOkta) Validate(ctx context.Context, settings ssoModels.SSOSettings) error {
	return nil
}

func (s *SocialOkta) Reload(ctx context.Context, settings ssoModels.SSOSettings) error {
	return nil
}

func (claims *OktaClaims) extractEmail() string {
	if claims.Email == "" && claims.PreferredUsername != "" {
		return claims.PreferredUsername
	}

	return claims.Email
}

func (s *SocialOkta) UserInfo(ctx context.Context, client *http.Client, token *oauth2.Token) (*social.BasicUserInfo, error) {
	idToken := token.Extra("id_token")
	if idToken == nil {
		return nil, fmt.Errorf("no id_token found")
	}

	parsedToken, err := jwt.ParseSigned(idToken.(string))
	if err != nil {
		return nil, fmt.Errorf("error parsing id token: %w", err)
	}

	var claims OktaClaims
	if err := parsedToken.UnsafeClaimsWithoutVerification(&claims); err != nil {
		return nil, fmt.Errorf("error getting claims from id token: %w", err)
	}

	email := claims.extractEmail()
	if email == "" {
		return nil, errors.New("error getting user info: no email found in access token")
	}

	var data OktaUserInfoJson
	err = s.extractAPI(ctx, &data, client)
	if err != nil {
		return nil, err
	}

	groups := s.GetGroups(&data)
	if !s.IsGroupMember(groups) {
		return nil, errMissingGroupMembership
	}

	var role roletype.RoleType
	var isGrafanaAdmin *bool
	if !s.skipOrgRoleSync {
		var grafanaAdmin bool
		role, grafanaAdmin, err = s.extractRoleAndAdmin(data.rawJSON, groups)
		if err != nil {
			return nil, err
		}

		if s.allowAssignGrafanaAdmin {
			isGrafanaAdmin = &grafanaAdmin
		}
	}
	if s.allowAssignGrafanaAdmin && s.skipOrgRoleSync {
		s.log.Debug("AllowAssignGrafanaAdmin and skipOrgRoleSync are both set, Grafana Admin role will not be synced, consider setting one or the other")
	}

	return &social.BasicUserInfo{
		Id:             claims.ID,
		Name:           claims.Name,
		Email:          email,
		Login:          email,
		Role:           role,
		IsGrafanaAdmin: isGrafanaAdmin,
		Groups:         groups,
	}, nil
}

func (s *SocialOkta) GetOAuthInfo() *social.OAuthInfo {
	return s.info
}

func (s *SocialOkta) extractAPI(ctx context.Context, data *OktaUserInfoJson, client *http.Client) error {
	rawUserInfoResponse, err := s.httpGet(ctx, client, s.apiUrl)
	if err != nil {
		s.log.Debug("Error getting user info response", "url", s.apiUrl, "error", err)
		return fmt.Errorf("error getting user info response: %w", err)
	}
	data.rawJSON = rawUserInfoResponse.Body

	err = json.Unmarshal(data.rawJSON, data)
	if err != nil {
		s.log.Debug("Error decoding user info response", "raw_json", data.rawJSON, "error", err)
		data.rawJSON = []byte{}
		return fmt.Errorf("error decoding user info response: %w", err)
	}

	s.log.Debug("Received user info response", "raw_json", string(data.rawJSON), "data", data)
	return nil
}

func (s *SocialOkta) GetGroups(data *OktaUserInfoJson) []string {
	groups := make([]string, 0)
	if len(data.Groups) > 0 {
		groups = data.Groups
	}
	return groups
}

// TODO: remove this in a separate PR and use the isGroupMember from the social.go
func (s *SocialOkta) IsGroupMember(groups []string) bool {
	if len(s.allowedGroups) == 0 {
		return true
	}

	for _, allowedGroup := range s.allowedGroups {
		for _, group := range groups {
			if group == allowedGroup {
				return true
			}
		}
	}

	return false
}
