package db

import (
	"context"
	"fmt"
	"reflect"
	"testing"
	"time"

	dbtesting "github.com/sourcegraph/sourcegraph/cmd/frontend/db/testing"
	"github.com/sourcegraph/sourcegraph/pkg/dbconn"
	"github.com/sourcegraph/sourcegraph/pkg/errcode"
)

func TestUserEmails_Get(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	ctx := dbtesting.TestContext(t)

	user, err := Users.Create(ctx, NewUser{
		Email:                 "a@example.com",
		Username:              "u2",
		Password:              "pw",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}
	if err := UserEmails.Add(ctx, user.ID, "b@example.com", nil); err != nil {
		t.Fatal(err)
	}

	emailA, verifiedA, err := UserEmails.Get(ctx, user.ID, "A@EXAMPLE.com")
	if err != nil {
		t.Fatal(err)
	}
	if want := "a@example.com"; emailA != want {
		t.Errorf("got email %q, want %q", emailA, want)
	}
	if verifiedA {
		t.Error("want verified == false")
	}

	emailB, verifiedB, err := UserEmails.Get(ctx, user.ID, "B@EXAMPLE.com")
	if err != nil {
		t.Fatal(err)
	}
	if want := "b@example.com"; emailB != want {
		t.Errorf("got email %q, want %q", emailB, want)
	}
	if verifiedB {
		t.Error("want verified == false")
	}

	if _, _, err := UserEmails.Get(ctx, user.ID, "doesntexist@example.com"); !errcode.IsNotFound(err) {
		t.Errorf("got %v, want IsNotFound", err)
	}
}

func TestUserEmails_GetPrimary(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	ctx := dbtesting.TestContext(t)

	user, err := Users.Create(ctx, NewUser{
		Email:                 "a@example.com",
		Username:              "u2",
		Password:              "pw",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}

	checkPrimaryEmail := func(t *testing.T, wantEmail string, wantVerified bool) {
		t.Helper()
		email, verified, err := UserEmails.GetPrimaryEmail(ctx, user.ID)
		if err != nil {
			t.Fatal(err)
		}
		if email != wantEmail {
			t.Errorf("got email %q, want %q", email, wantEmail)
		}
		if verified != wantVerified {
			t.Errorf("got verified %v, want %v", verified, wantVerified)
		}
	}

	checkPrimaryEmail(t, "a@example.com", false)

	if err := UserEmails.Add(ctx, user.ID, "b1@example.com", nil); err != nil {
		t.Fatal(err)
	}
	checkPrimaryEmail(t, "a@example.com", false)

	if err := UserEmails.Add(ctx, user.ID, "b2@example.com", nil); err != nil {
		t.Fatal(err)
	}
	checkPrimaryEmail(t, "a@example.com", false)

	if err := UserEmails.SetVerified(ctx, user.ID, "b1@example.com", true); err != nil {
		t.Fatal(err)
	}
	checkPrimaryEmail(t, "b1@example.com", true)

	if err := UserEmails.SetVerified(ctx, user.ID, "b2@example.com", true); err != nil {
		t.Fatal(err)
	}
	checkPrimaryEmail(t, "b1@example.com", true)
}

func TestUserEmails_ListByUser(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	ctx := dbtesting.TestContext(t)

	user, err := Users.Create(ctx, NewUser{
		Email:                 "a@example.com",
		Username:              "u2",
		Password:              "pw",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}

	testTime := time.Now().Round(time.Second).UTC()
	if _, err := dbconn.Global.ExecContext(ctx,
		`INSERT INTO user_emails(user_id, email, verification_code, verified_at) VALUES($1, $2, $3, $4)`,
		user.ID, "b@example.com", "c2", testTime); err != nil {
		t.Fatal(err)
	}

	userEmails, err := UserEmails.ListByUser(ctx, user.ID)
	if err != nil {
		t.Fatal(err)
	}
	normalizeUserEmails(userEmails)
	if want := []*UserEmail{
		{UserID: user.ID, Email: "a@example.com", VerificationCode: strptr("c")},
		{UserID: user.ID, Email: "b@example.com", VerificationCode: strptr("c2"), VerifiedAt: &testTime},
	}; !reflect.DeepEqual(userEmails, want) {
		t.Errorf("got  %s\n\nwant %s", toJSON(userEmails), toJSON(want))
	}
}

func normalizeUserEmails(userEmails []*UserEmail) {
	for _, v := range userEmails {
		v.CreatedAt = time.Time{}
		if v.VerifiedAt != nil {
			tmp := v.VerifiedAt.Round(time.Second).UTC()
			v.VerifiedAt = &tmp
		}
	}
}

func TestUserEmails_Add_Remove(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	ctx := dbtesting.TestContext(t)

	const emailA = "a@example.com"
	const emailB = "b@example.com"
	user, err := Users.Create(ctx, NewUser{
		Email:                 emailA,
		Username:              "u2",
		Password:              "pw",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}

	if err := UserEmails.Add(ctx, user.ID, emailB, nil); err != nil {
		t.Fatal(err)
	}
	if verified, err := isUserEmailVerified(ctx, user.ID, emailB); err != nil {
		t.Fatal(err)
	} else if want := false; verified != want {
		t.Fatalf("got verified %v, want %v", verified, want)
	}
	if emails, err := UserEmails.ListByUser(ctx, user.ID); err != nil {
		t.Fatal(err)
	} else if want := 2; len(emails) != want {
		t.Errorf("got %d emails, want %d", len(emails), want)
	}

	if err := UserEmails.Add(ctx, user.ID, emailB, nil); err == nil {
		t.Fatal("got err == nil for Add on existing email")
	}
	if err := UserEmails.Add(ctx, 12345 /* bad user ID */, "foo@example.com", nil); err == nil {
		t.Fatal("got err == nil for Add on bad user ID")
	}
	if emails, err := UserEmails.ListByUser(ctx, user.ID); err != nil {
		t.Fatal(err)
	} else if want := 2; len(emails) != want {
		t.Errorf("got %d emails, want %d", len(emails), want)
	}

	// Remove.
	if err := UserEmails.Remove(ctx, user.ID, emailB); err != nil {
		t.Fatal(err)
	}
	if emails, err := UserEmails.ListByUser(ctx, user.ID); err != nil {
		t.Fatal(err)
	} else if want := 1; len(emails) != want {
		t.Errorf("got %d emails (after removing), want %d", len(emails), want)
	}

	if err := UserEmails.Remove(ctx, user.ID, "foo@example.com"); err == nil {
		t.Fatal("got err == nil for Remove on nonexistent email")
	}
	if err := UserEmails.Remove(ctx, 12345 /* bad user ID */, "foo@example.com"); err == nil {
		t.Fatal("got err == nil for Remove on bad user ID")
	}
}

func TestUserEmails_SetVerified(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	ctx := dbtesting.TestContext(t)

	const email = "a@example.com"
	user, err := Users.Create(ctx, NewUser{
		Email:                 email,
		Username:              "u2",
		Password:              "pw",
		EmailVerificationCode: "c",
	})
	if err != nil {
		t.Fatal(err)
	}

	if verified, err := isUserEmailVerified(ctx, user.ID, email); err != nil {
		t.Fatal(err)
	} else if want := false; verified != want {
		t.Fatalf("before SetVerified, got verified %v, want %v", verified, want)
	}

	if err := UserEmails.SetVerified(ctx, user.ID, email, true); err != nil {
		t.Fatal(err)
	}
	if verified, err := isUserEmailVerified(ctx, user.ID, email); err != nil {
		t.Fatal(err)
	} else if want := true; verified != want {
		t.Fatalf("after SetVerified true, got verified %v, want %v", verified, want)
	}

	if err := UserEmails.SetVerified(ctx, user.ID, email, false); err != nil {
		t.Fatal(err)
	}
	if verified, err := isUserEmailVerified(ctx, user.ID, email); err != nil {
		t.Fatal(err)
	} else if want := false; verified != want {
		t.Fatalf("after SetVerified false, got verified %v, want %v", verified, want)
	}

	if err := UserEmails.SetVerified(ctx, user.ID, "otheremail@example.com", false); err == nil {
		t.Fatal("got err == nil for SetVerified on bad email")
	}
}

func isUserEmailVerified(ctx context.Context, userID int32, email string) (bool, error) {
	userEmails, err := UserEmails.ListByUser(ctx, userID)
	if err != nil {
		return false, err
	}
	for _, v := range userEmails {
		if v.Email == email {
			return v.VerifiedAt != nil, nil
		}
	}
	return false, fmt.Errorf("email not found: %s", email)
}

func strptr(s string) *string {
	return &s
}
