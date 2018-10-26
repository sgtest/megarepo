package db

import (
	"testing"

	dbtesting "github.com/sourcegraph/sourcegraph/cmd/frontend/db/testing"
	"github.com/sourcegraph/sourcegraph/pkg/api"
)

func TestSettings_ListAll(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	ctx := dbtesting.TestContext(t)

	user1, err := Users.Create(ctx, NewUser{Username: "u1"})
	if err != nil {
		t.Fatal(err)
	}
	user2, err := Users.Create(ctx, NewUser{Username: "u2"})
	if err != nil {
		t.Fatal(err)
	}

	// Try creating both with non-nil author and nil author.
	if _, err := Settings.CreateIfUpToDate(ctx, api.ConfigurationSubject{User: &user1.ID}, nil, &user1.ID, ""); err != nil {
		t.Fatal(err)
	}
	if _, err := Settings.CreateIfUpToDate(ctx, api.ConfigurationSubject{User: &user2.ID}, nil, nil, ""); err != nil {
		t.Fatal(err)
	}

	settings, err := Settings.ListAll(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if want := 2; len(settings) != want {
		t.Errorf("got %d settings, want %d", len(settings), want)
	}
}
