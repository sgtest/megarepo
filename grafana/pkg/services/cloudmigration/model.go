package cloudmigration

import (
	"time"

	"github.com/grafana/grafana/pkg/apimachinery/errutil"
)

var (
	ErrInternalNotImplementedError = errutil.Internal("cloudmigrations.notImplemented").Errorf("Internal server error")
	ErrFeatureDisabledError        = errutil.Internal("cloudmigrations.disabled").Errorf("Cloud migrations are disabled on this instance")
	ErrMigrationNotFound           = errutil.NotFound("cloudmigrations.sessionNotFound").Errorf("Session not found")
	ErrMigrationRunNotFound        = errutil.NotFound("cloudmigrations.migrationRunNotFound").Errorf("Migration run not found")
	ErrMigrationNotDeleted         = errutil.Internal("cloudmigrations.sessionNotDeleted").Errorf("Session not deleted")
	ErrTokenNotFound               = errutil.NotFound("cloudmigrations.tokenNotFound").Errorf("Token not found")
	ErrSnapshotNotFound            = errutil.NotFound("cloudmigrations.snapshotNotFound").Errorf("Snapshot not found")
)

// CloudMigration domain structs
type CloudMigrationSession struct {
	ID          int64  `xorm:"pk autoincr 'id'"`
	UID         string `xorm:"uid"`
	AuthToken   string
	Slug        string
	StackID     int `xorm:"stack_id"`
	RegionSlug  string
	ClusterSlug string
	Created     time.Time
	Updated     time.Time
}

type CloudMigrationSnapshot struct {
	ID             int64  `xorm:"pk autoincr 'id'"`
	UID            string `xorm:"uid"`
	SessionUID     string `xorm:"session_uid"`
	Status         SnapshotStatus
	EncryptionKey  string `xorm:"encryption_key"` // stored in the unified secrets table
	UploadURL      string `xorm:"upload_url"`
	LocalDir       string `xorm:"local_directory"`
	GMSSnapshotUID string `xorm:"gms_snapshot_uid"`
	ErrorString    string `xorm:"error_string"`
	Created        time.Time
	Updated        time.Time
	Finished       time.Time

	// Stored in the cloud_migration_resource table
	Resources []CloudMigrationResource `xorm:"-"`
}

type SnapshotStatus string

const (
	SnapshotStatusInitializing      = "initializing"
	SnapshotStatusCreating          = "creating"
	SnapshotStatusPendingUpload     = "pending_upload"
	SnapshotStatusUploading         = "uploading"
	SnapshotStatusPendingProcessing = "pending_processing"
	SnapshotStatusProcessing        = "processing"
	SnapshotStatusFinished          = "finished"
	SnapshotStatusError             = "error"
	SnapshotStatusUnknown           = "unknown"
)

type CloudMigrationResource struct {
	ID  int64  `xorm:"pk autoincr 'id'"`
	UID string `xorm:"uid"`

	Type   MigrateDataType `xorm:"resource_type"`
	RefID  string          `xorm:"resource_uid"`
	Status ItemStatus      `xorm:"status"`
	Error  string          `xorm:"error_string"`

	SnapshotUID string `xorm:"snapshot_uid"`
}

// Deprecated, use GetSnapshotResult for the async workflow
func (s CloudMigrationSnapshot) GetResult() (*MigrateDataResponse, error) {
	result := MigrateDataResponse{
		RunUID: s.UID,
		Items:  s.Resources,
	}
	return &result, nil
}

func (s CloudMigrationSnapshot) ShouldQueryGMS() bool {
	return s.Status == SnapshotStatusPendingProcessing || s.Status == SnapshotStatusProcessing
}

type CloudMigrationRunList struct {
	Runs []MigrateDataResponseList
}

type CloudMigrationSessionRequest struct {
	AuthToken string
}

type CloudMigrationSessionResponse struct {
	UID     string
	Slug    string
	Created time.Time
	Updated time.Time
}

type CloudMigrationSessionListResponse struct {
	Sessions []CloudMigrationSessionResponse
}

type GetSnapshotsQuery struct {
	SnapshotUID string
	SessionUID  string
	ResultPage  int
	ResultLimit int
}

type ListSnapshotsQuery struct {
	SessionUID string
	Page       int
	Limit      int
}

type UpdateSnapshotCmd struct {
	UID       string
	Status    SnapshotStatus
	Resources []CloudMigrationResource
}

// access token

type CreateAccessTokenResponse struct {
	Token string
}

type Base64EncodedTokenPayload struct {
	Token    string
	Instance Base64HGInstance
}

func (p Base64EncodedTokenPayload) ToMigration() CloudMigrationSession {
	return CloudMigrationSession{
		AuthToken:   p.Token,
		Slug:        p.Instance.Slug,
		StackID:     p.Instance.StackID,
		RegionSlug:  p.Instance.RegionSlug,
		ClusterSlug: p.Instance.ClusterSlug,
	}
}

type Base64HGInstance struct {
	StackID     int
	Slug        string
	RegionSlug  string
	ClusterSlug string
}

// GMS domain structs

type MigrateDataType string

const (
	DashboardDataType  MigrateDataType = "DASHBOARD"
	DatasourceDataType MigrateDataType = "DATASOURCE"
	FolderDataType     MigrateDataType = "FOLDER"
)

type MigrateDataRequest struct {
	Items []MigrateDataRequestItem
}

type MigrateDataRequestItem struct {
	Type  MigrateDataType
	RefID string
	Name  string
	Data  interface{}
}

type ItemStatus string

const (
	ItemStatusOK      ItemStatus = "OK"
	ItemStatusError   ItemStatus = "ERROR"
	ItemStatusPending ItemStatus = "PENDING"
	ItemStatusUnknown ItemStatus = "UNKNOWN"
)

type MigrateDataResponse struct {
	RunUID string
	Items  []CloudMigrationResource
}

type MigrateDataResponseList struct {
	RunUID string
}

type CreateSessionResponse struct {
	SnapshotUid string
}

type InitializeSnapshotResponse struct {
	EncryptionKey  string
	UploadURL      string
	GMSSnapshotUID string
}
