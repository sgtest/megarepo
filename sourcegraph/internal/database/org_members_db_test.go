package database

import (
	"context"
	"reflect"
	"testing"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func TestOrgMembers_CreateMembershipInOrgsForAllUsers(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Create fixtures.
	org1, err := db.Orgs().Create(ctx, "org1", nil)
	if err != nil {
		t.Fatal(err)
	}
	org2, err := db.Orgs().Create(ctx, "org2", nil)
	if err != nil {
		t.Fatal(err)
	}
	org3, err := db.Orgs().Create(ctx, "org3", nil)
	if err != nil {
		t.Fatal(err)
	}
	user1, err := db.Users().Create(ctx, NewUser{
		Email:                 "a1@example.com",
		Username:              "u1",
		Password:              "p",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}
	_, err = db.Users().Create(ctx, NewUser{
		Email:                 "a2@example.com",
		Username:              "u2",
		Password:              "p",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}
	if _, err := db.OrgMembers().Create(ctx, org1.ID, user1.ID); err != nil {
		t.Fatal(err)
	}

	check := func() error {
		want := map[string][]int32{
			"org1": {1, 2},
			"org2": {},
			"org3": {1, 2},
		}
		got := map[string][]int32{}
		for _, org := range []*types.Org{org1, org2, org3} {
			members, err := db.OrgMembers().GetByOrgID(ctx, org.ID)
			if err != nil {
				return err
			}
			if len(members) == 0 {
				got[org.Name] = []int32{}
			}
			for _, member := range members {
				got[org.Name] = append(got[org.Name], member.UserID)
			}
		}
		if !reflect.DeepEqual(got, want) {
			return errors.Errorf("got membership %+v, want %+v", got, want)
		}
		return nil
	}

	// Try twice; it should be idempotent.
	if err := db.OrgMembers().CreateMembershipInOrgsForAllUsers(ctx, []string{"org1", "org3"}); err != nil {
		t.Fatal(err)
	}
	if err := check(); err != nil {
		t.Fatal(err)
	}
	if err := db.OrgMembers().CreateMembershipInOrgsForAllUsers(ctx, []string{"org1", "org3"}); err != nil {
		t.Fatal(err)
	}
	if err := check(); err != nil {
		t.Fatal(err)
	}

	// Passing an org that does not exist should not be an error.
	if err := db.OrgMembers().CreateMembershipInOrgsForAllUsers(ctx, []string{"doesntexist"}); err != nil {
		t.Fatal(err)
	}
	if err := check(); err != nil {
		t.Fatal(err)
	}

	// An empty list shouldn't be an error.
	if err := db.OrgMembers().CreateMembershipInOrgsForAllUsers(ctx, []string{}); err != nil {
		t.Fatal(err)
	}
	if err := check(); err != nil {
		t.Fatal(err)
	}
}

func TestOrgMembers_MemberCount(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()
	// Create fixtures.
	org1, err := db.Orgs().Create(ctx, "org1", nil)
	if err != nil {
		t.Fatal(err)
	}
	org2, err := db.Orgs().Create(ctx, "org2", nil)
	if err != nil {
		t.Fatal(err)
	}
	org3, err := db.Orgs().Create(ctx, "org3", nil)
	if err != nil {
		t.Fatal(err)
	}
	user1, err := db.Users().Create(ctx, NewUser{
		Email:                 "a1@example.com",
		Username:              "u1",
		Password:              "p",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}
	user2, err := db.Users().Create(ctx, NewUser{
		Email:                 "a2@example.com",
		Username:              "u2",
		Password:              "p2",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}
	deletedUser, err := db.Users().Create(ctx, NewUser{
		Email:                 "deleted@example.com",
		Username:              "deleted",
		Password:              "p2",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}
	db.OrgMembers().Create(ctx, org1.ID, user1.ID)
	db.OrgMembers().Create(ctx, org2.ID, user1.ID)
	db.OrgMembers().Create(ctx, org2.ID, user2.ID)
	db.OrgMembers().Create(ctx, org3.ID, user1.ID)
	db.OrgMembers().Create(ctx, org3.ID, deletedUser.ID)
	err = db.Users().Delete(ctx, deletedUser.ID)
	if err != nil {
		t.Fatal(err)
	}
	for _, test := range []struct {
		name  string
		orgID int32
		want  int
	}{
		{"org with single member", org1.ID, 1},
		{"org with two members", org2.ID, 2},
		{"org with one deleted member", org3.ID, 1}} {
		t.Run(test.name, func(*testing.T) {
			got, err := db.OrgMembers().MemberCount(ctx, test.orgID)
			if err != nil {
				t.Fatal(err)
			}
			if test.want != got {
				t.Errorf("want %v, got %v", test.want, got)
			}
		})

	}

}

func TestOrgMembers_AutocompleteMembersSearch(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	tests := []struct {
		name     string
		username string
		email    string
	}{
		{
			name:     "test user1",
			username: "testuser1",
			email:    "em1@test.com",
		},
		{
			name:     "user maximum",
			username: "testuser2",
			email:    "em2@test.com",
		},

		{
			name:     "user fancy",
			username: "testuser3",
			email:    "em3@test.com",
		},
		{
			name:     "user notsofancy",
			username: "testuser4",
			email:    "em4@test.com",
		},
		{
			name:     "display name",
			username: "testuser5",
			email:    "em5@test.com",
		},
		{
			name:     "another name",
			username: "testuser6",
			email:    "em6@test.com",
		},
		{
			name:     "test user7",
			username: "testuser7",
			email:    "em14@test.com",
		},
		{
			name:     "test user8",
			username: "testuser8",
			email:    "em13@test.com",
		},
		{
			name:     "test user9",
			username: "testuser9",
			email:    "em18@test.com",
		},
		{
			name:     "test user10",
			username: "testuser10",
			email:    "em19@test.com",
		},
		{
			name:     "test user11",
			username: "testuser11",
			email:    "em119@test.com",
		},
		{
			name:     "searchabletrue",
			username: "testuser12",
			email:    "em19@test.com",
		},
		{
			name:     "test user12",
			username: "searchablefalse",
			email:    "em19@test.com",
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			_, err := db.Users().Create(ctx, NewUser{
				Username:              test.username,
				DisplayName:           test.name,
				Email:                 test.email,
				Password:              "p",
				EmailVerificationCode: "c",
			})
			if err != nil {
				t.Fatal(err)
			}
		})
	}

	users, err := db.OrgMembers().AutocompleteMembersSearch(ctx, 1, "testus")
	if err != nil {
		t.Fatal(err)
	}

	if want := 10; len(users) != want {
		t.Errorf("got %d, want %d", len(users), want)
	}

	user, err := db.Users().GetByUsername(ctx, "searchablefalse")
	if err != nil {
		t.Fatal(err)
	}
	if err := db.Users().Update(ctx, user.ID, UserUpdate{Searchable: boolptr(false)}); err != nil {
		t.Fatal(err)
	}

	users2, err := db.OrgMembers().AutocompleteMembersSearch(ctx, 1, "searchable")
	if err != nil {
		t.Fatal(err)
	}

	if want := 1; len(users2) != want {
		t.Errorf("got %d, want %d", len(users2), want)
	}
}
