package clients

import (
	"context"
	"encoding/base64"
	"fmt"
	"net/http"
	"testing"

	"github.com/stretchr/testify/assert"

	"github.com/grafana/grafana/pkg/components/apikeygen"
	"github.com/grafana/grafana/pkg/components/satokengen"
	"github.com/grafana/grafana/pkg/services/apikey"
	"github.com/grafana/grafana/pkg/services/apikey/apikeytest"
	"github.com/grafana/grafana/pkg/services/authn"
	"github.com/grafana/grafana/pkg/services/login"
	"github.com/grafana/grafana/pkg/services/org"
	"github.com/grafana/grafana/pkg/services/user"
	"github.com/grafana/grafana/pkg/services/user/usertest"
)

var (
	revoked      = true
	secret, hash = genApiKey(false)
)

func TestAPIKey_Authenticate(t *testing.T) {
	type TestCase struct {
		desc             string
		req              *authn.Request
		expectedKey      *apikey.APIKey
		expectedUser     *user.SignedInUser
		expectedErr      error
		expectedIdentity *authn.Identity
	}

	tests := []TestCase{
		{
			desc: "should success for valid token that is not connected to a service account",
			req: &authn.Request{HTTPRequest: &http.Request{
				Header: map[string][]string{
					"Authorization": {"Bearer " + secret},
				},
			}},
			expectedKey: &apikey.APIKey{
				ID:    1,
				OrgID: 1,
				Key:   hash,
				Role:  org.RoleAdmin,
			},
			expectedIdentity: &authn.Identity{
				ID:       "api-key:1",
				OrgID:    1,
				OrgRoles: map[int64]org.RoleType{1: org.RoleAdmin},
				ClientParams: authn.ClientParams{
					SyncPermissions: true,
				},
				AuthenticatedBy: login.APIKeyAuthModule,
			},
		},
		{
			desc: "should success for valid token that is connected to service account",
			req: &authn.Request{HTTPRequest: &http.Request{
				Header: map[string][]string{
					"Authorization": {"Bearer " + secret},
				},
			}},
			expectedKey: &apikey.APIKey{
				ID:               1,
				OrgID:            1,
				Key:              hash,
				ServiceAccountId: intPtr(1),
			},
			expectedUser: &user.SignedInUser{
				UserID:           1,
				OrgID:            1,
				IsServiceAccount: true,
				OrgRole:          org.RoleViewer,
				Name:             "test",
			},
			expectedIdentity: &authn.Identity{
				ID:             "service-account:1",
				OrgID:          1,
				Name:           "test",
				OrgRoles:       map[int64]org.RoleType{1: org.RoleViewer},
				IsGrafanaAdmin: boolPtr(false),
				ClientParams: authn.ClientParams{
					SyncPermissions: true,
				},
				AuthenticatedBy: login.APIKeyAuthModule,
			},
		},
		{
			desc: "should fail for expired api key",
			req:  &authn.Request{HTTPRequest: &http.Request{Header: map[string][]string{"Authorization": {"Bearer " + secret}}}},
			expectedKey: &apikey.APIKey{
				Key:     hash,
				Expires: intPtr(0),
			},
			expectedErr: errAPIKeyExpired,
		},
		{
			desc: "should fail for revoked api key",
			req:  &authn.Request{HTTPRequest: &http.Request{Header: map[string][]string{"Authorization": {"Bearer " + secret}}}},
			expectedKey: &apikey.APIKey{
				Key:       hash,
				IsRevoked: &revoked,
			},
			expectedErr: errAPIKeyRevoked,
		},
		{
			desc: "should fail for api key in another organization",
			req:  &authn.Request{OrgID: 1, HTTPRequest: &http.Request{Header: map[string][]string{"Authorization": {"Bearer " + secret}}}},
			expectedKey: &apikey.APIKey{
				ID:               1,
				OrgID:            2,
				Key:              hash,
				ServiceAccountId: intPtr(1),
			},
			expectedErr: errAPIKeyOrgMismatch,
		},
	}

	for _, tt := range tests {
		t.Run(tt.desc, func(t *testing.T) {
			c := ProvideAPIKey(&apikeytest.Service{
				ExpectedAPIKey: tt.expectedKey,
			}, &usertest.FakeUserService{
				ExpectedSignedInUser: tt.expectedUser,
			})

			identity, err := c.Authenticate(context.Background(), tt.req)
			if tt.expectedErr != nil {
				assert.Nil(t, identity)
				assert.ErrorIs(t, err, tt.expectedErr)
				return
			}

			assert.NoError(t, err)
			assert.EqualValues(t, *tt.expectedIdentity, *identity)
			assert.Equal(t, tt.req.OrgID, tt.expectedIdentity.OrgID, "the request organization should match the identity's one")
		})
	}
}

func TestAPIKey_Test(t *testing.T) {
	type TestCase struct {
		desc     string
		req      *authn.Request
		expected bool
	}

	tests := []TestCase{
		{
			desc: "should succeed when api key is provided in Authorization header as bearer token",
			req: &authn.Request{HTTPRequest: &http.Request{
				Header: map[string][]string{
					"Authorization": {"Bearer 123123"},
				},
			}},
			expected: true,
		},
		{
			desc: "should succeed when api key is provided in Authorization header as basic auth and api_key as username",
			req: &authn.Request{HTTPRequest: &http.Request{
				Header: map[string][]string{
					"Authorization": {encodeBasicAuth("api_key", "test")},
				},
			}},
			expected: true,
		},
		{
			desc:     "should fail when no http request is passed",
			req:      &authn.Request{},
			expected: false,
		},
		{
			desc: "should fail when no there is no Authorization header",
			req: &authn.Request{HTTPRequest: &http.Request{
				Header: map[string][]string{},
			}},
			expected: false,
		},
		{
			desc: "should fail when Authorization header is not prefixed with Basic or Bearer",
			req: &authn.Request{HTTPRequest: &http.Request{
				Header: map[string][]string{
					"Authorization": {"test"},
				},
			}},
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.desc, func(t *testing.T) {
			c := ProvideAPIKey(&apikeytest.Service{}, usertest.NewUserServiceFake())
			assert.Equal(t, tt.expected, c.Test(context.Background(), tt.req))
		})
	}
}

func TestAPIKey_GetAPIKeyIDFromIdentity(t *testing.T) {
	type TestCase struct {
		desc             string
		expectedKey      *apikey.APIKey
		expectedIdentity *authn.Identity
		expectedError    error
		expectedKeyID    int64
		expectedExists   bool
	}

	tests := []TestCase{
		{
			desc: "should return API Key ID for valid token that is connected to service account",
			expectedKey: &apikey.APIKey{
				ID:               1,
				OrgID:            1,
				Key:              hash,
				ServiceAccountId: intPtr(1),
			},
			expectedIdentity: &authn.Identity{
				ID:              "service-account:1",
				OrgID:           1,
				Name:            "test",
				AuthenticatedBy: login.APIKeyAuthModule,
			},
			expectedKeyID:  1,
			expectedExists: true,
		},
		{
			desc: "should return API Key ID for valid token for API key",
			expectedKey: &apikey.APIKey{
				ID:    2,
				OrgID: 1,
				Key:   hash,
			},
			expectedIdentity: &authn.Identity{
				ID:              "api-key:2",
				OrgID:           1,
				Name:            "test",
				AuthenticatedBy: login.APIKeyAuthModule,
			},
			expectedKeyID:  2,
			expectedExists: true,
		},
		{
			desc: "should not return any ID when the request is not made by API key or service account",
			expectedKey: &apikey.APIKey{
				ID:    2,
				OrgID: 1,
				Key:   hash,
			},
			expectedIdentity: &authn.Identity{
				ID:              "user:2",
				OrgID:           1,
				Name:            "test",
				AuthenticatedBy: login.APIKeyAuthModule,
			},
			expectedKeyID:  -1,
			expectedExists: false,
		},
		{
			desc: "should not return any ID when the can't fetch API Key",
			expectedKey: &apikey.APIKey{
				ID:    1,
				OrgID: 1,
				Key:   hash,
			},
			expectedIdentity: &authn.Identity{
				ID:              "service-account:2",
				OrgID:           1,
				Name:            "test",
				AuthenticatedBy: login.APIKeyAuthModule,
			},
			expectedError:  fmt.Errorf("invalid token"),
			expectedKeyID:  -1,
			expectedExists: false,
		},
	}

	req := &authn.Request{HTTPRequest: &http.Request{
		Header: map[string][]string{
			"Authorization": {"Bearer " + secret},
		},
	}}

	signedInUser := &user.SignedInUser{
		UserID: 1,
		OrgID:  1,
		Name:   "test",
	}

	for _, tt := range tests {
		t.Run(tt.desc, func(t *testing.T) {
			c := ProvideAPIKey(&apikeytest.Service{
				ExpectedError:  tt.expectedError,
				ExpectedAPIKey: tt.expectedKey,
			}, &usertest.FakeUserService{
				ExpectedSignedInUser: signedInUser,
			})
			id, exists := c.getAPIKeyID(context.Background(), tt.expectedIdentity, req)
			assert.Equal(t, tt.expectedExists, exists)
			assert.Equal(t, tt.expectedKeyID, id)
		})
	}
}

func intPtr(n int64) *int64 {
	return &n
}

func boolPtr(b bool) *bool {
	return &b
}

func genApiKey(legacy bool) (string, string) {
	if legacy {
		res, _ := apikeygen.New(1, "test")
		return res.ClientSecret, res.HashedKey
	}
	res, _ := satokengen.New("test")
	return res.ClientSecret, res.HashedKey
}

func encodeBasicAuth(username, password string) string {
	return "Basic " + base64.StdEncoding.EncodeToString([]byte(fmt.Sprintf("%s:%s", username, password)))
}
