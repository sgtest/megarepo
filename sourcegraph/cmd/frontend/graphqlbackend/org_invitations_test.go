package graphqlbackend

import (
	"context"
	"encoding/base64"
	"strings"
	"testing"
	"time"

	"github.com/golang-jwt/jwt/v4"
	"github.com/graph-gophers/graphql-go/errors"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/types"
	stderrors "github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/schema"
)

func mockTimeNow() {
	now := time.Date(2021, 1, 28, 0, 0, 0, 0, time.UTC)
	timeNow = func() time.Time {
		return now
	}
}

func mockSiteConfigSigningKey() string {
	signingKey := "Zm9v"
	conf.Mock(&conf.Unified{
		SiteConfiguration: schema.SiteConfiguration{
			OrganizationInvitations: &schema.OrganizationInvitations{
				SigningKey: signingKey,
			},
		},
	})

	return signingKey
}

func mockDefaultSiteConfig() {
	conf.Mock(&conf.Unified{SiteConfiguration: schema.SiteConfiguration{}})
}

func TestCreateJWT(t *testing.T) {
	t.Run("Fails when signingKey is not configured in site config", func(t *testing.T) {
		_, err := createInvitationJWT(1, 1, 1)

		expectedError := "signing key not provided, cannot create JWT for invitation URL. Please add organizationInvitations signingKey to site configuration."
		if err == nil || err.Error() != expectedError {
			t.Fatalf("Expected error about signing key not provided, got %v", err)
		}
	})
	t.Run("Returns JWT with encoded parameters", func(t *testing.T) {
		signingKey := mockSiteConfigSigningKey()
		defer mockDefaultSiteConfig()

		token, err := createInvitationJWT(1, 2, 3)
		if err != nil {
			t.Fatal(err)
		}

		parsed, err := jwt.ParseWithClaims(token, &orgInvitationClaims{}, func(token *jwt.Token) (interface{}, error) {
			// Validate the alg is what we expect
			if _, ok := token.Method.(*jwt.SigningMethodHMAC); !ok {
				return nil, stderrors.Newf("Not using HMAC for signing, found %v", token.Method)
			}

			return base64.StdEncoding.DecodeString(signingKey)
		})

		if err != nil {
			t.Fatal(err)
		}
		if !parsed.Valid {
			t.Fatalf("parsed JWT not valid")
		}

		claims, ok := parsed.Claims.(*orgInvitationClaims)
		if !ok {
			t.Fatalf("parsed JWT claims not ok")
		}
		if claims.Subject != "1" || claims.InvitationID != 2 || claims.SenderID != 3 {
			t.Fatalf("claims from JWT do not match expectations %v", claims)
		}
	})
}

func TestOrgInvitationURL(t *testing.T) {
	t.Run("Fails if site config is not defined", func(t *testing.T) {
		_, err := orgInvitationURL(1, 1, 1, 1, "foo@bar.baz", true)

		expectedError := "signing key not provided, cannot create JWT for invitation URL. Please add organizationInvitations signingKey to site configuration."
		if err == nil || err.Error() != expectedError {
			t.Fatalf("Expected error about signing key not provided, instead got %v", err)
		}
	})

	t.Run("Returns invitation URL with JWT", func(t *testing.T) {
		signingKey := mockSiteConfigSigningKey()
		defer mockDefaultSiteConfig()

		url, err := orgInvitationURL(1, 2, 3, 0, "foo@bar.baz", true)
		if err != nil {
			t.Fatal(err)
		}
		if !strings.HasPrefix(url, "/organizations/invitation/") {
			t.Fatalf("Url is malformed %s", url)
		}
		items := strings.Split(url, "/")
		token := items[len(items)-1]

		parsed, err := jwt.ParseWithClaims(token, &orgInvitationClaims{}, func(token *jwt.Token) (interface{}, error) {
			// Validate the alg is what we expect
			if _, ok := token.Method.(*jwt.SigningMethodHMAC); !ok {
				return nil, stderrors.Newf("Not using HMAC for signing, found %v", token.Method)
			}

			return base64.StdEncoding.DecodeString(signingKey)
		})

		if err != nil {
			t.Fatal(err)
		}
		if !parsed.Valid {
			t.Fatalf("parsed JWT not valid")
		}

		claims, ok := parsed.Claims.(*orgInvitationClaims)
		if !ok {
			t.Fatalf("parsed JWT claims not ok")
		}
		if claims.Subject != "1" || claims.InvitationID != 2 || claims.SenderID != 3 {
			t.Fatalf("claims from JWT do not match expectations %v", claims)
		}
	})
}

func TestInviteUserToOrganization(t *testing.T) {
	mockTimeNow()
	defer func() {
		timeNow = time.Now
	}()
	users := database.NewMockUserStore()
	users.GetByCurrentAuthUserFunc.SetDefaultReturn(&types.User{ID: 1}, nil)
	users.GetByUsernameFunc.SetDefaultReturn(&types.User{ID: 2, Username: "foo"}, nil)

	orgMembers := database.NewMockOrgMemberStore()
	orgMembers.GetByOrgIDAndUserIDFunc.SetDefaultHook(func(_ context.Context, orgID int32, userID int32) (*types.OrgMembership, error) {
		if userID == 1 {
			return &types.OrgMembership{}, nil
		}

		return nil, &database.ErrOrgMemberNotFound{}
	})

	orgs := database.NewMockOrgStore()
	orgName := "acme"
	mockedOrg := types.Org{ID: 1, Name: orgName}
	orgs.GetByNameFunc.SetDefaultReturn(&mockedOrg, nil)
	orgs.GetByIDFunc.SetDefaultReturn(&mockedOrg, nil)

	orgInvitations := database.NewMockOrgInvitationStore()
	orgInvitations.CreateFunc.SetDefaultReturn(&database.OrgInvitation{ID: 1}, nil)

	featureFlags := database.NewMockFeatureFlagStore()
	featureFlags.GetOrgFeatureFlagFunc.SetDefaultReturn(false, nil)

	db := database.NewMockDB()
	db.OrgsFunc.SetDefaultReturn(orgs)
	db.UsersFunc.SetDefaultReturn(users)
	db.OrgMembersFunc.SetDefaultReturn(orgMembers)
	db.OrgInvitationsFunc.SetDefaultReturn(orgInvitations)
	db.FeatureFlagsFunc.SetDefaultReturn(featureFlags)

	t.Run("Falls back to legacy URL if site settings not provided", func(t *testing.T) {
		RunTests(t, []*Test{
			{
				Schema: mustParseGraphQLSchema(t, db),
				Query: `
				mutation InviteUserToOrganization($organization: ID!, $username: String!) {
					inviteUserToOrganization(organization: $organization, username: $username) {
						sentInvitationEmail
						invitationURL
					}
				}
				`,
				Variables: map[string]interface{}{
					"organization": string(MarshalOrgID(1)),
					"username":     "foo",
				},
				ExpectedResult: `
				{
					"inviteUserToOrganization": {
						"invitationURL": "http://example.com/organizations/acme/invitation",
						"sentInvitationEmail": false
					}
				}
				`,
			},
		})
	})

	t.Run("Returns invitation URL in the response for username invitation", func(t *testing.T) {
		mockSiteConfigSigningKey()
		defer mockDefaultSiteConfig()
		RunTests(t, []*Test{
			{
				Schema: mustParseGraphQLSchema(t, db),
				Query: `
				mutation InviteUserToOrganization($organization: ID!, $username: String) {
					inviteUserToOrganization(organization: $organization, username: $username) {
						sentInvitationEmail
						invitationURL
					}
				}
				`,
				Variables: map[string]interface{}{
					"organization": string(MarshalOrgID(1)),
					"username":     "foo",
				},
				ExpectedResult: `
				{
					"inviteUserToOrganization": {
						"invitationURL": "http://example.com/organizations/invitation/eyJhbGciOiJIUzUxMiIsInR5cCI6IkpXVCJ9.eyJpbnZpdGVfSUQiOjEsInNlbmRlcl9pZCI6MSwiaXNzIjoiaHR0cDovL2V4YW1wbGUuY29tIiwic3ViIjoiMSIsImV4cCI6MTYxMTk2NDgwMH0.mYuEtDxbepKH00xRE6qzfXLKivkLAMw0MVXtQ5jaCVVWDPMrQuTU-cNQZjPKN5PDA5gRFj6C10d06nVz5TC63Q",
						"sentInvitationEmail": false
					}
				}
				`,
			},
		})
	})

	t.Run("Fails for email invitation if feature flag is not enabled", func(t *testing.T) {
		mockSiteConfigSigningKey()
		defer mockDefaultSiteConfig()
		RunTests(t, []*Test{
			{
				Schema: mustParseGraphQLSchema(t, db),
				Query: `
				mutation InviteUserToOrganization($organization: ID!, $email: String) {
					inviteUserToOrganization(organization: $organization, email: $email) {
						sentInvitationEmail
						invitationURL
					}
				}
				`,
				Variables: map[string]interface{}{
					"organization": string(MarshalOrgID(1)),
					"email":        "foo@bar.baz",
				},
				ExpectedResult: "null",
				ExpectedErrors: []*errors.QueryError{
					{
						Message: "inviting by email is not supported for this organization",
						Path:    []interface{}{"inviteUserToOrganization"},
					},
				},
			},
		})
	})

	t.Run("Returns invitation URL in the response for email invitation", func(t *testing.T) {
		mockSiteConfigSigningKey()
		defer mockDefaultSiteConfig()

		featureFlags.GetOrgFeatureFlagFunc.SetDefaultReturn(true, nil)
		defer func() {
			featureFlags.GetOrgFeatureFlagFunc.SetDefaultReturn(false, nil)
		}()
		RunTests(t, []*Test{
			{
				Schema: mustParseGraphQLSchema(t, db),
				Query: `
				mutation InviteUserToOrganization($organization: ID!, $email: String) {
					inviteUserToOrganization(organization: $organization, email: $email) {
						sentInvitationEmail
						invitationURL
					}
				}
				`,
				Variables: map[string]interface{}{
					"organization": string(MarshalOrgID(1)),
					"email":        "foo@bar.baz",
				},
				ExpectedResult: `
				{
					"inviteUserToOrganization": {
						"invitationURL": "http://example.com/organizations/invitation/eyJhbGciOiJIUzUxMiIsInR5cCI6IkpXVCJ9.eyJpbnZpdGVfSUQiOjEsInNlbmRlcl9pZCI6MSwiaXNzIjoiaHR0cDovL2V4YW1wbGUuY29tIiwic3ViIjoiMSIsImV4cCI6MTYxMTk2NDgwMH0.mYuEtDxbepKH00xRE6qzfXLKivkLAMw0MVXtQ5jaCVVWDPMrQuTU-cNQZjPKN5PDA5gRFj6C10d06nVz5TC63Q",
						"sentInvitationEmail": false
					}
				}
				`,
			},
		})
	})
}

func TestInvitationByToken(t *testing.T) {
	users := database.NewMockUserStore()
	users.GetByCurrentAuthUserFunc.SetDefaultReturn(&types.User{ID: 1}, nil)
	users.GetByUsernameFunc.SetDefaultReturn(&types.User{ID: 2, Username: "foo"}, nil)

	orgMembers := database.NewMockOrgMemberStore()
	orgMembers.GetByOrgIDAndUserIDFunc.SetDefaultHook(func(_ context.Context, orgID int32, userID int32) (*types.OrgMembership, error) {
		if userID == 1 {
			return &types.OrgMembership{}, nil
		}

		return nil, &database.ErrOrgMemberNotFound{}
	})

	orgs := database.NewMockOrgStore()
	orgName := "acme"
	mockedOrg := types.Org{ID: 1, Name: orgName}
	orgs.GetByNameFunc.SetDefaultReturn(&mockedOrg, nil)
	orgs.GetByIDFunc.SetDefaultReturn(&mockedOrg, nil)

	orgInvitations := database.NewMockOrgInvitationStore()
	orgInvitations.GetPendingByIDFunc.SetDefaultReturn(&database.OrgInvitation{ID: 1, OrgID: 1, RecipientUserID: 1}, nil)

	db := database.NewMockDB()
	db.OrgsFunc.SetDefaultReturn(orgs)
	db.UsersFunc.SetDefaultReturn(users)
	db.OrgMembersFunc.SetDefaultReturn(orgMembers)
	db.OrgInvitationsFunc.SetDefaultReturn(orgInvitations)

	ctx := actor.WithActor(context.Background(), &actor.Actor{UID: 1})

	t.Run("Fails if site config is not provided", func(t *testing.T) {
		token := "anything"
		RunTests(t, []*Test{
			{
				Schema:  mustParseGraphQLSchema(t, db),
				Context: ctx,
				Query: `
				query InvitationByToken($token: String!) {
					invitationByToken(token: $token) {
						organization {
							name
						}
					}
				}
				`,
				Variables: map[string]interface{}{
					"token": token,
				},
				ExpectedResult: `null`,
				ExpectedErrors: []*errors.QueryError{
					{
						Message: "signing key not provided, cannot validate JWT on invitation URL. Please add organizationInvitations signingKey to site configuration.",
						Path:    []interface{}{"invitationByToken"},
					},
				},
			},
		})
	})

	t.Run("Returns invitation URL in the response", func(t *testing.T) {
		mockSiteConfigSigningKey()
		defer mockDefaultSiteConfig()
		token, err := createInvitationJWT(1, 1, 1)
		if err != nil {
			t.Fatal(err)
		}
		RunTests(t, []*Test{
			{
				Schema:  mustParseGraphQLSchema(t, db),
				Context: ctx,
				Query: `
				query InvitationByToken($token: String!) {
					invitationByToken(token: $token) {
						id
						organization {
							name
						}
					}
				}
				`,
				Variables: map[string]interface{}{
					"token": token,
				},
				ExpectedResult: `{
					"invitationByToken": {
						"id": "T3JnSW52aXRhdGlvbjox",
						"organization": {
							"name": "acme"
						}
					}
				}`,
			},
		})
	})
}

func TestRespondToOrganizationInvitation(t *testing.T) {
	users := database.NewMockUserStore()
	users.GetByCurrentAuthUserFunc.SetDefaultReturn(&types.User{ID: 2}, nil)
	users.GetByUsernameFunc.SetDefaultReturn(&types.User{ID: 2, Username: "foo"}, nil)

	orgMembers := database.NewMockOrgMemberStore()
	orgMembers.GetByOrgIDAndUserIDFunc.SetDefaultHook(func(_ context.Context, orgID int32, userID int32) (*types.OrgMembership, error) {
		if userID == 1 {
			return &types.OrgMembership{}, nil
		}

		return nil, &database.ErrOrgMemberNotFound{}
	})

	orgs := database.NewMockOrgStore()
	orgName := "acme"
	mockedOrg := types.Org{ID: 1, Name: orgName}
	orgs.GetByNameFunc.SetDefaultReturn(&mockedOrg, nil)
	orgs.GetByIDFunc.SetDefaultReturn(&mockedOrg, nil)

	orgInvitations := database.NewMockOrgInvitationStore()
	orgInvitations.GetPendingByIDFunc.SetDefaultReturn(&database.OrgInvitation{ID: 1, OrgID: 1, RecipientUserID: 2}, nil)
	orgInvitations.RespondFunc.SetDefaultHook(func(ctx context.Context, id int64, userID int32, accept bool) (int32, error) {
		return int32(id), nil
	})

	db := database.NewMockDB()
	db.OrgsFunc.SetDefaultReturn(orgs)
	db.UsersFunc.SetDefaultReturn(users)
	db.OrgMembersFunc.SetDefaultReturn(orgMembers)
	db.OrgInvitationsFunc.SetDefaultReturn(orgInvitations)

	ctx := actor.WithActor(context.Background(), &actor.Actor{UID: 2})

	t.Run("User is able to decline an invitation", func(t *testing.T) {
		invitationID := int64(1)
		orgID := int32(1)
		orgInvitations.GetPendingByIDFunc.SetDefaultReturn(&database.OrgInvitation{ID: invitationID, OrgID: orgID, RecipientUserID: 2}, nil)

		RunTests(t, []*Test{
			{
				Schema:  mustParseGraphQLSchema(t, db),
				Context: ctx,
				Query: `
				mutation RespondToOrganizationInvitation($id: ID!, $response: OrganizationInvitationResponseType!) {
					respondToOrganizationInvitation(organizationInvitation:$id, responseType: $response) {
						alwaysNil
					}
				}
				`,
				Variables: map[string]interface{}{
					"id":       string(marshalOrgInvitationID(invitationID)),
					"response": "REJECT",
				},
				ExpectedResult: `{
					"respondToOrganizationInvitation": {
						"alwaysNil": null
					}
				}`,
			},
		})

		respondCalls := orgInvitations.RespondFunc.History()
		lastRespondCall := respondCalls[len(respondCalls)-1]
		if lastRespondCall.Arg1 != invitationID || lastRespondCall.Arg2 != 2 || lastRespondCall.Arg3 != false {
			t.Fatalf("db.OrgInvitations.Respond was not called with right args: %v", lastRespondCall.Args())
		}
		memberCalls := orgMembers.CreateFunc.History()
		if len(memberCalls) > 0 {
			t.Fatalf("db.OrgMembers.Create should not have been called, but got %d calls", len(memberCalls))
		}
	})

	t.Run("User is able to accept a user invitation", func(t *testing.T) {
		invitationID := int64(2)
		orgID := int32(2)
		orgInvitations.GetPendingByIDFunc.SetDefaultReturn(&database.OrgInvitation{ID: invitationID, OrgID: orgID, RecipientUserID: 2}, nil)

		RunTests(t, []*Test{
			{
				Schema:  mustParseGraphQLSchema(t, db),
				Context: ctx,
				Query: `
				mutation RespondToOrganizationInvitation($id: ID!, $response: OrganizationInvitationResponseType!) {
					respondToOrganizationInvitation(organizationInvitation:$id, responseType: $response) {
						alwaysNil
					}
				}
				`,
				Variables: map[string]interface{}{
					"id":       string(marshalOrgInvitationID(invitationID)),
					"response": "ACCEPT",
				},
				ExpectedResult: `{
					"respondToOrganizationInvitation": {
						"alwaysNil": null
					}
				}`,
			},
		})

		respondCalls := orgInvitations.RespondFunc.History()
		lastRespondCall := respondCalls[len(respondCalls)-1]
		if lastRespondCall.Arg1 != invitationID || lastRespondCall.Arg2 != 2 || lastRespondCall.Arg3 != true {
			t.Fatalf("db.OrgInvitations.Respond was not called with right args: %v", lastRespondCall.Args())
		}
		memberCalls := orgMembers.CreateFunc.History()
		lastMemberCall := memberCalls[len(memberCalls)-1]
		if lastMemberCall.Arg1 != orgID || lastMemberCall.Arg2 != 2 {
			t.Fatalf("db.OrgMembers.Create was not called with right args: %v", lastMemberCall.Args())
		}
	})

	t.Run("User is able to accept an email invitation", func(t *testing.T) {
		invitationID := int64(3)
		orgID := int32(3)
		email := "foo@bar.baz"
		orgInvitations.GetPendingByIDFunc.SetDefaultReturn(&database.OrgInvitation{ID: invitationID, OrgID: orgID, RecipientEmail: email}, nil)

		userEmails := database.NewMockUserEmailsStore()
		userEmails.ListByUserFunc.SetDefaultReturn([]*database.UserEmail{{Email: email, UserID: 2}}, nil)
		db.UserEmailsFunc.SetDefaultReturn(userEmails)

		RunTests(t, []*Test{
			{
				Schema:  mustParseGraphQLSchema(t, db),
				Context: ctx,
				Query: `
				mutation RespondToOrganizationInvitation($id: ID!, $response: OrganizationInvitationResponseType!) {
					respondToOrganizationInvitation(organizationInvitation:$id, responseType: $response) {
						alwaysNil
					}
				}
				`,
				Variables: map[string]interface{}{
					"id":       string(marshalOrgInvitationID(invitationID)),
					"response": "ACCEPT",
				},
				ExpectedResult: `{
					"respondToOrganizationInvitation": {
						"alwaysNil": null
					}
				}`,
			},
		})

		respondCalls := orgInvitations.RespondFunc.History()
		lastRespondCall := respondCalls[len(respondCalls)-1]
		if lastRespondCall.Arg1 != invitationID || lastRespondCall.Arg2 != 2 || lastRespondCall.Arg3 != true {
			t.Fatalf("db.OrgInvitations.Respond was not called with right args: %v", lastRespondCall.Args())
		}
		memberCalls := orgMembers.CreateFunc.History()
		lastMemberCall := memberCalls[len(memberCalls)-1]
		if lastMemberCall.Arg1 != orgID || lastMemberCall.Arg2 != 2 {
			t.Fatalf("db.OrgMembers.Create was not called with right args: %v", lastMemberCall.Args())
		}
	})

	t.Run("Fails if email on the invitation does not match user email", func(t *testing.T) {
		invitationID := int64(3)
		orgID := int32(3)
		email := "foo@bar.baz"
		orgInvitations.GetPendingByIDFunc.SetDefaultReturn(&database.OrgInvitation{ID: invitationID, OrgID: orgID, RecipientEmail: email}, nil)

		userEmails := database.NewMockUserEmailsStore()
		userEmails.ListByUserFunc.SetDefaultReturn([]*database.UserEmail{{Email: "something@else.invalid", UserID: 2}}, nil)
		db.UserEmailsFunc.SetDefaultReturn(userEmails)

		RunTests(t, []*Test{
			{
				Schema:  mustParseGraphQLSchema(t, db),
				Context: ctx,
				Query: `
				mutation RespondToOrganizationInvitation($id: ID!, $response: OrganizationInvitationResponseType!) {
					respondToOrganizationInvitation(organizationInvitation:$id, responseType: $response) {
						alwaysNil
					}
				}
				`,
				Variables: map[string]interface{}{
					"id":       string(marshalOrgInvitationID(invitationID)),
					"response": "ACCEPT",
				},
				ExpectedResult: "null",
				ExpectedErrors: []*errors.QueryError{
					{
						Message: "your email addresses [something@else.invalid] do not match the email address on the invitation.",
						Path:    []interface{}{"respondToOrganizationInvitation"},
					},
				},
			},
		})
	})
}
