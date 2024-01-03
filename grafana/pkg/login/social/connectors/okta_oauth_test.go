package connectors

import (
	"context"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
	"golang.org/x/oauth2"

	"github.com/grafana/grafana/pkg/login/social"
	"github.com/grafana/grafana/pkg/models/roletype"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	ssoModels "github.com/grafana/grafana/pkg/services/ssosettings/models"
	"github.com/grafana/grafana/pkg/services/ssosettings/ssosettingstests"
	"github.com/grafana/grafana/pkg/setting"
)

func TestSocialOkta_UserInfo(t *testing.T) {
	var boolPointer *bool

	tests := []struct {
		name                    string
		userRawJSON             string
		OAuth2Extra             any
		autoAssignOrgRole       string
		settingSkipOrgRoleSync  bool
		allowAssignGrafanaAdmin bool
		RoleAttributePath       string
		ExpectedEmail           string
		ExpectedRole            roletype.RoleType
		ExpectedGrafanaAdmin    *bool
		ExpectedErr             error
		wantErr                 bool
	}{
		{
			name:              "Should give role from JSON and email from id token",
			userRawJSON:       `{ "email": "okta-octopus@grafana.com", "role": "Admin" }`,
			RoleAttributePath: "role",
			OAuth2Extra: map[string]any{
				// {
				// "email": "okto.octopus@test.com"
				// },
				"id_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJyb2xlIjoiQWRtaW4iLCJlbWFpbCI6Im9rdG8ub2N0b3B1c0B0ZXN0LmNvbSJ9.yhg0nvYCpMVCVrRvwtmHzhF0RJqid_YFbjJ_xuBCyHs",
			},
			ExpectedEmail:        "okto.octopus@test.com",
			ExpectedRole:         "Admin",
			ExpectedGrafanaAdmin: boolPointer,
			wantErr:              false,
		},
		{
			name:                   "Should give empty role and nil pointer for GrafanaAdmin when skip org role sync enable",
			userRawJSON:            `{ "email": "okta-octopus@grafana.com", "role": "Admin" }`,
			RoleAttributePath:      "role",
			settingSkipOrgRoleSync: true,
			OAuth2Extra: map[string]any{
				// {
				// "email": "okto.octopus@test.com"
				// },
				"id_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJyb2xlIjoiQWRtaW4iLCJlbWFpbCI6Im9rdG8ub2N0b3B1c0B0ZXN0LmNvbSJ9.yhg0nvYCpMVCVrRvwtmHzhF0RJqid_YFbjJ_xuBCyHs",
			},
			ExpectedEmail:        "okto.octopus@test.com",
			ExpectedRole:         "",
			ExpectedGrafanaAdmin: boolPointer,
			wantErr:              false,
		},
		{
			name:                    "Should give grafanaAdmin role for specific GrafanaAdmin in the role assignement",
			userRawJSON:             fmt.Sprintf(`{ "email": "okta-octopus@grafana.com", "role": "%s" }`, social.RoleGrafanaAdmin),
			RoleAttributePath:       "role",
			allowAssignGrafanaAdmin: true,
			OAuth2Extra: map[string]any{
				// {
				// "email": "okto.octopus@test.com"
				// },
				"id_token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJyb2xlIjoiQWRtaW4iLCJlbWFpbCI6Im9rdG8ub2N0b3B1c0B0ZXN0LmNvbSJ9.yhg0nvYCpMVCVrRvwtmHzhF0RJqid_YFbjJ_xuBCyHs",
			},
			ExpectedEmail:        "okto.octopus@test.com",
			ExpectedRole:         "Admin",
			ExpectedGrafanaAdmin: trueBoolPtr(),
			wantErr:              false,
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
				writer.WriteHeader(http.StatusOK)
				// return JSON if matches user endpoint
				if strings.HasSuffix(request.URL.String(), "/user") {
					writer.Header().Set("Content-Type", "application/json")
					_, err := writer.Write([]byte(tt.userRawJSON))
					require.NoError(t, err)
				} else {
					writer.WriteHeader(http.StatusNotFound)
				}
			}))
			defer server.Close()

			provider := NewOktaProvider(
				&social.OAuthInfo{
					ApiUrl:                  server.URL + "/user",
					RoleAttributePath:       tt.RoleAttributePath,
					AllowAssignGrafanaAdmin: tt.allowAssignGrafanaAdmin,
					SkipOrgRoleSync:         tt.settingSkipOrgRoleSync,
				},
				&setting.Cfg{
					AutoAssignOrgRole:          tt.autoAssignOrgRole,
					OAuthSkipOrgRoleUpdateSync: false,
				},
				&ssosettingstests.MockService{},
				featuremgmt.WithFeatures())

			// create a oauth2 token with a id_token
			staticToken := oauth2.Token{
				AccessToken:  "",
				TokenType:    "",
				RefreshToken: "",
				Expiry:       time.Now(),
			}
			token := staticToken.WithExtra(tt.OAuth2Extra)
			got, err := provider.UserInfo(context.Background(), server.Client(), token)
			if (err != nil) != tt.wantErr {
				t.Errorf("UserInfo() error = %v, wantErr %v", err, tt.wantErr)
				return
			}
			require.Equal(t, tt.ExpectedEmail, got.Email)
			require.Equal(t, tt.ExpectedRole, got.Role)
			require.Equal(t, tt.ExpectedGrafanaAdmin, got.IsGrafanaAdmin)
		})
	}
}

func TestSocialOkta_Validate(t *testing.T) {
	testCases := []struct {
		name        string
		settings    ssoModels.SSOSettings
		expectError bool
	}{
		{
			name: "SSOSettings is valid",
			settings: ssoModels.SSOSettings{
				Settings: map[string]any{
					"client_id": "client-id",
				},
			},
			expectError: false,
		},
		{
			name: "fails if settings map contains an invalid field",
			settings: ssoModels.SSOSettings{
				Settings: map[string]any{
					"client_id":     "client-id",
					"invalid_field": []int{1, 2, 3},
				},
			},
			expectError: true,
		},
		{
			name: "fails if client id is empty",
			settings: ssoModels.SSOSettings{
				Settings: map[string]any{
					"client_id": "",
				},
			},
			expectError: true,
		},
		{
			name: "fails if client id does not exist",
			settings: ssoModels.SSOSettings{
				Settings: map[string]any{},
			},
			expectError: true,
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			s := NewOktaProvider(&social.OAuthInfo{}, &setting.Cfg{}, &ssosettingstests.MockService{}, featuremgmt.WithFeatures())

			err := s.Validate(context.Background(), tc.settings)
			if tc.expectError {
				require.Error(t, err)
			} else {
				require.NoError(t, err)
			}
		})
	}
}
