package scim

import (
	"context"
	"strconv"
	"testing"

	"github.com/elimity-com/scim"
	"github.com/scim2/filter-parser/v2"
	"github.com/stretchr/testify/assert"

	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

const sampleAccountData = `{
	"active": true,
	"emails": [
	  {
		"type": "work",
		"value": "primary@work.com",
		"primary": true
	  },
	  {
		"type": "work",
		"value": "secondary@work.com",
		"primary": false
	  }
	],
	"name": {
	  "givenName": "Nannie",
	  "familyName": "Krystina",
	  "formatted": "Reilly",
	  "middleName": "Camren"
	},
	"displayName": "N0LBQ9P0TTH4",
	"userName": "faye@rippinkozey.com"
  }`

func Test_UserResourceHandler_Patch_Username(t *testing.T) {
	t.Parallel()

	db := getMockDB([]*types.UserForSCIM{
		{User: types.User{ID: 1}},
		{User: types.User{ID: 2, Username: "test-user2", DisplayName: "First Last"}, Emails: []string{"a@example.com"}, SCIMExternalID: "id2"},
		{User: types.User{ID: 3}},
		{User: types.User{ID: 4, Username: "test-user4"}, Emails: []string{"primary@work.com"}, SCIMExternalID: "id4", SCIMAccountData: sampleAccountData},
		{User: types.User{ID: 5, Username: "test-user5"}, Emails: []string{"primary@work.com"}, SCIMExternalID: "id5", SCIMAccountData: sampleAccountData},
		{User: types.User{ID: 6, Username: "test-user6"}, Emails: []string{"primary@work.com"}, SCIMExternalID: "id6", SCIMAccountData: sampleAccountData},
		{User: types.User{ID: 7, Username: "test-user7"}, Emails: []string{"primary@work.com"}, SCIMExternalID: "id7", SCIMAccountData: sampleAccountData},
		{User: types.User{ID: 8, Username: "test-user8"}, Emails: []string{"primary@work.com"}, SCIMExternalID: "id8", SCIMAccountData: sampleAccountData},
		{User: types.User{ID: 9, Username: "test-user9"}, Emails: []string{"primary@work.com"}, SCIMExternalID: "id9", SCIMAccountData: sampleAccountData}})
	userResourceHandler := NewUserResourceHandler(context.Background(), &observation.TestContext, db)

	testCases := []struct {
		name       string
		userId     string
		operations []scim.PatchOperation
		testFunc   func(userRes scim.Resource)
	}{
		{
			name:   "patch username with replace operation",
			userId: "2",
			operations: []scim.PatchOperation{
				{Op: "replace", Path: createPath(AttrUserName, nil), Value: "test-user2-patched"},
			},
			testFunc: func(userRes scim.Resource) {
				assert.Equal(t, "test-user2-patched", userRes.Attributes[AttrUserName])
				userID, _ := strconv.Atoi(userRes.ID)
				user, err := db.Users().GetByID(context.Background(), int32(userID))
				assert.NoError(t, err)
				assert.Equal(t, "test-user2-patched", user.Username)
			},
		},
		{
			name:   "patch username with add operation",
			userId: "2",
			operations: []scim.PatchOperation{
				{Op: "add", Path: createPath(AttrUserName, nil), Value: "test-user2-added"},
			},
			testFunc: func(userRes scim.Resource) {
				assert.Equal(t, "test-user2-added", userRes.Attributes[AttrUserName])
				userID, _ := strconv.Atoi(userRes.ID)
				user, err := db.Users().GetByID(context.Background(), int32(userID))
				assert.NoError(t, err)
				assert.Equal(t, "test-user2-added", user.Username)
			},
		},
		{
			name:   "patch replace with filter",
			userId: "4",
			operations: []scim.PatchOperation{
				{Op: "replace", Path: parseStringPath("emails[type eq \"work\" and primary eq true].value"), Value: "nicolas@breitenbergbartell.uk"},
				{Op: "replace", Path: parseStringPath("emails[type eq \"work\" and primary eq false].type"), Value: "home"},
				{Op: "replace", Value: map[string]interface{}{
					"userName":        "updatedUN",
					"name.givenName":  "Gertrude",
					"name.familyName": "Everett",
					"name.formatted":  "Manuela",
					"name.middleName": "Ismael",
				}},
				{Op: "replace", Path: createPath(AttrNickName, nil), Value: "nickName"},
			},
			testFunc: func(userRes scim.Resource) {
				// Check toplevel attributes
				assert.Equal(t, "updatedUN", userRes.Attributes[AttrUserName])
				assert.Equal(t, "N0LBQ9P0TTH4", userRes.Attributes["displayName"])

				// Check filtered email changes
				emails := userRes.Attributes[AttrEmails].([]interface{})
				assert.Contains(t, emails, map[string]interface{}{"value": "nicolas@breitenbergbartell.uk", "primary": true, "type": "work"})
				assert.Contains(t, emails, map[string]interface{}{"value": "secondary@work.com", "primary": false, "type": "home"})

				// Check name attributes
				name := userRes.Attributes[AttrName].(map[string]interface{})
				assert.Equal(t, "Gertrude", name[AttrNameGiven])
				assert.Equal(t, "Everett", name[AttrNameFamily])
				assert.Equal(t, "Manuela", name[AttrNameFormatted])
				assert.Equal(t, "Ismael", name[AttrNameMiddle])

				// Check nickName added
				assert.Equal(t, "nickName", userRes.Attributes[AttrNickName])

				// Check user in DB
				userID, _ := strconv.Atoi(userRes.ID)
				user, err := db.Users().GetByID(context.Background(), int32(userID))
				assert.NoError(t, err)
				assert.Equal(t, "updatedUN", user.Username)
			},
		},
		{
			name:   "remove with filter",
			userId: "5",
			operations: []scim.PatchOperation{
				{Op: "remove", Path: parseStringPath("emails[type eq \"work\" and primary eq false]")},
				{Op: "remove", Path: createPath(AttrName, strPtr(AttrNameMiddle))},
			},
			testFunc: func(userRes scim.Resource) {
				// Check only one email remains
				emails := userRes.Attributes[AttrEmails].([]interface{})
				assert.Len(t, emails, 1)
				assert.Contains(t, emails, map[string]interface{}{"value": "primary@work.com", "primary": true, "type": "work"})

				// Check name attributes
				name := userRes.Attributes[AttrName].(map[string]interface{})
				assert.Nil(t, name[AttrNameMiddle])
			},
		},
		{
			name:   "replace whole array field",
			userId: "6",
			operations: []scim.PatchOperation{
				{Op: "replace", Path: parseStringPath("emails"), Value: toInterfaceSlice(map[string]interface{}{"value": "replaced@work.com", "type": "home", "primary": true})},
			},
			testFunc: func(userRes scim.Resource) {
				// Check if it has only one email
				emails := userRes.Attributes[AttrEmails].([]interface{})
				assert.Len(t, emails, 1)
				assert.Contains(t, emails, map[string]interface{}{"value": "replaced@work.com", "primary": true, "type": "home"})
			},
		},
		{
			name:   "remove non-existing field",
			userId: "7",
			operations: []scim.PatchOperation{
				{Op: "remove", Path: createPath(AttrNickName, nil)},
			},
			testFunc: func(userRes scim.Resource) {
				// Check nickname still empty
				assert.Nil(t, userRes.Attributes[AttrNickName])
			},
		},
		{
			name:   "add non-existing field",
			userId: "8",
			operations: []scim.PatchOperation{
				{Op: "add", Path: createPath(AttrNickName, nil), Value: "sampleNickName"},
			},
			testFunc: func(userRes scim.Resource) {
				// Check nickname
				assert.Equal(t, "sampleNickName", userRes.Attributes[AttrNickName])
			},
		},
		{
			name:   "no change",
			userId: "9",
			operations: []scim.PatchOperation{
				{Op: "replace", Path: createPath(AttrName, strPtr(AttrNameGiven)), Value: "Nannie"},
			},
			testFunc: func(userRes scim.Resource) {
				// Check name the same
				name := userRes.Attributes[AttrName].(map[string]interface{})
				assert.Equal(t, "Nannie", name[AttrNameGiven])
			},
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			userRes, err := userResourceHandler.Patch(createDummyRequest(), tc.userId, tc.operations)
			assert.NoError(t, err)
			tc.testFunc(userRes)
		})
	}
}

// createPath creates a path for a given attribute and sub-attribute.
func createPath(attr string, subAttr *string) *filter.Path {
	return &filter.Path{AttributePath: filter.AttributePath{AttributeName: attr, SubAttribute: subAttr}}
}

// parseStringPath parses a string path into a filter.Path.
func parseStringPath(path string) *filter.Path {
	f, _ := filter.ParsePath([]byte(path))
	return &f
}

// strPtr returns a pointer to the given string.
func strPtr(s string) *string {
	return &s
}

// toInterfaceSlice converts a slice of maps to a slice of interfaces.
func toInterfaceSlice(maps ...map[string]interface{}) []interface{} {
	s := make([]interface{}, 0, len(maps))
	for _, m := range maps {
		s = append(s, m)
	}
	return s
}
