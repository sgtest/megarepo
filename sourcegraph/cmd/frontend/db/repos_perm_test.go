package db

import (
	"context"
	"net/url"
	"testing"

	"github.com/keegancsmith/sqlf"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/authz"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/db/dbtesting"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc"
)

func Benchmark_authzFilter(b *testing.B) {
	user := &types.User{
		ID:        42,
		Username:  "john.doe",
		SiteAdmin: false,
	}

	Mocks.Users.GetByCurrentAuthUser = func(ctx context.Context) (*types.User, error) {
		return user, nil
	}
	defer func() { Mocks.Users.GetByCurrentAuthUser = nil }()

	providers := []authz.Provider{
		func() authz.Provider {
			baseURL, _ := url.Parse("http://fake.provider")
			codeHost := extsvc.NewCodeHost(baseURL, "fake")
			return &fakeProvider{
				codeHost: codeHost,
				extAcct: &extsvc.ExternalAccount{
					UserID: user.ID,
					ExternalAccountSpec: extsvc.ExternalAccountSpec{
						ServiceType: codeHost.ServiceType,
						ServiceID:   codeHost.ServiceID,
						AccountID:   "42_ext",
					},
					ExternalAccountData: extsvc.ExternalAccountData{AccountData: nil},
				},
			}
		}(),
		func() authz.Provider {
			baseURL, _ := url.Parse("https://github.com")
			codeHost := extsvc.NewCodeHost(baseURL, "github")
			return &fakeProvider{
				codeHost: codeHost,
				extAcct: &extsvc.ExternalAccount{
					UserID: user.ID,
					ExternalAccountSpec: extsvc.ExternalAccountSpec{
						ServiceType: codeHost.ServiceType,
						ServiceID:   codeHost.ServiceID,
						AccountID:   "42_ext",
					},
					ExternalAccountData: extsvc.ExternalAccountData{AccountData: nil},
				},
			}
		}(),
	}

	{
		authzAllowByDefault, providers := authz.GetProviders()
		defer authz.SetProviders(authzAllowByDefault, providers)
	}

	authz.SetProviders(false, providers)

	serviceIDs := make([]string, 0, len(providers))
	for _, p := range providers {
		serviceIDs = append(serviceIDs, p.ServiceID())
	}

	rs := make([]types.Repo, 30000)
	for i := range rs {
		id := i + 1
		serviceID := serviceIDs[i%len(serviceIDs)]
		rs[i] = types.Repo{
			ID:           api.RepoID(id),
			ExternalRepo: api.ExternalRepoSpec{ServiceID: serviceID},
		}
	}

	repos := make([][]*types.Repo, b.N)
	for i := range repos {
		repos[i] = make([]*types.Repo, len(rs))
		for j := range repos[i] {
			repos[i][j] = &rs[j]
		}
	}

	ctx := context.Background()

	Mocks.ExternalAccounts.List = func(opt ExternalAccountsListOptions) (
		accts []*extsvc.ExternalAccount,
		err error,
	) {
		for _, p := range providers {
			acct, _ := p.FetchAccount(ctx, user, nil)
			accts = append(accts, acct)
		}
		return accts, nil
	}
	defer func() { Mocks.ExternalAccounts.List = nil }()

	b.ReportAllocs()
	b.ResetTimer()

	for i := 0; i < b.N; i++ {
		_, err := authzFilter(ctx, repos[i], authz.Read)
		if err != nil {
			b.Fatal(err)
		}
	}
}

type fakeProvider struct {
	codeHost *extsvc.CodeHost
	extAcct  *extsvc.ExternalAccount
}

func (f fakeProvider) RepoPerms(
	ctx context.Context,
	userAccount *extsvc.ExternalAccount,
	repos []*types.Repo,
) ([]authz.RepoPerms, error) {
	authorized := make([]authz.RepoPerms, 0, len(repos))
	for _, repo := range repos {
		authorized = append(authorized, authz.RepoPerms{
			Repo:  repo,
			Perms: authz.Read,
		})
	}
	return authorized, nil
}

func (f fakeProvider) FetchAccount(
	ctx context.Context,
	user *types.User,
	current []*extsvc.ExternalAccount,
) (mine *extsvc.ExternalAccount, err error) {
	return f.extAcct, nil
}

func (f fakeProvider) ServiceType() string           { return f.codeHost.ServiceType }
func (f fakeProvider) ServiceID() string             { return f.codeHost.ServiceID }
func (f fakeProvider) Validate() (problems []string) { return nil }

// 🚨 SECURITY: test necessary to ensure security
func Test_getBySQL_permissionsCheck(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	defer func() { MockAuthzFilter = nil }()

	ctx := dbtesting.TestContext(t)

	allRepos := mustCreate(ctx, t,
		&types.Repo{

			Name: "r0",
			ExternalRepo: api.ExternalRepoSpec{
				ID:          "a0",
				ServiceType: "b0",
				ServiceID:   "c0",
			}},

		&types.Repo{

			Name: "r1",
			ExternalRepo: api.ExternalRepoSpec{
				ID:          "a1",
				ServiceType: "b1",
				ServiceID:   "c1",
			}},
	)
	{
		calledFilter := false
		MockAuthzFilter = func(ctx context.Context, repos []*types.Repo, p authz.Perms) ([]*types.Repo, error) {
			calledFilter = true
			return repos, nil
		}

		gotRepos, err := Repos.getBySQL(ctx, sqlf.Sprintf("true"))
		if err != nil {
			t.Fatal(err)
		}
		if !jsonEqual(t, gotRepos, allRepos) {
			t.Errorf("got %v, want %v", gotRepos, allRepos)
		}
		if !calledFilter {
			t.Error("did not call authzFilter (SECURITY)")
		}
	}
	{
		calledFilter := false
		MockAuthzFilter = func(ctx context.Context, repos []*types.Repo, p authz.Perms) ([]*types.Repo, error) {
			calledFilter = true
			return nil, nil
		}

		gotRepos, err := Repos.getBySQL(ctx, sqlf.Sprintf("true"))
		if err != nil {
			t.Fatal(err)
		}
		if !jsonEqual(t, gotRepos, nil) {
			t.Errorf("got %v, want %v", gotRepos, nil)
		}
		if !calledFilter {
			t.Error("did not call authzFilter (SECURITY)")
		}
	}
	{
		calledFilter := false
		filteredRepos := allRepos[0:1]
		MockAuthzFilter = func(ctx context.Context, repos []*types.Repo, p authz.Perms) ([]*types.Repo, error) {
			calledFilter = true
			return filteredRepos, nil
		}

		gotRepos, err := Repos.getBySQL(ctx, sqlf.Sprintf("true"))
		if err != nil {
			t.Fatal(err)
		}
		if !jsonEqual(t, gotRepos, filteredRepos) {
			t.Errorf("got %v, want %v", gotRepos, filteredRepos)
		}
		if !calledFilter {
			t.Error("did not call authzFilter (SECURITY)")
		}
	}
}
