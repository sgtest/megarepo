package access

import (
	"context"
	"database/sql"
	"fmt"
	"path/filepath"
	"time"

	v1 "k8s.io/apimachinery/pkg/apis/meta/v1"

	dashboardsV0 "github.com/grafana/grafana/pkg/apis/dashboard/v0alpha1"
	"github.com/grafana/grafana/pkg/components/simplejson"
	"github.com/grafana/grafana/pkg/infra/appcontext"
	"github.com/grafana/grafana/pkg/infra/db"
	"github.com/grafana/grafana/pkg/services/accesscontrol"
	"github.com/grafana/grafana/pkg/services/apiserver/endpoints/request"
	"github.com/grafana/grafana/pkg/services/apiserver/utils"
	"github.com/grafana/grafana/pkg/services/dashboards"
	"github.com/grafana/grafana/pkg/services/provisioning"
	"github.com/grafana/grafana/pkg/services/sqlstore/session"
)

var (
	_ DashboardAccess = (*dashboardSqlAccess)(nil)
)

type dashboardRow struct {
	// Dashboard resource
	Dash *dashboardsV0.Dashboard

	// Title -- this may come from saved metadata rather than the body
	Title string

	// The folder UID (needed for access control checks)
	FolderUID string

	// Needed for fast summary access
	Tags []string

	// Size (in bytes) of the dashboard payload
	Bytes int

	// The token we can use that will start a new connection that includes
	// this same dashboard
	token *continueToken
}

type dashboardSqlAccess struct {
	sql          db.DB
	sess         *session.SessionDB
	namespacer   request.NamespaceMapper
	dashStore    dashboards.Store
	provisioning provisioning.ProvisioningService
}

func NewDashboardAccess(sql db.DB, namespacer request.NamespaceMapper, dashStore dashboards.Store, provisioning provisioning.ProvisioningService) DashboardAccess {
	return &dashboardSqlAccess{
		sql:          sql,
		sess:         sql.GetSqlxSession(),
		namespacer:   namespacer,
		dashStore:    dashStore,
		provisioning: provisioning,
	}
}

const selector = `SELECT
	dashboard.org_id, dashboard.id,
	dashboard.uid,slug,
	dashboard.folder_uid,
	dashboard.created,dashboard.created_by,CreatedUSER.login,
	dashboard.updated,dashboard.updated_by,UpdatedUSER.login,
	plugin_id,
	dashboard_provisioning.name as origin_name,
	dashboard_provisioning.external_id as origin_path,
	dashboard_provisioning.check_sum as origin_key,
	dashboard_provisioning.updated as origin_ts,
	dashboard.version,
	title,
	dashboard.data
  FROM dashboard
  LEFT OUTER JOIN dashboard_provisioning ON dashboard.id = dashboard_provisioning.dashboard_id
  LEFT OUTER JOIN user AS CreatedUSER ON dashboard.created_by = CreatedUSER.id
  LEFT OUTER JOIN user AS UpdatedUSER ON dashboard.created_by = UpdatedUSER.id
  WHERE is_folder = false`

// GetDashboards implements DashboardAccess.
func (a *dashboardSqlAccess) GetDashboards(ctx context.Context, query *DashboardQuery) (*dashboardsV0.DashboardList, error) {
	token, err := readContinueToken(query)
	if err != nil {
		return nil, err
	}

	limit := query.Limit
	if limit < 1 {
		limit = 15 //
	}

	rows, err := a.doQuery(ctx, selector+`
		AND dashboard.org_id=$1
		AND dashboard.id>=$2
		ORDER BY dashboard.id asc
		LIMIT $3
	`, query.OrgID, token.id, (limit + 2))
	if err != nil {
		return nil, err
	}
	defer func() { _ = rows.Close() }()

	totalSize := 0
	list := &dashboardsV0.DashboardList{}
	if err != nil {
		return nil, err
	}
	for {
		row, err := rows.Next()
		if err != nil || row == nil {
			return list, err
		}

		totalSize += row.Bytes
		if len(list.Items) > 0 && (totalSize > query.MaxBytes || len(list.Items) >= limit) {
			row.token.folder = query.FolderUID
			list.Continue = row.token.String() // will skip this one but start here next time
			return list, err
		}
		list.Items = append(list.Items, *row.Dash)
	}
}

func (a *dashboardSqlAccess) GetDashboard(ctx context.Context, orgId int64, uid string) (*dashboardsV0.Dashboard, error) {
	r, err := a.GetDashboards(ctx, &DashboardQuery{
		OrgID: orgId,
		UID:   uid,
	})
	if err != nil {
		return nil, err
	}
	if len(r.Items) > 0 {
		return &r.Items[0], nil
	}
	return nil, fmt.Errorf("not found")
}

// GetDashboards implements DashboardAccess.
func (a *dashboardSqlAccess) GetDashboardSummaries(ctx context.Context, query *DashboardQuery) (*dashboardsV0.DashboardSummaryList, error) {
	token, err := readContinueToken(query)
	if err != nil {
		return nil, err
	}
	limit := query.Limit
	if limit < 1 {
		limit = 15 //
	}
	rows, err := a.doQuery(ctx, selector+`
		AND dashboard.org_id=$1
		AND dashboard.id>=$2
		ORDER BY dashboard.id asc
		LIMIT $3
	`, query.OrgID, token.id, (limit + 2))
	if err != nil {
		return nil, err
	}
	defer func() { _ = rows.Close() }()

	totalSize := 0
	list := &dashboardsV0.DashboardSummaryList{}
	if err != nil {
		return nil, err
	}
	for {
		row, err := rows.Next()
		if err != nil || row == nil {
			return list, err
		}

		totalSize += row.Bytes
		if len(list.Items) > 0 && (totalSize > query.MaxBytes || len(list.Items) >= limit) {
			row.token.folder = query.FolderUID
			list.Continue = row.token.String() // will skip this one but start here next time
			return list, err
		}
		list.Items = append(list.Items, toSummary(row))
	}
}

func (a *dashboardSqlAccess) GetDashboardSummary(ctx context.Context, orgId int64, uid string) (*dashboardsV0.DashboardSummary, error) {
	r, err := a.GetDashboardSummaries(ctx, &DashboardQuery{
		OrgID: orgId,
		UID:   uid,
	})
	if err != nil {
		return nil, err
	}
	if len(r.Items) > 0 {
		return &r.Items[0], nil
	}
	return nil, fmt.Errorf("not found")
}

func toSummary(row *dashboardRow) dashboardsV0.DashboardSummary {
	return dashboardsV0.DashboardSummary{
		ObjectMeta: row.Dash.ObjectMeta,
		Spec: dashboardsV0.DashboardSummarySpec{
			Title: row.Title,
			Tags:  row.Tags,
		},
	}
}

func (a *dashboardSqlAccess) doQuery(ctx context.Context, query string, args ...any) (*rowsWrapper, error) {
	user, err := appcontext.User(ctx)
	if err != nil {
		return nil, err
	}
	rows, err := a.sess.Query(ctx, query, args...)
	return &rowsWrapper{
		rows: rows,
		a:    a,
		// This looks up rules from the permissions on a user
		canReadDashboard: accesscontrol.Checker(user, dashboards.ActionDashboardsRead),
	}, err
}

type rowsWrapper struct {
	a     *dashboardSqlAccess
	rows  *sql.Rows
	idx   int
	total int64

	canReadDashboard func(scopes ...string) bool
}

func (r *rowsWrapper) Close() error {
	return r.rows.Close()
}

func (r *rowsWrapper) Next() (*dashboardRow, error) {
	// breaks after first readable value
	for r.rows.Next() {
		r.idx++
		d, err := r.a.scanRow(r.rows)
		if d != nil {
			// Access control checker
			scopes := []string{dashboards.ScopeDashboardsProvider.GetResourceScopeUID(d.Dash.Name)}
			if d.FolderUID != "" { // Copied from searchV2... not sure the logic is right
				scopes = append(scopes, dashboards.ScopeFoldersProvider.GetResourceScopeUID(d.FolderUID))
			}
			if !r.canReadDashboard(scopes...) {
				continue
			}
			d.token.size = r.total // size before next!
			r.total += int64(d.Bytes)
		}

		// returns the first folder it can
		return d, err
	}
	return nil, nil
}

func (a *dashboardSqlAccess) scanRow(rows *sql.Rows) (*dashboardRow, error) {
	dash := &dashboardsV0.Dashboard{
		TypeMeta:   dashboardsV0.DashboardResourceInfo.TypeMeta(),
		ObjectMeta: v1.ObjectMeta{Annotations: make(map[string]string)},
	}
	row := &dashboardRow{Dash: dash}

	var dashboard_id int64
	var orgId int64
	var slug string
	var folder_uid sql.NullString
	var updated time.Time
	var updatedByID int64
	var updatedByName sql.NullString

	var created time.Time
	var createdByID int64
	var createdByName sql.NullString

	var plugin_id string
	var origin_name sql.NullString
	var origin_path sql.NullString
	var origin_ts sql.NullInt64
	var origin_key sql.NullString
	var data []byte // the dashboard JSON
	var version int64

	err := rows.Scan(&orgId, &dashboard_id, &dash.Name,
		&slug, &folder_uid,
		&created, &createdByID, &createdByName,
		&updated, &updatedByID, &updatedByName,
		&plugin_id,
		&origin_name, &origin_path, &origin_key, &origin_ts,
		&version,
		&row.Title, &data,
	)

	row.token = &continueToken{orgId: orgId, id: dashboard_id}
	if err == nil {
		dash.ResourceVersion = fmt.Sprintf("%d", created.UnixMilli())
		dash.Namespace = a.namespacer(orgId)
		dash.UID = utils.CalculateClusterWideUID(dash)
		dash.SetCreationTimestamp(v1.NewTime(created))
		meta, err := utils.MetaAccessor(dash)
		if err != nil {
			return nil, err
		}
		meta.SetUpdatedTimestamp(&updated)
		meta.SetSlug(slug)
		if createdByID > 0 {
			meta.SetCreatedBy(fmt.Sprintf("user:%d/%s", createdByID, createdByName.String))
		}
		if updatedByID > 0 {
			meta.SetUpdatedBy(fmt.Sprintf("user:%d/%s", updatedByID, updatedByName.String))
		}
		if folder_uid.Valid {
			meta.SetFolder(folder_uid.String)
			row.FolderUID = folder_uid.String
		}

		if origin_name.Valid {
			ts := time.Unix(origin_ts.Int64, 0)

			originPath, err := filepath.Rel(
				a.provisioning.GetDashboardProvisionerResolvedPath(origin_name.String),
				origin_path.String,
			)
			if err != nil {
				return nil, err
			}

			meta.SetOriginInfo(&utils.ResourceOriginInfo{
				Name:      origin_name.String,
				Path:      originPath,
				Key:       origin_key.String,
				Timestamp: &ts,
			})
		} else if plugin_id != "" {
			meta.SetOriginInfo(&utils.ResourceOriginInfo{
				Name: "plugin",
				Path: plugin_id,
			})
		}

		row.Bytes = len(data)
		if row.Bytes > 0 {
			err = dash.Spec.UnmarshalJSON(data)
			if err != nil {
				return row, err
			}
			dash.Spec.Set("id", dashboard_id) // add it so we can get it from the body later
			row.Title = dash.Spec.GetNestedString("title")
			row.Tags = dash.Spec.GetNestedStringSlice("tags")
		}
	}
	return row, err
}

// DeleteDashboard implements DashboardAccess.
func (a *dashboardSqlAccess) DeleteDashboard(ctx context.Context, orgId int64, uid string) (*dashboardsV0.Dashboard, bool, error) {
	dash, err := a.GetDashboard(ctx, orgId, uid)
	if err != nil {
		return nil, false, err
	}

	id := dash.Spec.GetNestedInt64("id")
	if id == 0 {
		return nil, false, fmt.Errorf("could not find id in saved body")
	}

	err = a.dashStore.DeleteDashboard(ctx, &dashboards.DeleteDashboardCommand{
		OrgID: orgId,
		ID:    id,
	})
	if err != nil {
		return nil, false, err
	}
	return dash, true, nil
}

// SaveDashboard implements DashboardAccess.
func (a *dashboardSqlAccess) SaveDashboard(ctx context.Context, orgId int64, dash *dashboardsV0.Dashboard) (*dashboardsV0.Dashboard, bool, error) {
	created := false
	user, err := appcontext.User(ctx)
	if err != nil {
		return nil, created, err
	}
	if dash.Name != "" {
		dash.Spec.Set("uid", dash.Name)

		// Get the previous version to set the internal ID
		old, _ := a.dashStore.GetDashboard(ctx, &dashboards.GetDashboardQuery{
			OrgID: orgId,
			UID:   dash.Name,
		})
		if old != nil {
			dash.Spec.Set("id", old.ID)
		} else {
			dash.Spec.Remove("id") // existing of "id" makes it an update
			created = true
		}
	} else {
		dash.Spec.Remove("id")
		dash.Spec.Remove("uid")
	}

	meta, err := utils.MetaAccessor(dash)
	if err != nil {
		return nil, false, err
	}
	out, err := a.dashStore.SaveDashboard(ctx, dashboards.SaveDashboardCommand{
		OrgID:     orgId,
		Dashboard: simplejson.NewFromAny(dash.Spec.UnstructuredContent()),
		FolderUID: meta.GetFolder(),
		Overwrite: true, // already passed the revisionVersion checks!
		UserID:    user.UserID,
	})
	if err != nil {
		return nil, false, err
	}
	if out != nil {
		created = (out.Created.Unix() == out.Updated.Unix()) // and now?
	}
	dash, err = a.GetDashboard(ctx, orgId, out.UID)
	return dash, created, err
}
