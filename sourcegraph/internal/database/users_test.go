package database

import (
	"context"
	"reflect"
	"sort"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// usernamesForTests is a list of test cases containing valid and invalid usernames and org names.
var usernamesForTests = []struct {
	name      string
	wantValid bool
}{
	{"nick", true},
	{"n1ck", true},
	{"Nick2", true},
	{"N-S", true},
	{"nick-s", true},
	{"renfred-xh", true},
	{"renfred-x-h", true},
	{"deadmau5", true},
	{"deadmau-5", true},
	{"3blindmice", true},
	{"nick.com", true},
	{"nick.com.uk", true},
	{"nick.com-put-er", true},
	{"nick-", true},
	{"777", true},
	{"7-7", true},
	{"long-butnotquitelongenoughtoreachlimit", true},

	{".nick", false},
	{"-nick", false},
	{"nick.", false},
	{"nick--s", false},
	{"nick--sny", false},
	{"nick..sny", false},
	{"nick.-sny", false},
	{"nick_s", false},
	{"_", false},
	{"_nick", false},
	{"nick_", false},
	{"ke$ha", false},
	{"ni%k", false},
	{"#nick", false},
	{"@nick", false},
	{"", false},
	{"nick s", false},
	{" ", false},
	{"-", false},
	{"--", false},
	{"-s", false},
	{"レンフレッド", false},
	{"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx", false},
}

func TestUsers_ValidUsernames(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	for _, test := range usernamesForTests {
		t.Run(test.name, func(t *testing.T) {
			valid := true
			if _, err := db.Users().Create(ctx, NewUser{Username: test.name}); err != nil {
				var e errCannotCreateUser
				if errors.As(err, &e) && (e.Code() == "users_username_max_length" || e.Code() == "users_username_valid_chars") {
					valid = false
				} else {
					t.Fatal(err)
				}
			}
			if valid != test.wantValid {
				t.Errorf("%q: got valid %v, want %v", test.name, valid, test.wantValid)
			}
		})
	}
}

func TestUsers_Create_SiteAdmin(t *testing.T) {
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	if _, err := db.GlobalState().Get(ctx); err != nil {
		t.Fatal(err)
	}

	// Create site admin.
	user, err := db.Users().Create(ctx, NewUser{
		Email:                 "a@a.com",
		Username:              "u",
		Password:              "p",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}
	if !user.SiteAdmin {
		t.Fatal("!user.SiteAdmin")
	}

	// Creating a non-site-admin now that the site has already been initialized.
	u2, err := db.Users().Create(ctx, NewUser{
		Email:                 "a2@a2.com",
		Username:              "u2",
		Password:              "p2",
		EmailVerificationCode: "c2",
	})
	if err != nil {
		t.Fatal(err)
	}
	if u2.SiteAdmin {
		t.Fatal("want u2 not site admin because site is already initialized")
	}
	// Similar to the above, but expect an error because we pass FailIfNotInitialUser: true.
	_, err = db.Users().Create(ctx, NewUser{
		Email:                 "a3@a3.com",
		Username:              "u3",
		Password:              "p3",
		EmailVerificationCode: "c3",
		FailIfNotInitialUser:  true,
	})
	if want := (errCannotCreateUser{"site_already_initialized"}); !errors.Is(err, want) {
		t.Fatalf("got error %v, want %v", err, want)
	}

	// Delete the site admin.
	if err := db.Users().Delete(ctx, user.ID); err != nil {
		t.Fatal(err)
	}

	// Disallow creating a site admin when a user already exists (even if the site is not yet initialized).
	if _, err := db.ExecContext(ctx, "UPDATE site_config SET initialized=false"); err != nil {
		t.Fatal(err)
	}
	u4, err := db.Users().Create(ctx, NewUser{
		Email:                 "a4@a4.com",
		Username:              "u4",
		Password:              "p4",
		EmailVerificationCode: "c4",
	})
	if err != nil {
		t.Fatal(err)
	}
	if u4.SiteAdmin {
		t.Fatal("want u4 not site admin because site is already initialized")
	}
	// Similar to the above, but expect an error because we pass FailIfNotInitialUser: true.
	if _, err := db.ExecContext(ctx, "UPDATE site_config SET initialized=false"); err != nil {
		t.Fatal(err)
	}
	_, err = db.Users().Create(ctx, NewUser{
		Email:                 "a5@a5.com",
		Username:              "u5",
		Password:              "p5",
		EmailVerificationCode: "c5",
		FailIfNotInitialUser:  true,
	})
	if want := (errCannotCreateUser{"initial_site_admin_must_be_first_user"}); !errors.Is(err, want) {
		t.Fatalf("got error %v, want %v", err, want)
	}
}

func TestUsers_CheckAndDecrementInviteQuota(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	user, err := db.Users().Create(ctx, NewUser{
		Email:                 "a@a.com",
		Username:              "u",
		Password:              "p",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}

	// Check default invite quota.
	var inviteQuota int
	row := db.QueryRowContext(ctx, "SELECT invite_quota FROM users WHERE id=$1", user.ID)
	if err := row.Scan(&inviteQuota); err != nil {
		t.Fatal(err)
	}
	// Check that it's within some reasonable bounds. The upper bound number here can increased
	// if we increase the default.
	if lo, hi := 0, 100; inviteQuota <= lo || inviteQuota > hi {
		t.Fatalf("got default user invite quota %d, want in [%d,%d)", inviteQuota, lo, hi)
	}

	// Decrementing should succeed while we have remaining quota. Keep going until we exhaust it.
	// Since the quota is fairly low, this isn't too slow.
	for inviteQuota > 0 {
		if ok, err := db.Users().CheckAndDecrementInviteQuota(ctx, user.ID); !ok || err != nil {
			t.Fatal("initial CheckAndDecrementInviteQuota failed:", err)
		}
		inviteQuota--
	}

	// Now our quota is exhausted, and CheckAndDecrementInviteQuota should fail.
	if ok, err := db.Users().CheckAndDecrementInviteQuota(ctx, user.ID); ok || err != nil {
		t.Fatalf("over-limit CheckAndDecrementInviteQuota #1: got error %v", err)
	}

	// Check again that we're still over quota, just in case.
	if ok, err := db.Users().CheckAndDecrementInviteQuota(ctx, user.ID); ok || err != nil {
		t.Fatalf("over-limit CheckAndDecrementInviteQuota #2: got error %v", err)
	}
}

func TestUsers_ListCount(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	user, err := db.Users().Create(ctx, NewUser{
		Email:                 "a@a.com",
		Username:              "u",
		Password:              "p",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}
	user.Tags = []string{}

	if count, err := db.Users().Count(ctx, &UsersListOptions{}); err != nil {
		t.Fatal(err)
	} else if want := 1; count != want {
		t.Errorf("got %d, want %d", count, want)
	}
	if users, err := db.Users().List(ctx, &UsersListOptions{}); err != nil {
		t.Fatal(err)
	} else if users, want := normalizeUsers(users), normalizeUsers([]*types.User{user}); !reflect.DeepEqual(users, want) {
		t.Errorf("got %+v, want %+v", users, want)
	}

	if count, err := db.Users().Count(ctx, &UsersListOptions{UserIDs: []int32{}}); err != nil {
		t.Fatal(err)
	} else if want := 0; count != want {
		t.Errorf("got %d, want %d", count, want)
	}
	if users, err := db.Users().List(ctx, &UsersListOptions{UserIDs: []int32{}}); err != nil {
		t.Fatal(err)
	} else if len(users) > 0 {
		t.Errorf("got %d, want empty", len(users))
	}

	if users, err := db.Users().List(ctx, &UsersListOptions{}); err != nil {
		t.Fatal(err)
	} else if users, want := normalizeUsers(users), normalizeUsers([]*types.User{user}); !reflect.DeepEqual(users, want) {
		t.Errorf("got %+v, want %+v", users[0], user)
	}

	if err := db.Users().Delete(ctx, user.ID); err != nil {
		t.Fatal(err)
	}

	if count, err := db.Users().Count(ctx, &UsersListOptions{}); err != nil {
		t.Fatal(err)
	} else if want := 0; count != want {
		t.Errorf("got %d, want %d", count, want)
	}
}

func TestUsers_Update(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	user, err := db.Users().Create(ctx, NewUser{
		Email:                 "a@a.com",
		Username:              "u",
		Password:              "p",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}

	if err := db.Users().Update(ctx, user.ID, UserUpdate{
		Username:    "u1",
		DisplayName: strptr("d1"),
		AvatarURL:   strptr("a1"),
	}); err != nil {
		t.Fatal(err)
	}
	user, err = db.Users().GetByID(ctx, user.ID)
	if err != nil {
		t.Fatal(err)
	}
	if want := "u1"; user.Username != want {
		t.Errorf("got username %q, want %q", user.Username, want)
	}
	if want := "d1"; user.DisplayName != want {
		t.Errorf("got display name %q, want %q", user.DisplayName, want)
	}
	if want := "a1"; user.AvatarURL != want {
		t.Errorf("got avatar URL %q, want %q", user.AvatarURL, want)
	}

	if err := db.Users().Update(ctx, user.ID, UserUpdate{
		DisplayName: strptr(""),
	}); err != nil {
		t.Fatal(err)
	}
	user, err = db.Users().GetByID(ctx, user.ID)
	if err != nil {
		t.Fatal(err)
	}
	if want := "u1"; user.Username != want {
		t.Errorf("got username %q, want %q", user.Username, want)
	}
	if user.DisplayName != "" {
		t.Errorf("got display name %q, want nil", user.DisplayName)
	}
	if want := "a1"; user.AvatarURL != want {
		t.Errorf("got avatar URL %q, want %q", user.AvatarURL, want)
	}

	// Can't update to duplicate username.
	user2, err := db.Users().Create(ctx, NewUser{
		Email:                 "a2@a.com",
		Username:              "u2",
		Password:              "p2",
		EmailVerificationCode: "c2",
	})
	if err != nil {
		t.Fatal(err)
	}
	err = db.Users().Update(ctx, user2.ID, UserUpdate{Username: "u1"})
	if diff := cmp.Diff(err.Error(), "Username is already in use."); diff != "" {
		t.Fatal(diff)
	}

	// Can't update nonexistent user.
	if err := db.Users().Update(ctx, 12345, UserUpdate{Username: "u12345"}); err == nil {
		t.Fatal("want error when updating nonexistent user")
	}
}

func TestUsers_GetByVerifiedEmail(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	user, err := db.Users().Create(ctx, NewUser{
		Email:                 "a@a.com",
		Username:              "u",
		Password:              "p",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}

	if _, err := db.Users().GetByVerifiedEmail(ctx, "a@a.com"); !errcode.IsNotFound(err) {
		t.Errorf("for unverified email, got error %v, want IsNotFound", err)
	}

	if err := db.UserEmails().SetVerified(ctx, user.ID, "a@a.com", true); err != nil {
		t.Fatal(err)
	}

	gotUser, err := db.Users().GetByVerifiedEmail(ctx, "a@a.com")
	if err != nil {
		t.Fatal(err)
	}
	if gotUser.ID != user.ID {
		t.Errorf("got user %d, want %d", gotUser.ID, user.ID)
	}
}

func TestUsers_GetByUsername(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	newUsers := []NewUser{
		{
			Email:           "alice@example.com",
			Username:        "alice",
			EmailIsVerified: true,
		},
		{
			Email:           "bob@example.com",
			Username:        "bob",
			EmailIsVerified: true,
		},
	}

	for _, newUser := range newUsers {
		_, err := db.Users().Create(ctx, newUser)
		if err != nil {
			t.Fatal(err)
		}
	}

	for _, want := range []string{"alice", "bob", "cindy"} {
		have, err := db.Users().GetByUsername(ctx, want)
		if want == "cindy" {
			// Make sure the returned err fulfils the NotFounder interface.
			if !errcode.IsNotFound(err) {
				t.Fatalf("invalid error, expected not found got %v", err)
			}
			continue
		} else if err != nil {
			t.Fatal(err)
		}
		if have.Username != want {
			t.Errorf("got %s, but want %s", have.Username, want)
		}
	}

}

func TestUsers_GetByUsernames(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	newUsers := []NewUser{
		{
			Email:           "alice@example.com",
			Username:        "alice",
			EmailIsVerified: true,
		},
		{
			Email:           "bob@example.com",
			Username:        "bob",
			EmailIsVerified: true,
		},
	}

	for _, newUser := range newUsers {
		_, err := db.Users().Create(ctx, newUser)
		if err != nil {
			t.Fatal(err)
		}
	}

	users, err := db.Users().GetByUsernames(ctx, "alice", "bob", "cindy")
	if err != nil {
		t.Fatal(err)
	}
	if len(users) != 2 {
		t.Fatalf("got %d users, but want 2", len(users))
	}
	for i := range users {
		if users[i].Username != newUsers[i].Username {
			t.Errorf("got %s, but want %s", users[i].Username, newUsers[i].Username)
		}
	}
}

func TestUsers_Delete(t *testing.T) {
	for name, hard := range map[string]bool{"soft": false, "hard": true} {
		t.Run(name, func(t *testing.T) {
			if testing.Short() {
				t.Skip()
			}
			t.Parallel()
			logger := logtest.Scoped(t)
			db := NewDB(logger, dbtest.NewDB(logger, t))
			ctx := context.Background()
			ctx = actor.WithActor(ctx, &actor.Actor{UID: 1, Internal: true})

			otherUser, err := db.Users().Create(ctx, NewUser{Username: "other"})
			if err != nil {
				t.Fatal(err)
			}

			user, err := db.Users().Create(ctx, NewUser{
				Email:                 "a@a.com",
				Username:              "u",
				Password:              "p",
				EmailVerificationCode: "c",
			})
			if err != nil {
				t.Fatal(err)
			}

			// Create external service owned by the user
			confGet := func() *conf.Unified {
				return &conf.Unified{}
			}
			err = db.ExternalServices().Create(ctx, confGet, &types.ExternalService{
				Kind:            extsvc.KindGitHub,
				DisplayName:     "GITHUB #1",
				Config:          `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc", "authorization": {}}`,
				NamespaceUserID: user.ID,
			})
			if err != nil {
				t.Fatal(err)
			}

			// Create settings for the user, and for another user authored by this user.
			if _, err := db.Settings().CreateIfUpToDate(ctx, api.SettingsSubject{User: &user.ID}, nil, &user.ID, "{}"); err != nil {
				t.Fatal(err)
			}
			if _, err := db.Settings().CreateIfUpToDate(ctx, api.SettingsSubject{User: &otherUser.ID}, nil, &user.ID, "{}"); err != nil {
				t.Fatal(err)
			}

			// Create a repository to comply with the postgres repo constraint.
			if err := upsertRepo(ctx, db, InsertRepoOp{Name: "myrepo", Description: "", Fork: false}); err != nil {
				t.Fatal(err)
			}

			// Create a saved search owned by the user.
			if _, err := db.SavedSearches().Create(ctx, &types.SavedSearch{
				Description: "desc",
				Query:       "foo",
				UserID:      &user.ID,
			}); err != nil {
				t.Fatal(err)
			}

			// Create an event log
			err = db.EventLogs().Insert(ctx, &Event{
				Name:            "something",
				URL:             "http://example.com",
				UserID:          uint32(user.ID),
				AnonymousUserID: "",
				Source:          "Test",
				Timestamp:       time.Now(),
			})
			if err != nil {
				t.Fatal(err)
			}

			if hard {
				// Hard delete user.
				if err := db.Users().HardDelete(ctx, user.ID); err != nil {
					t.Fatal(err)
				}
			} else {
				// Delete user.
				if err := db.Users().Delete(ctx, user.ID); err != nil {
					t.Fatal(err)
				}
			}

			// User no longer exists.
			_, err = db.Users().GetByID(ctx, user.ID)
			if !errcode.IsNotFound(err) {
				t.Errorf("got error %v, want ErrUserNotFound", err)
			}
			users, err := db.Users().List(ctx, nil)
			if err != nil {
				t.Fatal(err)
			}
			if len(users) > 1 {
				// The otherUser should still exist, which is why we check for 1 not 0.
				t.Errorf("got %d users, want 1", len(users))
			}

			// User's settings no longer exist.
			if settings, err := db.Settings().GetLatest(ctx, api.SettingsSubject{User: &user.ID}); err != nil {
				t.Error(err)
			} else if settings != nil {
				t.Errorf("got settings %+v, want nil", settings)
			}
			// Settings authored by user still exist but have nil author.
			if settings, err := db.Settings().GetLatest(ctx, api.SettingsSubject{User: &otherUser.ID}); err != nil {
				t.Fatal(err)
			} else if settings.AuthorUserID != nil {
				t.Errorf("got author %v, want nil", *settings.AuthorUserID)
			}

			// User's external services no longer exist
			ess, err := db.ExternalServices().List(ctx, ExternalServicesListOptions{
				NamespaceUserID: user.ID,
			})
			if err != nil {
				t.Fatal(err)
			}
			if len(ess) > 0 {
				t.Errorf("got %d external services, want 0", len(ess))
			}

			// Can't delete already-deleted user.
			err = db.Users().Delete(ctx, user.ID)
			if !errcode.IsNotFound(err) {
				t.Errorf("got error %v, want ErrUserNotFound", err)
			}

			// Check event logs
			eventLogs, err := db.EventLogs().ListAll(ctx, EventLogsListOptions{})
			if err != nil {
				t.Fatal(err)
			}
			if len(eventLogs) != 1 {
				t.Fatal("Expected 1 event log")
			}
			eventLog := eventLogs[0]
			if hard {
				// Event logs should now be anonymous
				if *eventLog.UserID != 0 {
					t.Error("After hard delete user id should be 0")
				}
				if len(eventLog.AnonymousUserID) == 0 {
					t.Error("After hard anonymous user id should not be blank")
				}
			} else {
				// Event logs are unchanged
				if *eventLog.UserID != user.ID {
					t.Error("After soft delete user id should be non zero")
				}
				if len(eventLog.AnonymousUserID) != 0 {
					t.Error("After soft delete anonymous user id should be blank")
				}
			}
		})
	}
}

func TestUsers_HasTag(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	var id int32
	if err := db.QueryRowContext(ctx, "INSERT INTO users (username, tags) VALUES ('karim', '{\"foo\", \"bar\"}') RETURNING id").Scan(&id); err != nil {
		t.Fatal(err)
	}

	// lookup existing tag
	ok, err := db.Users().HasTag(ctx, id, "foo")
	if err != nil {
		t.Fatal(err)
	}
	if !ok {
		t.Fatal("expected tag to be found")
	}

	// lookup non-existing tag
	ok, err = db.Users().HasTag(ctx, id, "baz")
	if err != nil {
		t.Fatal(err)
	}
	if ok {
		t.Fatal("expected tag to be not found")
	}

	// lookup non-existing user
	ok, err = db.Users().HasTag(ctx, id+1, "bar")
	if err == nil || ok {
		t.Fatal("expected user to be not found")
	}
}

func TestUsers_InvalidateSessions(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	newUsers := []NewUser{
		{
			Email:           "alice@example.com",
			Username:        "alice",
			EmailIsVerified: true,
		},
		{
			Email:           "bob@example.com",
			Username:        "bob",
			EmailIsVerified: true,
		},
	}

	for _, newUser := range newUsers {
		_, err := db.Users().Create(ctx, newUser)
		if err != nil {
			t.Fatal(err)
		}
	}

	users, err := db.Users().GetByUsernames(ctx, "alice", "bob")
	if err != nil {
		t.Fatal(err)
	}

	if len(users) != 2 {
		t.Fatalf("got %d users, but want 2", len(users))
	}
	for i := range users {
		if err := db.Users().InvalidateSessionsByID(ctx, users[i].ID); err != nil {
			t.Fatal(err)
		}
	}
}

func TestUsers_SetTag(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Create user.
	u, err := db.Users().Create(ctx, NewUser{Username: "u"})
	if err != nil {
		t.Fatal(err)
	}

	checkTags := func(t *testing.T, userID int32, wantTags []string) {
		t.Helper()
		u, err := db.Users().GetByID(ctx, userID)
		if err != nil {
			t.Fatal(err)
		}
		sort.Strings(u.Tags)
		sort.Strings(wantTags)
		if !reflect.DeepEqual(u.Tags, wantTags) {
			t.Errorf("got tags %v, want %v", u.Tags, wantTags)
		}
	}
	checkUsersWithTag := func(t *testing.T, tag string, wantUsers []int32) {
		t.Helper()
		users, err := db.Users().List(ctx, &UsersListOptions{Tag: tag})
		if err != nil {
			t.Fatal(err)
		}
		userIDs := make([]int32, len(users))
		for i, u := range users {
			userIDs[i] = u.ID
		}
		if !reflect.DeepEqual(userIDs, wantUsers) {
			t.Errorf("got user IDs %v, want %v", userIDs, wantUsers)
		}
	}

	t.Run("fails on nonexistent user", func(t *testing.T) {
		if err := db.Users().SetTag(ctx, 1234 /* doesn't exist */, "t", true); !errcode.IsNotFound(err) {
			t.Errorf("got err %v, want errcode.IsNotFound", err)
		}
		if err := db.Users().SetTag(ctx, 1234 /* doesn't exist */, "t", false); !errcode.IsNotFound(err) {
			t.Errorf("got err %v, want errcode.IsNotFound", err)
		}
	})

	t.Run("tags begins empty", func(t *testing.T) {
		checkTags(t, u.ID, []string{})
		checkUsersWithTag(t, "t1", []int32{})
	})

	t.Run("adds and removes tag", func(t *testing.T) {
		if err := db.Users().SetTag(ctx, u.ID, "t1", true); err != nil {
			t.Fatal(err)
		}
		checkTags(t, u.ID, []string{"t1"})
		checkUsersWithTag(t, "t1", []int32{u.ID})

		t.Run("deduplicates", func(t *testing.T) {
			if err := db.Users().SetTag(ctx, u.ID, "t1", true); err != nil {
				t.Fatal(err)
			}
			checkTags(t, u.ID, []string{"t1"})
		})

		if err := db.Users().SetTag(ctx, u.ID, "t2", true); err != nil {
			t.Fatal(err)
		}
		checkTags(t, u.ID, []string{"t1", "t2"})
		checkUsersWithTag(t, "t1", []int32{u.ID})
		checkUsersWithTag(t, "t2", []int32{u.ID})

		if err := db.Users().SetTag(ctx, u.ID, "t1", false); err != nil {
			t.Fatal(err)
		}
		checkTags(t, u.ID, []string{"t2"})
		checkUsersWithTag(t, "t1", []int32{})
		checkUsersWithTag(t, "t2", []int32{u.ID})

		t.Run("removing nonexistent tag is noop", func(t *testing.T) {
			if err := db.Users().SetTag(ctx, u.ID, "t1", false); err != nil {
				t.Fatal(err)
			}
			checkTags(t, u.ID, []string{"t2"})
		})

		if err := db.Users().SetTag(ctx, u.ID, "t2", false); err != nil {
			t.Fatal(err)
		}
		checkTags(t, u.ID, []string{})
		checkUsersWithTag(t, "t2", []int32{})
	})
}

func normalizeUsers(users []*types.User) []*types.User {
	for _, u := range users {
		u.CreatedAt = u.CreatedAt.Local().Round(time.Second)
		u.UpdatedAt = u.UpdatedAt.Local().Round(time.Second)
		u.InvalidatedSessionsAt = u.InvalidatedSessionsAt.Local().Round(time.Second)
	}
	return users
}
