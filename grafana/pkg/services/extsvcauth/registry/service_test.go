package registry

import (
	"context"
	"testing"

	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/infra/serverlock"
	"github.com/grafana/grafana/pkg/services/extsvcauth"
	"github.com/grafana/grafana/pkg/services/extsvcauth/tests"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/stretchr/testify/mock"
	"github.com/stretchr/testify/require"
)

type TestEnv struct {
	r        *Registry
	oauthReg *tests.ExternalServiceRegistryMock
	saReg    *tests.ExternalServiceRegistryMock
}

// Never lock in tests
type fakeServerLock struct{}

func (f *fakeServerLock) LockExecuteAndReleaseWithRetries(ctx context.Context, actionName string, timeConfig serverlock.LockTimeConfig, fn func(ctx context.Context), retryOpts ...serverlock.RetryOpt) error {
	fn(ctx)
	return nil
}

func setupTestEnv(t *testing.T) *TestEnv {
	env := TestEnv{}
	env.oauthReg = tests.NewExternalServiceRegistryMock(t)
	env.saReg = tests.NewExternalServiceRegistryMock(t)
	env.r = &Registry{
		features:        featuremgmt.WithFeatures(featuremgmt.FlagExternalServiceAuth, featuremgmt.FlagExternalServiceAccounts),
		logger:          log.New("extsvcauth.registry.test"),
		oauthReg:        env.oauthReg,
		saReg:           env.saReg,
		extSvcProviders: map[string]extsvcauth.AuthProvider{},
		serverLock:      &fakeServerLock{},
	}
	return &env
}

func TestRegistry_CleanUpOrphanedExternalServices(t *testing.T) {
	tests := []struct {
		name string
		init func(*TestEnv)
	}{
		{
			name: "should not clean up when every service registered",
			init: func(te *TestEnv) {
				// Have registered two services one requested a service account, the other requested to be an oauth client
				te.r.extSvcProviders = map[string]extsvcauth.AuthProvider{"sa-svc": extsvcauth.ServiceAccounts, "oauth-svc": extsvcauth.OAuth2Server}

				te.oauthReg.On("GetExternalServiceNames", mock.Anything).Return([]string{"oauth-svc"}, nil)
				// Also return the external service account attached to the OAuth Server
				te.saReg.On("GetExternalServiceNames", mock.Anything).Return([]string{"sa-svc", "oauth-svc"}, nil)
			},
		},
		{
			name: "should clean up an orphaned service account",
			init: func(te *TestEnv) {
				// Have registered two services one requested a service account, the other requested to be an oauth client
				te.r.extSvcProviders = map[string]extsvcauth.AuthProvider{"sa-svc": extsvcauth.ServiceAccounts, "oauth-svc": extsvcauth.OAuth2Server}

				te.oauthReg.On("GetExternalServiceNames", mock.Anything).Return([]string{"oauth-svc"}, nil)
				// Also return the external service account attached to the OAuth Server
				te.saReg.On("GetExternalServiceNames", mock.Anything).Return([]string{"sa-svc", "orphaned-sa-svc", "oauth-svc"}, nil)

				te.saReg.On("RemoveExternalService", mock.Anything, "orphaned-sa-svc").Return(nil)
			},
		},
		{
			name: "should clean up an orphaned OAuth Client",
			init: func(te *TestEnv) {
				// Have registered two services one requested a service account, the other requested to be an oauth client
				te.r.extSvcProviders = map[string]extsvcauth.AuthProvider{"sa-svc": extsvcauth.ServiceAccounts, "oauth-svc": extsvcauth.OAuth2Server}

				te.oauthReg.On("GetExternalServiceNames", mock.Anything).Return([]string{"oauth-svc", "orphaned-oauth-svc"}, nil)
				// Also return the external service account attached to the OAuth Server
				te.saReg.On("GetExternalServiceNames", mock.Anything).Return([]string{"sa-svc", "orphaned-oauth-svc", "oauth-svc"}, nil)

				te.oauthReg.On("RemoveExternalService", mock.Anything, "orphaned-oauth-svc").Return(nil)
			},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			env := setupTestEnv(t)
			tt.init(env)

			err := env.r.CleanUpOrphanedExternalServices(context.Background())
			require.NoError(t, err)

			env.oauthReg.AssertExpectations(t)
			env.saReg.AssertExpectations(t)
		})
	}
}

func TestRegistry_GetExternalServiceNames(t *testing.T) {
	tests := []struct {
		name string
		init func(*TestEnv)
		want []string
	}{
		{
			name: "should deduplicate names",
			init: func(te *TestEnv) {
				te.saReg.On("GetExternalServiceNames", mock.Anything).Return([]string{"sa-svc", "oauth-svc"}, nil)
				te.oauthReg.On("GetExternalServiceNames", mock.Anything).Return([]string{"oauth-svc"}, nil)
			},
			want: []string{"sa-svc", "oauth-svc"},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			env := setupTestEnv(t)
			tt.init(env)

			names, err := env.r.GetExternalServiceNames(context.Background())
			require.NoError(t, err)
			require.ElementsMatch(t, tt.want, names)

			env.oauthReg.AssertExpectations(t)
			env.saReg.AssertExpectations(t)
		})
	}
}
