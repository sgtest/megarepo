package cloudmigrationimpl

import (
	"context"
	"time"

	"github.com/grafana/grafana/pkg/services/cloudmigration"
	"github.com/grafana/grafana/pkg/services/contexthandler"
	"github.com/grafana/grafana/pkg/services/dashboards"
	"github.com/grafana/grafana/pkg/services/datasources"
	"github.com/grafana/grafana/pkg/services/folder"
	"github.com/grafana/grafana/pkg/util/retryer"
)

func (s *Service) getMigrationDataJSON(ctx context.Context) (*cloudmigration.MigrateDataRequest, error) {
	// Data sources
	dataSources, err := s.getDataSources(ctx)
	if err != nil {
		s.log.Error("Failed to get datasources", "err", err)
		return nil, err
	}

	// Dashboards
	dashboards, err := s.getDashboards(ctx)
	if err != nil {
		s.log.Error("Failed to get dashboards", "err", err)
		return nil, err
	}

	// Folders
	folders, err := s.getFolders(ctx)
	if err != nil {
		s.log.Error("Failed to get folders", "err", err)
		return nil, err
	}

	migrationDataSlice := make(
		[]cloudmigration.MigrateDataRequestItem, 0,
		len(dataSources)+len(dashboards)+len(folders),
	)

	for _, ds := range dataSources {
		migrationDataSlice = append(migrationDataSlice, cloudmigration.MigrateDataRequestItem{
			Type:  cloudmigration.DatasourceDataType,
			RefID: ds.UID,
			Name:  ds.Name,
			Data:  ds,
		})
	}

	for _, dashboard := range dashboards {
		dashboard.Data.Del("id")
		migrationDataSlice = append(migrationDataSlice, cloudmigration.MigrateDataRequestItem{
			Type:  cloudmigration.DashboardDataType,
			RefID: dashboard.UID,
			Name:  dashboard.Title,
			Data:  map[string]any{"dashboard": dashboard.Data},
		})
	}

	for _, f := range folders {
		migrationDataSlice = append(migrationDataSlice, cloudmigration.MigrateDataRequestItem{
			Type:  cloudmigration.FolderDataType,
			RefID: f.UID,
			Name:  f.Title,
			Data:  f,
		})
	}

	migrationData := &cloudmigration.MigrateDataRequest{
		Items: migrationDataSlice,
	}

	return migrationData, nil
}

func (s *Service) getDataSources(ctx context.Context) ([]datasources.AddDataSourceCommand, error) {
	dataSources, err := s.dsService.GetAllDataSources(ctx, &datasources.GetAllDataSourcesQuery{})
	if err != nil {
		s.log.Error("Failed to get all datasources", "err", err)
		return nil, err
	}

	result := []datasources.AddDataSourceCommand{}
	for _, dataSource := range dataSources {
		// Decrypt secure json to send raw credentials
		decryptedData, err := s.secretsService.DecryptJsonData(ctx, dataSource.SecureJsonData)
		if err != nil {
			s.log.Error("Failed to decrypt secure json data", "err", err)
			return nil, err
		}
		dataSourceCmd := datasources.AddDataSourceCommand{
			OrgID:           dataSource.OrgID,
			Name:            dataSource.Name,
			Type:            dataSource.Type,
			Access:          dataSource.Access,
			URL:             dataSource.URL,
			User:            dataSource.User,
			Database:        dataSource.Database,
			BasicAuth:       dataSource.BasicAuth,
			BasicAuthUser:   dataSource.BasicAuthUser,
			WithCredentials: dataSource.WithCredentials,
			IsDefault:       dataSource.IsDefault,
			JsonData:        dataSource.JsonData,
			SecureJsonData:  decryptedData,
			ReadOnly:        dataSource.ReadOnly,
			UID:             dataSource.UID,
		}
		result = append(result, dataSourceCmd)
	}
	return result, err
}

func (s *Service) getFolders(ctx context.Context) ([]folder.Folder, error) {
	reqCtx := contexthandler.FromContext(ctx)
	folders, err := s.folderService.GetFolders(ctx, folder.GetFoldersQuery{
		SignedInUser: reqCtx.SignedInUser,
	})
	if err != nil {
		return nil, err
	}

	result := make([]folder.Folder, len(folders))
	for i, folder := range folders {
		result[i] = *folder
	}

	return result, nil
}

func (s *Service) getDashboards(ctx context.Context) ([]dashboards.Dashboard, error) {
	dashs, err := s.dashboardService.GetAllDashboards(ctx)
	if err != nil {
		return nil, err
	}

	result := make([]dashboards.Dashboard, len(dashs))
	for i, dashboard := range dashs {
		result[i] = *dashboard
	}

	return result, nil
}

// asynchronous process for writing the snapshot to the filesystem and updating the snapshot status
func (s *Service) buildSnapshot(ctx context.Context, snapshotMeta cloudmigration.CloudMigrationSnapshot) {
	// TODO -- make sure we can only build one snapshot at a time
	s.buildSnapshotMutex.Lock()
	defer s.buildSnapshotMutex.Unlock()
	s.buildSnapshotError = false

	// update snapshot status to creating, add some retries since this is a background task
	if err := retryer.Retry(func() (retryer.RetrySignal, error) {
		err := s.store.UpdateSnapshot(ctx, cloudmigration.UpdateSnapshotCmd{
			UID:    snapshotMeta.UID,
			Status: cloudmigration.SnapshotStatusCreating,
		})
		return retryer.FuncComplete, err
	}, 10, time.Millisecond*100, time.Second*10); err != nil {
		s.log.Error("failed to set snapshot status to 'creating'", "err", err)
		s.buildSnapshotError = true
		return
	}

	// build snapshot
	// just sleep for now to simulate snapshot creation happening
	// need to do a couple of fancy things when we implement this:
	//   - some sort of regular check-in so we know we haven't timed out
	//   - a channel to listen for cancel events
	//   - retries baked into the snapshot writing process?
	s.log.Debug("snapshot meta", "snapshot", snapshotMeta)
	time.Sleep(3 * time.Second)

	// update snapshot status to pending upload with retry
	if err := retryer.Retry(func() (retryer.RetrySignal, error) {
		err := s.store.UpdateSnapshot(ctx, cloudmigration.UpdateSnapshotCmd{
			UID:    snapshotMeta.UID,
			Status: cloudmigration.SnapshotStatusPendingUpload,
		})
		return retryer.FuncComplete, err
	}, 10, time.Millisecond*100, time.Second*10); err != nil {
		s.log.Error("failed to set snapshot status to 'pending upload'", "err", err)
		s.buildSnapshotError = true
	}
}

// asynchronous process for and updating the snapshot status
func (s *Service) uploadSnapshot(ctx context.Context, snapshotMeta cloudmigration.CloudMigrationSnapshot) {
	// TODO -- make sure we can only upload one snapshot at a time
	s.buildSnapshotMutex.Lock()
	defer s.buildSnapshotMutex.Unlock()
	s.buildSnapshotError = false

	// update snapshot status to uploading, add some retries since this is a background task
	if err := retryer.Retry(func() (retryer.RetrySignal, error) {
		err := s.store.UpdateSnapshot(ctx, cloudmigration.UpdateSnapshotCmd{
			UID:    snapshotMeta.UID,
			Status: cloudmigration.SnapshotStatusUploading,
		})
		return retryer.FuncComplete, err
	}, 10, time.Millisecond*100, time.Second*10); err != nil {
		s.log.Error("failed to set snapshot status to 'creating'", "err", err)
		s.buildSnapshotError = true
		return
	}

	// upload snapshot
	// just sleep for now to simulate snapshot creation happening
	s.log.Debug("snapshot meta", "snapshot", snapshotMeta)
	time.Sleep(3 * time.Second)

	// update snapshot status to pending processing with retry
	if err := retryer.Retry(func() (retryer.RetrySignal, error) {
		err := s.store.UpdateSnapshot(ctx, cloudmigration.UpdateSnapshotCmd{
			UID:    snapshotMeta.UID,
			Status: cloudmigration.SnapshotStatusPendingProcessing,
		})
		return retryer.FuncComplete, err
	}, 10, time.Millisecond*100, time.Second*10); err != nil {
		s.log.Error("failed to set snapshot status to 'pending upload'", "err", err)
		s.buildSnapshotError = true
	}

	// simulate the rest
	// processing
	time.Sleep(3 * time.Second)
	if err := s.store.UpdateSnapshot(ctx, cloudmigration.UpdateSnapshotCmd{
		UID:    snapshotMeta.UID,
		Status: cloudmigration.SnapshotStatusProcessing,
	}); err != nil {
		s.log.Error("updating snapshot", "err", err)
	}
	// end here as the GetSnapshot handler will fill in the rest when called
}
