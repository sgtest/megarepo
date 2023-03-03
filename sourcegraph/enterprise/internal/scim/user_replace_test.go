package scim

import (
	"context"
	"strconv"
	"testing"

	"github.com/elimity-com/scim"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/stretchr/testify/assert"
)

func Test_UserResourceHandler_Replace(t *testing.T) {
	t.Parallel()

	db := getMockDB([]*types.UserForSCIM{
		{User: types.User{ID: 1}},
		{User: types.User{ID: 2, Username: "user1", DisplayName: "First Last"}, Emails: []string{"a@example.com"}, SCIMExternalID: "id1"},
		{User: types.User{ID: 3}},
	})
	userResourceHandler := NewUserResourceHandler(context.Background(), &observation.TestContext, db)

	testCases := []struct {
		name     string
		userId   string
		attrs    scim.ResourceAttributes
		testFunc func(userRes scim.Resource, err error)
	}{
		{
			name:   "replace username",
			userId: "2",
			attrs: scim.ResourceAttributes{
				AttrUserName: "user6",
			},
			testFunc: func(userRes scim.Resource, err error) {
				assert.NoError(t, err)
				assert.Equal(t, "user6", userRes.Attributes[AttrUserName])
				assert.Equal(t, false, userRes.ExternalID.Present())
				userID, _ := strconv.Atoi(userRes.ID)
				user, _ := db.Users().GetByID(context.Background(), int32(userID))
				assert.Equal(t, "user6", user.Username)
			},
		},
		{
			name:   "replace emails",
			userId: "2",
			attrs: scim.ResourceAttributes{
				AttrEmails: []interface{}{
					map[string]interface{}{
						"value":   "email@address.test",
						"primary": true,
					},
				},
			},
			testFunc: func(userRes scim.Resource, err error) {
				assert.NoError(t, err)
				assert.Nil(t, userRes.Attributes[AttrUserName])
			},
		},
		{
			name:   "replace many",
			userId: "2",
			attrs: scim.ResourceAttributes{
				AttrDisplayName: "Test User",
				AttrNickName:    "testy",
				AttrEmails: []interface{}{
					map[string]interface{}{
						"value":   "email@address.test",
						"primary": true,
					},
				},
			},
			testFunc: func(userRes scim.Resource, err error) {
				assert.NoError(t, err)
				assert.Nil(t, userRes.Attributes[AttrUserName])
				assert.Equal(t, "Test User", userRes.Attributes[AttrDisplayName])
				assert.Equal(t, "testy", userRes.Attributes[AttrNickName])
				assert.Len(t, userRes.Attributes[AttrEmails], 1)
				assert.Equal(t, userRes.Attributes[AttrEmails].([]interface{})[0].(map[string]interface{})["value"], "email@address.test")
			},
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			user, err := userResourceHandler.Replace(createDummyRequest(), tc.userId, tc.attrs)
			tc.testFunc(user, err)
		})
	}
}
