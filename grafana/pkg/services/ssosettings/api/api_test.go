package api

import (
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"testing"

	"github.com/stretchr/testify/mock"
	"github.com/stretchr/testify/require"

	"github.com/grafana/grafana/pkg/api/routing"
	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/login/social"
	"github.com/grafana/grafana/pkg/services/accesscontrol"
	"github.com/grafana/grafana/pkg/services/accesscontrol/acimpl"
	"github.com/grafana/grafana/pkg/services/org"
	"github.com/grafana/grafana/pkg/services/ssosettings"
	"github.com/grafana/grafana/pkg/services/ssosettings/models"
	"github.com/grafana/grafana/pkg/services/ssosettings/ssosettingstests"
	"github.com/grafana/grafana/pkg/services/user"
	"github.com/grafana/grafana/pkg/setting"
	"github.com/grafana/grafana/pkg/web/webtest"
)

func TestSSOSettingsAPI_Update(t *testing.T) {
	type TestCase struct {
		desc                string
		key                 string
		body                string
		action              string
		scope               string
		expectedError       error
		expectedServiceCall bool
		expectedStatusCode  int
	}

	tests := []TestCase{
		{
			desc:                "successfully updates SSO settings",
			key:                 social.GitHubProviderName,
			body:                `{"settings": {"enabled": true}}`,
			action:              "settings:write",
			scope:               "settings:auth.github:*",
			expectedError:       nil,
			expectedServiceCall: true,
			expectedStatusCode:  http.StatusNoContent,
		},
		{
			desc:                "fails when action doesn't match",
			key:                 social.GitHubProviderName,
			body:                `{"settings": {"enabled": true}}`,
			action:              "settings:read",
			scope:               "settings:auth.github:*",
			expectedError:       nil,
			expectedServiceCall: false,
			expectedStatusCode:  http.StatusForbidden,
		},
		{
			desc:                "fails when scope doesn't match",
			key:                 social.GitHubProviderName,
			body:                `{"settings": {"enabled": true}}`,
			action:              "settings:write",
			scope:               "settings:auth.github:read",
			expectedError:       nil,
			expectedServiceCall: false,
			expectedStatusCode:  http.StatusForbidden,
		},
		{
			desc:                "fails when scope contains another provider",
			key:                 social.GitHubProviderName,
			body:                `{"settings": {"enabled": true}}`,
			action:              "settings:write",
			scope:               "settings:auth.okta:*",
			expectedError:       nil,
			expectedServiceCall: false,
			expectedStatusCode:  http.StatusForbidden,
		},
		{
			desc:                "fails with not found when key is empty",
			key:                 "",
			body:                `{"settings": {"enabled": true}}`,
			action:              "settings:write",
			scope:               "settings:auth.github:*",
			expectedError:       nil,
			expectedServiceCall: false,
			expectedStatusCode:  http.StatusNotFound,
		},
		{
			desc:                "fails with bad request when body contains invalid json",
			key:                 social.GitHubProviderName,
			body:                `{ invalid json }`,
			action:              "settings:write",
			scope:               "settings:auth.github:*",
			expectedError:       nil,
			expectedServiceCall: false,
			expectedStatusCode:  http.StatusBadRequest,
		},
		{
			desc:                "fails with bad request when key was not found",
			key:                 social.GitHubProviderName,
			body:                `{"settings": {"enabled": true}}`,
			action:              "settings:write",
			scope:               "settings:auth.github:*",
			expectedError:       ssosettings.ErrInvalidProvider.Errorf("invalid provider"),
			expectedServiceCall: true,
			expectedStatusCode:  http.StatusBadRequest,
		},
		{
			desc:                "fails with internal server error when service returns an error",
			key:                 social.GitHubProviderName,
			body:                `{"settings": {"enabled": true}}`,
			action:              "settings:write",
			scope:               "settings:auth.github:*",
			expectedError:       errors.New("something went wrong"),
			expectedServiceCall: true,
			expectedStatusCode:  http.StatusInternalServerError,
		},
	}

	for _, tt := range tests {
		t.Run(tt.desc, func(t *testing.T) {
			var input models.SSOSettings
			_ = json.Unmarshal([]byte(tt.body), &input)

			settings := models.SSOSettings{
				Provider: tt.key,
				Settings: input.Settings,
			}

			service := ssosettingstests.NewMockService(t)
			if tt.expectedServiceCall {
				service.On("Upsert", mock.Anything, settings).Return(tt.expectedError).Once()
			}
			server := setupTests(t, service)

			path := fmt.Sprintf("/api/v1/sso-settings/%s", tt.key)
			req := server.NewRequest(http.MethodPut, path, bytes.NewBufferString(tt.body))
			webtest.RequestWithSignedInUser(req, &user.SignedInUser{
				OrgRole:     org.RoleEditor,
				OrgID:       1,
				Permissions: getPermissionsForActionAndScope(tt.action, tt.scope),
			})
			res, err := server.SendJSON(req)
			require.NoError(t, err)

			require.Equal(t, tt.expectedStatusCode, res.StatusCode)
			require.NoError(t, res.Body.Close())
		})
	}
}

func TestSSOSettingsAPI_Delete(t *testing.T) {
	type TestCase struct {
		desc                string
		key                 string
		action              string
		scope               string
		expectedError       error
		expectedServiceCall bool
		expectedStatusCode  int
	}

	tests := []TestCase{
		{
			desc:                "successfully deletes SSO settings",
			key:                 social.AzureADProviderName,
			action:              "settings:write",
			scope:               "settings:auth.azuread:*",
			expectedError:       nil,
			expectedServiceCall: true,
			expectedStatusCode:  http.StatusNoContent,
		},
		{
			desc:                "fails when action doesn't match",
			key:                 social.AzureADProviderName,
			action:              "settings:read",
			scope:               "settings:auth.azuread:*",
			expectedError:       nil,
			expectedServiceCall: false,
			expectedStatusCode:  http.StatusForbidden,
		},
		{
			desc:                "fails when scope doesn't match",
			key:                 social.AzureADProviderName,
			action:              "settings:write",
			scope:               "settings:auth.azuread:read",
			expectedError:       nil,
			expectedServiceCall: false,
			expectedStatusCode:  http.StatusForbidden,
		},
		{
			desc:                "fails when scope contains another provider",
			key:                 social.AzureADProviderName,
			action:              "settings:write",
			scope:               "settings:auth.github:*",
			expectedError:       nil,
			expectedServiceCall: false,
			expectedStatusCode:  http.StatusForbidden,
		},
		{
			desc:                "fails with not found when key is empty",
			key:                 "",
			action:              "settings:write",
			scope:               "settings:auth.azuread:*",
			expectedError:       nil,
			expectedServiceCall: false,
			expectedStatusCode:  http.StatusNotFound,
		},
		{
			desc:                "fails with not found when key was not found",
			key:                 social.AzureADProviderName,
			action:              "settings:write",
			scope:               "settings:auth.azuread:*",
			expectedError:       ssosettings.ErrNotFound,
			expectedServiceCall: true,
			expectedStatusCode:  http.StatusNotFound,
		},
		{
			desc:                "fails with internal server error when service returns an error",
			key:                 social.AzureADProviderName,
			action:              "settings:write",
			scope:               "settings:auth.azuread:*",
			expectedError:       errors.New("something went wrong"),
			expectedServiceCall: true,
			expectedStatusCode:  http.StatusInternalServerError,
		},
	}

	for _, tt := range tests {
		t.Run(tt.desc, func(t *testing.T) {
			service := ssosettingstests.NewMockService(t)
			if tt.expectedServiceCall {
				service.On("Delete", mock.Anything, tt.key).Return(tt.expectedError).Once()
			}
			server := setupTests(t, service)

			path := fmt.Sprintf("/api/v1/sso-settings/%s", tt.key)
			req := server.NewRequest(http.MethodDelete, path, nil)
			webtest.RequestWithSignedInUser(req, &user.SignedInUser{
				OrgRole:     org.RoleEditor,
				OrgID:       1,
				Permissions: getPermissionsForActionAndScope(tt.action, tt.scope),
			})
			res, err := server.SendJSON(req)
			require.NoError(t, err)

			require.Equal(t, tt.expectedStatusCode, res.StatusCode)
			require.NoError(t, res.Body.Close())
		})
	}
}

func getPermissionsForActionAndScope(action, scope string) map[int64]map[string][]string {
	return map[int64]map[string][]string{
		1: accesscontrol.GroupScopesByAction([]accesscontrol.Permission{{
			Action: action, Scope: scope,
		}}),
	}
}

func setupTests(t *testing.T, service ssosettings.Service) *webtest.Server {
	t.Helper()

	cfg := setting.NewCfg()
	logger := log.NewNopLogger()

	api := &Api{
		Log:                logger,
		RouteRegister:      routing.NewRouteRegister(),
		AccessControl:      acimpl.ProvideAccessControl(cfg),
		SSOSettingsService: service,
	}

	api.RegisterAPIEndpoints()

	return webtest.NewServer(t, api.RouteRegister)
}
