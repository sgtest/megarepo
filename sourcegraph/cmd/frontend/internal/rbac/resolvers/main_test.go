package resolvers

import (
	"context"
	"fmt"
	"sync"
	"testing"

	"github.com/graph-gophers/graphql-go"
	"github.com/keegancsmith/sqlf"

	gql "github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func newSchema(db database.DB, r gql.RBACResolver) (*graphql.Schema, error) {
	return gql.NewSchemaWithRBACResolver(db, r)
}

var createTestUser = func() func(*testing.T, database.DB, bool) *types.User {
	var mu sync.Mutex
	count := 0

	// This function replicates the minimum amount of work required by
	// database.Users.Create to create a new user, but it doesn't require passing in
	// a full database.NewUser every time.
	return func(t *testing.T, db database.DB, siteAdmin bool) *types.User {
		t.Helper()

		mu.Lock()
		num := count
		count++
		mu.Unlock()

		user := &types.User{
			Username:    fmt.Sprintf("testuser-%d", num),
			DisplayName: "testuser",
		}

		q := sqlf.Sprintf("INSERT INTO users (username, site_admin) VALUES (%s, %t) RETURNING id, site_admin", user.Username, siteAdmin)
		err := db.QueryRowContext(context.Background(), q.Query(sqlf.PostgresBindVar), q.Args()...).Scan(&user.ID, &user.SiteAdmin)
		if err != nil {
			t.Fatal(err)
		}

		if user.SiteAdmin != siteAdmin {
			t.Fatalf("user.SiteAdmin=%t, but expected is %t", user.SiteAdmin, siteAdmin)
		}

		_, err = db.ExecContext(context.Background(), "INSERT INTO names(name, user_id) VALUES($1, $2)", user.Username, user.ID)
		if err != nil {
			t.Fatalf("failed to create name: %s", err)
		}

		return user
	}
}()
