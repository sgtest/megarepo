package service

import (
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/prometheus/client_golang/prometheus"

	"golang.org/x/exp/slices"

	"github.com/grafana/grafana-plugin-sdk-go/backend/gtime"

	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/services/accesscontrol"
	"github.com/grafana/grafana/pkg/services/alerting"
	"github.com/grafana/grafana/pkg/services/auth/identity"
	"github.com/grafana/grafana/pkg/services/dashboards"
	"github.com/grafana/grafana/pkg/services/dashboards/dashboardaccess"
	"github.com/grafana/grafana/pkg/services/datasources"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/services/folder"
	"github.com/grafana/grafana/pkg/services/guardian"
	"github.com/grafana/grafana/pkg/services/org"
	"github.com/grafana/grafana/pkg/services/search/model"
	"github.com/grafana/grafana/pkg/services/store/entity"
	"github.com/grafana/grafana/pkg/setting"
	"github.com/grafana/grafana/pkg/util"
)

var (
	provisionerPermissions = []accesscontrol.Permission{
		{Action: dashboards.ActionFoldersCreate},
		{Action: dashboards.ActionFoldersWrite, Scope: dashboards.ScopeFoldersAll},
		{Action: dashboards.ActionDashboardsCreate, Scope: dashboards.ScopeFoldersAll},
		{Action: dashboards.ActionDashboardsWrite, Scope: dashboards.ScopeFoldersAll},
		{Action: datasources.ActionRead, Scope: datasources.ScopeAll},
	}
	// DashboardServiceImpl implements the DashboardService interface
	_ dashboards.DashboardService             = (*DashboardServiceImpl)(nil)
	_ dashboards.DashboardProvisioningService = (*DashboardServiceImpl)(nil)
	_ dashboards.PluginService                = (*DashboardServiceImpl)(nil)
)

type DashboardServiceImpl struct {
	cfg                  *setting.Cfg
	log                  log.Logger
	dashboardStore       dashboards.Store
	folderStore          folder.FolderStore
	folderService        folder.Service
	dashAlertExtractor   alerting.DashAlertExtractor
	features             featuremgmt.FeatureToggles
	folderPermissions    accesscontrol.FolderPermissionsService
	dashboardPermissions accesscontrol.DashboardPermissionsService
	ac                   accesscontrol.AccessControl
	metrics              *dashboardsMetrics
}

// This is the uber service that implements a three smaller services
func ProvideDashboardServiceImpl(
	cfg *setting.Cfg, dashboardStore dashboards.Store, folderStore folder.FolderStore, dashAlertExtractor alerting.DashAlertExtractor,
	features featuremgmt.FeatureToggles, folderPermissionsService accesscontrol.FolderPermissionsService,
	dashboardPermissionsService accesscontrol.DashboardPermissionsService, ac accesscontrol.AccessControl,
	folderSvc folder.Service, r prometheus.Registerer,
) (*DashboardServiceImpl, error) {
	dashSvc := &DashboardServiceImpl{
		cfg:                  cfg,
		log:                  log.New("dashboard-service"),
		dashboardStore:       dashboardStore,
		dashAlertExtractor:   dashAlertExtractor,
		features:             features,
		folderPermissions:    folderPermissionsService,
		dashboardPermissions: dashboardPermissionsService,
		ac:                   ac,
		folderStore:          folderStore,
		folderService:        folderSvc,
		metrics:              newDashboardsMetrics(r),
	}

	ac.RegisterScopeAttributeResolver(dashboards.NewDashboardIDScopeResolver(folderStore, dashSvc, folderSvc))
	ac.RegisterScopeAttributeResolver(dashboards.NewDashboardUIDScopeResolver(folderStore, dashSvc, folderSvc))

	if err := folderSvc.RegisterService(dashSvc); err != nil {
		return nil, err
	}

	return dashSvc, nil
}

func (dr *DashboardServiceImpl) GetProvisionedDashboardData(ctx context.Context, name string) ([]*dashboards.DashboardProvisioning, error) {
	return dr.dashboardStore.GetProvisionedDashboardData(ctx, name)
}

func (dr *DashboardServiceImpl) GetProvisionedDashboardDataByDashboardID(ctx context.Context, dashboardID int64) (*dashboards.DashboardProvisioning, error) {
	return dr.dashboardStore.GetProvisionedDataByDashboardID(ctx, dashboardID)
}

func (dr *DashboardServiceImpl) GetProvisionedDashboardDataByDashboardUID(ctx context.Context, orgID int64, dashboardUID string) (*dashboards.DashboardProvisioning, error) {
	return dr.dashboardStore.GetProvisionedDataByDashboardUID(ctx, orgID, dashboardUID)
}

//nolint:gocyclo
func (dr *DashboardServiceImpl) BuildSaveDashboardCommand(ctx context.Context, dto *dashboards.SaveDashboardDTO, shouldValidateAlerts bool,
	validateProvisionedDashboard bool) (*dashboards.SaveDashboardCommand, error) {
	dash := dto.Dashboard

	dash.OrgID = dto.OrgID
	dash.Title = strings.TrimSpace(dash.Title)
	dash.Data.Set("title", dash.Title)
	dash.SetUID(strings.TrimSpace(dash.UID))

	if dash.Title == "" {
		return nil, dashboards.ErrDashboardTitleEmpty
	}

	// nolint:staticcheck
	if dash.IsFolder && dash.FolderID > 0 {
		return nil, dashboards.ErrDashboardFolderCannotHaveParent
	}

	if dash.IsFolder && strings.EqualFold(dash.Title, dashboards.RootFolderName) {
		return nil, dashboards.ErrDashboardFolderNameExists
	}

	if !util.IsValidShortUID(dash.UID) {
		return nil, dashboards.ErrDashboardInvalidUid
	} else if util.IsShortUIDTooLong(dash.UID) {
		return nil, dashboards.ErrDashboardUidTooLong
	}

	if err := validateDashboardRefreshInterval(dash); err != nil {
		return nil, err
	}

	if shouldValidateAlerts {
		dashAlertInfo := alerting.DashAlertInfo{Dash: dash, User: dto.User, OrgID: dash.OrgID}
		if err := dr.dashAlertExtractor.ValidateAlerts(ctx, dashAlertInfo); err != nil {
			return nil, err
		}
	}

	// Validate folder
	if dash.FolderUID != "" {
		folder, err := dr.folderStore.GetFolderByUID(ctx, dash.OrgID, dash.FolderUID)
		if err != nil {
			return nil, err
		}
		// nolint:staticcheck
		dash.FolderID = folder.ID
	} else if dash.FolderID != 0 { // nolint:staticcheck
		// nolint:staticcheck
		folder, err := dr.folderStore.GetFolderByID(ctx, dash.OrgID, dash.FolderID)
		if err != nil {
			return nil, err
		}
		dash.FolderUID = folder.UID
	}

	isParentFolderChanged, err := dr.dashboardStore.ValidateDashboardBeforeSave(ctx, dash, dto.Overwrite)
	if err != nil {
		return nil, err
	}

	if isParentFolderChanged {
		// Check that the user is allowed to add a dashboard to the folder
		guardian, err := guardian.NewByDashboard(ctx, dash, dto.OrgID, dto.User)
		if err != nil {
			return nil, err
		}
		// nolint:staticcheck
		if canSave, err := guardian.CanCreate(dash.FolderID, dash.IsFolder); err != nil || !canSave {
			if err != nil {
				return nil, err
			}
			return nil, dashboards.ErrDashboardUpdateAccessDenied
		}
	}

	if validateProvisionedDashboard {
		provisionedData, err := dr.GetProvisionedDashboardDataByDashboardID(ctx, dash.ID)
		if err != nil {
			return nil, err
		}

		if provisionedData != nil {
			return nil, dashboards.ErrDashboardCannotSaveProvisionedDashboard
		}
	}

	guard, err := getGuardianForSavePermissionCheck(ctx, dash, dto.User)
	if err != nil {
		return nil, err
	}

	if dash.ID == 0 {
		// nolint:staticcheck
		if canCreate, err := guard.CanCreate(dash.FolderID, dash.IsFolder); err != nil || !canCreate {
			if err != nil {
				return nil, err
			}
			return nil, dashboards.ErrDashboardUpdateAccessDenied
		}
	} else {
		if canSave, err := guard.CanSave(); err != nil || !canSave {
			if err != nil {
				return nil, err
			}
			return nil, dashboards.ErrDashboardUpdateAccessDenied
		}
	}

	userID, err := resolveUserID(dto.User, dr.log)
	if err != nil {
		return nil, err
	}

	cmd := &dashboards.SaveDashboardCommand{
		Dashboard: dash.Data,
		Message:   dto.Message,
		OrgID:     dto.OrgID,
		Overwrite: dto.Overwrite,
		UserID:    userID,
		FolderID:  dash.FolderID, // nolint:staticcheck
		FolderUID: dash.FolderUID,
		IsFolder:  dash.IsFolder,
		PluginID:  dash.PluginID,
	}

	if !dto.UpdatedAt.IsZero() {
		cmd.UpdatedAt = dto.UpdatedAt
	}

	return cmd, nil
}

func resolveUserID(user identity.Requester, log log.Logger) (int64, error) {
	userID := int64(0)
	namespaceID, identifier := user.GetNamespacedID()
	if namespaceID != identity.NamespaceUser && namespaceID != identity.NamespaceServiceAccount {
		log.Debug("User does not belong to a user or service account namespace", "namespaceID", namespaceID, "userID", identifier)
	}

	userID, err := identity.IntIdentifier(namespaceID, identifier)

	if err != nil {
		log.Debug("failed to parse user ID", "namespaceID", namespaceID, "userID", identifier, "error", err)
	}
	return userID, nil
}

func (dr *DashboardServiceImpl) DeleteOrphanedProvisionedDashboards(ctx context.Context, cmd *dashboards.DeleteOrphanedProvisionedDashboardsCommand) error {
	return dr.dashboardStore.DeleteOrphanedProvisionedDashboards(ctx, cmd)
}

// getGuardianForSavePermissionCheck returns the guardian to be used for checking permission of dashboard
// It replaces deleted Dashboard.GetDashboardIdForSavePermissionCheck()
func getGuardianForSavePermissionCheck(ctx context.Context, d *dashboards.Dashboard, user identity.Requester) (guardian.DashboardGuardian, error) {
	newDashboard := d.ID == 0

	if newDashboard {
		// if it's a new dashboard/folder check the parent folder permissions
		// nolint:staticcheck
		guard, err := guardian.New(ctx, d.FolderID, d.OrgID, user)
		if err != nil {
			return nil, err
		}
		return guard, nil
	}
	guard, err := guardian.NewByDashboard(ctx, d, d.OrgID, user)
	if err != nil {
		return nil, err
	}
	return guard, nil
}

func validateDashboardRefreshInterval(dash *dashboards.Dashboard) error {
	if setting.MinRefreshInterval == "" {
		return nil
	}

	refresh := dash.Data.Get("refresh").MustString("")
	if refresh == "" {
		// since no refresh is set it is a valid refresh rate
		return nil
	}

	minRefreshInterval, err := gtime.ParseDuration(setting.MinRefreshInterval)
	if err != nil {
		return fmt.Errorf("parsing min refresh interval %q failed: %w", setting.MinRefreshInterval, err)
	}
	d, err := gtime.ParseDuration(refresh)
	if err != nil {
		return fmt.Errorf("parsing refresh duration %q failed: %w", refresh, err)
	}

	if d < minRefreshInterval {
		return dashboards.ErrDashboardRefreshIntervalTooShort
	}

	return nil
}

func (dr *DashboardServiceImpl) SaveProvisionedDashboard(ctx context.Context, dto *dashboards.SaveDashboardDTO,
	provisioning *dashboards.DashboardProvisioning) (*dashboards.Dashboard, error) {
	if err := validateDashboardRefreshInterval(dto.Dashboard); err != nil {
		dr.log.Warn("Changing refresh interval for provisioned dashboard to minimum refresh interval", "dashboardUid",
			dto.Dashboard.UID, "dashboardTitle", dto.Dashboard.Title, "minRefreshInterval", setting.MinRefreshInterval)
		dto.Dashboard.Data.Set("refresh", setting.MinRefreshInterval)
	}

	dto.User = accesscontrol.BackgroundUser("dashboard_provisioning", dto.OrgID, org.RoleAdmin, provisionerPermissions)

	cmd, err := dr.BuildSaveDashboardCommand(ctx, dto, setting.IsLegacyAlertingEnabled(), false)
	if err != nil {
		return nil, err
	}

	// dashboard
	dash, err := dr.dashboardStore.SaveProvisionedDashboard(ctx, *cmd, provisioning)
	if err != nil {
		return nil, err
	}

	// alerts
	dashAlertInfo := alerting.DashAlertInfo{
		User:  dto.User,
		Dash:  dash,
		OrgID: dto.OrgID,
	}

	// extract/save legacy alerts only if legacy alerting is enabled
	if setting.IsLegacyAlertingEnabled() {
		alerts, err := dr.dashAlertExtractor.GetAlerts(ctx, dashAlertInfo)
		if err != nil {
			return nil, err
		}

		err = dr.dashboardStore.SaveAlerts(ctx, dash.ID, alerts)
		if err != nil {
			return nil, err
		}
	}

	if dto.Dashboard.ID == 0 {
		dr.setDefaultPermissions(ctx, dto, dash, true)
	}

	return dash, nil
}

func (dr *DashboardServiceImpl) SaveFolderForProvisionedDashboards(ctx context.Context, dto *dashboards.SaveDashboardDTO) (*dashboards.Dashboard, error) {
	dto.User = accesscontrol.BackgroundUser("dashboard_provisioning", dto.OrgID, org.RoleAdmin, provisionerPermissions)
	cmd, err := dr.BuildSaveDashboardCommand(ctx, dto, false, false)
	if err != nil {
		return nil, err
	}

	dash, err := dr.dashboardStore.SaveDashboard(ctx, *cmd)
	if err != nil {
		return nil, err
	}

	dashAlertInfo := alerting.DashAlertInfo{
		User:  dto.User,
		Dash:  dash,
		OrgID: dto.OrgID,
	}

	// extract/save legacy alerts only if legacy alerting is enabled
	if setting.IsLegacyAlertingEnabled() {
		alerts, err := dr.dashAlertExtractor.GetAlerts(ctx, dashAlertInfo)
		if err != nil {
			return nil, err
		}

		err = dr.dashboardStore.SaveAlerts(ctx, dash.ID, alerts)
		if err != nil {
			return nil, err
		}
	}

	if dto.Dashboard.ID == 0 {
		dr.setDefaultPermissions(ctx, dto, dash, true)
	}

	return dash, nil
}

func (dr *DashboardServiceImpl) SaveDashboard(ctx context.Context, dto *dashboards.SaveDashboardDTO,
	allowUiUpdate bool) (*dashboards.Dashboard, error) {
	if err := validateDashboardRefreshInterval(dto.Dashboard); err != nil {
		dr.log.Warn("Changing refresh interval for imported dashboard to minimum refresh interval",
			"dashboardUid", dto.Dashboard.UID, "dashboardTitle", dto.Dashboard.Title, "minRefreshInterval",
			setting.MinRefreshInterval)
		dto.Dashboard.Data.Set("refresh", setting.MinRefreshInterval)
	}

	cmd, err := dr.BuildSaveDashboardCommand(ctx, dto, setting.IsLegacyAlertingEnabled(), !allowUiUpdate)
	if err != nil {
		return nil, err
	}

	dash, err := dr.dashboardStore.SaveDashboard(ctx, *cmd)
	if err != nil {
		return nil, fmt.Errorf("saving dashboard failed: %w", err)
	}

	dashAlertInfo := alerting.DashAlertInfo{
		User:  dto.User,
		Dash:  dash,
		OrgID: dto.OrgID,
	}

	// extract/save legacy alerts only if legacy alerting is enabled
	if setting.IsLegacyAlertingEnabled() {
		alerts, err := dr.dashAlertExtractor.GetAlerts(ctx, dashAlertInfo)
		if err != nil {
			return nil, err
		}

		err = dr.dashboardStore.SaveAlerts(ctx, dash.ID, alerts)
		if err != nil {
			return nil, err
		}
	}

	// new dashboard created
	if dto.Dashboard.ID == 0 {
		dr.setDefaultPermissions(ctx, dto, dash, false)
	}

	return dash, nil
}

// DeleteDashboard removes dashboard from the DB. Errors out if the dashboard was provisioned. Should be used for
// operations by the user where we want to make sure user does not delete provisioned dashboard.
func (dr *DashboardServiceImpl) DeleteDashboard(ctx context.Context, dashboardId int64, orgId int64) error {
	return dr.deleteDashboard(ctx, dashboardId, orgId, true)
}

func (dr *DashboardServiceImpl) GetDashboardByPublicUid(ctx context.Context, dashboardPublicUid string) (*dashboards.Dashboard, error) {
	return nil, nil
}

// DeleteProvisionedDashboard removes dashboard from the DB even if it is provisioned.
func (dr *DashboardServiceImpl) DeleteProvisionedDashboard(ctx context.Context, dashboardId int64, orgId int64) error {
	return dr.deleteDashboard(ctx, dashboardId, orgId, false)
}

func (dr *DashboardServiceImpl) deleteDashboard(ctx context.Context, dashboardId int64, orgId int64, validateProvisionedDashboard bool) error {
	if validateProvisionedDashboard {
		provisionedData, err := dr.GetProvisionedDashboardDataByDashboardID(ctx, dashboardId)
		if err != nil {
			return fmt.Errorf("%v: %w", "failed to check if dashboard is provisioned", err)
		}

		if provisionedData != nil {
			return dashboards.ErrDashboardCannotDeleteProvisionedDashboard
		}
	}
	cmd := &dashboards.DeleteDashboardCommand{OrgID: orgId, ID: dashboardId}
	return dr.dashboardStore.DeleteDashboard(ctx, cmd)
}

func (dr *DashboardServiceImpl) ImportDashboard(ctx context.Context, dto *dashboards.SaveDashboardDTO) (
	*dashboards.Dashboard, error) {
	if err := validateDashboardRefreshInterval(dto.Dashboard); err != nil {
		dr.log.Warn("Changing refresh interval for imported dashboard to minimum refresh interval",
			"dashboardUid", dto.Dashboard.UID, "dashboardTitle", dto.Dashboard.Title,
			"minRefreshInterval", setting.MinRefreshInterval)
		dto.Dashboard.Data.Set("refresh", setting.MinRefreshInterval)
	}

	cmd, err := dr.BuildSaveDashboardCommand(ctx, dto, false, true)
	if err != nil {
		return nil, err
	}

	dash, err := dr.dashboardStore.SaveDashboard(ctx, *cmd)
	if err != nil {
		return nil, err
	}

	dr.setDefaultPermissions(ctx, dto, dash, false)

	return dash, nil
}

// UnprovisionDashboard removes info about dashboard being provisioned. Used after provisioning configs are changed
// and provisioned dashboards are left behind but not deleted.
func (dr *DashboardServiceImpl) UnprovisionDashboard(ctx context.Context, dashboardId int64) error {
	return dr.dashboardStore.UnprovisionDashboard(ctx, dashboardId)
}

func (dr *DashboardServiceImpl) GetDashboardsByPluginID(ctx context.Context, query *dashboards.GetDashboardsByPluginIDQuery) ([]*dashboards.Dashboard, error) {
	return dr.dashboardStore.GetDashboardsByPluginID(ctx, query)
}

func (dr *DashboardServiceImpl) setDefaultPermissions(ctx context.Context, dto *dashboards.SaveDashboardDTO, dash *dashboards.Dashboard, provisioned bool) {
	// nolint:staticcheck
	inFolder := dash.FolderID > 0
	var permissions []accesscontrol.SetResourcePermissionCommand

	if !provisioned {
		namespaceID, userIDstr := dto.User.GetNamespacedID()
		userID, err := identity.IntIdentifier(namespaceID, userIDstr)

		if err != nil {
			dr.log.Error("Could not make user admin", "dashboard", dash.Title, "namespaceID", namespaceID, "userID", userID, "error", err)
		} else if namespaceID == identity.NamespaceUser && userID > 0 {
			permissions = append(permissions, accesscontrol.SetResourcePermissionCommand{
				UserID: userID, Permission: dashboardaccess.PERMISSION_ADMIN.String(),
			})
		}
	}

	if !inFolder {
		permissions = append(permissions, []accesscontrol.SetResourcePermissionCommand{
			{BuiltinRole: string(org.RoleEditor), Permission: dashboardaccess.PERMISSION_EDIT.String()},
			{BuiltinRole: string(org.RoleViewer), Permission: dashboardaccess.PERMISSION_VIEW.String()},
		}...)
	}

	svc := dr.dashboardPermissions
	if dash.IsFolder {
		svc = dr.folderPermissions
	}

	if _, err := svc.SetPermissions(ctx, dto.OrgID, dash.UID, permissions...); err != nil {
		dr.log.Error("Could not set default permissions", "dashboard", dash.Title, "error", err)
	}
}

func (dr *DashboardServiceImpl) GetDashboard(ctx context.Context, query *dashboards.GetDashboardQuery) (*dashboards.Dashboard, error) {
	return dr.dashboardStore.GetDashboard(ctx, query)
}

func (dr *DashboardServiceImpl) GetDashboardUIDByID(ctx context.Context, query *dashboards.GetDashboardRefByIDQuery) (*dashboards.DashboardRef, error) {
	return dr.dashboardStore.GetDashboardUIDByID(ctx, query)
}

func (dr *DashboardServiceImpl) GetDashboards(ctx context.Context, query *dashboards.GetDashboardsQuery) ([]*dashboards.Dashboard, error) {
	return dr.dashboardStore.GetDashboards(ctx, query)
}

func (dr *DashboardServiceImpl) GetDashboardsSharedWithUser(ctx context.Context, user identity.Requester) ([]*dashboards.Dashboard, error) {
	return dr.getDashboardsSharedWithUser(ctx, user)
}

func (dr *DashboardServiceImpl) getDashboardsSharedWithUser(ctx context.Context, user identity.Requester) ([]*dashboards.Dashboard, error) {
	permissions := user.GetPermissions()
	dashboardPermissions := permissions[dashboards.ActionDashboardsRead]
	sharedDashboards := make([]*dashboards.Dashboard, 0)
	dashboardUids := make([]string, 0)
	for _, p := range dashboardPermissions {
		if dashboardUid, found := strings.CutPrefix(p, dashboards.ScopeDashboardsPrefix); found {
			if !slices.Contains(dashboardUids, dashboardUid) {
				dashboardUids = append(dashboardUids, dashboardUid)
			}
		}
	}

	if len(dashboardUids) == 0 {
		return sharedDashboards, nil
	}

	dashboardsQuery := &dashboards.GetDashboardsQuery{
		DashboardUIDs: dashboardUids,
		OrgID:         user.GetOrgID(),
	}
	sharedDashboards, err := dr.dashboardStore.GetDashboards(ctx, dashboardsQuery)
	if err != nil {
		return nil, err
	}
	return dr.filterUserSharedDashboards(ctx, user, sharedDashboards)
}

// filterUserSharedDashboards filter dashboards directly assigned to user, but not located in folders with view permissions
func (dr *DashboardServiceImpl) filterUserSharedDashboards(ctx context.Context, user identity.Requester, userDashboards []*dashboards.Dashboard) ([]*dashboards.Dashboard, error) {
	filteredDashboards := make([]*dashboards.Dashboard, 0)

	folderUIDs := make([]string, 0)
	for _, dashboard := range userDashboards {
		folderUIDs = append(folderUIDs, dashboard.FolderUID)
	}

	dashFolders, err := dr.folderStore.GetFolders(ctx, user.GetOrgID(), folderUIDs)
	if err != nil {
		return nil, folder.ErrInternal.Errorf("failed to fetch parent folders from store: %w", err)
	}

	for _, dashboard := range userDashboards {
		// Filter out dashboards if user has access to parent folder
		if dashboard.FolderUID == "" {
			continue
		}

		dashFolder, ok := dashFolders[dashboard.FolderUID]
		if !ok {
			dr.log.Error("failed to fetch folder by UID from store", "uid", dashboard.FolderUID)
			continue
		}

		g, err := guardian.NewByFolder(ctx, dashFolder, user.GetOrgID(), user)
		if err != nil {
			dr.log.Error("failed to check folder permissions", "folder uid", dashboard.FolderUID, "error", err)
			continue
		}

		canView, err := g.CanView()
		if err != nil {
			dr.log.Error("failed to fetch dashboard", "uid", dashboard.UID, "error", err)
			continue
		}
		if !canView {
			filteredDashboards = append(filteredDashboards, dashboard)
		}
	}
	return filteredDashboards, nil
}

func (dr *DashboardServiceImpl) getUserSharedDashboardUIDs(ctx context.Context, user identity.Requester) ([]string, error) {
	userDashboards, err := dr.getDashboardsSharedWithUser(ctx, user)
	if err != nil {
		return nil, err
	}
	userDashboardUIDs := make([]string, 0)
	for _, dashboard := range userDashboards {
		userDashboardUIDs = append(userDashboardUIDs, dashboard.UID)
	}
	return userDashboardUIDs, nil
}

func (dr *DashboardServiceImpl) FindDashboards(ctx context.Context, query *dashboards.FindPersistedDashboardsQuery) ([]dashboards.DashboardSearchProjection, error) {
	if dr.features.IsEnabled(ctx, featuremgmt.FlagNestedFolders) && len(query.FolderUIDs) > 0 && slices.Contains(query.FolderUIDs, folder.SharedWithMeFolderUID) {
		start := time.Now()
		userDashboardUIDs, err := dr.getUserSharedDashboardUIDs(ctx, query.SignedInUser)
		if err != nil {
			dr.metrics.sharedWithMeFetchDashboardsRequestsDuration.WithLabelValues("failure").Observe(time.Since(start).Seconds())
			return nil, err
		}
		if len(userDashboardUIDs) == 0 {
			return []dashboards.DashboardSearchProjection{}, nil
		}
		query.DashboardUIDs = userDashboardUIDs
		query.FolderUIDs = []string{}

		defer func(t time.Time) {
			dr.metrics.sharedWithMeFetchDashboardsRequestsDuration.WithLabelValues("success").Observe(time.Since(start).Seconds())
		}(time.Now())
	}
	return dr.dashboardStore.FindDashboards(ctx, query)
}

func (dr *DashboardServiceImpl) SearchDashboards(ctx context.Context, query *dashboards.FindPersistedDashboardsQuery) (model.HitList, error) {
	res, err := dr.FindDashboards(ctx, query)
	if err != nil {
		return nil, err
	}

	hits := makeQueryResult(query, res)

	return hits, nil
}

func getHitType(item dashboards.DashboardSearchProjection) model.HitType {
	var hitType model.HitType
	if item.IsFolder {
		hitType = model.DashHitFolder
	} else {
		hitType = model.DashHitDB
	}

	return hitType
}

func makeQueryResult(query *dashboards.FindPersistedDashboardsQuery, res []dashboards.DashboardSearchProjection) model.HitList {
	hitList := make([]*model.Hit, 0)
	hits := make(map[int64]*model.Hit)

	for _, item := range res {
		hit, exists := hits[item.ID]
		if !exists {
			hit = &model.Hit{
				ID:          item.ID,
				UID:         item.UID,
				Title:       item.Title,
				URI:         "db/" + item.Slug,
				URL:         dashboards.GetDashboardFolderURL(item.IsFolder, item.UID, item.Slug),
				Type:        getHitType(item),
				FolderID:    item.FolderID, // nolint:staticcheck
				FolderUID:   item.FolderUID,
				FolderTitle: item.FolderTitle,
				Tags:        []string{},
			}

			// nolint:staticcheck
			if item.FolderID > 0 {
				hit.FolderURL = dashboards.GetFolderURL(item.FolderUID, item.FolderSlug)
			}

			if query.Sort.MetaName != "" {
				hit.SortMeta = item.SortMeta
				hit.SortMetaName = query.Sort.MetaName
			}

			hitList = append(hitList, hit)
			hits[item.ID] = hit
		}
		if len(item.Term) > 0 {
			hit.Tags = append(hit.Tags, item.Term)
		}
	}
	return hitList
}

func (dr *DashboardServiceImpl) GetDashboardTags(ctx context.Context, query *dashboards.GetDashboardTagsQuery) ([]*dashboards.DashboardTagCloudItem, error) {
	return dr.dashboardStore.GetDashboardTags(ctx, query)
}

func (dr DashboardServiceImpl) CountInFolder(ctx context.Context, orgID int64, folderUID string, u identity.Requester) (int64, error) {
	folder, err := dr.folderService.Get(ctx, &folder.GetFolderQuery{UID: &folderUID, OrgID: orgID, SignedInUser: u})
	if err != nil {
		return 0, err
	}

	// nolint:staticcheck
	return dr.dashboardStore.CountDashboardsInFolder(ctx, &dashboards.CountDashboardsInFolderRequest{FolderID: folder.ID, OrgID: orgID})
}

func (dr *DashboardServiceImpl) DeleteInFolder(ctx context.Context, orgID int64, folderUID string, u identity.Requester) error {
	return dr.dashboardStore.DeleteDashboardsInFolder(ctx, &dashboards.DeleteDashboardsInFolderRequest{FolderUID: folderUID, OrgID: orgID})
}

func (dr *DashboardServiceImpl) Kind() string { return entity.StandardKindDashboard }
