package graphqlbackend

import (
	"testing"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/usagestats"
)

func TestUser_UsageStatistics(t *testing.T) {
	users := database.NewMockUserStore()
	users.GetByIDFunc.SetDefaultReturn(&types.User{ID: 1, Username: "alice"}, nil)

	db := database.NewMockDB()
	db.UsersFunc.SetDefaultReturn(users)

	usagestats.MockGetByUserID = func(userID int32) (*types.UserUsageStatistics, error) {
		return &types.UserUsageStatistics{
			SearchQueries: 2,
		}, nil
	}
	defer func() { usagestats.MockGetByUserID = nil }()

	RunTests(t, []*Test{
		{
			Schema: mustParseGraphQLSchema(t, db),
			Query: `
				{
					node(id: "VXNlcjox") {
						id
						... on User {
							usageStatistics {
								searchQueries
							}
						}
					}
				}
			`,
			ExpectedResult: `
				{
					"node": {
						"id": "VXNlcjox",
						"usageStatistics": {
							"searchQueries": 2
						}
					}
				}
			`,
		},
	})
}
