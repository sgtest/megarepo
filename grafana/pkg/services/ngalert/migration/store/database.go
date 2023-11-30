package store

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/grafana/grafana/pkg/infra/db"
	"github.com/grafana/grafana/pkg/infra/kvstore"
	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/services/accesscontrol"
	legacyalerting "github.com/grafana/grafana/pkg/services/alerting"
	legacymodels "github.com/grafana/grafana/pkg/services/alerting/models"
	"github.com/grafana/grafana/pkg/services/auth/identity"
	"github.com/grafana/grafana/pkg/services/dashboards"
	"github.com/grafana/grafana/pkg/services/datasources"
	"github.com/grafana/grafana/pkg/services/folder"
	apimodels "github.com/grafana/grafana/pkg/services/ngalert/api/tooling/definitions"
	migmodels "github.com/grafana/grafana/pkg/services/ngalert/migration/models"
	"github.com/grafana/grafana/pkg/services/ngalert/models"
	"github.com/grafana/grafana/pkg/services/ngalert/notifier"
	"github.com/grafana/grafana/pkg/services/ngalert/store"
	"github.com/grafana/grafana/pkg/services/org"
	"github.com/grafana/grafana/pkg/services/sqlstore/migrator"
	"github.com/grafana/grafana/pkg/services/user"
	"github.com/grafana/grafana/pkg/setting"
)

// Store is the database abstraction for migration persistence.
type Store interface {
	InsertAlertRules(ctx context.Context, rules ...models.AlertRule) error

	SaveAlertmanagerConfiguration(ctx context.Context, orgID int64, amConfig *apimodels.PostableUserConfig) error

	GetAllOrgs(ctx context.Context) ([]*org.OrgDTO, error)

	GetDatasource(ctx context.Context, datasourceID int64, user identity.Requester) (*datasources.DataSource, error)

	GetNotificationChannels(ctx context.Context, orgID int64) ([]*legacymodels.AlertNotification, error)

	GetOrgDashboardAlerts(ctx context.Context, orgID int64) (map[int64][]*DashAlert, int, error)

	GetDashboardPermissions(ctx context.Context, user identity.Requester, resourceID string) ([]accesscontrol.ResourcePermission, error)
	GetFolderPermissions(ctx context.Context, user identity.Requester, resourceID string) ([]accesscontrol.ResourcePermission, error)
	SetDashboardPermissions(ctx context.Context, orgID int64, resourceID string, commands ...accesscontrol.SetResourcePermissionCommand) ([]accesscontrol.ResourcePermission, error)
	SetFolderPermissions(ctx context.Context, orgID int64, resourceID string, commands ...accesscontrol.SetResourcePermissionCommand) ([]accesscontrol.ResourcePermission, error)
	MapActions(permission accesscontrol.ResourcePermission) string

	GetDashboard(ctx context.Context, orgID int64, id int64) (*dashboards.Dashboard, error)
	GetFolder(ctx context.Context, cmd *folder.GetFolderQuery) (*folder.Folder, error)
	CreateFolder(ctx context.Context, cmd *folder.CreateFolderCommand) (*folder.Folder, error)

	IsMigrated(ctx context.Context, orgID int64) (bool, error)
	SetMigrated(ctx context.Context, orgID int64, migrated bool) error
	GetCurrentAlertingType(ctx context.Context) (AlertingType, error)
	SetCurrentAlertingType(ctx context.Context, t AlertingType) error
	GetOrgMigrationState(ctx context.Context, orgID int64) (*migmodels.OrgMigrationState, error)
	SetOrgMigrationState(ctx context.Context, orgID int64, summary *migmodels.OrgMigrationState) error

	RevertAllOrgs(ctx context.Context) error

	CaseInsensitive() bool
}

type migrationStore struct {
	store                db.DB
	cfg                  *setting.Cfg
	log                  log.Logger
	kv                   kvstore.KVStore
	alertingStore        *store.DBstore
	dashboardService     dashboards.DashboardService
	folderService        folder.Service
	dataSourceCache      datasources.CacheService
	folderPermissions    accesscontrol.FolderPermissionsService
	dashboardPermissions accesscontrol.DashboardPermissionsService
	orgService           org.Service

	legacyAlertNotificationService *legacyalerting.AlertNotificationService
}

// MigrationStore implements the Store interface.
var _ Store = (*migrationStore)(nil)

func ProvideMigrationStore(
	cfg *setting.Cfg,
	sqlStore db.DB,
	kv kvstore.KVStore,
	alertingStore *store.DBstore,
	dashboardService dashboards.DashboardService,
	folderService folder.Service,
	dataSourceCache datasources.CacheService,
	folderPermissions accesscontrol.FolderPermissionsService,
	dashboardPermissions accesscontrol.DashboardPermissionsService,
	orgService org.Service,
	legacyAlertNotificationService *legacyalerting.AlertNotificationService,
) (Store, error) {
	return &migrationStore{
		log:                            log.New("ngalert.migration-store"),
		cfg:                            cfg,
		store:                          sqlStore,
		kv:                             kv,
		alertingStore:                  alertingStore,
		dashboardService:               dashboardService,
		folderService:                  folderService,
		dataSourceCache:                dataSourceCache,
		folderPermissions:              folderPermissions,
		dashboardPermissions:           dashboardPermissions,
		orgService:                     orgService,
		legacyAlertNotificationService: legacyAlertNotificationService,
	}, nil
}

// KVNamespace is the kvstore namespace used for the migration status.
const KVNamespace = "ngalert.migration"

// migratedKey is the kvstore key used for the migration status.
const migratedKey = "migrated"

// stateKey is the kvstore key used for the OrgMigrationState.
const stateKey = "stateKey"

// typeKey is the kvstore key used for the current AlertingType.
const typeKey = "currentAlertingType"

// IsMigrated returns the migration status from the kvstore.
func (ms *migrationStore) IsMigrated(ctx context.Context, orgID int64) (bool, error) {
	kv := kvstore.WithNamespace(ms.kv, orgID, KVNamespace)
	content, exists, err := kv.Get(ctx, migratedKey)
	if err != nil {
		return false, err
	}

	if !exists {
		return false, nil
	}

	return strconv.ParseBool(content)
}

// SetMigrated sets the migration status in the kvstore.
func (ms *migrationStore) SetMigrated(ctx context.Context, orgID int64, migrated bool) error {
	kv := kvstore.WithNamespace(ms.kv, orgID, KVNamespace)
	return kv.Set(ctx, migratedKey, strconv.FormatBool(migrated))
}

// AlertingType represents the current alerting type of Grafana. This is used to detect transitions between
// Legacy and UnifiedAlerting by comparing to the desired type in the configuration.
type AlertingType string

const (
	Legacy          AlertingType = "Legacy"
	UnifiedAlerting AlertingType = "UnifiedAlerting"
)

// typeFromString converts a string to an AlertingType.
func typeFromString(s string) (AlertingType, error) {
	switch s {
	case "Legacy":
		return Legacy, nil
	case "UnifiedAlerting":
		return UnifiedAlerting, nil
	default:
		return "", fmt.Errorf("unknown alerting type: %s", s)
	}
}

const anyOrg = 0

// GetCurrentAlertingType returns the current AlertingType of Grafana.
func (ms *migrationStore) GetCurrentAlertingType(ctx context.Context) (AlertingType, error) {
	kv := kvstore.WithNamespace(ms.kv, anyOrg, KVNamespace)
	content, exists, err := kv.Get(ctx, typeKey)
	if err != nil {
		return "", err
	}

	if !exists {
		return Legacy, nil
	}

	t, err := typeFromString(content)
	if err != nil {
		return "", err
	}

	return t, nil
}

// SetCurrentAlertingType stores the current AlertingType of Grafana.
func (ms *migrationStore) SetCurrentAlertingType(ctx context.Context, t AlertingType) error {
	kv := kvstore.WithNamespace(ms.kv, anyOrg, KVNamespace)
	return kv.Set(ctx, typeKey, string(t))
}

// GetOrgMigrationState returns a summary of a previous migration.
func (ms *migrationStore) GetOrgMigrationState(ctx context.Context, orgID int64) (*migmodels.OrgMigrationState, error) {
	kv := kvstore.WithNamespace(ms.kv, orgID, KVNamespace)
	content, exists, err := kv.Get(ctx, stateKey)
	if err != nil {
		return nil, err
	}

	if !exists {
		return &migmodels.OrgMigrationState{OrgID: orgID}, nil
	}

	var summary migmodels.OrgMigrationState
	err = json.Unmarshal([]byte(content), &summary)
	if err != nil {
		return nil, err
	}

	return &summary, nil
}

// SetOrgMigrationState sets the summary of a previous migration.
func (ms *migrationStore) SetOrgMigrationState(ctx context.Context, orgID int64, summary *migmodels.OrgMigrationState) error {
	kv := kvstore.WithNamespace(ms.kv, orgID, KVNamespace)
	raw, err := json.Marshal(summary)
	if err != nil {
		return err
	}

	return kv.Set(ctx, stateKey, string(raw))
}

func (ms *migrationStore) InsertAlertRules(ctx context.Context, rules ...models.AlertRule) error {
	if ms.store.GetDialect().DriverName() == migrator.Postgres {
		// Postgresql which will automatically rollback the whole transaction on constraint violation.
		// So, for postgresql, insertions will execute in a subtransaction.
		err := ms.store.InTransaction(ctx, func(subCtx context.Context) error {
			_, err := ms.alertingStore.InsertAlertRules(subCtx, rules)
			if err != nil {
				return err
			}
			return nil
		})
		if err != nil {
			return err
		}
	} else {
		_, err := ms.alertingStore.InsertAlertRules(ctx, rules)
		if err != nil {
			return err
		}
	}

	return nil
}

func (ms *migrationStore) SaveAlertmanagerConfiguration(ctx context.Context, orgID int64, amConfig *apimodels.PostableUserConfig) error {
	rawAmConfig, err := json.Marshal(amConfig)
	if err != nil {
		return err
	}

	cmd := models.SaveAlertmanagerConfigurationCmd{
		AlertmanagerConfiguration: string(rawAmConfig),
		ConfigurationVersion:      fmt.Sprintf("v%d", models.AlertConfigurationVersion),
		Default:                   false,
		OrgID:                     orgID,
		LastApplied:               0,
	}
	return ms.alertingStore.SaveAlertmanagerConfiguration(ctx, &cmd)
}

// revertPermissions are the permissions required for the background user to revert the migration.
var revertPermissions = []accesscontrol.Permission{
	{Action: dashboards.ActionFoldersDelete, Scope: dashboards.ScopeFoldersAll},
	{Action: dashboards.ActionFoldersRead, Scope: dashboards.ScopeFoldersAll},
}

// RevertAllOrgs reverts the migration, deleting all unified alerting resources such as alert rules, alertmanager configurations, and silence files.
// In addition, it will delete all folders and permissions originally created by this migration, as well as the various migration statuses stored
// in kvstore, both org-specific and anyOrg.
func (ms *migrationStore) RevertAllOrgs(ctx context.Context) error {
	return ms.store.InTransaction(ctx, func(ctx context.Context) error {
		return ms.store.WithDbSession(ctx, func(sess *db.Session) error {
			if _, err := sess.Exec("DELETE FROM alert_rule"); err != nil {
				return err
			}

			if _, err := sess.Exec("DELETE FROM alert_rule_version"); err != nil {
				return err
			}

			orgs, err := ms.GetAllOrgs(ctx)
			if err != nil {
				return fmt.Errorf("get orgs: %w", err)
			}
			for _, o := range orgs {
				if err := ms.DeleteMigratedFolders(ctx, o.ID); err != nil {
					ms.log.Warn("Failed to delete migrated folders", "orgID", o.ID, "err", err)
					continue
				}
			}

			if _, err := sess.Exec("DELETE FROM alert_configuration"); err != nil {
				return err
			}

			if _, err := sess.Exec("DELETE FROM ngalert_configuration"); err != nil {
				return err
			}

			if _, err := sess.Exec("DELETE FROM alert_instance"); err != nil {
				return err
			}

			if _, err := sess.Exec("DELETE FROM kv_store WHERE namespace = ?", notifier.KVNamespace); err != nil {
				return err
			}

			if _, err := sess.Exec("DELETE FROM kv_store WHERE namespace = ?", KVNamespace); err != nil {
				return err
			}

			files, err := filepath.Glob(filepath.Join(ms.cfg.DataPath, "alerting", "*", "silences"))
			if err != nil {
				return err
			}
			for _, f := range files {
				if err := os.Remove(f); err != nil {
					ms.log.Error("Failed to remove silence file", "file", f, "err", err)
				}
			}

			return nil
		})
	})
}

// DeleteMigratedFolders deletes all folders created by the previous migration run for the given org. This includes all folder permissions.
// If the folder is not empty of all descendants the operation will fail and return an error.
func (ms *migrationStore) DeleteMigratedFolders(ctx context.Context, orgID int64) error {
	summary, err := ms.GetOrgMigrationState(ctx, orgID)
	if err != nil {
		return err
	}
	return ms.DeleteFolders(ctx, orgID, summary.CreatedFolders...)
}

var ErrFolderNotDeleted = fmt.Errorf("folder not deleted")

// DeleteFolders deletes the folders from the given orgs with the given UIDs. This includes all folder permissions.
// If the folder is not empty of all descendants the operation will fail and return an error.
func (ms *migrationStore) DeleteFolders(ctx context.Context, orgID int64, uids ...string) error {
	if len(uids) == 0 {
		return nil
	}

	var errs error
	usr := accesscontrol.BackgroundUser("ngalert_migration_revert", orgID, org.RoleAdmin, revertPermissions)
	for _, folderUID := range uids {
		// Check if folder is empty. If not, we should not delete it.
		uid := folderUID
		countCmd := folder.GetDescendantCountsQuery{
			UID:          &uid,
			OrgID:        orgID,
			SignedInUser: usr.(*user.SignedInUser),
		}
		count, err := ms.folderService.GetDescendantCounts(ctx, &countCmd)
		if err != nil {
			errs = errors.Join(errs, fmt.Errorf("folder %s: %w", folderUID, err))
			continue
		}
		var descendantCounts []string
		var cntErr error
		for kind, cnt := range count {
			if cnt > 0 {
				descendantCounts = append(descendantCounts, fmt.Sprintf("%d %s", cnt, kind))
				if err != nil {
					cntErr = errors.Join(cntErr, err)
					continue
				}
			}
		}
		if cntErr != nil {
			errs = errors.Join(errs, fmt.Errorf("folder %s: %w", folderUID, cntErr))
			continue
		}

		if len(descendantCounts) > 0 {
			errs = errors.Join(errs, fmt.Errorf("folder %s contains descendants: %s", folderUID, strings.Join(descendantCounts, ", ")))
			continue
		}

		cmd := folder.DeleteFolderCommand{
			UID:          uid,
			OrgID:        orgID,
			SignedInUser: usr.(*user.SignedInUser),
		}
		err = ms.folderService.Delete(ctx, &cmd) // Also handles permissions and other related entities.
		if err != nil {
			errs = errors.Join(errs, fmt.Errorf("folder %s: %w", folderUID, err))
			continue
		}
	}
	if errs != nil {
		return fmt.Errorf("%w: %w", ErrFolderNotDeleted, errs)
	}
	return nil
}

func (ms *migrationStore) GetDashboard(ctx context.Context, orgID int64, id int64) (*dashboards.Dashboard, error) {
	return ms.dashboardService.GetDashboard(ctx, &dashboards.GetDashboardQuery{ID: id, OrgID: orgID})
}

func (ms *migrationStore) GetAllOrgs(ctx context.Context) ([]*org.OrgDTO, error) {
	orgQuery := &org.SearchOrgsQuery{}
	return ms.orgService.Search(ctx, orgQuery)
}

func (ms *migrationStore) GetDatasource(ctx context.Context, datasourceID int64, user identity.Requester) (*datasources.DataSource, error) {
	return ms.dataSourceCache.GetDatasource(ctx, datasourceID, user, false)
}

// GetNotificationChannels returns all channels for this org.
func (ms *migrationStore) GetNotificationChannels(ctx context.Context, orgID int64) ([]*legacymodels.AlertNotification, error) {
	return ms.legacyAlertNotificationService.GetAllAlertNotifications(ctx, &legacymodels.GetAllAlertNotificationsQuery{
		OrgID: orgID,
	})
}

// GetOrgDashboardAlerts loads all legacy dashboard alerts for the given org mapped by dashboard id.
func (ms *migrationStore) GetOrgDashboardAlerts(ctx context.Context, orgID int64) (map[int64][]*DashAlert, int, error) {
	var alerts []legacymodels.Alert
	err := ms.store.WithDbSession(ctx, func(sess *db.Session) error {
		return sess.SQL("select * from alert WHERE org_id = ? AND dashboard_id IN (SELECT id from dashboard)", orgID).Find(&alerts)
	})
	if err != nil {
		return nil, 0, err
	}

	mappedAlerts := make(map[int64][]*DashAlert)
	for i := range alerts {
		alert := alerts[i]

		rawSettings, err := json.Marshal(alert.Settings)
		if err != nil {
			return nil, 0, fmt.Errorf("get settings for alert rule ID:%d, name:'%s', orgID:%d: %w", alert.ID, alert.Name, alert.OrgID, err)
		}
		var parsedSettings DashAlertSettings
		err = json.Unmarshal(rawSettings, &parsedSettings)
		if err != nil {
			return nil, 0, fmt.Errorf("parse settings for alert rule ID:%d, name:'%s', orgID:%d: %w", alert.ID, alert.Name, alert.OrgID, err)
		}

		mappedAlerts[alert.DashboardID] = append(mappedAlerts[alert.DashboardID], &DashAlert{
			Alert:          &alerts[i],
			ParsedSettings: &parsedSettings,
		})
	}
	return mappedAlerts, len(alerts), nil
}

func (ms *migrationStore) GetDashboardPermissions(ctx context.Context, user identity.Requester, resourceID string) ([]accesscontrol.ResourcePermission, error) {
	return ms.dashboardPermissions.GetPermissions(ctx, user, resourceID)
}

func (ms *migrationStore) GetFolderPermissions(ctx context.Context, user identity.Requester, resourceID string) ([]accesscontrol.ResourcePermission, error) {
	return ms.folderPermissions.GetPermissions(ctx, user, resourceID)
}

func (ms *migrationStore) GetFolder(ctx context.Context, cmd *folder.GetFolderQuery) (*folder.Folder, error) {
	return ms.folderService.Get(ctx, cmd)
}

func (ms *migrationStore) CreateFolder(ctx context.Context, cmd *folder.CreateFolderCommand) (*folder.Folder, error) {
	return ms.folderService.Create(ctx, cmd)
}

func (ms *migrationStore) SetDashboardPermissions(ctx context.Context, orgID int64, resourceID string, commands ...accesscontrol.SetResourcePermissionCommand) ([]accesscontrol.ResourcePermission, error) {
	return ms.dashboardPermissions.SetPermissions(ctx, orgID, resourceID, commands...)
}

func (ms *migrationStore) SetFolderPermissions(ctx context.Context, orgID int64, resourceID string, commands ...accesscontrol.SetResourcePermissionCommand) ([]accesscontrol.ResourcePermission, error) {
	return ms.folderPermissions.SetPermissions(ctx, orgID, resourceID, commands...)
}

func (ms *migrationStore) MapActions(permission accesscontrol.ResourcePermission) string {
	return ms.dashboardPermissions.MapActions(permission)
}

func (ms *migrationStore) CaseInsensitive() bool {
	return ms.store.GetDialect().SupportEngine()
}
