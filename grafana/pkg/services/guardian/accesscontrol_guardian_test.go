package guardian

import (
	"context"
	"fmt"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/mock"
	"github.com/stretchr/testify/require"

	"github.com/grafana/grafana/pkg/api/routing"
	"github.com/grafana/grafana/pkg/infra/db"
	"github.com/grafana/grafana/pkg/infra/localcache"
	"github.com/grafana/grafana/pkg/services/accesscontrol"
	"github.com/grafana/grafana/pkg/services/accesscontrol/acimpl"
	acdb "github.com/grafana/grafana/pkg/services/accesscontrol/database"
	accesscontrolmock "github.com/grafana/grafana/pkg/services/accesscontrol/mock"
	"github.com/grafana/grafana/pkg/services/accesscontrol/ossaccesscontrol"
	"github.com/grafana/grafana/pkg/services/dashboards"
	"github.com/grafana/grafana/pkg/services/featuremgmt"
	"github.com/grafana/grafana/pkg/services/folder"
	"github.com/grafana/grafana/pkg/services/folder/foldertest"
	"github.com/grafana/grafana/pkg/services/licensing/licensingtest"
	"github.com/grafana/grafana/pkg/services/quota/quotatest"
	"github.com/grafana/grafana/pkg/services/supportbundles/supportbundlestest"
	"github.com/grafana/grafana/pkg/services/team/teamimpl"
	"github.com/grafana/grafana/pkg/services/user"
	"github.com/grafana/grafana/pkg/services/user/userimpl"
	"github.com/grafana/grafana/pkg/setting"
)

const (
	dashUID          = "1"
	folderID         = 42
	folderUID        = "42"
	invalidFolderUID = "142"
)

var (
	folderUIDScope        = fmt.Sprintf("folders:uid:%s", folderUID)
	invalidFolderUIDScope = fmt.Sprintf("folders:uid:%s", invalidFolderUID)
	dashboard             = &dashboards.Dashboard{OrgID: orgID, UID: dashUID, IsFolder: false, FolderID: folderID}
	fldr                  = &dashboards.Dashboard{OrgID: orgID, UID: folderUID, IsFolder: true}
)

type accessControlGuardianTestCase struct {
	desc           string
	dashboard      *dashboards.Dashboard
	permissions    []accesscontrol.Permission
	viewersCanEdit bool
	expected       bool
}

func TestAccessControlDashboardGuardian_CanSave(t *testing.T) {
	tests := []accessControlGuardianTestCase{
		{
			desc:      "should be able to save dashboard with dashboard wildcard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsWrite,
					Scope:  "dashboards:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to save dashboard with folder wildcard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsWrite,
					Scope:  "folders:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to save dashboard with dashboard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsWrite,
					Scope:  "dashboards:uid:1",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to save dashboard under root with general folder scope",
			dashboard: &dashboards.Dashboard{OrgID: orgID, UID: dashUID, IsFolder: false, FolderID: 0},
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsWrite,
					Scope:  "folders:uid:general",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to save dashboard with folder scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsWrite,
					Scope:  folderUIDScope,
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to save dashboard with incorrect dashboard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsWrite,
					Scope:  "dashboards:uid:10",
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to save dashboard with incorrect folder scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsWrite,
					Scope:  invalidFolderUIDScope,
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to save folder with folder write and dashboard wildcard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersWrite,
					Scope:  "dashboards:*",
				},
			},
			expected: false,
		},
		{
			desc:      "should be able to save folder with folder write and folder wildcard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersWrite,
					Scope:  "folders:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to save folder with folder write and dashboard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersWrite,
					Scope:  "dashboards:uid:1",
				},
			},
			expected: false,
		},
		{
			desc:      "should be able to save folder with folder write and folder scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersWrite,
					Scope:  folderUIDScope,
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to save folder with folder write and incorrect dashboard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersWrite,
					Scope:  "dashboards:uid:10",
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to save folder with folder write and incorrect folder scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersWrite,
					Scope:  invalidFolderUID,
				},
			},
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.desc, func(t *testing.T) {
			guardian := setupAccessControlGuardianTest(t, tt.dashboard, tt.permissions, nil, nil, nil)
			can, err := guardian.CanSave()
			require.NoError(t, err)
			assert.Equal(t, tt.expected, can)
		})
	}
}

func TestAccessControlDashboardGuardian_CanEdit(t *testing.T) {
	tests := []accessControlGuardianTestCase{
		{
			desc:      "should be able to edit dashboard with dashboard wildcard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsWrite,
					Scope:  "dashboards:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to edit dashboard with folder wildcard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsWrite,
					Scope:  "folders:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to edit dashboard with dashboard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsWrite,
					Scope:  "dashboards:uid:1",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to edit dashboard under root with general folder scope",
			dashboard: &dashboards.Dashboard{OrgID: orgID, UID: dashUID, IsFolder: false, FolderID: 0},
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsWrite,
					Scope:  "folders:uid:general",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to edit dashboard with folder scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsWrite,
					Scope:  folderUIDScope,
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to edit dashboard with incorrect dashboard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsWrite,
					Scope:  "dashboards:uid:10",
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to edit dashboard with incorrect folder scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsWrite,
					Scope:  invalidFolderUIDScope,
				},
			},
			expected: false,
		},
		{
			desc:      "should be able to edit dashboard with read action when viewer_can_edit is true",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsRead,
					Scope:  "dashboards:uid:1",
				},
			},
			viewersCanEdit: true,
			expected:       true,
		},
		{
			desc:      "should not be able to edit folder with folder write and dashboard wildcard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersWrite,
					Scope:  "dashboards:*",
				},
			},
			expected: false,
		},
		{
			desc:      "should be able to edit folder with folder write and folder wildcard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersWrite,
					Scope:  "folders:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to edit folder with folder write and dashboard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersWrite,
					Scope:  "dashboards:uid:1",
				},
			},
			expected: false,
		},
		{
			desc:      "should be able to edit folder with folder write and folder scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersWrite,
					Scope:  folderUIDScope,
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to edit folder with folder write and incorrect folder scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersWrite,
					Scope:  invalidFolderUIDScope,
				},
			},
			expected: false,
		},
		{
			desc:      "should be able to edit folder with folder read action when viewer_can_edit is true",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersRead,
					Scope:  folderUIDScope,
				},
			},
			viewersCanEdit: true,
			expected:       true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.desc, func(t *testing.T) {
			cfg := setting.NewCfg()
			cfg.ViewersCanEdit = tt.viewersCanEdit
			guardian := setupAccessControlGuardianTest(t, tt.dashboard, tt.permissions, cfg, nil, nil)

			can, err := guardian.CanEdit()
			require.NoError(t, err)
			assert.Equal(t, tt.expected, can)
		})
	}
}

func TestAccessControlDashboardGuardian_CanView(t *testing.T) {
	tests := []accessControlGuardianTestCase{
		{
			desc:      "should be able to view dashboard with dashboard wildcard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsRead,
					Scope:  "dashboards:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to view dashboard with folder wildcard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsRead,
					Scope:  "folders:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to view dashboard with dashboard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsRead,
					Scope:  "dashboards:uid:1",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to view dashboard under root with general folder scope",
			dashboard: &dashboards.Dashboard{OrgID: orgID, UID: dashUID, IsFolder: false, FolderID: 0},
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsRead,
					Scope:  "folders:uid:general",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to view dashboard with folder scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsRead,
					Scope:  folderUIDScope,
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to view dashboard with incorrect dashboard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsRead,
					Scope:  "dashboards:uid:10",
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to view dashboard with incorrect folder scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsRead,
					Scope:  invalidFolderUIDScope,
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to view folder with folders read and dashboard wildcard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersRead,
					Scope:  "dashboards:*",
				},
			},
			expected: false,
		},
		{
			desc:      "should be able to view folder with folders read and folder wildcard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersRead,
					Scope:  "folders:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to folder view with folders read and dashboard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersRead,
					Scope:  "dashboards:uid:1",
				},
			},
			expected: false,
		},
		{
			desc:      "should be able to view folder with folders read and folder scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersRead,
					Scope:  folderUIDScope,
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to view folder with folders read incorrect dashboard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersRead,
					Scope:  "dashboards:uid:10",
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to view folder with folders read and incorrect folder scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersRead,
					Scope:  invalidFolderUIDScope,
				},
			},
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.desc, func(t *testing.T) {
			guardian := setupAccessControlGuardianTest(t, tt.dashboard, tt.permissions, nil, nil, nil)

			can, err := guardian.CanView()
			require.NoError(t, err)
			assert.Equal(t, tt.expected, can)
		})
	}
}
func TestAccessControlDashboardGuardian_CanAdmin(t *testing.T) {
	tests := []accessControlGuardianTestCase{
		{
			desc:      "should be able to admin dashboard with dashboard wildcard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsPermissionsRead,
					Scope:  "dashboards:*",
				},
				{
					Action: dashboards.ActionDashboardsPermissionsWrite,
					Scope:  "dashboards:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to admin dashboard with folder wildcard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsPermissionsRead,
					Scope:  "folders:*",
				},
				{
					Action: dashboards.ActionDashboardsPermissionsWrite,
					Scope:  "folders:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to admin dashboard with dashboard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsPermissionsRead,
					Scope:  "dashboards:uid:1",
				},
				{
					Action: dashboards.ActionDashboardsPermissionsWrite,
					Scope:  "dashboards:uid:1",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to admin dashboard under root with general folder scope",
			dashboard: &dashboards.Dashboard{OrgID: orgID, UID: dashUID, IsFolder: false, FolderID: 0},
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsPermissionsRead,
					Scope:  "folders:uid:general",
				},
				{
					Action: dashboards.ActionDashboardsPermissionsWrite,
					Scope:  "folders:uid:general",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to admin dashboard with folder scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsPermissionsRead,
					Scope:  folderUIDScope,
				},
				{
					Action: dashboards.ActionDashboardsPermissionsWrite,
					Scope:  folderUIDScope,
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to admin dashboard with incorrect dashboard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsPermissionsRead,
					Scope:  "dashboards:uid:10",
				},
				{
					Action: dashboards.ActionDashboardsPermissionsWrite,
					Scope:  "dashboards:uid:10",
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to admin dashboard with incorrect folder scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsPermissionsRead,
					Scope:  invalidFolderUIDScope,
				},
				{
					Action: dashboards.ActionDashboardsPermissionsWrite,
					Scope:  invalidFolderUIDScope,
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to admin folder with folder read and write and dashboard wildcard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersPermissionsRead,
					Scope:  "dashboards:*",
				},
				{
					Action: dashboards.ActionFoldersPermissionsWrite,
					Scope:  "dashboards:*",
				},
			},
			expected: false,
		},
		{
			desc:      "should be able to admin folder with folder read and write and wildcard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersPermissionsRead,
					Scope:  "folders:*",
				},
				{
					Action: dashboards.ActionFoldersPermissionsWrite,
					Scope:  "folders:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to admin folder with folder read and wildcard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersPermissionsRead,
					Scope:  "folders:*",
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to admin folder with folder write and wildcard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersPermissionsWrite,
					Scope:  "folders:*",
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to admin folder with folder read and write and dashboard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersPermissionsRead,
					Scope:  "dashboards:uid:1",
				},
				{
					Action: dashboards.ActionFoldersPermissionsWrite,
					Scope:  "dashboards:uid:1",
				},
			},
			expected: false,
		},
		{
			desc:      "should be able to admin folder with folder read and write and folder scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersPermissionsRead,
					Scope:  folderUIDScope,
				},
				{
					Action: dashboards.ActionFoldersPermissionsWrite,
					Scope:  folderUIDScope,
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to admin folder with folder read and folder scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersPermissionsRead,
					Scope:  folderUIDScope,
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to admin folder with folder write and folder scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersPermissionsWrite,
					Scope:  folderUIDScope,
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to admin folder with folder read and write and incorrect dashboard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersPermissionsRead,
					Scope:  "dashboards:uid:10",
				},
				{
					Action: dashboards.ActionFoldersPermissionsWrite,
					Scope:  "dashboards:uid:10",
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to admin folder with folder read and write and incorrect folder scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersPermissionsRead,
					Scope:  invalidFolderUIDScope,
				},
				{
					Action: dashboards.ActionFoldersPermissionsWrite,
					Scope:  invalidFolderUIDScope,
				},
			},
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.desc, func(t *testing.T) {
			guardian := setupAccessControlGuardianTest(t, tt.dashboard, tt.permissions, nil, nil, nil)

			can, err := guardian.CanAdmin()
			require.NoError(t, err)
			assert.Equal(t, tt.expected, can)
		})
	}
}

func TestAccessControlDashboardGuardian_CanDelete(t *testing.T) {
	tests := []accessControlGuardianTestCase{
		{
			desc:      "should be able to delete dashboard with dashboard wildcard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsDelete,
					Scope:  "dashboards:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to delete dashboard with folder wildcard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsDelete,
					Scope:  "folders:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to delete dashboard with dashboard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsDelete,
					Scope:  "dashboards:uid:1",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to delete dashboard under root with general folder scope",
			dashboard: &dashboards.Dashboard{OrgID: orgID, UID: dashUID, IsFolder: false, FolderID: 0},
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsDelete,
					Scope:  "folders:uid:general",
				},
			},
			expected: true,
		},
		{
			desc:      "should be able to delete dashboard with folder scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsDelete,
					Scope:  folderUIDScope,
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to delete dashboard with incorrect dashboard scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsDelete,
					Scope:  "dashboards:uid:10",
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to delete dashboard with incorrect folder scope",
			dashboard: dashboard,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionDashboardsDelete,
					Scope:  invalidFolderUIDScope,
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to delete folder with folder delete and dashboard wildcard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersDelete,
					Scope:  "dashboards:*",
				},
			},
			expected: false,
		},
		{
			desc:      "should be able to delete folder with folder deletea and folder wildcard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersDelete,
					Scope:  "folders:*",
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to delete folder with folder delete and dashboard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersDelete,
					Scope:  "dashboards:uid:1",
				},
			},
			expected: false,
		},
		{
			desc:      "should be able to delete folder with folder delete and folder scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersDelete,
					Scope:  folderUIDScope,
				},
			},
			expected: true,
		},
		{
			desc:      "should not be able to delete folder with folder delete and incorrect dashboard scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersDelete,
					Scope:  "dashboards:uid:10",
				},
			},
			expected: false,
		},
		{
			desc:      "should not be able to delete folder with folder delete and incorrect folder scope",
			dashboard: fldr,
			permissions: []accesscontrol.Permission{
				{
					Action: dashboards.ActionFoldersDelete,
					Scope:  invalidFolderUIDScope,
				},
			},
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.desc, func(t *testing.T) {
			guardian := setupAccessControlGuardianTest(t, tt.dashboard, tt.permissions, nil, nil, nil)

			can, err := guardian.CanDelete()
			require.NoError(t, err)
			assert.Equal(t, tt.expected, can)
		})
	}
}

type accessControlGuardianCanCreateTestCase struct {
	desc        string
	isFolder    bool
	folderID    int64
	permissions []accesscontrol.Permission
	expected    bool
}

func TestAccessControlDashboardGuardian_CanCreate(t *testing.T) {
	tests := []accessControlGuardianCanCreateTestCase{
		{
			desc:     "should be able to create dashboard in general folder",
			isFolder: false,
			folderID: 0,
			permissions: []accesscontrol.Permission{
				{Action: dashboards.ActionDashboardsCreate, Scope: "folders:uid:general"},
			},
			expected: true,
		},
		{
			desc:     "should be able to create dashboard in any folder",
			isFolder: false,
			folderID: 0,
			permissions: []accesscontrol.Permission{
				{Action: dashboards.ActionDashboardsCreate, Scope: "folders:*"},
			},
			expected: true,
		},
		{
			desc:        "should not be able to create dashboard without permissions",
			isFolder:    false,
			folderID:    0,
			permissions: []accesscontrol.Permission{},
			expected:    false,
		},
		{
			desc:     "should be able to create folder with correct permissions",
			isFolder: true,
			folderID: 0,
			permissions: []accesscontrol.Permission{
				{Action: dashboards.ActionFoldersCreate},
			},
			expected: true,
		},
		{
			desc:        "should not be able to create folders without permissions",
			isFolder:    true,
			folderID:    0,
			permissions: []accesscontrol.Permission{},
			expected:    false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.desc, func(t *testing.T) {
			guardian := setupAccessControlGuardianTest(t, &dashboards.Dashboard{OrgID: orgID, UID: "0", IsFolder: tt.isFolder}, tt.permissions, nil, nil, nil)

			can, err := guardian.CanCreate(tt.folderID, tt.isFolder)
			require.NoError(t, err)
			assert.Equal(t, tt.expected, can)
		})
	}
}

type accessControlGuardianGetHiddenACLTestCase struct {
	desc        string
	permissions []accesscontrol.ResourcePermission
	hiddenUsers map[string]struct{}
	isFolder    bool
}

func TestAccessControlDashboardGuardian_GetHiddenACL(t *testing.T) {
	tests := []accessControlGuardianGetHiddenACLTestCase{
		{
			desc: "should only return permissions containing hidden users",
			permissions: []accesscontrol.ResourcePermission{
				{RoleName: "managed:users:1:permissions", UserId: 1, UserLogin: "user1", IsManaged: true},
				{RoleName: "managed:teams:1:permissions", TeamId: 1, Team: "team1", IsManaged: true},
				{RoleName: "managed:users:2:permissions", UserId: 2, UserLogin: "user2", IsManaged: true},
				{RoleName: "managed:users:3:permissions", UserId: 3, UserLogin: "user3", IsManaged: true},
				{RoleName: "managed:users:4:permissions", UserId: 4, UserLogin: "user4", IsManaged: true},
			},
			hiddenUsers: map[string]struct{}{"user2": {}, "user3": {}},
		},
		{
			desc: "should only return permissions containing hidden users",
			permissions: []accesscontrol.ResourcePermission{
				{RoleName: "managed:users:1:permissions", UserId: 1, UserLogin: "user1", IsManaged: true},
				{RoleName: "managed:teams:1:permissions", TeamId: 1, Team: "team1", IsManaged: true},
				{RoleName: "managed:users:2:permissions", UserId: 2, UserLogin: "user2", IsManaged: true},
				{RoleName: "managed:users:3:permissions", UserId: 3, UserLogin: "user3", IsManaged: true},
				{RoleName: "managed:users:4:permissions", UserId: 4, UserLogin: "user4", IsManaged: true},
			},
			hiddenUsers: map[string]struct{}{"user2": {}, "user3": {}},
			isFolder:    true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.desc, func(t *testing.T) {
			mocked := accesscontrolmock.NewMockedPermissionsService()
			mocked.On("MapActions", mock.Anything).Return("View")
			mocked.On("GetPermissions", mock.Anything, mock.Anything, mock.Anything).Return(tt.permissions, nil)
			guardian := setupAccessControlGuardianTest(t, &dashboards.Dashboard{OrgID: orgID, UID: "1", IsFolder: tt.isFolder}, nil, nil, mocked, mocked)

			cfg := setting.NewCfg()
			cfg.HiddenUsers = tt.hiddenUsers
			permissions, err := guardian.GetHiddenACL(cfg)
			require.NoError(t, err)
			var hiddenUserNames []string
			for name := range tt.hiddenUsers {
				hiddenUserNames = append(hiddenUserNames, name)
			}
			assert.Len(t, permissions, len(hiddenUserNames))
			for _, p := range permissions {
				assert.Contains(t, hiddenUserNames, fmt.Sprintf("user%d", p.UserID))
			}
		})
	}
}

func setupAccessControlGuardianTest(t *testing.T, d *dashboards.Dashboard,
	permissions []accesscontrol.Permission,
	cfg *setting.Cfg,
	dashboardPermissions accesscontrol.DashboardPermissionsService, folderPermissions accesscontrol.FolderPermissionsService) DashboardGuardian {
	t.Helper()
	store := db.InitTestDB(t)

	fakeDashboardService := dashboards.NewFakeDashboardService(t)
	fakeDashboardService.On("GetDashboard", mock.Anything, mock.AnythingOfType("*dashboards.GetDashboardQuery")).Maybe().Return(d, nil)

	ac := acimpl.ProvideAccessControl(cfg)
	folderSvc := foldertest.NewFakeService()

	folderStore := foldertest.NewFakeFolderStore(t)
	folderStore.On("GetFolderByID", mock.Anything, mock.Anything, mock.Anything).Maybe().Return(&folder.Folder{ID: folderID, UID: folderUID, OrgID: orgID}, nil)

	ac.RegisterScopeAttributeResolver(dashboards.NewDashboardUIDScopeResolver(folderStore, fakeDashboardService, folderSvc))
	ac.RegisterScopeAttributeResolver(dashboards.NewFolderUIDScopeResolver(folderSvc))
	ac.RegisterScopeAttributeResolver(dashboards.NewFolderIDScopeResolver(folderStore, folderSvc))

	license := licensingtest.NewFakeLicensing()
	license.On("FeatureEnabled", "accesscontrol.enforcement").Return(true).Maybe()
	teamSvc := teamimpl.ProvideService(store, store.Cfg)
	userSvc, err := userimpl.ProvideService(store, nil, store.Cfg, nil, nil, quotatest.New(false, nil), supportbundlestest.NewFakeBundleService())
	require.NoError(t, err)

	acSvc := acimpl.ProvideOSSService(cfg, acdb.ProvideService(store), localcache.ProvideService(), featuremgmt.WithFeatures())
	if folderPermissions == nil {
		folderPermissions, err = ossaccesscontrol.ProvideFolderPermissions(
			featuremgmt.WithFeatures(), routing.NewRouteRegister(), store, ac, license, &dashboards.FakeDashboardStore{}, folderSvc, acSvc, teamSvc, userSvc)
		require.NoError(t, err)
	}
	if dashboardPermissions == nil {
		dashboardPermissions, err = ossaccesscontrol.ProvideDashboardPermissions(
			featuremgmt.WithFeatures(), routing.NewRouteRegister(), store, ac, license, &dashboards.FakeDashboardStore{}, folderSvc, acSvc, teamSvc, userSvc)
		require.NoError(t, err)
	}

	userPermissions := map[int64]map[string][]string{}
	for _, p := range permissions {
		if _, ok := userPermissions[orgID]; !ok {
			userPermissions[orgID] = map[string][]string{}
		}
		userPermissions[orgID][p.Action] = append(userPermissions[orgID][p.Action], p.Scope)
	}

	g, err := NewAccessControlDashboardGuardianByDashboard(context.Background(), cfg, d, &user.SignedInUser{OrgID: orgID, Permissions: userPermissions}, store, ac, folderPermissions, dashboardPermissions, fakeDashboardService)
	require.NoError(t, err)
	return g
}
