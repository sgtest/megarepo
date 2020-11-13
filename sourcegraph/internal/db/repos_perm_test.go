package db

import (
	"context"
	"fmt"
	"net/url"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/globals"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/db/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/db/dbtesting"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/schema"
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
				extAcct: &extsvc.Account{
					UserID: user.ID,
					AccountSpec: extsvc.AccountSpec{
						ServiceType: codeHost.ServiceType,
						ServiceID:   codeHost.ServiceID,
						AccountID:   "42_ext",
					},
					AccountData: extsvc.AccountData{Data: nil},
				},
			}
		}(),
		func() authz.Provider {
			baseURL, _ := url.Parse("https://github.com")
			codeHost := extsvc.NewCodeHost(baseURL, extsvc.TypeGitHub)
			return &fakeProvider{
				codeHost: codeHost,
				extAcct: &extsvc.Account{
					UserID: user.ID,
					AccountSpec: extsvc.AccountSpec{
						ServiceType: codeHost.ServiceType,
						ServiceID:   codeHost.ServiceID,
						AccountID:   "42_ext",
					},
					AccountData: extsvc.AccountData{Data: nil},
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
		accts []*extsvc.Account,
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
	extAcct  *extsvc.Account
}

func (p *fakeProvider) FetchAccount(
	ctx context.Context,
	user *types.User,
	current []*extsvc.Account,
) (mine *extsvc.Account, err error) {
	return p.extAcct, nil
}

func (p *fakeProvider) ServiceType() string           { return p.codeHost.ServiceType }
func (p *fakeProvider) ServiceID() string             { return p.codeHost.ServiceID }
func (p *fakeProvider) URN() string                   { return extsvc.URN(p.codeHost.ServiceType, 0) }
func (p *fakeProvider) Validate() (problems []string) { return nil }

func (p *fakeProvider) FetchUserPerms(context.Context, *extsvc.Account) ([]extsvc.RepoID, error) {
	return nil, nil
}

func (p *fakeProvider) FetchRepoPerms(context.Context, *extsvc.Repository) ([]extsvc.AccountID, error) {
	return nil, nil
}

// 🚨 SECURITY: Tests are necessary to ensure security.
func TestAuthzQueryConds(t *testing.T) {
	cmpOpts := cmp.AllowUnexported(sqlf.Query{})

	t.Run("Conflict with permissions user mapping", func(t *testing.T) {
		before := globals.PermissionsUserMapping()
		globals.SetPermissionsUserMapping(&schema.PermissionsUserMapping{Enabled: true})
		defer globals.SetPermissionsUserMapping(before)

		authz.SetProviders(false, []authz.Provider{&fakeProvider{}})
		defer authz.SetProviders(true, nil)

		_, err := authzQueryConds(context.Background())
		gotErr := fmt.Sprintf("%v", err)
		if diff := cmp.Diff(errPermissionsUserMappingConflict.Error(), gotErr); diff != "" {
			t.Fatalf("Mismatch (-want +got):\n%s", diff)
		}
	})

	t.Run("When permissions user mapping is enabled", func(t *testing.T) {
		before := globals.PermissionsUserMapping()
		globals.SetPermissionsUserMapping(&schema.PermissionsUserMapping{Enabled: true})
		defer globals.SetPermissionsUserMapping(before)

		got, err := authzQueryConds(context.Background())
		if err != nil {
			t.Fatal(err)
		}
		want := sqlf.Sprintf(authzQueryCondsFmtstr, false, true, int32(0), authz.Read.String())
		if diff := cmp.Diff(want, got, cmpOpts); diff != "" {
			t.Fatalf("Mismatch (-want +got):\n%s", diff)
		}
	})

	defer func() {
		Mocks.Users.GetByCurrentAuthUser = nil
	}()
	tests := []struct {
		name                string
		setup               func() context.Context
		authzAllowByDefault bool
		wantQuery           *sqlf.Query
	}{
		{
			name: "internal actor bypass checks",
			setup: func() context.Context {
				return actor.WithInternalActor(context.Background())
			},
			wantQuery: sqlf.Sprintf(authzQueryCondsFmtstr, true, false, int32(0), authz.Read.String()),
		},

		{
			name: "no authz provider and not allow by default",
			setup: func() context.Context {
				return context.Background()
			},
			wantQuery: sqlf.Sprintf(authzQueryCondsFmtstr, false, false, int32(0), authz.Read.String()),
		},
		{
			name: "no authz provider but allow by default",
			setup: func() context.Context {
				return context.Background()
			},
			authzAllowByDefault: true,
			wantQuery:           sqlf.Sprintf(authzQueryCondsFmtstr, true, false, int32(0), authz.Read.String()),
		},

		{
			name: "authenticated user is a site admin",
			setup: func() context.Context {
				Mocks.Users.GetByCurrentAuthUser = func(ctx context.Context) (*types.User, error) {
					return &types.User{ID: 1, SiteAdmin: true}, nil
				}
				return actor.WithActor(context.Background(), &actor.Actor{UID: 1})
			},
			wantQuery: sqlf.Sprintf(authzQueryCondsFmtstr, true, false, int32(1), authz.Read.String()),
		},
		{
			name: "authenticated user is not a site admin",
			setup: func() context.Context {
				Mocks.Users.GetByCurrentAuthUser = func(ctx context.Context) (*types.User, error) {
					return &types.User{ID: 1}, nil
				}
				return actor.WithActor(context.Background(), &actor.Actor{UID: 1})
			},
			wantQuery: sqlf.Sprintf(authzQueryCondsFmtstr, false, false, int32(1), authz.Read.String()),
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			authz.SetProviders(test.authzAllowByDefault, nil)
			defer authz.SetProviders(true, nil)

			q, err := authzQueryConds(test.setup())
			if err != nil {
				t.Fatal(err)
			}

			if diff := cmp.Diff(test.wantQuery, q, cmpOpts); diff != "" {
				t.Fatalf("Mismatch (-want +got):\n%s", diff)
			}
		})
	}
}

// 🚨 SECURITY: Tests are necessary to ensure security.
func TestRepos_getReposBySQL_checkPermissions(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	dbtesting.SetupGlobalTestDB(t)
	ctx := context.Background()

	// Set up three users: alice, bob and admin
	alice, err := Users.Create(ctx, NewUser{
		Email:                 "alice@example.com",
		Username:              "alice",
		Password:              "alice",
		EmailVerificationCode: "alice",
	})
	if err != nil {
		t.Fatal(err)
	}
	bob, err := Users.Create(ctx, NewUser{
		Email:                 "bob@example.com",
		Username:              "bob",
		Password:              "bob",
		EmailVerificationCode: "bob",
	})
	if err != nil {
		t.Fatal(err)
	}
	admin, err := Users.Create(ctx, NewUser{
		Email:                 "admin@example.com",
		Username:              "admin",
		Password:              "admin",
		EmailVerificationCode: "admin",
	})
	if err != nil {
		t.Fatal(err)
	}

	// Ensure only "admin" is the site admin, alice was prompted as site admin
	// because it was the first user.
	err = Users.SetIsSiteAdmin(ctx, admin.ID, true)
	if err != nil {
		t.Fatal(err)
	}
	err = Users.SetIsSiteAdmin(ctx, alice.ID, false)
	if err != nil {
		t.Fatal(err)
	}

	// Set up some repositories: public and private for both alice and bob
	internalCtx := actor.WithInternalActor(ctx)
	alicePublicRepo := mustCreate(internalCtx, t,
		&types.Repo{
			Name: "alice_public_repo",
			ExternalRepo: api.ExternalRepoSpec{
				ID:          "alice_public_repo",
				ServiceType: extsvc.TypeGitHub,
				ServiceID:   "https://github.com/",
			},
		},
	)[0]
	alicePrivateRepo := mustCreate(internalCtx, t,
		&types.Repo{
			Name:    "alice_private_repo",
			Private: true,
			ExternalRepo: api.ExternalRepoSpec{
				ID:          "alice_private_repo",
				ServiceType: extsvc.TypeGitHub,
				ServiceID:   "https://github.com/",
			},
		},
	)[0]
	bobPublicRepo := mustCreate(internalCtx, t,
		&types.Repo{
			Name: "bob_public_repo",
			ExternalRepo: api.ExternalRepoSpec{
				ID:          "bob_public_repo",
				ServiceType: extsvc.TypeGitHub,
				ServiceID:   "https://github.com/",
			},
		},
	)[0]
	bobPrivateRepo := mustCreate(internalCtx, t,
		&types.Repo{
			Name:    "bob_private_repo",
			Private: true,
			ExternalRepo: api.ExternalRepoSpec{
				ID:          "bob_private_repo",
				ServiceType: extsvc.TypeGitHub,
				ServiceID:   "https://github.com/",
			},
		},
	)[0]

	// Set up another unrestricted private repo from cindy
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	cindyExternalService := &types.ExternalService{
		Kind:         extsvc.KindGitHub,
		DisplayName:  "GITHUB #1",
		Config:       `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
		Unrestricted: true,
	}
	err = ExternalServices.Create(ctx, confGet, cindyExternalService)
	if err != nil {
		t.Fatal(err)
	}

	cindyPrivateRepo := mustCreate(internalCtx, t,
		&types.Repo{
			Name:    "cindy_private_repo",
			Private: true,
			ExternalRepo: api.ExternalRepoSpec{
				ID:          "cindy_private_repo",
				ServiceType: extsvc.TypeGitHub,
				ServiceID:   "https://github.com/",
			},
		},
	)[0]
	cindyPrivateRepo.Sources = map[string]*types.SourceInfo{
		cindyExternalService.URN(): {ID: cindyExternalService.URN()},
	}

	q := sqlf.Sprintf(`
INSERT INTO external_service_repos (external_service_id, repo_id, clone_url)
VALUES (%s, %s, '')
`, cindyExternalService.ID, cindyPrivateRepo.ID)
	_, err = dbconn.Global.ExecContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
	if err != nil {
		t.Fatal(err)
	}

	// Set up permissions: alice and bob have access to their own private repositories
	q = sqlf.Sprintf(`
INSERT INTO user_permissions (user_id, permission, object_type, object_ids_ints, updated_at)
VALUES
	(%s, 'read', 'repos', %s, NOW()),
	(%s, 'read', 'repos', %s, NOW())
`,
		alice.ID, pq.Array([]int32{int32(alicePrivateRepo.ID)}),
		bob.ID, pq.Array([]int32{int32(bobPrivateRepo.ID)}),
	)
	_, err = dbconn.Global.ExecContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
	if err != nil {
		t.Fatal(err)
	}

	authz.SetProviders(false, []authz.Provider{&fakeProvider{}})
	defer authz.SetProviders(true, nil)

	// Alice should see "alice_public_repo", "alice_private_repo", "bob_public_repo", "cindy_private_repo"
	aliceCtx := actor.WithActor(ctx, &actor.Actor{UID: alice.ID})
	repos, err := Repos.List(aliceCtx, ReposListOptions{})
	if err != nil {
		t.Fatal(err)
	}
	wantRepos := []*types.Repo{alicePublicRepo, alicePrivateRepo, bobPublicRepo, cindyPrivateRepo}
	if diff := cmp.Diff(wantRepos, repos); diff != "" {
		t.Fatalf("Mismatch (-want +got):\n%s", diff)
	}

	// Bob should see "alice_public_repo", "bob_private_repo", "bob_public_repo", "cindy_private_repo"
	bobCtx := actor.WithActor(ctx, &actor.Actor{UID: bob.ID})
	repos, err = Repos.List(bobCtx, ReposListOptions{})
	if err != nil {
		t.Fatal(err)
	}
	wantRepos = []*types.Repo{alicePublicRepo, bobPublicRepo, bobPrivateRepo, cindyPrivateRepo}
	if diff := cmp.Diff(wantRepos, repos); diff != "" {
		t.Fatalf("Mismatch (-want +got):\n%s", diff)
	}

	// Admin should see all repositories
	adminCtx := actor.WithActor(ctx, &actor.Actor{UID: admin.ID})
	repos, err = Repos.List(adminCtx, ReposListOptions{})
	if err != nil {
		t.Fatal(err)
	}
	wantRepos = []*types.Repo{alicePublicRepo, alicePrivateRepo, bobPublicRepo, bobPrivateRepo, cindyPrivateRepo}
	if diff := cmp.Diff(wantRepos, repos); diff != "" {
		t.Fatalf("Mismatch (-want +got):\n%s", diff)
	}

	// A random user should only see "alice_public_repo", "bob_public_repo", "cindy_private_repo"
	repos, err = Repos.List(ctx, ReposListOptions{})
	if err != nil {
		t.Fatal(err)
	}
	wantRepos = []*types.Repo{alicePublicRepo, bobPublicRepo, cindyPrivateRepo}
	if diff := cmp.Diff(wantRepos, repos); diff != "" {
		t.Fatalf("Mismatch (-want +got):\n%s", diff)
	}
}

// 🚨 SECURITY: Tests are necessary to ensure security.
func TestRepos_getReposBySQL_permissionsUserMapping(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	dbtesting.SetupGlobalTestDB(t)
	ctx := context.Background()

	// Set up three users: alice, bob and admin
	alice, err := Users.Create(ctx, NewUser{
		Email:                 "alice@example.com",
		Username:              "alice",
		Password:              "alice",
		EmailVerificationCode: "alice",
	})
	if err != nil {
		t.Fatal(err)
	}
	bob, err := Users.Create(ctx, NewUser{
		Email:                 "bob@example.com",
		Username:              "bob",
		Password:              "bob",
		EmailVerificationCode: "bob",
	})
	if err != nil {
		t.Fatal(err)
	}
	admin, err := Users.Create(ctx, NewUser{
		Email:                 "admin@example.com",
		Username:              "admin",
		Password:              "admin",
		EmailVerificationCode: "admin",
	})
	if err != nil {
		t.Fatal(err)
	}

	// Ensure only "admin" is the site admin, alice was prompted as site admin
	// because it was the first user.
	err = Users.SetIsSiteAdmin(ctx, admin.ID, true)
	if err != nil {
		t.Fatal(err)
	}
	err = Users.SetIsSiteAdmin(ctx, alice.ID, false)
	if err != nil {
		t.Fatal(err)
	}

	// Set up some repositories: public and private for both alice and bob
	internalCtx := actor.WithInternalActor(ctx)
	alicePublicRepo := mustCreate(internalCtx, t,
		&types.Repo{
			Name: "alice_public_repo",
			ExternalRepo: api.ExternalRepoSpec{
				ID:          "alice_public_repo",
				ServiceType: extsvc.TypeGitHub,
				ServiceID:   "https://github.com/",
			},
		},
	)[0]
	alicePrivateRepo := mustCreate(internalCtx, t,
		&types.Repo{
			Name:    "alice_private_repo",
			Private: true,
			ExternalRepo: api.ExternalRepoSpec{
				ID:          "alice_private_repo",
				ServiceType: extsvc.TypeGitHub,
				ServiceID:   "https://github.com/",
			},
		},
	)[0]
	bobPublicRepo := mustCreate(internalCtx, t,
		&types.Repo{
			Name: "bob_public_repo",
			ExternalRepo: api.ExternalRepoSpec{
				ID:          "bob_public_repo",
				ServiceType: extsvc.TypeGitHub,
				ServiceID:   "https://github.com/",
			},
		},
	)[0]
	bobPrivateRepo := mustCreate(internalCtx, t,
		&types.Repo{
			Name:    "bob_private_repo",
			Private: true,
			ExternalRepo: api.ExternalRepoSpec{
				ID:          "bob_private_repo",
				ServiceType: extsvc.TypeGitHub,
				ServiceID:   "https://github.com/",
			},
		},
	)[0]

	// Set up permissions: alice and bob have access to their own private repositories
	q := sqlf.Sprintf(`
INSERT INTO user_permissions (user_id, permission, object_type, object_ids_ints, updated_at)
VALUES
	(%s, 'read', 'repos', %s, NOW()),
	(%s, 'read', 'repos', %s, NOW())
`,
		alice.ID, pq.Array([]int32{int32(alicePrivateRepo.ID)}),
		bob.ID, pq.Array([]int32{int32(bobPrivateRepo.ID)}),
	)
	_, err = dbconn.Global.ExecContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
	if err != nil {
		t.Fatal(err)
	}

	before := globals.PermissionsUserMapping()
	globals.SetPermissionsUserMapping(&schema.PermissionsUserMapping{Enabled: true})
	defer globals.SetPermissionsUserMapping(before)

	// Alice should see "alice_private_repo" but not "alice_public_repo", "bob_public_repo", "bob_private_repo"
	aliceCtx := actor.WithActor(ctx, &actor.Actor{UID: alice.ID})
	repos, err := Repos.List(aliceCtx, ReposListOptions{})
	if err != nil {
		t.Fatal(err)
	}
	wantRepos := []*types.Repo{alicePrivateRepo}
	if diff := cmp.Diff(wantRepos, repos); diff != "" {
		t.Fatalf("Mismatch (-want +got):\n%s", diff)
	}

	// Bob should see "bob_private_repo" but not "alice_public_repo" or "bob_public_repo"
	bobCtx := actor.WithActor(ctx, &actor.Actor{UID: bob.ID})
	repos, err = Repos.List(bobCtx, ReposListOptions{})
	if err != nil {
		t.Fatal(err)
	}
	wantRepos = []*types.Repo{bobPrivateRepo}
	if diff := cmp.Diff(wantRepos, repos); diff != "" {
		t.Fatalf("Mismatch (-want +got):\n%s", diff)
	}

	// Admin should see all repositories
	adminCtx := actor.WithActor(ctx, &actor.Actor{UID: admin.ID})
	repos, err = Repos.List(adminCtx, ReposListOptions{})
	if err != nil {
		t.Fatal(err)
	}
	wantRepos = []*types.Repo{alicePublicRepo, alicePrivateRepo, bobPublicRepo, bobPrivateRepo}
	if diff := cmp.Diff(wantRepos, repos); diff != "" {
		t.Fatalf("Mismatch (-want +got):\n%s", diff)
	}

	// A random user sees nothing
	repos, err = Repos.List(ctx, ReposListOptions{})
	if err != nil {
		t.Fatal(err)
	}
	wantRepos = ([]*types.Repo)(nil)
	if diff := cmp.Diff(wantRepos, repos); diff != "" {
		t.Fatalf("Mismatch (-want +got):\n%s", diff)
	}
}
