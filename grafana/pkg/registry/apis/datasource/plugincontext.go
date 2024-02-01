package datasource

import (
	"context"
	"fmt"

	"github.com/grafana/grafana-plugin-sdk-go/backend"
	metav1 "k8s.io/apimachinery/pkg/apis/meta/v1"

	"github.com/grafana/grafana/pkg/apis/datasource/v0alpha1"
	"github.com/grafana/grafana/pkg/infra/appcontext"
	"github.com/grafana/grafana/pkg/services/apiserver/endpoints/request"
	"github.com/grafana/grafana/pkg/services/apiserver/utils"
	"github.com/grafana/grafana/pkg/services/datasources"
	"github.com/grafana/grafana/pkg/services/pluginsintegration/plugincontext"
)

// This provides access to settings saved in the database.
// Authorization checks will happen within each function, and the user in ctx will
// limit which namespace/tenant/org we are talking to
type PluginDatasourceProvider interface {
	// Get gets a specific datasource (that the user in context can see)
	Get(ctx context.Context, pluginID, uid string) (*v0alpha1.DataSourceConnection, error)

	// List lists all data sources the user in context can see
	List(ctx context.Context, pluginID string) (*v0alpha1.DataSourceConnectionList, error)

	// Return settings (decrypted!) for a specific plugin
	// This will require "query" permission for the user in context
	GetInstanceSettings(ctx context.Context, pluginID, uid string) (*backend.DataSourceInstanceSettings, error)
}

// PluginContext requires adding system settings (feature flags, etc) to the datasource config
type PluginContextWrapper interface {
	PluginContextForDataSource(ctx context.Context, datasourceSettings *backend.DataSourceInstanceSettings) (backend.PluginContext, error)
}

func ProvideDefaultPluginConfigs(
	dsService datasources.DataSourceService,
	dsCache datasources.CacheService,
	contextProvider *plugincontext.Provider) PluginDatasourceProvider {
	return &defaultPluginDatasourceProvider{
		dsService:       dsService,
		dsCache:         dsCache,
		contextProvider: contextProvider,
	}
}

type defaultPluginDatasourceProvider struct {
	dsService       datasources.DataSourceService
	dsCache         datasources.CacheService
	contextProvider *plugincontext.Provider
}

var (
	_ PluginDatasourceProvider = (*defaultPluginDatasourceProvider)(nil)
)

func (q *defaultPluginDatasourceProvider) Get(ctx context.Context, pluginID, uid string) (*v0alpha1.DataSourceConnection, error) {
	info, err := request.NamespaceInfoFrom(ctx, true)
	if err != nil {
		return nil, err
	}
	user, err := appcontext.User(ctx)
	if err != nil {
		return nil, err
	}
	ds, err := q.dsCache.GetDatasourceByUID(ctx, uid, user, false)
	if err != nil {
		return nil, err
	}
	return asConnection(ds, info.Value)
}

func (q *defaultPluginDatasourceProvider) List(ctx context.Context, pluginID string) (*v0alpha1.DataSourceConnectionList, error) {
	info, err := request.NamespaceInfoFrom(ctx, true)
	if err != nil {
		return nil, err
	}

	dss, err := q.dsService.GetDataSourcesByType(ctx, &datasources.GetDataSourcesByTypeQuery{
		OrgID: info.OrgID,
		Type:  pluginID,
	})
	if err != nil {
		return nil, err
	}
	result := &v0alpha1.DataSourceConnectionList{
		Items: []v0alpha1.DataSourceConnection{},
	}
	for _, ds := range dss {
		v, _ := asConnection(ds, info.Value)
		result.Items = append(result.Items, *v)
	}
	return result, nil
}

func (q *defaultPluginDatasourceProvider) GetInstanceSettings(ctx context.Context, pluginID, uid string) (*backend.DataSourceInstanceSettings, error) {
	if q.contextProvider == nil {
		// NOTE!!! this is only here for the standalone example
		// if we cleanup imports this can throw an error
		return nil, nil
	}
	return q.contextProvider.GetDataSourceInstanceSettings(ctx, uid)
}

func asConnection(ds *datasources.DataSource, ns string) (*v0alpha1.DataSourceConnection, error) {
	v := &v0alpha1.DataSourceConnection{
		ObjectMeta: metav1.ObjectMeta{
			Name:              ds.UID,
			Namespace:         ns,
			CreationTimestamp: metav1.NewTime(ds.Created),
			ResourceVersion:   fmt.Sprintf("%d", ds.Updated.UnixMilli()),
		},
		Title: ds.Name,
	}
	v.UID = utils.CalculateClusterWideUID(v) // indicates if the value changed on the server
	meta, err := utils.MetaAccessor(v)
	if err != nil {
		meta.SetUpdatedTimestamp(&ds.Updated)
	}
	return v, err
}
