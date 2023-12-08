package connectors

import (
	"context"
	"errors"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"

	"github.com/go-jose/go-jose/v3"
	"github.com/go-jose/go-jose/v3/jwt"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"golang.org/x/oauth2"

	"github.com/grafana/grafana/pkg/login/social"
	"github.com/grafana/grafana/pkg/models/roletype"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/services/ssosettings/ssosettingstests"
	"github.com/grafana/grafana/pkg/setting"
)

func TestSocialGoogle_retrieveGroups(t *testing.T) {
	type fields struct {
		Scopes []string
	}
	type args struct {
		client   *http.Client
		userData *googleUserData
	}
	tests := []struct {
		name    string
		fields  fields
		args    args
		want    []string
		wantErr bool
	}{
		{
			name: "No scope",
			fields: fields{
				Scopes: []string{},
			},
			args: args{
				client: &http.Client{},
				userData: &googleUserData{
					Email: "test@example.com",
				},
			},
			want:    nil,
			wantErr: false,
		},
		{
			name: "No groups",
			fields: fields{
				Scopes: []string{"https://www.googleapis.com/auth/cloud-identity.groups.readonly"},
			},
			args: args{
				client: &http.Client{
					Transport: &roundTripperFunc{
						fn: func(req *http.Request) (*http.Response, error) {
							resp := httptest.NewRecorder()
							_, _ = resp.WriteString(`{
                                "memberships": [
                                ],
                                "nextPageToken": ""
                            }`)
							return resp.Result(), nil
						},
					},
				},
				userData: &googleUserData{
					Email: "test@example.com",
				},
			},
			want:    []string{},
			wantErr: false,
		},
		{
			name: "error retrieving groups",
			fields: fields{
				Scopes: []string{"https://www.googleapis.com/auth/cloud-identity.groups.readonly"},
			},
			args: args{
				client: &http.Client{
					Transport: &roundTripperFunc{
						fn: func(req *http.Request) (*http.Response, error) {
							return nil, errors.New("error retrieving groups")
						},
					},
				},
				userData: &googleUserData{
					Email: "test@example.com",
				},
			},
			want:    nil,
			wantErr: true,
		},

		{
			name: "Has 2 pages to fetch",
			fields: fields{
				Scopes: []string{"https://www.googleapis.com/auth/cloud-identity.groups.readonly"},
			},
			args: args{
				client: &http.Client{
					Transport: &roundTripperFunc{
						fn: func(req *http.Request) (*http.Response, error) {
							resp := httptest.NewRecorder()
							// First page
							if req.URL.Query().Get("pageToken") == "" {
								_, _ = resp.WriteString(`{
                        "memberships": [
                            {
                                "group": "test-group",
                                "groupKey": {
                                    "id": "test-group@google.com"
                                },
                                "displayName": "Test Group"
                            }
                        ],
                        "nextPageToken": "page-2"
                    }`)
							} else {
								// Second page
								_, _ = resp.WriteString(`{
                        "memberships": [
                            {
                                "group": "test-group-2",
                                "groupKey": {
                                    "id": "test-group-2@google.com"
                                },
                                "displayName": "Test Group 2"
                            }
                        ],
                        "nextPageToken": ""
                    }`)
							}
							return resp.Result(), nil
						},
					},
				},
				userData: &googleUserData{
					Email: "test@example.com",
				},
			},
			want:    []string{"test-group@google.com", "test-group-2@google.com"},
			wantErr: false,
		},
		{
			name: "Has one page to fetch",
			fields: fields{
				Scopes: []string{"https://www.googleapis.com/auth/cloud-identity.groups.readonly"},
			},
			args: args{
				client: &http.Client{
					Transport: &roundTripperFunc{
						fn: func(req *http.Request) (*http.Response, error) {
							resp := httptest.NewRecorder()
							_, _ = resp.WriteString(`{
                                "memberships": [
                                    {
                                        "group": "test-group",
                                        "groupKey": {
                                            "id": "test-group@google.com"
                                        },
                                        "displayName": "Test Group"
                                    }
                                ],
                                "nextPageToken": ""
                            }`)
							return resp.Result(), nil
						},
					},
				},
				userData: &googleUserData{
					Email: "test@example.com",
				},
			},
			want:    []string{"test-group@google.com"},
			wantErr: false,
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s := NewGoogleProvider(
				&social.OAuthInfo{
					ApiUrl:                  "",
					Scopes:                  tt.fields.Scopes,
					HostedDomain:            "",
					AllowedDomains:          []string{},
					AllowSignup:             false,
					RoleAttributePath:       "",
					RoleAttributeStrict:     false,
					AllowAssignGrafanaAdmin: false,
				},
				&setting.Cfg{
					AutoAssignOrgRole:     "",
					GoogleSkipOrgRoleSync: false,
				},
				&ssosettingstests.MockService{},
				featuremgmt.WithFeatures())

			got, err := s.retrieveGroups(context.Background(), tt.args.client, tt.args.userData)
			if (err != nil) != tt.wantErr {
				t.Errorf("SocialGoogle.retrieveGroups() error = %v, wantErr %v", err, tt.wantErr)
				return
			}
			require.Equal(t, tt.want, got)
		})
	}
}

type roundTripperFunc struct {
	fn func(req *http.Request) (*http.Response, error)
}

func (f *roundTripperFunc) RoundTrip(req *http.Request) (*http.Response, error) {
	return f.fn(req)
}

func TestSocialGoogle_UserInfo(t *testing.T) {
	cl := jwt.Claims{
		Subject:   "88888888888888",
		Issuer:    "issuer",
		NotBefore: jwt.NewNumericDate(time.Date(2016, 1, 1, 0, 0, 0, 0, time.UTC)),
		Audience:  jwt.Audience{"823123"},
	}

	sig, err := jose.NewSigner(jose.SigningKey{Algorithm: jose.HS256, Key: []byte("secret")}, (&jose.SignerOptions{}).WithType("JWT"))
	require.NoError(t, err)
	idMap := map[string]any{
		"email":          "test@example.com",
		"name":           "Test User",
		"hd":             "example.com",
		"email_verified": true,
	}

	raw, err := jwt.Signed(sig).Claims(cl).Claims(idMap).CompactSerialize()
	require.NoError(t, err)

	tokenWithID := (&oauth2.Token{
		AccessToken: "fake_token",
	}).WithExtra(map[string]any{"id_token": raw})

	tokenWithoutID := &oauth2.Token{}

	type fields struct {
		Scopes                  []string
		apiURL                  string
		allowedGroups           []string
		roleAttributePath       string
		roleAttributeStrict     bool
		allowAssignGrafanaAdmin bool
		skipOrgRoleSync         bool
	}
	type args struct {
		client *http.Client
		token  *oauth2.Token
	}
	tests := []struct {
		name       string
		fields     fields
		args       args
		wantData   *social.BasicUserInfo
		wantErr    bool
		wantErrMsg string
	}{
		{
			name: "Success id_token",
			fields: fields{
				Scopes:          []string{},
				skipOrgRoleSync: true,
			},
			args: args{
				token: tokenWithID,
			},
			wantData: &social.BasicUserInfo{
				Id:    "88888888888888",
				Login: "test@example.com",
				Email: "test@example.com",
				Name:  "Test User",
			},
			wantErr: false,
		},
		{
			name: "Success id_token - groups requested",
			fields: fields{
				Scopes:          []string{"https://www.googleapis.com/auth/cloud-identity.groups.readonly"},
				skipOrgRoleSync: true,
			},
			args: args{
				token: tokenWithID,
				client: &http.Client{
					Transport: &roundTripperFunc{
						fn: func(req *http.Request) (*http.Response, error) {
							resp := httptest.NewRecorder()
							_, _ = resp.WriteString(`{
                                "memberships": [
                                    {
                                        "group": "test-group",
                                        "groupKey": {
                                            "id": "test-group@google.com"
                                        },
                                        "displayName": "Test Group"
                                    }
                                ],
                                "nextPageToken": ""
                            }`)
							return resp.Result(), nil
						},
					},
				},
			},
			wantData: &social.BasicUserInfo{
				Id:     "88888888888888",
				Login:  "test@example.com",
				Email:  "test@example.com",
				Name:   "Test User",
				Groups: []string{"test-group@google.com"},
			},
			wantErr: false,
		},
		{
			name: "Legacy API URL",
			fields: fields{
				apiURL:          legacyAPIURL,
				skipOrgRoleSync: true,
			},
			args: args{
				token: tokenWithoutID,
				client: &http.Client{
					Transport: &roundTripperFunc{
						fn: func(req *http.Request) (*http.Response, error) {
							resp := httptest.NewRecorder()
							_, _ = resp.WriteString(`{
                                "id": "99999999999999",
                                "name": "Test User",
                                "email": "test@example.com",
                                "verified_email": true
                            }`)
							return resp.Result(), nil
						},
					},
				},
			},
			wantData: &social.BasicUserInfo{
				Id:    "99999999999999",
				Login: "test@example.com",
				Email: "test@example.com",
				Name:  "Test User",
			},
			wantErr: false,
		},
		{
			name: "Legacy API URL - no id provided",
			fields: fields{
				apiURL:          legacyAPIURL,
				skipOrgRoleSync: true,
			},
			args: args{
				token: tokenWithoutID,
				client: &http.Client{
					Transport: &roundTripperFunc{
						fn: func(req *http.Request) (*http.Response, error) {
							resp := httptest.NewRecorder()
							_, _ = resp.WriteString(`{
                                "name": "Test User",
                                "email": "test@example.com",
                                "verified_email": true
                            }`)
							return resp.Result(), nil
						},
					},
				},
			},
			wantData:   nil,
			wantErr:    true,
			wantErrMsg: "error getting user info: id is empty",
		},
		{
			name: "Error unmarshalling legacy user info",
			fields: fields{
				apiURL: legacyAPIURL,
			},
			args: args{
				token: tokenWithoutID,
				client: &http.Client{
					Transport: &roundTripperFunc{
						fn: func(req *http.Request) (*http.Response, error) {
							resp := httptest.NewRecorder()
							_, _ = resp.WriteString(`invalid json`)
							return resp.Result(), nil
						},
					},
				},
			},
			wantData:   nil,
			wantErr:    true,
			wantErrMsg: "error unmarshalling legacy user info",
		},
		{
			name: "Error getting user info",
			fields: fields{
				apiURL: "https://openidconnect.googleapis.com/v1/userinfo",
			},
			args: args{
				token: tokenWithoutID,
				client: &http.Client{
					Transport: &roundTripperFunc{
						fn: func(req *http.Request) (*http.Response, error) {
							return nil, errors.New("error getting user info")
						},
					},
				},
			},
			wantData:   nil,
			wantErr:    true,
			wantErrMsg: "error getting user info",
		},
		{
			name: "Error unmarshalling user info",
			fields: fields{
				apiURL: "https://openidconnect.googleapis.com/v1/userinfo",
			},
			args: args{
				token: tokenWithoutID,
				client: &http.Client{
					Transport: &roundTripperFunc{
						fn: func(req *http.Request) (*http.Response, error) {
							resp := httptest.NewRecorder()
							_, _ = resp.WriteString(`invalid json`)
							return resp.Result(), nil
						},
					},
				},
			},
			wantData:   nil,
			wantErr:    true,
			wantErrMsg: "error unmarshalling user info",
		},
		{
			name: "Success",
			fields: fields{
				apiURL:          "https://openidconnect.googleapis.com/v1/userinfo",
				skipOrgRoleSync: true,
			},
			args: args{
				token: tokenWithoutID,
				client: &http.Client{
					Transport: &roundTripperFunc{
						fn: func(req *http.Request) (*http.Response, error) {
							resp := httptest.NewRecorder()
							_, _ = resp.WriteString(`{
                                "sub": "92222222222222222",
                                "name": "Test User",
                                "email": "test@example.com",
                                "email_verified": true
                            }`)
							return resp.Result(), nil
						},
					},
				},
			},
			wantData: &social.BasicUserInfo{
				Id:    "92222222222222222",
				Name:  "Test User",
				Email: "test@example.com",
				Login: "test@example.com",
			},
			wantErr: false,
		}, {
			name: "Unverified Email userinfo",
			fields: fields{
				apiURL: "https://openidconnect.googleapis.com/v1/userinfo",
			},
			args: args{
				token: tokenWithoutID,
				client: &http.Client{
					Transport: &roundTripperFunc{
						fn: func(req *http.Request) (*http.Response, error) {
							resp := httptest.NewRecorder()
							_, _ = resp.WriteString(`{
                                "sub": "92222222222222222",
                                "name": "Test User",
                                "email": "test@example.com",
                                "email_verified": false
                            }`)
							return resp.Result(), nil
						},
					},
				},
			},
			wantData:   nil,
			wantErr:    true,
			wantErrMsg: "email is not verified",
		},
		{
			name: "not in allowed Groups",
			fields: fields{
				Scopes:        []string{"https://www.googleapis.com/auth/cloud-identity.groups.readonly"},
				allowedGroups: []string{"not-that-one"},
			},
			args: args{
				token: tokenWithID,
				client: &http.Client{
					Transport: &roundTripperFunc{
						fn: func(req *http.Request) (*http.Response, error) {
							resp := httptest.NewRecorder()
							_, _ = resp.WriteString(`{
                                "memberships": [
                                    {
                                        "group": "test-group",
                                        "groupKey": {
                                            "id": "test-group@google.com"
                                        },
                                        "displayName": "Test Group"
                                    }
                                ],
                                "nextPageToken": ""
                            }`)
							return resp.Result(), nil
						},
					},
				},
			},
			wantData: &social.BasicUserInfo{
				Id:     "88888888888888",
				Login:  "test@example.com",
				Email:  "test@example.com",
				Name:   "Test User",
				Groups: []string{"test-group@google.com"},
			},
			wantErr:    true,
			wantErrMsg: "user not a member of one of the required groups",
		},
		{
			name: "Role mapping - strict",
			fields: fields{
				Scopes:              []string{},
				allowedGroups:       []string{},
				roleAttributePath:   "this",
				roleAttributeStrict: true,
			},
			args: args{
				token: tokenWithID,
			},
			wantData: &social.BasicUserInfo{
				Id:     "88888888888888",
				Login:  "test@example.com",
				Email:  "test@example.com",
				Name:   "Test User",
				Groups: []string{"test-group@google.com"},
			},
			wantErr:    true,
			wantErrMsg: "idP did not return a role attribute, but role_attribute_strict is set",
		},
		{
			name: "role mapping from id_token - no allowed assign Grafana Admin",
			fields: fields{
				Scopes:                  []string{},
				allowAssignGrafanaAdmin: false,
				roleAttributePath:       "email_verified && 'GrafanaAdmin'",
			},
			args: args{
				token: tokenWithID,
			},
			wantData: &social.BasicUserInfo{
				Id:             "88888888888888",
				Login:          "test@example.com",
				Email:          "test@example.com",
				Name:           "Test User",
				Role:           roletype.RoleAdmin,
				IsGrafanaAdmin: nil,
			},
			wantErr: false,
		},
		{
			name: "role mapping from id_token - allowed assign Grafana Admin",
			fields: fields{
				Scopes:                  []string{},
				allowAssignGrafanaAdmin: true,
				roleAttributePath:       "email_verified && 'GrafanaAdmin'",
			},
			args: args{
				token: tokenWithID,
			},
			wantData: &social.BasicUserInfo{
				Id:             "88888888888888",
				Login:          "test@example.com",
				Email:          "test@example.com",
				Name:           "Test User",
				Role:           roletype.RoleAdmin,
				IsGrafanaAdmin: trueBoolPtr(),
			},
			wantErr: false,
		},
		{
			name: "mapping from groups",
			fields: fields{
				Scopes:            []string{"https://www.googleapis.com/auth/cloud-identity.groups.readonly"},
				roleAttributePath: "contains(groups[*], 'test-group@google.com') && 'Editor'",
			},
			args: args{
				token: tokenWithID,
				client: &http.Client{
					Transport: &roundTripperFunc{
						fn: func(req *http.Request) (*http.Response, error) {
							resp := httptest.NewRecorder()
							_, _ = resp.WriteString(`{
                                "memberships": [
                                    {
                                        "group": "test-group",
                                        "groupKey": {
                                            "id": "test-group@google.com"
                                        },
                                        "displayName": "Test Group"
                                    }
                                ],
                                "nextPageToken": ""
                            }`)
							return resp.Result(), nil
						},
					},
				},
			},
			wantData: &social.BasicUserInfo{
				Id:     "88888888888888",
				Login:  "test@example.com",
				Email:  "test@example.com",
				Name:   "Test User",
				Role:   "Editor",
				Groups: []string{"test-group@google.com"},
			},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			s := NewGoogleProvider(
				&social.OAuthInfo{
					ApiUrl:                  tt.fields.apiURL,
					Scopes:                  tt.fields.Scopes,
					AllowedGroups:           tt.fields.allowedGroups,
					AllowSignup:             false,
					RoleAttributePath:       tt.fields.roleAttributePath,
					RoleAttributeStrict:     tt.fields.roleAttributeStrict,
					AllowAssignGrafanaAdmin: tt.fields.allowAssignGrafanaAdmin,
					// TODO: use this setting when SkipOrgRoleSync has moved to OAuthInfo
					// SkipOrgRoleSync: tt.fields.skipOrgRoleSync,
				},
				&setting.Cfg{
					GoogleSkipOrgRoleSync: tt.fields.skipOrgRoleSync,
				},
				&ssosettingstests.MockService{},
				featuremgmt.WithFeatures())

			gotData, err := s.UserInfo(context.Background(), tt.args.client, tt.args.token)
			if tt.wantErr {
				require.Error(t, err)
				assert.Contains(t, err.Error(), tt.wantErrMsg)
			} else {
				require.NoError(t, err)
				assert.Equal(t, tt.wantData, gotData)
			}
		})
	}
}
