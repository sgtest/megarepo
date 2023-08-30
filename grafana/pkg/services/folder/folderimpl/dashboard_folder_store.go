package folderimpl

import (
	"context"
	"strings"

	"github.com/grafana/grafana/pkg/infra/db"
	"github.com/grafana/grafana/pkg/services/dashboards"
	"github.com/grafana/grafana/pkg/services/folder"
)

// DashboardStore implements the FolderStore interface
// It fetches folders from the dashboard DB table
type DashboardFolderStoreImpl struct {
	store db.DB
}

func ProvideDashboardFolderStore(sqlStore db.DB) *DashboardFolderStoreImpl {
	return &DashboardFolderStoreImpl{store: sqlStore}
}

func (d *DashboardFolderStoreImpl) GetFolderByTitle(ctx context.Context, orgID int64, title string) (*folder.Folder, error) {
	if title == "" {
		return nil, dashboards.ErrFolderTitleEmpty
	}

	// there is a unique constraint on org_id, folder_id, title
	// there are no nested folders so the parent folder id is always 0
	dashboard := dashboards.Dashboard{OrgID: orgID, FolderID: 0, Title: title}
	err := d.store.WithTransactionalDbSession(ctx, func(sess *db.Session) error {
		has, err := sess.Table(&dashboards.Dashboard{}).Where("is_folder = " + d.store.GetDialect().BooleanStr(true)).Where("folder_id=0").Get(&dashboard)
		if err != nil {
			return err
		}
		if !has {
			return dashboards.ErrFolderNotFound
		}
		dashboard.SetID(dashboard.ID)
		dashboard.SetUID(dashboard.UID)
		return nil
	})
	return dashboards.FromDashboard(&dashboard), err
}

func (d *DashboardFolderStoreImpl) GetFolderByID(ctx context.Context, orgID int64, id int64) (*folder.Folder, error) {
	dashboard := dashboards.Dashboard{OrgID: orgID, FolderID: 0, ID: id}
	err := d.store.WithTransactionalDbSession(ctx, func(sess *db.Session) error {
		has, err := sess.Table(&dashboards.Dashboard{}).Where("is_folder = " + d.store.GetDialect().BooleanStr(true)).Where("folder_id=0").Get(&dashboard)
		if err != nil {
			return err
		}
		if !has {
			return dashboards.ErrFolderNotFound
		}
		dashboard.SetID(dashboard.ID)
		dashboard.SetUID(dashboard.UID)
		return nil
	})
	if err != nil {
		return nil, err
	}
	return dashboards.FromDashboard(&dashboard), nil
}

func (d *DashboardFolderStoreImpl) GetFolderByUID(ctx context.Context, orgID int64, uid string) (*folder.Folder, error) {
	if uid == "" {
		return nil, dashboards.ErrDashboardIdentifierNotSet
	}

	dashboard := dashboards.Dashboard{OrgID: orgID, FolderID: 0, UID: uid}
	err := d.store.WithTransactionalDbSession(ctx, func(sess *db.Session) error {
		has, err := sess.Table(&dashboards.Dashboard{}).Where("is_folder = " + d.store.GetDialect().BooleanStr(true)).Where("folder_id=0").Get(&dashboard)
		if err != nil {
			return err
		}
		if !has {
			return dashboards.ErrFolderNotFound
		}
		dashboard.SetID(dashboard.ID)
		dashboard.SetUID(dashboard.UID)
		return nil
	})
	if err != nil {
		return nil, err
	}
	return dashboards.FromDashboard(&dashboard), nil
}

func (d *DashboardFolderStoreImpl) GetFolders(ctx context.Context, orgID int64, uids []string) (map[string]*folder.Folder, error) {
	m := make(map[string]*folder.Folder, len(uids))
	var folders []*folder.Folder
	if err := d.store.WithDbSession(ctx, func(sess *db.Session) error {
		b := strings.Builder{}
		args := make([]any, 0, len(uids)+1)

		b.WriteString("SELECT * FROM dashboard WHERE org_id=? ")
		args = append(args, orgID)
		for i, uid := range uids {
			if i == 0 {
				b.WriteString("  AND (")
			}

			if i > 0 {
				b.WriteString(" OR ")
			}
			b.WriteString(" uid=? ")
			args = append(args, uid)

			if i == len(uids)-1 {
				b.WriteString(")")
			}
		}
		return sess.SQL(b.String(), args...).Find(&folders)
	}); err != nil {
		return nil, err
	}

	for _, f := range folders {
		m[f.UID] = f
	}
	return m, nil
}
