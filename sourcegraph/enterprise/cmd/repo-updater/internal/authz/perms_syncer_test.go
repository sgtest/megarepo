package authz

import (
	"context"
	"net/http"
	"strconv"
	"testing"
	"time"

	mockrequire "github.com/derision-test/go-mockgen/testutil/require"
	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/conc/pool"
	"github.com/sourcegraph/log/logtest"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"go.uber.org/atomic"

	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/licensing"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitlab"
	"github.com/sourcegraph/sourcegraph/internal/repos"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func init() {
	collectMetricsDisabled = true
}

func TestPermsSyncer_ScheduleUsers(t *testing.T) {
	authz.SetProviders(true, []authz.Provider{&mockProvider{}})
	defer authz.SetProviders(true, nil)

	licensing.MockCheckFeatureError("")
	s := NewPermsSyncer(logtest.Scoped(t), nil, nil, nil, nil, nil)
	s.ScheduleUsers(context.Background(), authz.FetchPermsOptions{}, 1)

	expHeap := []*syncRequest{
		{requestMeta: &requestMeta{
			Priority: priorityHigh,
			Type:     requestTypeUser,
			ID:       1,
		}, acquired: false, index: 0},
	}
	if diff := cmp.Diff(expHeap, s.queue.heap, cmpOpts); diff != "" {
		t.Fatalf("heap: %v", diff)
	}
}

func TestPermsSyncer_ScheduleRepos(t *testing.T) {
	authz.SetProviders(true, []authz.Provider{&mockProvider{}})
	defer authz.SetProviders(true, nil)

	licensing.MockCheckFeatureError("")
	s := NewPermsSyncer(logtest.Scoped(t), nil, nil, nil, nil, nil)
	s.ScheduleRepos(context.Background(), 1)

	expHeap := []*syncRequest{
		{requestMeta: &requestMeta{
			Priority: priorityHigh,
			Type:     requestTypeRepo,
			ID:       1,
		}, acquired: false, index: 0},
	}
	if diff := cmp.Diff(expHeap, s.queue.heap, cmpOpts); diff != "" {
		t.Fatalf("heap: %v", diff)
	}
}

type mockProvider struct {
	id          int64
	serviceType string
	serviceID   string

	fetchUserPerms        func(context.Context, *extsvc.Account) (*authz.ExternalUserPermissions, error)
	fetchUserPermsByToken func(ctx context.Context, token string) (*authz.ExternalUserPermissions, error)
	fetchRepoPerms        func(ctx context.Context, repo *extsvc.Repository, opts authz.FetchPermsOptions) ([]extsvc.AccountID, error)
}

func (*mockProvider) FetchAccount(context.Context, *types.User, []*extsvc.Account, []string) (*extsvc.Account, error) {
	return nil, nil
}

func (p *mockProvider) ServiceType() string { return p.serviceType }
func (p *mockProvider) ServiceID() string   { return p.serviceID }
func (p *mockProvider) URN() string         { return extsvc.URN(p.serviceType, p.id) }

func (*mockProvider) ValidateConnection(context.Context) error { return nil }

func (p *mockProvider) FetchUserPerms(ctx context.Context, acct *extsvc.Account, opts authz.FetchPermsOptions) (*authz.ExternalUserPermissions, error) {
	return p.fetchUserPerms(ctx, acct)
}

func (p *mockProvider) FetchUserPermsByToken(ctx context.Context, token string, opts authz.FetchPermsOptions) (*authz.ExternalUserPermissions, error) {
	return p.fetchUserPermsByToken(ctx, token)
}

func (p *mockProvider) FetchRepoPerms(ctx context.Context, repo *extsvc.Repository, opts authz.FetchPermsOptions) ([]extsvc.AccountID, error) {
	return p.fetchRepoPerms(ctx, repo, opts)
}

func TestPermsSyncer_syncUserPerms(t *testing.T) {
	p := &mockProvider{
		id:          1,
		serviceType: extsvc.TypeGitLab,
		serviceID:   "https://gitlab.com/",
	}
	authz.SetProviders(false, []authz.Provider{p})
	defer authz.SetProviders(true, nil)

	extAccount := extsvc.Account{
		AccountSpec: extsvc.AccountSpec{
			ServiceType: p.ServiceType(),
			ServiceID:   p.ServiceID(),
		},
	}

	users := database.NewMockUserStore()
	users.GetByIDFunc.SetDefaultHook(func(ctx context.Context, id int32) (*types.User, error) {
		return &types.User{ID: id}, nil
	})

	mockRepos := database.NewMockRepoStore()
	mockRepos.ListMinimalReposFunc.SetDefaultHook(func(ctx context.Context, opt database.ReposListOptions) ([]types.MinimalRepo, error) {
		if !opt.OnlyPrivate {
			return nil, errors.New("OnlyPrivate want true but got false")
		}

		names := make([]types.MinimalRepo, 0, len(opt.ExternalRepos))
		for _, r := range opt.ExternalRepos {
			id, _ := strconv.Atoi(r.ID)
			names = append(names, types.MinimalRepo{ID: api.RepoID(id)})
		}
		return names, nil
	})

	userEmails := database.NewMockUserEmailsStore()

	externalAccounts := database.NewMockUserExternalAccountsStore()
	externalAccounts.ListFunc.SetDefaultHook(func(_ context.Context, opts database.ExternalAccountsListOptions) ([]*extsvc.Account, error) {
		if opts.OnlyExpired {
			return []*extsvc.Account{}, nil
		}
		return []*extsvc.Account{&extAccount}, nil
	})

	featureFlags := database.NewMockFeatureFlagStore()

	db := database.NewMockDB()
	db.UsersFunc.SetDefaultReturn(users)
	db.ReposFunc.SetDefaultReturn(mockRepos)
	db.UserEmailsFunc.SetDefaultReturn(userEmails)
	db.UserExternalAccountsFunc.SetDefaultReturn(externalAccounts)
	db.FeatureFlagsFunc.SetDefaultReturn(featureFlags)

	reposStore := repos.NewMockStoreFrom(repos.NewStore(logtest.Scoped(t), db))
	reposStore.RepoStoreFunc.SetDefaultReturn(mockRepos)

	perms := edb.NewMockPermsStore()
	perms.SetUserPermissionsFunc.SetDefaultHook(func(_ context.Context, p *authz.UserPermissions) (*database.SetPermissionsResult, error) {
		wantIDs := []int32{1, 2, 3, 4}
		assert.Equal(t, wantIDs, p.GenerateSortedIDsSlice())
		return nil, nil
	})

	s := NewPermsSyncer(logtest.Scoped(t), db, reposStore, perms, timeutil.Now, nil)

	p.fetchUserPerms = func(context.Context, *extsvc.Account) (*authz.ExternalUserPermissions, error) {
		return &authz.ExternalUserPermissions{
			Exacts: []extsvc.RepoID{"1", "2", "3", "4"},
		}, nil
	}

	_, providers, err := s.syncUserPerms(context.Background(), 1, true, authz.FetchPermsOptions{})
	if err != nil {
		t.Fatal(err)
	}
	assert.Equal(t, database.CodeHostStatusesSet{{
		ProviderID:   "https://gitlab.com/",
		ProviderType: "gitlab",
		Status:       "SUCCESS",
		Message:      "FetchUserPerms",
	}}, providers)
}

func TestPermsSyncer_syncUserPerms_touchUserPermissions(t *testing.T) {
	p := &mockProvider{
		id:          1,
		serviceType: extsvc.TypeGitLab,
		serviceID:   "https://gitlab.com/",
	}
	authz.SetProviders(false, []authz.Provider{p})
	defer authz.SetProviders(true, nil)

	users := database.NewMockUserStore()
	users.GetByIDFunc.SetDefaultHook(func(ctx context.Context, id int32) (*types.User, error) {
		return &types.User{ID: id}, nil
	})

	mockRepos := database.NewMockRepoStore()
	mockRepos.ListMinimalReposFunc.SetDefaultHook(func(ctx context.Context, opt database.ReposListOptions) ([]types.MinimalRepo, error) {
		if !opt.OnlyPrivate {
			return nil, errors.New("OnlyPrivate want true but got false")
		}

		names := make([]types.MinimalRepo, 0, len(opt.ExternalRepos))
		for _, r := range opt.ExternalRepos {
			id, _ := strconv.Atoi(r.ID)
			names = append(names, types.MinimalRepo{ID: api.RepoID(id)})
		}
		return names, nil
	})

	externalAccounts := database.NewMockUserExternalAccountsStore()
	externalAccounts.ListFunc.SetDefaultHook(func(ctx context.Context, options database.ExternalAccountsListOptions) ([]*extsvc.Account, error) {
		// Force an error here to bail out of fetchUserPermsViaExternalAccounts
		return nil, errors.New("forced error")
	})

	userEmails := database.NewMockUserEmailsStore()
	userEmails.ListByUserFunc.SetDefaultHook(func(ctx context.Context, options database.UserEmailsListOptions) ([]*database.UserEmail, error) {
		return []*database.UserEmail{}, nil
	})

	db := database.NewMockDB()
	db.UsersFunc.SetDefaultReturn(users)
	db.ReposFunc.SetDefaultReturn(mockRepos)
	db.UserExternalAccountsFunc.SetDefaultReturn(externalAccounts)
	db.UserEmailsFunc.SetDefaultReturn(userEmails)

	reposStore := repos.NewMockStoreFrom(repos.NewStore(logtest.Scoped(t), db))
	reposStore.RepoStoreFunc.SetDefaultReturn(mockRepos)

	perms := edb.NewMockPermsStore()
	perms.SetUserPermissionsFunc.SetDefaultHook(func(_ context.Context, p *authz.UserPermissions) (*database.SetPermissionsResult, error) {
		wantIDs := []int32{1, 2, 3, 4, 5}
		assert.Equal(t, wantIDs, p.GenerateSortedIDsSlice())
		return nil, nil
	})
	perms.TouchUserPermissionsFunc.SetDefaultHook(func(ctx context.Context, i int32) error {
		return nil
	})

	s := NewPermsSyncer(logtest.Scoped(t), db, reposStore, perms, timeutil.Now, nil)

	t.Run("fetchUserPermsViaExternalAccounts", func(t *testing.T) {
		_, _, err := s.syncUserPerms(context.Background(), 1, true, authz.FetchPermsOptions{})
		if err == nil {
			t.Fatal("expected an error")
		}
		mockrequire.CalledN(t, perms.TouchUserPermissionsFunc, 1)
	})
}

// If we hit a temporary error from the provider we should fetch existing
// permissions from the database
func TestPermsSyncer_syncUserPermsTemporaryProviderError(t *testing.T) {
	p := &mockProvider{
		id:          1,
		serviceType: extsvc.TypeGitLab,
		serviceID:   "https://gitlab.com/",
	}
	authz.SetProviders(false, []authz.Provider{p})
	defer authz.SetProviders(true, nil)

	extAccount := extsvc.Account{
		AccountSpec: extsvc.AccountSpec{
			ServiceType: p.ServiceType(),
			ServiceID:   p.ServiceID(),
		},
	}

	users := database.NewMockUserStore()
	users.GetByIDFunc.SetDefaultHook(func(ctx context.Context, id int32) (*types.User, error) {
		return &types.User{ID: id}, nil
	})

	mockRepos := database.NewMockRepoStore()
	mockRepos.ListMinimalReposFunc.SetDefaultHook(func(ctx context.Context, opt database.ReposListOptions) ([]types.MinimalRepo, error) {
		if !opt.OnlyPrivate {
			return nil, errors.New("OnlyPrivate want true but got false")
		}

		names := make([]types.MinimalRepo, 0, len(opt.ExternalRepos))
		for _, r := range opt.ExternalRepos {
			id, _ := strconv.Atoi(r.ID)
			names = append(names, types.MinimalRepo{ID: api.RepoID(id)})
		}
		return names, nil
	})

	userEmails := database.NewMockUserEmailsStore()

	externalAccounts := database.NewMockUserExternalAccountsStore()
	externalAccounts.ListFunc.SetDefaultHook(func(_ context.Context, opts database.ExternalAccountsListOptions) ([]*extsvc.Account, error) {
		if opts.OnlyExpired {
			return []*extsvc.Account{}, nil
		}
		return []*extsvc.Account{&extAccount}, nil
	})
	featureFlags := database.NewMockFeatureFlagStore()

	subRepoPerms := edb.NewMockSubRepoPermsStore()
	subRepoPerms.GetByUserAndServiceFunc.SetDefaultReturn(nil, nil)

	db := edb.NewMockEnterpriseDB()
	db.UsersFunc.SetDefaultReturn(users)
	db.ReposFunc.SetDefaultReturn(mockRepos)
	db.UserEmailsFunc.SetDefaultReturn(userEmails)
	db.UserExternalAccountsFunc.SetDefaultReturn(externalAccounts)
	db.SubRepoPermsFunc.SetDefaultReturn(subRepoPerms)
	db.FeatureFlagsFunc.SetDefaultReturn(featureFlags)

	reposStore := repos.NewMockStoreFrom(repos.NewStore(logtest.Scoped(t), db))
	reposStore.RepoStoreFunc.SetDefaultReturn(mockRepos)

	perms := edb.NewMockPermsStore()
	perms.SetUserPermissionsFunc.SetDefaultHook(func(_ context.Context, p *authz.UserPermissions) (*database.SetPermissionsResult, error) {
		wantIDs := []int32{}
		assert.Equal(t, wantIDs, p.GenerateSortedIDsSlice())
		return nil, nil
	})

	s := NewPermsSyncer(logtest.Scoped(t), db, reposStore, perms, timeutil.Now, nil)

	p.fetchUserPerms = func(context.Context, *extsvc.Account) (*authz.ExternalUserPermissions, error) {
		// DeadlineExceeded implements the Temporary interface
		return nil, context.DeadlineExceeded
	}

	_, providers, err := s.syncUserPerms(context.Background(), 1, true, authz.FetchPermsOptions{})
	if err != nil {
		t.Fatal(err)
	}
	assert.Equal(t, database.CodeHostStatusesSet{{
		ProviderID:   "https://gitlab.com/",
		ProviderType: "gitlab",
		Status:       "ERROR",
		Message:      "FetchUserPerms: context deadline exceeded",
	}}, providers)
}

func TestPermsSyncer_syncUserPerms_noPerms(t *testing.T) {
	p := &mockProvider{
		id:          1,
		serviceType: extsvc.TypeGitLab,
		serviceID:   "https://gitlab.com/",
	}
	authz.SetProviders(false, []authz.Provider{p})
	defer authz.SetProviders(true, nil)

	extAccount := extsvc.Account{
		AccountSpec: extsvc.AccountSpec{
			ServiceType: p.ServiceType(),
			ServiceID:   p.ServiceID(),
		},
	}

	users := database.NewMockUserStore()
	users.GetByIDFunc.SetDefaultHook(func(ctx context.Context, id int32) (*types.User, error) {
		return &types.User{ID: id}, nil
	})

	mockRepos := database.NewMockRepoStore()
	mockRepos.ListMinimalReposFunc.SetDefaultHook(func(ctx context.Context, opt database.ReposListOptions) ([]types.MinimalRepo, error) {
		if !opt.OnlyPrivate {
			return nil, errors.New("OnlyPrivate want true but got false")
		}
		return []types.MinimalRepo{{ID: 1}}, nil
	})

	userEmails := database.NewMockUserEmailsStore()
	externalAccounts := database.NewMockUserExternalAccountsStore()
	externalAccounts.ListFunc.SetDefaultReturn([]*extsvc.Account{&extAccount}, nil)

	featureFlags := database.NewMockFeatureFlagStore()

	db := database.NewMockDB()
	db.UsersFunc.SetDefaultReturn(users)
	db.ReposFunc.SetDefaultReturn(mockRepos)
	db.UserEmailsFunc.SetDefaultReturn(userEmails)
	db.UserExternalAccountsFunc.SetDefaultReturn(externalAccounts)
	db.FeatureFlagsFunc.SetDefaultReturn(featureFlags)

	reposStore := repos.NewMockStoreFrom(repos.NewStore(logtest.Scoped(t), db))
	reposStore.RepoStoreFunc.SetDefaultReturn(mockRepos)

	perms := edb.NewMockPermsStore()
	perms.SetUserPermissionsFunc.SetDefaultHook(func(_ context.Context, p *authz.UserPermissions) (*database.SetPermissionsResult, error) {
		assert.Equal(t, int32(1), p.UserID)
		assert.Equal(t, []int32{1}, p.GenerateSortedIDsSlice())
		return nil, nil
	})

	s := NewPermsSyncer(logtest.Scoped(t), db, reposStore, perms, timeutil.Now, nil)

	tests := []struct {
		name     string
		noPerms  bool
		fetchErr error
	}{
		{
			name:     "sync for the first time and encounter an error",
			noPerms:  true,
			fetchErr: errors.New("random error"),
		},
		{
			name:    "sync for the second time and succeed",
			noPerms: false,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			p.fetchUserPerms = func(context.Context, *extsvc.Account) (*authz.ExternalUserPermissions, error) {
				return &authz.ExternalUserPermissions{
					Exacts: []extsvc.RepoID{"1"},
				}, test.fetchErr
			}

			_, _, err := s.syncUserPerms(context.Background(), 1, test.noPerms, authz.FetchPermsOptions{})
			if err != nil {
				t.Fatal(err)
			}
		})
	}
}

func TestPermsSyncer_syncUserPerms_tokenExpire(t *testing.T) {
	p := &mockProvider{
		serviceType: extsvc.TypeGitHub,
		serviceID:   "https://github.com/",
	}
	authz.SetProviders(false, []authz.Provider{p})
	defer authz.SetProviders(true, nil)

	extAccount := extsvc.Account{
		AccountSpec: extsvc.AccountSpec{
			ServiceType: p.ServiceType(),
			ServiceID:   p.ServiceID(),
		},
	}

	users := database.NewMockUserStore()
	users.GetByIDFunc.SetDefaultHook(func(ctx context.Context, id int32) (*types.User, error) {
		return &types.User{ID: id}, nil
	})

	mockRepos := database.NewMockRepoStore()
	mockRepos.ListMinimalReposFunc.SetDefaultHook(func(ctx context.Context, opt database.ReposListOptions) ([]types.MinimalRepo, error) {
		if !opt.OnlyPrivate {
			return nil, errors.New("OnlyPrivate want true but got false")
		}
		return []types.MinimalRepo{{ID: 1}}, nil
	})

	externalServices := database.NewMockExternalServiceStore()
	userEmails := database.NewMockUserEmailsStore()

	externalAccounts := database.NewMockUserExternalAccountsStore()
	externalAccounts.ListFunc.SetDefaultReturn([]*extsvc.Account{&extAccount}, nil)

	db := database.NewMockDB()
	db.UsersFunc.SetDefaultReturn(users)
	db.ReposFunc.SetDefaultReturn(mockRepos)
	db.ExternalServicesFunc.SetDefaultReturn(externalServices)
	db.UserEmailsFunc.SetDefaultReturn(userEmails)
	db.UserExternalAccountsFunc.SetDefaultReturn(externalAccounts)
	db.FeatureFlagsFunc.SetDefaultReturn(database.NewMockFeatureFlagStore())

	reposStore := repos.NewMockStore()

	perms := edb.NewMockPermsStore()
	s := NewPermsSyncer(logtest.Scoped(t), db, reposStore, perms, timeutil.Now, nil)

	t.Run("invalid token", func(t *testing.T) {
		p.fetchUserPerms = func(ctx context.Context, account *extsvc.Account) (*authz.ExternalUserPermissions, error) {
			return nil, &github.APIError{Code: http.StatusUnauthorized}
		}

		_, _, err := s.syncUserPerms(context.Background(), 1, false, authz.FetchPermsOptions{})
		if err != nil {
			t.Fatal(err)
		}

		mockrequire.Called(t, externalAccounts.TouchExpiredFunc)
	})

	t.Run("forbidden", func(t *testing.T) {
		p.fetchUserPerms = func(ctx context.Context, account *extsvc.Account) (*authz.ExternalUserPermissions, error) {
			return nil, gitlab.NewHTTPError(http.StatusForbidden, nil)
		}

		_, _, err := s.syncUserPerms(context.Background(), 1, false, authz.FetchPermsOptions{})
		if err != nil {
			t.Fatal(err)
		}

		mockrequire.Called(t, externalAccounts.TouchExpiredFunc)
	})

	t.Run("account suspension", func(t *testing.T) {
		p.fetchUserPerms = func(ctx context.Context, account *extsvc.Account) (*authz.ExternalUserPermissions, error) {
			return nil, &github.APIError{
				URL:     "https://api.github.com/user/repos",
				Code:    http.StatusForbidden,
				Message: "Sorry. Your account was suspended",
			}
		}

		_, _, err := s.syncUserPerms(context.Background(), 1, false, authz.FetchPermsOptions{})
		if err != nil {
			t.Fatal(err)
		}

		mockrequire.Called(t, externalAccounts.TouchExpiredFunc)
	})
}

func TestPermsSyncer_syncUserPerms_prefixSpecs(t *testing.T) {
	p := &mockProvider{
		serviceType: extsvc.TypePerforce,
		serviceID:   "ssl:111.222.333.444:1666",
	}
	authz.SetProviders(false, []authz.Provider{p})
	defer authz.SetProviders(true, nil)

	extAccount := extsvc.Account{
		AccountSpec: extsvc.AccountSpec{
			ServiceType: p.ServiceType(),
			ServiceID:   p.ServiceID(),
		},
	}

	users := database.NewMockUserStore()
	users.GetByIDFunc.SetDefaultHook(func(ctx context.Context, id int32) (*types.User, error) {
		return &types.User{ID: id}, nil
	})

	mockRepos := database.NewMockRepoStore()
	mockRepos.ListMinimalReposFunc.SetDefaultHook(func(ctx context.Context, opt database.ReposListOptions) ([]types.MinimalRepo, error) {
		if !opt.OnlyPrivate {
			return nil, errors.New("OnlyPrivate want true but got false")
		} else if len(opt.ExternalRepoIncludeContains) == 0 {
			return nil, errors.New("ExternalRepoIncludeContains want non-zero but got zero")
		} else if len(opt.ExternalRepoExcludeContains) == 0 {
			return nil, errors.New("ExternalRepoExcludeContains want non-zero but got zero")
		}
		return []types.MinimalRepo{{ID: 1}}, nil
	})

	externalServices := database.NewMockExternalServiceStore()
	userEmails := database.NewMockUserEmailsStore()

	externalAccounts := database.NewMockUserExternalAccountsStore()
	externalAccounts.ListFunc.SetDefaultReturn([]*extsvc.Account{&extAccount}, nil)

	featureFlags := database.NewMockFeatureFlagStore()

	db := database.NewMockDB()
	db.UsersFunc.SetDefaultReturn(users)
	db.ReposFunc.SetDefaultReturn(mockRepos)
	db.ExternalServicesFunc.SetDefaultReturn(externalServices)
	db.UserEmailsFunc.SetDefaultReturn(userEmails)
	db.UserExternalAccountsFunc.SetDefaultReturn(externalAccounts)
	db.FeatureFlagsFunc.SetDefaultReturn(featureFlags)

	reposStore := repos.NewMockStoreFrom(repos.NewStore(logtest.Scoped(t), db))
	reposStore.RepoStoreFunc.SetDefaultReturn(mockRepos)
	reposStore.ExternalServiceStoreFunc.SetDefaultReturn(externalServices)

	perms := edb.NewMockPermsStore()

	s := NewPermsSyncer(logtest.Scoped(t), db, reposStore, perms, timeutil.Now, nil)

	p.fetchUserPerms = func(context.Context, *extsvc.Account) (*authz.ExternalUserPermissions, error) {
		return &authz.ExternalUserPermissions{
			IncludeContains: []extsvc.RepoID{"//Engineering/"},
			ExcludeContains: []extsvc.RepoID{"//Engineering/Security/"},
		}, nil
	}

	_, _, err := s.syncUserPerms(context.Background(), 1, false, authz.FetchPermsOptions{})
	if err != nil {
		t.Fatal(err)
	}
}

func TestPermsSyncer_syncUserPerms_subRepoPermissions(t *testing.T) {
	p := &mockProvider{
		serviceType: extsvc.TypePerforce,
		serviceID:   "ssl:111.222.333.444:1666",
	}
	authz.SetProviders(false, []authz.Provider{p})
	defer authz.SetProviders(true, nil)

	extAccount := extsvc.Account{
		AccountSpec: extsvc.AccountSpec{
			ServiceType: p.ServiceType(),
			ServiceID:   p.ServiceID(),
		},
	}

	users := database.NewMockUserStore()
	users.GetByIDFunc.SetDefaultHook(func(ctx context.Context, id int32) (*types.User, error) {
		return &types.User{ID: id}, nil
	})

	mockRepos := database.NewMockRepoStore()
	mockRepos.ListMinimalReposFunc.SetDefaultHook(func(ctx context.Context, opt database.ReposListOptions) ([]types.MinimalRepo, error) {
		if !opt.OnlyPrivate {
			return nil, errors.New("OnlyPrivate want true but got false")
		} else if len(opt.ExternalRepoIncludeContains) == 0 {
			return nil, errors.New("ExternalRepoIncludeContains want non-zero but got zero")
		} else if len(opt.ExternalRepoExcludeContains) == 0 {
			return nil, errors.New("ExternalRepoExcludeContains want non-zero but got zero")
		}
		return []types.MinimalRepo{{ID: 1}}, nil
	})

	externalServices := database.NewMockExternalServiceStore()
	userEmails := database.NewMockUserEmailsStore()

	externalAccounts := database.NewMockUserExternalAccountsStore()
	externalAccounts.ListFunc.SetDefaultReturn([]*extsvc.Account{&extAccount}, nil)

	subRepoPerms := edb.NewMockSubRepoPermsStore()

	featureFlags := database.NewMockFeatureFlagStore()

	db := edb.NewMockEnterpriseDB()
	db.UsersFunc.SetDefaultReturn(users)
	db.ReposFunc.SetDefaultReturn(mockRepos)
	db.ExternalServicesFunc.SetDefaultReturn(externalServices)
	db.UserEmailsFunc.SetDefaultReturn(userEmails)
	db.UserExternalAccountsFunc.SetDefaultReturn(externalAccounts)
	db.SubRepoPermsFunc.SetDefaultReturn(subRepoPerms)
	db.FeatureFlagsFunc.SetDefaultReturn(featureFlags)

	reposStore := repos.NewMockStoreFrom(repos.NewStore(logtest.Scoped(t), db))
	reposStore.RepoStoreFunc.SetDefaultReturn(mockRepos)
	reposStore.ExternalServiceStoreFunc.SetDefaultReturn(externalServices)

	perms := edb.NewMockPermsStore()

	s := NewPermsSyncer(logtest.Scoped(t), db, reposStore, perms, timeutil.Now, nil)

	p.fetchUserPerms = func(context.Context, *extsvc.Account) (*authz.ExternalUserPermissions, error) {
		return &authz.ExternalUserPermissions{
			IncludeContains: []extsvc.RepoID{"//Engineering/"},
			ExcludeContains: []extsvc.RepoID{"//Engineering/Security/"},

			SubRepoPermissions: map[extsvc.RepoID]*authz.SubRepoPermissions{
				"abc": {
					Paths: []string{"/include1", "/include2", "-/exclude1", "-/exclude2"},
				},
				"def": {
					Paths: []string{"/include1", "/include2", "-/exclude1", "-/exclude2"},
				},
			},
		}, nil
	}

	_, _, err := s.syncUserPerms(context.Background(), 1, false, authz.FetchPermsOptions{})
	if err != nil {
		t.Fatal(err)
	}

	mockrequire.CalledN(t, subRepoPerms.UpsertWithSpecFunc, 2)
}

func TestPermsSyncer_syncRepoPerms(t *testing.T) {
	mockRepos := database.NewMockRepoStore()
	mockFeatureFlags := database.NewMockFeatureFlagStore()

	db := database.NewMockDB()
	db.ReposFunc.SetDefaultReturn(mockRepos)
	db.FeatureFlagsFunc.SetDefaultReturn(mockFeatureFlags)

	newPermsSyncer := func(reposStore repos.Store, perms edb.PermsStore) *PermsSyncer {
		return NewPermsSyncer(logtest.Scoped(t), db, reposStore, perms, timeutil.Now, nil)
	}

	t.Run("TouchRepoPermissions is called when no authz provider", func(t *testing.T) {
		mockRepos.GetFunc.SetDefaultReturn(
			&types.Repo{
				ID:      1,
				Private: true,
				ExternalRepo: api.ExternalRepoSpec{
					ServiceID: "https://gitlab.com/",
				},
				Sources: map[string]*types.SourceInfo{
					extsvc.URN(extsvc.TypeGitLab, 0): {},
				},
			},
			nil,
		)

		reposStore := repos.NewMockStoreFrom(repos.NewStore(logtest.Scoped(t), db))
		reposStore.RepoStoreFunc.SetDefaultReturn(mockRepos)

		perms := edb.NewMockPermsStore()
		s := newPermsSyncer(reposStore, perms)

		_, _, err := s.syncRepoPerms(context.Background(), 1, false, authz.FetchPermsOptions{})
		if err != nil {
			t.Fatal(err)
		}

		mockrequire.Called(t, perms.TouchRepoPermissionsFunc)
	})

	t.Run("identify authz provider by URN", func(t *testing.T) {
		// Even though both p1 and p2 are pointing to the same code host,
		// but p2 should not be used because it is not responsible for listing
		// test repository.
		p1 := &mockProvider{
			id:          1,
			serviceType: extsvc.TypeGitLab,
			serviceID:   "https://gitlab.com/",
			fetchRepoPerms: func(ctx context.Context, repo *extsvc.Repository, opts authz.FetchPermsOptions) ([]extsvc.AccountID, error) {
				return []extsvc.AccountID{"user"}, nil
			},
		}
		p2 := &mockProvider{
			id:          2,
			serviceType: extsvc.TypeGitLab,
			serviceID:   "https://gitlab.com/",
			fetchRepoPerms: func(ctx context.Context, repo *extsvc.Repository, opts authz.FetchPermsOptions) ([]extsvc.AccountID, error) {
				return nil, errors.New("not supposed to be called")
			},
		}
		authz.SetProviders(false, []authz.Provider{p1, p2})
		defer authz.SetProviders(true, nil)

		mockRepos.ListFunc.SetDefaultReturn(
			[]*types.Repo{
				{
					ID:      1,
					Private: true,
					ExternalRepo: api.ExternalRepoSpec{
						ServiceID: p1.ServiceID(),
					},
					Sources: map[string]*types.SourceInfo{
						p1.URN(): {},
					},
				},
			},
			nil,
		)

		reposStore := repos.NewMockStoreFrom(repos.NewStore(logtest.Scoped(t), db))
		reposStore.RepoStoreFunc.SetDefaultReturn(mockRepos)

		perms := edb.NewMockPermsStore()
		perms.TransactFunc.SetDefaultReturn(perms, nil)
		perms.GetUserIDsByExternalAccountsFunc.SetDefaultReturn(map[string]authz.UserIDWithExternalAccountID{"user": {UserID: 1, ExternalAccountID: 1}}, nil)
		perms.SetRepoPermissionsFunc.SetDefaultHook(func(_ context.Context, p *authz.RepoPermissions) (*database.SetPermissionsResult, error) {
			assert.Equal(t, int32(1), p.RepoID)
			assert.Equal(t, []int32{1}, p.GenerateSortedIDsSlice())
			return nil, nil
		})

		s := newPermsSyncer(reposStore, perms)

		_, _, err := s.syncRepoPerms(context.Background(), 1, false, authz.FetchPermsOptions{})
		if err != nil {
			t.Fatal(err)
		}
	})

	t.Run("repo sync with external service userid but no providers", func(t *testing.T) {
		mockRepos.ListFunc.SetDefaultReturn(
			[]*types.Repo{
				{
					ID:      1,
					Private: true,
				},
			},
			nil,
		)

		reposStore := repos.NewMockStoreFrom(repos.NewStore(logtest.Scoped(t), db))
		reposStore.RepoStoreFunc.SetDefaultReturn(mockRepos)

		perms := edb.NewMockPermsStore()
		perms.TransactFunc.SetDefaultReturn(perms, nil)
		perms.GetUserIDsByExternalAccountsFunc.SetDefaultReturn(map[string]authz.UserIDWithExternalAccountID{"user": {UserID: 1, ExternalAccountID: 1}}, nil)
		perms.SetRepoPermissionsFunc.SetDefaultHook(func(_ context.Context, p *authz.RepoPermissions) (*database.SetPermissionsResult, error) {
			assert.Equal(t, int32(1), p.RepoID)
			assert.Equal(t, []int32{1}, p.GenerateSortedIDsSlice())
			return nil, nil
		})

		s := newPermsSyncer(reposStore, perms)

		_, _, err := s.syncRepoPerms(context.Background(), 1, false, authz.FetchPermsOptions{})
		if err != nil {
			t.Fatal(err)
		}

		mockrequire.NotCalled(t, perms.SetRepoPendingPermissionsFunc)
	})

	p := &mockProvider{
		serviceType: extsvc.TypeGitLab,
		serviceID:   "https://gitlab.com/",
	}
	authz.SetProviders(false, []authz.Provider{p})
	defer authz.SetProviders(true, nil)

	mockRepos.ListFunc.SetDefaultReturn(
		[]*types.Repo{
			{
				ID:      1,
				Private: true,
				ExternalRepo: api.ExternalRepoSpec{
					ServiceID: p.ServiceID(),
				},
				Sources: map[string]*types.SourceInfo{
					p.URN(): {},
				},
			},
		},
		nil,
	)

	reposStore := repos.NewMockStoreFrom(repos.NewStore(logtest.Scoped(t), db))
	reposStore.RepoStoreFunc.SetDefaultReturn(mockRepos)

	perms := edb.NewMockPermsStore()
	perms.TransactFunc.SetDefaultReturn(perms, nil)
	perms.GetUserIDsByExternalAccountsFunc.SetDefaultReturn(map[string]authz.UserIDWithExternalAccountID{"user": {UserID: 1, ExternalAccountID: 1}}, nil)
	perms.SetRepoPermissionsFunc.SetDefaultHook(func(_ context.Context, p *authz.RepoPermissions) (*database.SetPermissionsResult, error) {
		assert.Equal(t, int32(1), p.RepoID)
		assert.Equal(t, []int32{1}, p.GenerateSortedIDsSlice())
		return nil, nil
	})
	perms.SetRepoPendingPermissionsFunc.SetDefaultHook(func(_ context.Context, accounts *extsvc.Accounts, _ *authz.RepoPermissions) error {
		wantAccounts := &extsvc.Accounts{
			ServiceType: p.ServiceType(),
			ServiceID:   p.ServiceID(),
			AccountIDs:  []string{"pending_user"},
		}
		assert.Equal(t, wantAccounts, accounts)
		return nil
	})

	s := newPermsSyncer(reposStore, perms)

	tests := []struct {
		name     string
		noPerms  bool
		fetchErr error
	}{
		{
			name:     "sync for the first time and encounter an error",
			noPerms:  true,
			fetchErr: errors.New("random error"),
		},
		{
			name:    "sync for the second time and succeed",
			noPerms: false,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			p.fetchRepoPerms = func(context.Context, *extsvc.Repository, authz.FetchPermsOptions) ([]extsvc.AccountID, error) {
				return []extsvc.AccountID{"user", "pending_user"}, test.fetchErr
			}

			_, _, err := s.syncRepoPerms(context.Background(), 1, test.noPerms, authz.FetchPermsOptions{})
			if err != nil {
				t.Fatal(err)
			}
		})
	}
}

func TestPermsSyncer_syncPerms(t *testing.T) {
	ctx := context.Background()

	t.Run("invalid type", func(t *testing.T) {
		request := &syncRequest{
			requestMeta: &requestMeta{
				Type: 3,
				ID:   1,
			},
			acquired: true,
		}

		// Request should be removed from the queue even if error occurred.
		s := NewPermsSyncer(logtest.Scoped(t), nil, nil, nil, nil, nil)
		s.queue.Push(request)

		s.syncPerms(context.Background(), nil, request)
		assert.Equal(t, 0, s.queue.Len())
	})

	t.Run("max concurrency", func(t *testing.T) {
		t.Skip("flaky https://github.com/sourcegraph/sourcegraph/issues/40917")
		// Enough buffer to make two goroutines to send data without being blocked, to
		// avoid the case that the second goroutine is blocked by trying to send data to
		// channel rather than being throttled by the max concurrency (1) we imposed.
		wait := make(chan struct{}, 2)
		// Enough buffer to send data twice to avoid blocking on failing test
		ready := make(chan struct{}, 2)
		called := atomic.NewInt32(0)

		users := database.NewMockUserStore()
		users.GetByIDFunc.SetDefaultHook(func(_ context.Context, _ int32) (*types.User, error) {
			wait <- struct{}{}
			called.Inc()
			<-ready

			// We only log errors in `syncPerms` method and return an error here would
			// effectively stop/finish the method call, so we don't need to mock tons of
			// other things.
			return nil, errors.New("fail")
		})
		db := database.NewMockDB()
		db.UsersFunc.SetDefaultReturn(users)

		s := NewPermsSyncer(logtest.Scoped(t), db, nil, nil, timeutil.Now, nil)

		syncGroups := map[requestType]*pool.ContextPool{
			requestTypeUser: pool.New().WithContext(ctx).WithMaxGoroutines(1),
		}

		request1 := &syncRequest{
			requestMeta: &requestMeta{
				Type: requestTypeUser,
				ID:   1,
			},
			acquired: true,
		}
		request2 := &syncRequest{
			requestMeta: &requestMeta{
				Type: requestTypeUser,
				ID:   2,
			},
			acquired: true,
		}
		s.queue.Push(request1)
		s.queue.Push(request2)

		started := make(chan struct{}, 2)
		go func() {
			started <- struct{}{}
			s.syncPerms(ctx, syncGroups, request1)
		}()
		go func() {
			started <- struct{}{}
			s.syncPerms(ctx, syncGroups, request2)
		}()

		getOrFail := func(c <-chan struct{}) (failed bool) {
			select {
			case <-c:
				return false
			case <-time.After(100 * time.Millisecond): // Fail fast if something went wrong
				return true
			}
		}

		// Wait until two goroutine are started
		require.False(t, getOrFail(started))
		require.False(t, getOrFail(started))

		// Only one goroutine should have been called
		require.False(t, getOrFail(wait))
		require.True(t, getOrFail(wait)) // There should be no second signal coming in
		require.Equal(t, int32(1), called.Load())
		ready <- struct{}{} // Unblock the execution of the first goroutine

		// Now the second goroutine should be called
		require.False(t, getOrFail(wait))
		require.Equal(t, int32(2), called.Load())
		ready <- struct{}{}
	})
}
