package graphqlbackend

import (
	"testing"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/usagestatsdeprecated"
	"github.com/sourcegraph/sourcegraph/internal/database/dbmock"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestUser_UsageStatistics(t *testing.T) {
	users := dbmock.NewMockUserStore()
	users.GetByIDFunc.SetDefaultReturn(&types.User{ID: 1, Username: "alice"}, nil)

	db := dbmock.NewMockDB()
	db.UsersFunc.SetDefaultReturn(users)

	usagestatsdeprecated.MockGetByUserID = func(userID int32) (*types.UserUsageStatistics, error) {
		return &types.UserUsageStatistics{
			SearchQueries: 2,
		}, nil
	}
	defer func() { usagestatsdeprecated.MockGetByUserID = nil }()

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
