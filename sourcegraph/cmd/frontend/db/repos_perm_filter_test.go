package db

import (
	"context"
	"reflect"
	"testing"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/authz"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/actor"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc"
)

type authzFilter_Test struct {
	description string

	authzAllowByDefault bool
	authzProviders      []authz.Provider

	calls []authzFilter_call
}

type authzFilter_call struct {
	description string

	user         *types.User
	userAccounts []*extsvc.ExternalAccount

	repos []*types.Repo
	perm  authz.Perm

	expFilteredRepos []*types.Repo
}

func (r authzFilter_Test) run(t *testing.T) {
	t.Logf("Test case %q", r.description)
	authz.SetProviders(r.authzAllowByDefault, r.authzProviders)

	for _, c := range r.calls {
		t.Logf("Call %q", c.description)

		Mocks.Users.GetByCurrentAuthUser = func(ctx context.Context) (*types.User, error) {
			if c.user == nil {
				t.Fatal("unexpected call to GetByCurrentAuthUser when request is not authenticated")
			}
			actr := actor.FromContext(ctx)
			if !actr.IsAuthenticated() || actr.UID != c.user.ID {
				t.Fatalf("unexpected actor in request context: %+v", actr)
			}
			return c.user, nil
		}

		ctx := context.Background()
		if c.user != nil {
			ctx = actor.WithActor(ctx, &actor.Actor{UID: c.user.ID})
		}

		Mocks.ExternalAccounts.AssociateUserAndSave = func(userID int32, spec extsvc.ExternalAccountSpec, data extsvc.ExternalAccountData) error { return nil }
		Mocks.ExternalAccounts.List = func(ExternalAccountsListOptions) ([]*extsvc.ExternalAccount, error) { return c.userAccounts, nil }

		filteredRepos, err := authzFilter(ctx, c.repos, c.perm)
		if err != nil {
			t.Fatal(err)
		}
		if !reflect.DeepEqual(filteredRepos, c.expFilteredRepos) {
			a := make([]api.RepoName, len(filteredRepos))
			for i, v := range filteredRepos {
				a[i] = v.Name
			}
			e := make([]api.RepoName, len(c.expFilteredRepos))
			for i, v := range c.expFilteredRepos {
				e[i] = v.Name
			}
			t.Errorf("Expected filtered repos\n\t%v\n, but got\n\t%v", e, a)
		}
	}
}

func Test_authzFilter(t *testing.T) {
	tests := []authzFilter_Test{
		{
			description:         "1 authz provider, ext account exists",
			authzAllowByDefault: true,
			authzProviders: []authz.Provider{
				&MockAuthzProvider{
					serviceID:   "https://gitlab.mine/",
					serviceType: "gitlab",
					repos: map[api.RepoName]struct{}{
						"gitlab.mine/u1/r0":            struct{}{},
						"gitlab.mine/u2/r0":            struct{}{},
						"gitlab.mine/sharedPrivate/r0": struct{}{},
						"gitlab.mine/org/r0":           struct{}{},
					},
					perms: map[extsvc.ExternalAccount]map[api.RepoName]map[authz.Perm]bool{
						*acct(1, "gitlab", "https://gitlab.mine/", "u1"): map[api.RepoName]map[authz.Perm]bool{
							"gitlab.mine/u1/r0":            map[authz.Perm]bool{authz.Read: true},
							"gitlab.mine/sharedPrivate/r0": map[authz.Perm]bool{authz.Read: true},
							"gitlab.mine/org/r0":           map[authz.Perm]bool{authz.Read: true},
						},
						*acct(2, "gitlab", "https://gitlab.mine/", "u2"): map[api.RepoName]map[authz.Perm]bool{
							"gitlab.mine/u2/r0":            map[authz.Perm]bool{authz.Read: true},
							"gitlab.mine/sharedPrivate/r0": map[authz.Perm]bool{authz.Read: true},
							"gitlab.mine/org/r0":           map[authz.Perm]bool{authz.Read: true},
						},
						extsvc.ExternalAccount{}: map[api.RepoName]map[authz.Perm]bool{
							"gitlab.mine/org/r0": map[authz.Perm]bool{authz.Read: true},
						},
					},
				},
			},
			calls: []authzFilter_call{
				{
					description:  "u1 can read its own repo",
					user:         &types.User{ID: 1},
					userAccounts: []*extsvc.ExternalAccount{acct(1, "gitlab", "https://gitlab.mine/", "u1")},
					repos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
					},
					perm: authz.Read,
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
					},
				},
				{
					description:  "u1 not allowed to read u2's repo",
					user:         &types.User{ID: 1},
					userAccounts: []*extsvc.ExternalAccount{acct(1, "gitlab", "https://gitlab.mine/", "u1")},
					repos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/u2/r0"},
						{Name: "gitlab.mine/sharedPrivate/r0"},
						{Name: "gitlab.mine/org/r0"},
					},
					perm: authz.Read,
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/sharedPrivate/r0"},
						{Name: "gitlab.mine/org/r0"},
					},
				},
				{
					description:  "u2 not allowed to read u0's repo",
					user:         &types.User{ID: 1},
					userAccounts: []*extsvc.ExternalAccount{acct(2, "gitlab", "https://gitlab.mine/", "u2")},
					repos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/u2/r0"},
						{Name: "gitlab.mine/sharedPrivate/r0"},
						{Name: "gitlab.mine/org/r0"},
					},
					perm: authz.Read,
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab.mine/u2/r0"},
						{Name: "gitlab.mine/sharedPrivate/r0"},
						{Name: "gitlab.mine/org/r0"},
					},
				}, {
					description:  "u99 not allowed to read anyone's repo",
					user:         &types.User{ID: 1},
					userAccounts: []*extsvc.ExternalAccount{acct(99, "gitlab", "https://gitlab.mine/", "u99")},
					repos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/u2/r0"},
						{Name: "gitlab.mine/sharedPrivate/r0"},
						{Name: "gitlab.mine/org/r0"},
					},
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab.mine/org/r0"},
					},
					perm: authz.Read,
				}, {
					description:  "u99 can read unmanaged repo",
					user:         &types.User{ID: 1},
					userAccounts: []*extsvc.ExternalAccount{acct(99, "gitlab", "https://gitlab.mine/", "u99")},
					repos: []*types.Repo{
						{Name: "other.mine/r"},
					},
					expFilteredRepos: []*types.Repo{
						{Name: "other.mine/r"},
					},
					perm: authz.Read,
				}, {
					description:  "u1 can read its own, public, and unmanaged repos",
					user:         &types.User{ID: 1},
					userAccounts: []*extsvc.ExternalAccount{acct(1, "gitlab", "https://gitlab.mine/", "u1")},
					repos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/u2/r0"},
						{Name: "gitlab.mine/sharedPrivate/r0"},
						{Name: "gitlab.mine/org/r0"},
						{Name: "otherHost/r0"},
					},
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/sharedPrivate/r0"},
						{Name: "gitlab.mine/org/r0"},
						{Name: "otherHost/r0"},
					},
					perm: authz.Read,
				}, {
					description:  "authenticated but 0 accounts can read public anad unmanaged repos",
					user:         &types.User{ID: 1},
					userAccounts: nil,
					repos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/u2/r0"},
						{Name: "gitlab.mine/sharedPrivate/r0"},
						{Name: "gitlab.mine/org/r0"},
						{Name: "otherHost/r0"},
					},
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab.mine/org/r0"},
						{Name: "otherHost/r0"},
					},
					perm: authz.Read,
				}, {
					description:  "unauthenticated can read public and unmanaged repos",
					user:         nil,
					userAccounts: nil,
					repos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/u2/r0"},
						{Name: "gitlab.mine/sharedPrivate/r0"},
						{Name: "gitlab.mine/org/r0"},
						{Name: "otherHost/r0"},
					},
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab.mine/org/r0"},
						{Name: "otherHost/r0"},
					},
					perm: authz.Read,
				}, {
					description:  "admin user can read all repos",
					user:         &types.User{ID: 777, SiteAdmin: true},
					userAccounts: nil,
					repos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/u2/r0"},
						{Name: "gitlab.mine/sharedPrivate/r0"},
						{Name: "gitlab.mine/org/r0"},
						{Name: "otherHost/r0"},
					},
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/u2/r0"},
						{Name: "gitlab.mine/sharedPrivate/r0"},
						{Name: "gitlab.mine/org/r0"},
						{Name: "otherHost/r0"},
					},
					perm: authz.Read,
				},
			},
		},
		{
			description:         "2 authz providers, ext accounts exist",
			authzAllowByDefault: true,
			authzProviders: []authz.Provider{
				&MockAuthzProvider{
					serviceID:   "https://gitlab0.mine/",
					serviceType: "gitlab",
					repos: map[api.RepoName]struct{}{
						"gitlab0.mine/u1/r0":  struct{}{},
						"gitlab0.mine/u2/r0":  struct{}{},
						"gitlab0.mine/org/r0": struct{}{},
					},
					perms: map[extsvc.ExternalAccount]map[api.RepoName]map[authz.Perm]bool{
						*acct(1, "gitlab", "https://gitlab0.mine/", "u1"): map[api.RepoName]map[authz.Perm]bool{
							"gitlab0.mine/u1/r0":  map[authz.Perm]bool{authz.Read: true},
							"gitlab0.mine/org/r0": map[authz.Perm]bool{authz.Read: true},
						},
						*acct(2, "gitlab", "https://gitlab0.mine/", "u2"): map[api.RepoName]map[authz.Perm]bool{
							"gitlab0.mine/u1/r0":  map[authz.Perm]bool{},
							"gitlab0.mine/u2/r0":  map[authz.Perm]bool{authz.Read: true},
							"gitlab0.mine/org/r0": map[authz.Perm]bool{authz.Read: true},
						},
					},
				},
				&MockAuthzProvider{
					serviceID:   "https://gitlab1.mine/",
					serviceType: "gitlab",
					repos: map[api.RepoName]struct{}{
						"gitlab1.mine/u1/r0":  struct{}{},
						"gitlab1.mine/u2/r0":  struct{}{},
						"gitlab1.mine/org/r0": struct{}{},
					},
					perms: map[extsvc.ExternalAccount]map[api.RepoName]map[authz.Perm]bool{
						*acct(1, "gitlab", "https://gitlab1.mine/", "u1"): map[api.RepoName]map[authz.Perm]bool{
							"gitlab1.mine/u1/r0":  map[authz.Perm]bool{authz.Read: true},
							"gitlab1.mine/org/r0": map[authz.Perm]bool{authz.Read: true},
						},
						*acct(2, "gitlab", "https://gitlab1.mine/", "u2"): map[api.RepoName]map[authz.Perm]bool{
							"gitlab1.mine/u2/r0":  map[authz.Perm]bool{authz.Read: true},
							"gitlab1.mine/org/r0": map[authz.Perm]bool{authz.Read: true},
						},
					},
				},
			},
			calls: []authzFilter_call{
				{
					description: "u1 can read its own repos, but not others'",
					user:        &types.User{ID: 1},
					userAccounts: []*extsvc.ExternalAccount{
						acct(1, "gitlab", "https://gitlab0.mine/", "u1"),
						acct(1, "gitlab", "https://gitlab1.mine/", "u1"),
					},
					repos: []*types.Repo{
						{Name: "gitlab0.mine/u1/r0"},
						{Name: "gitlab0.mine/u2/r0"},
						{Name: "gitlab0.mine/org/r0"},
						{Name: "gitlab1.mine/u1/r0"},
						{Name: "gitlab1.mine/u2/r0"},
						{Name: "gitlab1.mine/org/r0"},
						{Name: "gitlab2.mine/u2/r0"},
						{Name: "otherHost/r0"},
					},
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab0.mine/u1/r0"},
						{Name: "gitlab0.mine/org/r0"},
						{Name: "gitlab1.mine/u1/r0"},
						{Name: "gitlab1.mine/org/r0"},
						{Name: "gitlab2.mine/u2/r0"},
						{Name: "otherHost/r0"},
					},
					perm: authz.Read,
				},
				{
					description: "u1 with external account on one instance, can't read repos from the other'",
					user:        &types.User{ID: 1},
					userAccounts: []*extsvc.ExternalAccount{
						acct(1, "gitlab", "https://gitlab1.mine/", "u1"),
					},
					repos: []*types.Repo{
						{Name: "gitlab0.mine/u1/r0"},
						{Name: "gitlab0.mine/u2/r0"},
						{Name: "gitlab0.mine/org/r0"},
						{Name: "gitlab1.mine/u1/r0"},
						{Name: "gitlab1.mine/u2/r0"},
						{Name: "gitlab1.mine/org/r0"},
						{Name: "gitlab2.mine/u2/r0"},
						{Name: "otherHost/r0"},
					},
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab1.mine/u1/r0"},
						{Name: "gitlab1.mine/org/r0"},
						{Name: "gitlab2.mine/u2/r0"},
						{Name: "otherHost/r0"},
					},
					perm: authz.Read,
				},
			},
		},
		{
			description:         "2 authz providers, ext account exists, authzAllowByDefault=false",
			authzAllowByDefault: false,
			authzProviders: []authz.Provider{
				&MockAuthzProvider{
					serviceID:   "https://gitlab0.mine/",
					serviceType: "gitlab",
					repos: map[api.RepoName]struct{}{
						"gitlab0.mine/u1/r0":  struct{}{},
						"gitlab0.mine/u2/r0":  struct{}{},
						"gitlab0.mine/org/r0": struct{}{},
					},
					perms: map[extsvc.ExternalAccount]map[api.RepoName]map[authz.Perm]bool{
						*acct(1, "gitlab", "https://gitlab0.mine/", "u1"): map[api.RepoName]map[authz.Perm]bool{
							"gitlab0.mine/u1/r0":  map[authz.Perm]bool{authz.Read: true},
							"gitlab0.mine/org/r0": map[authz.Perm]bool{authz.Read: true},
						},
						*acct(2, "gitlab", "https://gitlab0.mine/", "u2"): map[api.RepoName]map[authz.Perm]bool{
							"gitlab0.mine/u1/r0":  map[authz.Perm]bool{},
							"gitlab0.mine/u2/r0":  map[authz.Perm]bool{authz.Read: true},
							"gitlab0.mine/org/r0": map[authz.Perm]bool{authz.Read: true},
						},
					},
				},
				&MockAuthzProvider{
					serviceID:   "https://gitlab1.mine/",
					serviceType: "gitlab",
					repos: map[api.RepoName]struct{}{
						"gitlab1.mine/u1/r0":  struct{}{},
						"gitlab1.mine/u2/r0":  struct{}{},
						"gitlab1.mine/org/r0": struct{}{},
					},
					perms: map[extsvc.ExternalAccount]map[api.RepoName]map[authz.Perm]bool{
						*acct(1, "gitlab", "https://gitlab1.mine/", "u1"): map[api.RepoName]map[authz.Perm]bool{
							"gitlab1.mine/u1/r0":  map[authz.Perm]bool{authz.Read: true},
							"gitlab1.mine/org/r0": map[authz.Perm]bool{authz.Read: true},
						},
						*acct(2, "gitlab", "https://gitlab1.mine/", "u2"): map[api.RepoName]map[authz.Perm]bool{
							"gitlab1.mine/u2/r0":  map[authz.Perm]bool{authz.Read: true},
							"gitlab1.mine/org/r0": map[authz.Perm]bool{authz.Read: true},
						},
					},
				},
			},
			calls: []authzFilter_call{
				{
					description: "u1 can read its own repos, but not others'",
					user:        &types.User{ID: 1},
					userAccounts: []*extsvc.ExternalAccount{
						acct(1, "gitlab", "https://gitlab0.mine/", "u1"),
						acct(1, "gitlab", "https://gitlab1.mine/", "u1"),
					},
					repos: []*types.Repo{
						{Name: "gitlab0.mine/u1/r0"},
						{Name: "gitlab0.mine/u2/r0"},
						{Name: "gitlab0.mine/org/r0"},
						{Name: "gitlab1.mine/u1/r0"},
						{Name: "gitlab1.mine/u2/r0"},
						{Name: "gitlab1.mine/org/r0"},
						{Name: "gitlab2.mine/u2/r0"},
						{Name: "otherHost/r0"},
					},
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab0.mine/u1/r0"},
						{Name: "gitlab0.mine/org/r0"},
						{Name: "gitlab1.mine/u1/r0"},
						{Name: "gitlab1.mine/org/r0"},
					},
					perm: authz.Read,
				},
			},
		},
		{
			description:         "1 authz provider, ext account doesn't exist",
			authzAllowByDefault: true,
			authzProviders: []authz.Provider{
				&MockAuthzProvider{
					serviceID:    "https://gitlab.mine/",
					serviceType:  "gitlab",
					okServiceIDs: map[string]struct{}{"https://okta.mine/": struct{}{}},
					repos: map[api.RepoName]struct{}{
						"gitlab.mine/u1/r0":     struct{}{},
						"gitlab.mine/u2/r0":     struct{}{},
						"gitlab.mine/org/r0":    struct{}{},
						"gitlab.mine/public/r0": struct{}{},
					},
					perms: map[extsvc.ExternalAccount]map[api.RepoName]map[authz.Perm]bool{
						*acct(1, "gitlab", "https://gitlab.mine/", "u1"): map[api.RepoName]map[authz.Perm]bool{
							"gitlab.mine/u1/r0":     map[authz.Perm]bool{authz.Read: true},
							"gitlab.mine/org/r0":    map[authz.Perm]bool{authz.Read: true},
							"gitlab.mine/public/r0": map[authz.Perm]bool{authz.Read: true},
						},
						*acct(2, "gitlab", "https://gitlab.mine/", "u2"): map[api.RepoName]map[authz.Perm]bool{
							"gitlab.mine/u2/r0":     map[authz.Perm]bool{authz.Read: true},
							"gitlab.mine/org/r0":    map[authz.Perm]bool{authz.Read: true},
							"gitlab.mine/public/r0": map[authz.Perm]bool{authz.Read: true},
						},
						// entry for nil account / anonymous users
						extsvc.ExternalAccount{}: map[api.RepoName]map[authz.Perm]bool{
							"gitlab.mine/public/r0": map[authz.Perm]bool{authz.Read: true},
						},
					},
				},
			},
			calls: []authzFilter_call{
				{
					description:  "u1 has access to the right repos",
					user:         &types.User{ID: 1},
					userAccounts: []*extsvc.ExternalAccount{acct(1, "saml", "https://okta.mine/", "u1")},
					repos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/u2/r0"},
						{Name: "gitlab.mine/org/r0"},
						{Name: "gitlab.mine/public/r0"},
					},
					perm: authz.Read,
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/org/r0"},
						{Name: "gitlab.mine/public/r0"},
					},
				},
				{
					description:  "u99 has access to publice repos only",
					user:         &types.User{ID: 1},
					userAccounts: []*extsvc.ExternalAccount{acct(1, "saml", "https://okta.mine/", "u99")},
					repos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/u2/r0"},
						{Name: "gitlab.mine/org/r0"},
						{Name: "gitlab.mine/public/r0"},
					},
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab.mine/public/r0"},
					},
					perm: authz.Read,
				},
				{
					description:  "service ID does not match",
					user:         &types.User{ID: 1},
					userAccounts: []*extsvc.ExternalAccount{acct(1, "saml", "https://rando.mine/", "u1")},
					repos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/u2/r0"},
						{Name: "gitlab.mine/org/r0"},
						{Name: "gitlab.mine/public/r0"},
					},
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab.mine/public/r0"},
					},
					perm: authz.Read,
				},
				{
					description:  "unauthenticated user",
					user:         nil,
					userAccounts: nil,
					repos: []*types.Repo{
						{Name: "gitlab.mine/u1/r0"},
						{Name: "gitlab.mine/u2/r0"},
						{Name: "gitlab.mine/org/r0"},
						{Name: "gitlab.mine/public/r0"},
					},
					expFilteredRepos: []*types.Repo{
						{Name: "gitlab.mine/public/r0"},
					},
					perm: authz.Read,
				},
			},
		},
	}
	for _, test := range tests {
		test.run(t)
	}
}

func Test_authzFilter_createsNewUsers(t *testing.T) {
	associateUserAndSaveCount := make(map[int32]map[extsvc.ExternalAccountSpec]int)
	Mocks.ExternalAccounts.AssociateUserAndSave = func(userID int32, spec extsvc.ExternalAccountSpec, data extsvc.ExternalAccountData) error {
		if _, ok := associateUserAndSaveCount[userID]; !ok {
			associateUserAndSaveCount[userID] = make(map[extsvc.ExternalAccountSpec]int)
		}
		associateUserAndSaveCount[userID][spec]++
		return nil
	}
	mockUser23Accounts := []*extsvc.ExternalAccount{{
		UserID: 23,
		ExternalAccountSpec: extsvc.ExternalAccountSpec{
			ServiceType: "okta",
			ServiceID:   "https://okta.mine/",
			AccountID:   "101",
		},
	}, {
		UserID: 23,
		ExternalAccountSpec: extsvc.ExternalAccountSpec{
			ServiceType: "other",
			ServiceID:   "https://other.mine/",
			AccountID:   "99",
		},
	}}
	Mocks.ExternalAccounts.List = func(op ExternalAccountsListOptions) ([]*extsvc.ExternalAccount, error) {
		if op.UserID == 23 {
			return mockUser23Accounts, nil
		}
		return nil, nil
	}
	Mocks.Users.GetByCurrentAuthUser = func(ctx context.Context) (*types.User, error) {
		if actr := actor.FromContext(ctx); actr != nil {
			return &types.User{ID: actr.UID}, nil
		}
		return nil, nil
	}
	authz.SetProviders(true, []authz.Provider{
		&MockAuthzProvider{
			serviceID:    "https://gitlab.mine/",
			serviceType:  "gitlab",
			okServiceIDs: map[string]struct{}{"https://okta.mine/": struct{}{}},
			repos:        map[api.RepoName]struct{}{},
			perms: map[extsvc.ExternalAccount]map[api.RepoName]map[authz.Perm]bool{
				*acct(23, "gitlab", "https://gitlab.mine/", "101"): map[api.RepoName]map[authz.Perm]bool{},
			},
		},
	})

	var (
		expNewAcct           = extsvc.ExternalAccountSpec{ServiceID: "https://gitlab.mine/", ServiceType: "gitlab", AccountID: "101"}
		unAuthdCtx           = context.Background()
		authd23Ctx           = actor.WithActor(unAuthdCtx, &actor.Actor{UID: 23})
		authd99Ctx           = actor.WithActor(unAuthdCtx, &actor.Actor{UID: 99})
		account23CreatedOnce = map[int32]map[extsvc.ExternalAccountSpec]int{
			23: map[extsvc.ExternalAccountSpec]int{
				expNewAcct: 1,
			},
		}
		account23CreatedTwice = map[int32]map[extsvc.ExternalAccountSpec]int{
			23: map[extsvc.ExternalAccountSpec]int{
				expNewAcct: 2,
			},
		}
	)
	// Initial counts 0
	if exp := map[int32]map[extsvc.ExternalAccountSpec]int{}; !reflect.DeepEqual(associateUserAndSaveCount, exp) {
		t.Errorf("expected counts to be %s, but was %s", asJSON(t, exp), asJSON(t, associateUserAndSaveCount))
	}

	// Unauthed filter does not trigger new account creation
	if _, err := authzFilter(unAuthdCtx, []*types.Repo{{ID: 77}}, authz.Read); err != nil {
		t.Fatal(err)
	}
	if exp := map[int32]map[extsvc.ExternalAccountSpec]int{}; !reflect.DeepEqual(associateUserAndSaveCount, exp) {
		t.Errorf("expected counts to be %s, but was %s", asJSON(t, exp), asJSON(t, associateUserAndSaveCount))
	}

	// Authed filter triggers new account creation
	if _, err := authzFilter(authd23Ctx, []*types.Repo{{ID: 77}}, authz.Read); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(associateUserAndSaveCount, account23CreatedOnce) {
		t.Errorf("expected counts to be %+v, but was %+v", account23CreatedOnce, associateUserAndSaveCount)
	}

	// Unauthed filter does not trigger new account creation
	if _, err := authzFilter(unAuthdCtx, []*types.Repo{{ID: 77}}, authz.Read); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(associateUserAndSaveCount, account23CreatedOnce) {
		t.Errorf("expected counts to be %+v, but was %+v", account23CreatedOnce, associateUserAndSaveCount)
	}

	// Authed filter triggers new account creation
	if _, err := authzFilter(authd23Ctx, []*types.Repo{{ID: 77}}, authz.Read); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(associateUserAndSaveCount, account23CreatedTwice) {
		t.Errorf("expected counts to be %+v, but was %+v", account23CreatedTwice, associateUserAndSaveCount)
	}

	// Authed filter under another user for whom FetchAccount returns empty doesn't trigger new account creation
	if _, err := authzFilter(authd99Ctx, []*types.Repo{{ID: 77}}, authz.Read); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(associateUserAndSaveCount, account23CreatedTwice) {
		t.Errorf("expected counts to be %+v, but was %+v", account23CreatedTwice, associateUserAndSaveCount)
	}

	// Authed filter does NOT trigger new account creation if new account is already provided
	mockUser23Accounts = append(mockUser23Accounts, &extsvc.ExternalAccount{
		UserID: 23,
		ExternalAccountSpec: extsvc.ExternalAccountSpec{
			ServiceType: "gitlab",
			ServiceID:   "https://gitlab.mine/",
			AccountID:   "101",
		},
	})
	if _, err := authzFilter(authd23Ctx, []*types.Repo{{ID: 77, ExternalRepo: &api.ExternalRepoSpec{}}}, authz.Read); err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(associateUserAndSaveCount, account23CreatedTwice) {
		t.Errorf("expected counts to be %+v, but was %+v", account23CreatedTwice, associateUserAndSaveCount)
	}
}

func acct(userID int32, serviceType, serviceID, accountID string) *extsvc.ExternalAccount {
	return &extsvc.ExternalAccount{
		UserID: userID,
		ExternalAccountSpec: extsvc.ExternalAccountSpec{
			ServiceType: serviceType,
			ServiceID:   serviceID,
			AccountID:   accountID,
		},
	}
}

type MockAuthzProvider struct {
	serviceID   string
	serviceType string

	// okServiceIDs indicate services whose external accounts will be straightforwardly translated
	// into external accounts belonging to this provider.
	okServiceIDs map[string]struct{}

	// perms is the map from external user account to repository permissions. The key set must
	// include all user external accounts that are available in this mock instance.
	perms map[extsvc.ExternalAccount]map[api.RepoName]map[authz.Perm]bool
	repos map[api.RepoName]struct{}
}

func (m *MockAuthzProvider) FetchAccount(ctx context.Context, user *types.User, current []*extsvc.ExternalAccount) (mine *extsvc.ExternalAccount, err error) {
	if user == nil {
		return nil, nil
	}
	for _, acct := range current {
		if (extsvc.ExternalAccount{}) == *acct {
			continue
		}
		if _, ok := m.okServiceIDs[acct.ServiceID]; ok {
			myAcct := *acct
			myAcct.ServiceType = m.serviceType
			myAcct.ServiceID = m.serviceID
			if _, acctExistsInPerms := m.perms[myAcct]; acctExistsInPerms {
				return &myAcct, nil
			}
		}
	}
	return nil, nil
}

func (m *MockAuthzProvider) RepoPerms(ctx context.Context, acct *extsvc.ExternalAccount, repos map[authz.Repo]struct{}) (map[api.RepoName]map[authz.Perm]bool, error) {
	retPerms := make(map[api.RepoName]map[authz.Perm]bool)
	repos, _ = m.Repos(ctx, repos)

	if acct == nil {
		acct = &extsvc.ExternalAccount{}
	}
	if _, existsInPerms := m.perms[*acct]; !existsInPerms {
		acct = &extsvc.ExternalAccount{}
	}

	var userPerms map[api.RepoName]map[authz.Perm]bool = m.perms[*acct]
	for repo := range repos {
		if userRepoPerms, ok := userPerms[repo.URI]; ok {
			retPerms[repo.URI] = make(map[authz.Perm]bool)
			for k, v := range userRepoPerms {
				retPerms[repo.URI][k] = v
			}
		}
	}
	return retPerms, nil
}

func (m *MockAuthzProvider) Repos(ctx context.Context, repos map[authz.Repo]struct{}) (mine map[authz.Repo]struct{}, others map[authz.Repo]struct{}) {
	mine, others = make(map[authz.Repo]struct{}), make(map[authz.Repo]struct{})
	for repo := range repos {
		if _, ok := m.repos[repo.URI]; ok {
			mine[repo] = struct{}{}
		} else {
			others[repo] = struct{}{}
		}
	}
	return mine, others
}

func (m *MockAuthzProvider) ServiceID() string   { return m.serviceID }
func (m *MockAuthzProvider) ServiceType() string { return m.serviceType }
func (m *MockAuthzProvider) Validate() []string  { return nil }
