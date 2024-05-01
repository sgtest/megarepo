package cloudmigration

import (
	"context"
)

type Service interface {
	CreateToken(context.Context) (CreateAccessTokenResponse, error)
	ValidateToken(context.Context, CloudMigration) error

	CreateMigration(context.Context, CloudMigrationRequest) (*CloudMigrationResponse, error)
	GetMigration(ctx context.Context, uid string) (*CloudMigration, error)
	DeleteMigration(ctx context.Context, uid string) (*CloudMigration, error)
	UpdateMigration(ctx context.Context, uid string, request CloudMigrationRequest) (*CloudMigrationResponse, error)
	GetMigrationList(context.Context) (*CloudMigrationListResponse, error)

	RunMigration(ctx context.Context, uid string) (*MigrateDataResponseDTO, error)
	CreateMigrationRun(context.Context, CloudMigrationRun) (string, error)
	GetMigrationStatus(ctx context.Context, runUID string) (*CloudMigrationRun, error)
	GetMigrationRunList(context.Context, string) (*CloudMigrationRunList, error)
}
