package migration

import (
	"context"
	"testing"

	"github.com/grafana/grafana/pkg/infra/log/logtest"
	"github.com/grafana/grafana/pkg/infra/serverlock"
	"github.com/grafana/grafana/pkg/infra/tracing"
	apimodels "github.com/grafana/grafana/pkg/services/ngalert/api/tooling/definitions"
	migrationStore "github.com/grafana/grafana/pkg/services/ngalert/migration/store"
	fake_secrets "github.com/grafana/grafana/pkg/services/secrets/fakes"
	"github.com/grafana/grafana/pkg/services/sqlstore"
	"github.com/grafana/grafana/pkg/setting"
)

func NewTestMigrationService(t *testing.T, sqlStore *sqlstore.SQLStore, cfg *setting.Cfg) *migrationService {
	t.Helper()
	if cfg == nil {
		cfg = setting.NewCfg()
	}
	return &migrationService{
		lock:              serverlock.ProvideService(sqlStore, tracing.InitializeTracerForTest()),
		log:               &logtest.Fake{},
		cfg:               cfg,
		store:             sqlStore,
		migrationStore:    migrationStore.NewTestMigrationStore(t, sqlStore, cfg),
		encryptionService: fake_secrets.NewFakeSecretsService(),
	}
}

func NewFakeMigrationService(t testing.TB) *fakeMigrationService {
	t.Helper()
	return &fakeMigrationService{}
}

type fakeMigrationService struct {
}

func (ms *fakeMigrationService) Run(_ context.Context) error {
	// Do nothing.
	return nil
}

func (ms *fakeMigrationService) MigrateAlert(ctx context.Context, orgID int64, dashboardID int64, panelID int64) (apimodels.OrgMigrationSummary, error) {
	//TODO implement me
	panic("implement me")
}

func (ms *fakeMigrationService) MigrateDashboardAlerts(ctx context.Context, orgID int64, dashboardID int64, skipExisting bool) (apimodels.OrgMigrationSummary, error) {
	//TODO implement me
	panic("implement me")
}

func (ms *fakeMigrationService) MigrateAllDashboardAlerts(ctx context.Context, orgID int64, skipExisting bool) (apimodels.OrgMigrationSummary, error) {
	//TODO implement me
	panic("implement me")
}

func (ms *fakeMigrationService) MigrateChannel(ctx context.Context, orgID int64, channelID int64) (apimodels.OrgMigrationSummary, error) {
	//TODO implement me
	panic("implement me")
}

func (ms *fakeMigrationService) MigrateAllChannels(ctx context.Context, orgID int64, skipExisting bool) (apimodels.OrgMigrationSummary, error) {
	//TODO implement me
	panic("implement me")
}

func (ms *fakeMigrationService) MigrateOrg(ctx context.Context, orgID int64, skipExisting bool) (apimodels.OrgMigrationSummary, error) {
	//TODO implement me
	panic("implement me")
}

func (ms *fakeMigrationService) GetOrgMigrationState(ctx context.Context, orgID int64) (*apimodels.OrgMigrationState, error) {
	//TODO implement me
	panic("implement me")
}

func (ms *fakeMigrationService) RevertOrg(ctx context.Context, orgID int64) error {
	//TODO implement me
	panic("implement me")
}
