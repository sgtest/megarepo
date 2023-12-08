package connectors

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"slices"
	"strings"

	"golang.org/x/oauth2"

	"github.com/grafana/grafana/pkg/login/social"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/services/ssosettings"
	ssoModels "github.com/grafana/grafana/pkg/services/ssosettings/models"
	"github.com/grafana/grafana/pkg/setting"
)

const (
	legacyAPIURL            = "https://www.googleapis.com/oauth2/v1/userinfo"
	googleIAMGroupsEndpoint = "https://content-cloudidentity.googleapis.com/v1/groups/-/memberships:searchDirectGroups"
	googleIAMScope          = "https://www.googleapis.com/auth/cloud-identity.groups.readonly"
)

var _ social.SocialConnector = (*SocialGoogle)(nil)
var _ ssosettings.Reloadable = (*SocialGoogle)(nil)

type SocialGoogle struct {
	*SocialBase
	hostedDomain    string
	apiUrl          string
	skipOrgRoleSync bool
}

type googleUserData struct {
	ID            string `json:"sub"`
	Email         string `json:"email"`
	Name          string `json:"name"`
	EmailVerified bool   `json:"email_verified"`
	rawJSON       []byte `json:"-"`
}

func NewGoogleProvider(info *social.OAuthInfo, cfg *setting.Cfg, ssoSettings ssosettings.Service, features *featuremgmt.FeatureManager) *SocialGoogle {
	config := createOAuthConfig(info, cfg, social.GoogleProviderName)
	provider := &SocialGoogle{
		SocialBase:      newSocialBase(social.GoogleProviderName, config, info, cfg.AutoAssignOrgRole, cfg.OAuthSkipOrgRoleUpdateSync, *features),
		hostedDomain:    info.HostedDomain,
		apiUrl:          info.ApiUrl,
		skipOrgRoleSync: cfg.GoogleSkipOrgRoleSync,
		// FIXME: Move skipOrgRoleSync to OAuthInfo
		// skipOrgRoleSync: info.SkipOrgRoleSync
	}

	if strings.HasPrefix(info.ApiUrl, legacyAPIURL) {
		provider.log.Warn("Using legacy Google API URL, please update your configuration")
	}

	if features.IsEnabledGlobally(featuremgmt.FlagSsoSettingsApi) {
		ssoSettings.RegisterReloadable(social.GoogleProviderName, provider)
	}

	return provider
}

func (s *SocialGoogle) Validate(ctx context.Context, settings ssoModels.SSOSettings) error {
	return nil
}

func (s *SocialGoogle) Reload(ctx context.Context, settings ssoModels.SSOSettings) error {
	return nil
}

func (s *SocialGoogle) UserInfo(ctx context.Context, client *http.Client, token *oauth2.Token) (*social.BasicUserInfo, error) {
	data, errToken := s.extractFromToken(ctx, client, token)
	if errToken != nil {
		return nil, errToken
	}

	if data == nil {
		var errAPI error
		data, errAPI = s.extractFromAPI(ctx, client)
		if errAPI != nil {
			return nil, errAPI
		}
	}

	if data.ID == "" {
		return nil, fmt.Errorf("error getting user info: id is empty")
	}

	if !data.EmailVerified {
		return nil, fmt.Errorf("user email is not verified")
	}

	groups, errPage := s.retrieveGroups(ctx, client, data)
	if errPage != nil {
		s.log.Warn("Error retrieving groups", "error", errPage)
	}

	if !s.isGroupMember(groups) {
		return nil, errMissingGroupMembership
	}

	userInfo := &social.BasicUserInfo{
		Id:             data.ID,
		Name:           data.Name,
		Email:          data.Email,
		Login:          data.Email,
		Role:           "",
		IsGrafanaAdmin: nil,
		Groups:         groups,
	}

	if !s.skipOrgRoleSync {
		role, grafanaAdmin, errRole := s.extractRoleAndAdmin(data.rawJSON, groups)
		if errRole != nil {
			return nil, errRole
		}

		if s.allowAssignGrafanaAdmin {
			userInfo.IsGrafanaAdmin = &grafanaAdmin
		}

		userInfo.Role = role
	}

	s.log.Debug("Resolved user info", "data", fmt.Sprintf("%+v", userInfo))

	return userInfo, nil
}

func (s *SocialGoogle) GetOAuthInfo() *social.OAuthInfo {
	return s.info
}

type googleAPIData struct {
	ID            string `json:"id"`
	Name          string `json:"name"`
	Email         string `json:"email"`
	EmailVerified bool   `json:"verified_email"`
}

func (s *SocialGoogle) extractFromAPI(ctx context.Context, client *http.Client) (*googleUserData, error) {
	if strings.HasPrefix(s.apiUrl, legacyAPIURL) {
		data := googleAPIData{}
		response, err := s.httpGet(ctx, client, s.apiUrl)
		if err != nil {
			return nil, fmt.Errorf("error retrieving legacy user info: %s", err)
		}

		if err := json.Unmarshal(response.Body, &data); err != nil {
			return nil, fmt.Errorf("error unmarshalling legacy user info: %s", err)
		}

		return &googleUserData{
			ID:            data.ID,
			Name:          data.Name,
			Email:         data.Email,
			EmailVerified: data.EmailVerified,
			rawJSON:       response.Body,
		}, nil
	}

	data := googleUserData{}
	response, err := s.httpGet(ctx, client, s.apiUrl)
	if err != nil {
		return nil, fmt.Errorf("error getting user info: %s", err)
	}

	if err := json.Unmarshal(response.Body, &data); err != nil {
		return nil, fmt.Errorf("error unmarshalling user info: %s", err)
	}

	return &data, nil
}

func (s *SocialGoogle) AuthCodeURL(state string, opts ...oauth2.AuthCodeOption) string {
	if s.features.IsEnabledGlobally(featuremgmt.FlagAccessTokenExpirationCheck) && s.useRefreshToken {
		opts = append(opts, oauth2.AccessTypeOffline, oauth2.ApprovalForce)
	}
	return s.SocialBase.AuthCodeURL(state, opts...)
}

func (s *SocialGoogle) extractFromToken(ctx context.Context, client *http.Client, token *oauth2.Token) (*googleUserData, error) {
	s.log.Debug("Extracting user info from OAuth token")

	idToken := token.Extra("id_token")
	if idToken == nil {
		s.log.Debug("No id_token found, defaulting to API access", "token", token)
		return nil, nil
	}

	rawJSON, err := s.retrieveRawIDToken(idToken)
	if err != nil {
		s.log.Warn("Error retrieving id_token", "error", err, "token", fmt.Sprintf("%+v", idToken))
		return nil, nil
	}

	if setting.Env == setting.Dev {
		s.log.Debug("Received id_token", "raw_json", string(rawJSON))
	}

	var data googleUserData
	if err := json.Unmarshal(rawJSON, &data); err != nil {
		return nil, fmt.Errorf("Error getting user info: %s", err)
	}

	data.rawJSON = rawJSON

	return &data, nil
}

type googleGroupResp struct {
	Memberships []struct {
		Group    string `json:"group"`
		GroupKey struct {
			ID string `json:"id"`
		} `json:"groupKey"`
		DisplayName string `json:"displayName"`
	} `json:"memberships"`
	NextPageToken string `json:"nextPageToken"`
}

func (s *SocialGoogle) retrieveGroups(ctx context.Context, client *http.Client, userData *googleUserData) ([]string, error) {
	s.log.Debug("Retrieving groups", "scopes", s.SocialBase.Config.Scopes)
	if !slices.Contains(s.Scopes, googleIAMScope) {
		return nil, nil
	}

	groups := []string{}

	url := fmt.Sprintf("%s?query=member_key_id=='%s'", googleIAMGroupsEndpoint, userData.Email)
	nextPageToken := ""
	for page, errPage := s.getGroupsPage(ctx, client, url, nextPageToken); ; page, errPage = s.getGroupsPage(ctx, client, url, nextPageToken) {
		if errPage != nil {
			return nil, errPage
		}

		for _, group := range page.Memberships {
			groups = append(groups, group.GroupKey.ID)
		}

		nextPageToken = page.NextPageToken
		if nextPageToken == "" {
			break
		}
	}

	return groups, nil
}

func (s *SocialGoogle) getGroupsPage(ctx context.Context, client *http.Client, url, nextPageToken string) (*googleGroupResp, error) {
	if nextPageToken != "" {
		url = fmt.Sprintf("%s&pageToken=%s", url, nextPageToken)
	}

	s.log.Debug("Retrieving groups", "url", url)
	resp, err := s.httpGet(ctx, client, url)
	if err != nil {
		return nil, fmt.Errorf("error getting groups: %s", err)
	}

	var data googleGroupResp
	if err := json.Unmarshal(resp.Body, &data); err != nil {
		return nil, fmt.Errorf("error unmarshalling groups: %s", err)
	}

	return &data, nil
}
