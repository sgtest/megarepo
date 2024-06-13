package fake

import (
	"context"
	"encoding/json"
	"fmt"
	"time"

	"github.com/grafana/grafana/pkg/services/cloudmigration"
	"github.com/grafana/grafana/pkg/services/gcom"
)

var fixedDate = time.Date(2024, 6, 5, 17, 30, 40, 0, time.UTC)

// FakeServiceImpl fake implementation of cloudmigration.Service for testing purposes
type FakeServiceImpl struct {
	ReturnError bool
}

var _ cloudmigration.Service = (*FakeServiceImpl)(nil)

func (m FakeServiceImpl) GetToken(_ context.Context) (gcom.TokenView, error) {
	if m.ReturnError {
		return gcom.TokenView{}, fmt.Errorf("mock error")
	}
	return gcom.TokenView{ID: "mock_id", DisplayName: "mock_name"}, nil
}

func (m FakeServiceImpl) CreateToken(_ context.Context) (cloudmigration.CreateAccessTokenResponse, error) {
	if m.ReturnError {
		return cloudmigration.CreateAccessTokenResponse{}, fmt.Errorf("mock error")
	}
	return cloudmigration.CreateAccessTokenResponse{Token: "mock_token"}, nil
}

func (m FakeServiceImpl) ValidateToken(ctx context.Context, migration cloudmigration.CloudMigrationSession) error {
	panic("implement me")
}

func (m FakeServiceImpl) DeleteToken(_ context.Context, _ string) error {
	if m.ReturnError {
		return fmt.Errorf("mock error")
	}
	return nil
}

func (m FakeServiceImpl) CreateSession(_ context.Context, _ cloudmigration.CloudMigrationSessionRequest) (*cloudmigration.CloudMigrationSessionResponse, error) {
	if m.ReturnError {
		return nil, fmt.Errorf("mock error")
	}
	return &cloudmigration.CloudMigrationSessionResponse{
		UID:     "fake_uid",
		Slug:    "fake_stack",
		Created: fixedDate,
		Updated: fixedDate,
	}, nil
}

func (m FakeServiceImpl) GetSession(_ context.Context, _ string) (*cloudmigration.CloudMigrationSession, error) {
	if m.ReturnError {
		return nil, fmt.Errorf("mock error")
	}
	return &cloudmigration.CloudMigrationSession{UID: "fake"}, nil
}

func (m FakeServiceImpl) DeleteSession(_ context.Context, _ string) (*cloudmigration.CloudMigrationSession, error) {
	if m.ReturnError {
		return nil, fmt.Errorf("mock error")
	}
	return &cloudmigration.CloudMigrationSession{UID: "fake"}, nil
}

func (m FakeServiceImpl) GetSessionList(_ context.Context) (*cloudmigration.CloudMigrationSessionListResponse, error) {
	if m.ReturnError {
		return nil, fmt.Errorf("mock error")
	}
	return &cloudmigration.CloudMigrationSessionListResponse{
		Sessions: []cloudmigration.CloudMigrationSessionResponse{
			{UID: "mock_uid_1", Slug: "mock_stack_1", Created: fixedDate, Updated: fixedDate},
			{UID: "mock_uid_2", Slug: "mock_stack_2", Created: fixedDate, Updated: fixedDate},
		},
	}, nil
}

func (m FakeServiceImpl) RunMigration(_ context.Context, _ string) (*cloudmigration.MigrateDataResponse, error) {
	if m.ReturnError {
		return nil, fmt.Errorf("mock error")
	}
	r := fakeMigrateDataResponseDTO()
	return &r, nil
}

func fakeMigrateDataResponseDTO() cloudmigration.MigrateDataResponse {
	return cloudmigration.MigrateDataResponse{
		RunUID: "fake_uid",
		Items: []cloudmigration.MigrateDataResponseItem{
			{Type: "type", RefID: "make_refid", Status: "ok", Error: "none"},
		},
	}
}

func (m FakeServiceImpl) CreateMigrationRun(ctx context.Context, run cloudmigration.CloudMigrationSnapshot) (string, error) {
	panic("implement me")
}

func (m FakeServiceImpl) GetMigrationStatus(_ context.Context, _ string) (*cloudmigration.CloudMigrationSnapshot, error) {
	if m.ReturnError {
		return nil, fmt.Errorf("mock error")
	}
	result, err := json.Marshal(fakeMigrateDataResponseDTO())
	if err != nil {
		return nil, err
	}
	return &cloudmigration.CloudMigrationSnapshot{
		ID:         0,
		UID:        "fake_uid",
		SessionUID: "fake_mig_uid",
		Result:     result,
		Created:    fixedDate,
		Updated:    fixedDate,
		Finished:   fixedDate,
	}, nil
}

func (m FakeServiceImpl) GetMigrationRunList(_ context.Context, _ string) (*cloudmigration.SnapshotList, error) {
	if m.ReturnError {
		return nil, fmt.Errorf("mock error")
	}
	return &cloudmigration.SnapshotList{
		Runs: []cloudmigration.MigrateDataResponseList{
			{RunUID: "fake_run_uid_1"},
			{RunUID: "fake_run_uid_2"},
		},
	}, nil
}
