package perforce

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"strings"
	"testing"

	"github.com/google/go-cmp/cmp"
	jsoniter "github.com/json-iterator/go"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/perforce"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestProvider_FetchAccount(t *testing.T) {
	ctx := context.Background()
	user := &types.User{
		ID:       1,
		Username: "alice",
	}

	execer := p4ExecFunc(func(ctx context.Context, host, user, password string, args ...string) (io.ReadCloser, http.Header, error) {
		data := `
alice <alice@example.com> (Alice) accessed 2020/12/04
cindy <cindy@example.com> (Cindy) accessed 2020/12/04
`
		return io.NopCloser(strings.NewReader(data)), nil, nil
	})

	t.Run("no matching account", func(t *testing.T) {
		p := NewTestProvider("", "ssl:111.222.333.444:1666", "admin", "password", execer)
		got, err := p.FetchAccount(ctx, user, nil, []string{"bob@example.com"})
		if err != nil {
			t.Fatal(err)
		}

		if got != nil {
			t.Fatalf("Want nil but got %v", got)
		}
	})

	t.Run("found matching account", func(t *testing.T) {
		p := NewTestProvider("", "ssl:111.222.333.444:1666", "admin", "password", execer)
		got, err := p.FetchAccount(ctx, user, nil, []string{"alice@example.com"})
		if err != nil {
			t.Fatal(err)
		}

		accountData, err := jsoniter.Marshal(
			perforce.AccountData{
				Username: "alice",
				Email:    "alice@example.com",
			},
		)
		if err != nil {
			t.Fatal(err)
		}

		want := &extsvc.Account{
			UserID: user.ID,
			AccountSpec: extsvc.AccountSpec{
				ServiceType: p.codeHost.ServiceType,
				ServiceID:   p.codeHost.ServiceID,
				AccountID:   "alice@example.com",
			},
			AccountData: extsvc.AccountData{
				Data: (*json.RawMessage)(&accountData),
			},
		}
		if diff := cmp.Diff(want, got); diff != "" {
			t.Fatalf("Mismatch (-want got):\n%s", diff)
		}
	})
}

func TestProvider_FetchUserPerms(t *testing.T) {
	ctx := context.Background()

	t.Run("nil account", func(t *testing.T) {
		p := NewProvider("", "ssl:111.222.333.444:1666", "admin", "password")
		_, err := p.FetchUserPerms(ctx, nil)
		want := "no account provided"
		got := fmt.Sprintf("%v", err)
		if got != want {
			t.Fatalf("err: want %q but got %q", want, got)
		}
	})

	t.Run("not the code host of the account", func(t *testing.T) {
		p := NewProvider("", "ssl:111.222.333.444:1666", "admin", "password")
		_, err := p.FetchUserPerms(context.Background(),
			&extsvc.Account{
				AccountSpec: extsvc.AccountSpec{
					ServiceType: extsvc.TypeGitLab,
					ServiceID:   "https://gitlab.com/",
				},
			},
		)
		want := `not a code host of the account: want "https://gitlab.com/" but have "ssl:111.222.333.444:1666"`
		got := fmt.Sprintf("%v", err)
		if got != want {
			t.Fatalf("err: want %q but got %q", want, got)
		}
	})

	t.Run("no user found in account data", func(t *testing.T) {
		p := NewProvider("", "ssl:111.222.333.444:1666", "admin", "password")
		_, err := p.FetchUserPerms(ctx,
			&extsvc.Account{
				AccountSpec: extsvc.AccountSpec{
					ServiceType: extsvc.TypePerforce,
					ServiceID:   "ssl:111.222.333.444:1666",
				},
				AccountData: extsvc.AccountData{},
			},
		)
		want := `no user found in the external account data`
		got := fmt.Sprintf("%v", err)
		if got != want {
			t.Fatalf("err: want %q but got %q", want, got)
		}
	})

	accountData, err := jsoniter.Marshal(
		perforce.AccountData{
			Username: "alice",
			Email:    "alice@example.com",
		},
	)
	if err != nil {
		t.Fatal(err)
	}

	tests := []struct {
		name      string
		response  string
		wantPerms *authz.ExternalUserPermissions
	}{
		{
			name: "include only",
			response: `
list user alice * //Sourcegraph/Security/... ## "list" can't grant read access
read user alice * //Sourcegraph/Engineering/...
owner user alice * //Sourcegraph/Engineering/Backend/...
open user alice * //Sourcegraph/Engineering/Frontend/...
review user alice * //Sourcegraph/Handbook/...
`,
			wantPerms: &authz.ExternalUserPermissions{
				IncludePrefixes: []extsvc.RepoID{
					"//Sourcegraph/Engineering/",
					"//Sourcegraph/Engineering/Backend/",
					"//Sourcegraph/Engineering/Frontend/",
					"//Sourcegraph/Handbook/",
				},
			},
		},
		{
			name: "exclude only",
			response: `
list user alice * -//Sourcegraph/Security/...
read user alice * -//Sourcegraph/Engineering/...
owner user alice * -//Sourcegraph/Engineering/Backend/...
open user alice * -//Sourcegraph/Engineering/Frontend/...
review user alice * -//Sourcegraph/Handbook/...
`,
			wantPerms: &authz.ExternalUserPermissions{},
		},
		{
			name: "include and exclude",
			response: `
read user alice * //Sourcegraph/Security/...
read user alice * //Sourcegraph/Engineering/...
owner user alice * //Sourcegraph/Engineering/Backend/...
open user alice * //Sourcegraph/Engineering/Frontend/...
review user alice * //Sourcegraph/Handbook/...

list user alice * -//Sourcegraph/Security/...                        ## "list" can revoke read access
=read user alice * -//Sourcegraph/Engineering/Frontend/...           ## exact match of a previous include
open user alice * -//Sourcegraph/Engineering/Backend/Credentials/... ## sub-match of a previous include
`,
			wantPerms: &authz.ExternalUserPermissions{
				IncludePrefixes: []extsvc.RepoID{
					"//Sourcegraph/Engineering/",
					"//Sourcegraph/Engineering/Backend/",
					"//Sourcegraph/Engineering/Frontend/",
					"//Sourcegraph/Handbook/",
				},
				ExcludePrefixes: []extsvc.RepoID{
					"//Sourcegraph/Engineering/Frontend/",
					"//Sourcegraph/Engineering/Backend/Credentials/",
				},
			},
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			execer := p4ExecFunc(func(ctx context.Context, host, user, password string, args ...string) (io.ReadCloser, http.Header, error) {
				return io.NopCloser(strings.NewReader(test.response)), nil, nil
			})

			p := NewTestProvider("", "ssl:111.222.333.444:1666", "admin", "password", execer)
			got, err := p.FetchUserPerms(ctx,
				&extsvc.Account{
					AccountSpec: extsvc.AccountSpec{
						ServiceType: extsvc.TypePerforce,
						ServiceID:   "ssl:111.222.333.444:1666",
					},
					AccountData: extsvc.AccountData{
						Data: (*json.RawMessage)(&accountData),
					},
				},
			)
			if err != nil {
				t.Fatal(err)
			}

			if diff := cmp.Diff(test.wantPerms, got); diff != "" {
				t.Fatalf("Mismatch (-want +got):\n%s", diff)
			}
		})
	}
}

func TestProvider_FetchRepoPerms(t *testing.T) {
	ctx := context.Background()

	t.Run("nil repository", func(t *testing.T) {
		p := NewProvider("", "ssl:111.222.333.444:1666", "admin", "password")
		_, err := p.FetchRepoPerms(ctx, nil)
		want := "no repository provided"
		got := fmt.Sprintf("%v", err)
		if got != want {
			t.Fatalf("err: want %q but got %q", want, got)
		}
	})

	t.Run("not the code host of the repository", func(t *testing.T) {
		p := NewProvider("", "ssl:111.222.333.444:1666", "admin", "password")
		_, err := p.FetchRepoPerms(ctx,
			&extsvc.Repository{
				URI: "gitlab.com/user/repo",
				ExternalRepoSpec: api.ExternalRepoSpec{
					ServiceType: extsvc.TypeGitLab,
					ServiceID:   "https://gitlab.com/",
				},
			},
		)
		want := `not a code host of the repository: want "https://gitlab.com/" but have "ssl:111.222.333.444:1666"`
		got := fmt.Sprintf("%v", err)
		if got != want {
			t.Fatalf("err: want %q but got %q", want, got)
		}
	})
	execer := p4ExecFunc(func(ctx context.Context, host, user, password string, args ...string) (io.ReadCloser, http.Header, error) {
		var data string

		switch args[0] {

		case "protects":
			data = `
## The actual depot prefix does not matter, the "-" sign does
list user * * -//...
write user alice * //Sourcegraph/...
write user bob * //Sourcegraph/...
admin group Backend * //Sourcegraph/...   ## includes "alice" and "cindy"

admin group Frontend * -//Sourcegraph/... ## excludes "bob", "david" and "frank"
read user cindy * -//Sourcegraph/...

list user david * //Sourcegraph/...       ## "list" can't grant read access
`
		case "users":
			data = `
alice <alice@example.com> (Alice) accessed 2020/12/04
bob <bob@example.com> (Bob) accessed 2020/12/04
cindy <cindy@example.com> (Cindy) accessed 2020/12/04
david <david@example.com> (David) accessed 2020/12/04
frank <frank@example.com> (Frank) accessed 2020/12/04
`
		case "group":
			switch args[2] {
			case "Backend":
				data = `
Users:
	alice
	cindy
`
			case "Frontend":
				data = `
Users:
	bob
	david
	frank
`
			}
		}

		return io.NopCloser(strings.NewReader(data)), nil, nil
	})

	p := NewTestProvider("", "ssl:111.222.333.444:1666", "admin", "password", execer)
	got, err := p.FetchRepoPerms(ctx,
		&extsvc.Repository{
			URI: "gitlab.com/user/repo",
			ExternalRepoSpec: api.ExternalRepoSpec{
				ServiceType: extsvc.TypePerforce,
				ServiceID:   "ssl:111.222.333.444:1666",
			},
		},
	)
	if err != nil {
		t.Fatal(err)
	}

	want := []extsvc.AccountID{"alice@example.com"}
	if diff := cmp.Diff(want, got); diff != "" {
		t.Fatalf("Mismatch (-want +got):\n%s", diff)
	}
}

func TestScanAllUsers(t *testing.T) {
	ctx := context.Background()
	f, err := os.Open("testdata/sample-protects.txt")
	if err != nil {
		t.Fatal(err)
	}

	data, err := io.ReadAll(f)
	if err != nil {
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}

	rc := io.NopCloser(bytes.NewReader(data))

	execer := p4ExecFunc(func(ctx context.Context, host, user, password string, args ...string) (io.ReadCloser, http.Header, error) {
		return rc, nil, nil
	})

	p := NewTestProvider("", "ssl:111.222.333.444:1666", "admin", "password", execer)
	p.cachedGroupMembers = map[string][]string{
		"dev": {"user1", "user2"},
	}
	p.cachedAllUserEmails = map[string]string{
		"user1": "user1@example.com",
		"user2": "user2@example.com",
	}

	users, err := p.scanAllUsers(ctx, rc)
	if err != nil {
		t.Fatal(err)
	}
	want := map[string]struct{}{
		"user1": {},
		"user2": {},
	}
	if diff := cmp.Diff(want, users); diff != "" {
		t.Fatal(diff)
	}
}

func NewTestProvider(urn, host, user, password string, execer p4Execer) *Provider {
	p := NewProvider(urn, host, user, password)
	p.p4Execer = execer
	return p
}

type p4ExecFunc func(ctx context.Context, host, user, password string, args ...string) (io.ReadCloser, http.Header, error)

func (p p4ExecFunc) P4Exec(ctx context.Context, host, user, password string, args ...string) (io.ReadCloser, http.Header, error) {
	return p(ctx, host, user, password, args...)
}
