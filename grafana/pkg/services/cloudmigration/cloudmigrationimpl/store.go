package cloudmigrationimpl

import (
	"context"

	"github.com/grafana/grafana/pkg/services/cloudmigration"
)

type store interface {
	MigrateDatasources(context.Context, *cloudmigration.MigrateDatasourcesRequest) (*cloudmigration.MigrateDatasourcesResponse, error)
	CreateMigration(ctx context.Context, token cloudmigration.CloudMigration) error
	GetAllCloudMigrations(ctx context.Context) ([]*cloudmigration.CloudMigration, error)
}
