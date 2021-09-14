package indexing

import (
	"context"
	"sort"
	"testing"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/dbstore"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/shared"
	"github.com/sourcegraph/sourcegraph/lib/codeintel/precise"
)

func TestDependencyIndexingSchedulerHandler(t *testing.T) {
	mockDBStore := NewMockDBStore()
	mockExtSvcStore := NewMockExternalServiceStore()
	mockScanner := NewMockPackageReferenceScanner()
	mockDBStore.WithFunc.SetDefaultReturn(mockDBStore)
	mockDBStore.GetUploadByIDFunc.SetDefaultReturn(dbstore.Upload{ID: 42, RepositoryID: 50, Indexer: "lsif-go"}, true, nil)
	mockDBStore.ReferencesForUploadFunc.SetDefaultReturn(mockScanner, nil)

	mockScanner.NextFunc.PushReturn(shared.PackageReference{Package: shared.Package{DumpID: 42, Scheme: "test", Name: "name1", Version: "v2.2.0"}}, true, nil)
	mockScanner.NextFunc.PushReturn(shared.PackageReference{Package: shared.Package{DumpID: 42, Scheme: "test", Name: "name1", Version: "v3.2.0"}}, true, nil)
	mockScanner.NextFunc.PushReturn(shared.PackageReference{Package: shared.Package{DumpID: 42, Scheme: "test", Name: "name2", Version: "v3.2.2"}}, true, nil)
	mockScanner.NextFunc.PushReturn(shared.PackageReference{Package: shared.Package{DumpID: 42, Scheme: "test", Name: "name2", Version: "v2.2.1"}}, true, nil)
	mockScanner.NextFunc.PushReturn(shared.PackageReference{Package: shared.Package{DumpID: 42, Scheme: "test", Name: "name2", Version: "v4.2.3"}}, true, nil)
	mockScanner.NextFunc.PushReturn(shared.PackageReference{Package: shared.Package{DumpID: 42, Scheme: "test", Name: "name1", Version: "v1.2.0"}}, true, nil)
	mockScanner.NextFunc.SetDefaultReturn(shared.PackageReference{}, false, nil)

	indexEnqueuer := NewMockIndexEnqueuer()

	handler := &dependencyIndexingSchedulerHandler{
		dbStore:       mockDBStore,
		indexEnqueuer: indexEnqueuer,
		extsvcStore:   mockExtSvcStore,
	}

	job := dbstore.DependencyIndexingJob{
		UploadID: 42,
	}
	if err := handler.Handle(context.Background(), job); err != nil {
		t.Fatalf("unexpected error performing update: %s", err)
	}

	if len(indexEnqueuer.QueueIndexesForPackageFunc.History()) != 6 {
		t.Errorf("unexpected number of calls to QueueIndexesForPackage. want=%d have=%d", 6, len(indexEnqueuer.QueueIndexesFunc.History()))
	} else {
		var packages []precise.Package
		for _, call := range indexEnqueuer.QueueIndexesForPackageFunc.History() {
			packages = append(packages, call.Arg1)
		}
		sort.Slice(packages, func(i, j int) bool {
			for _, pair := range [][2]string{
				{packages[i].Scheme, packages[j].Scheme},
				{packages[i].Name, packages[j].Name},
				{packages[i].Version, packages[j].Version},
			} {
				if pair[0] < pair[1] {
					return true
				}
				if pair[1] < pair[0] {
					break
				}
			}

			return false
		})

		expectedPackages := []precise.Package{
			{Scheme: "test", Name: "name1", Version: "v1.2.0"},
			{Scheme: "test", Name: "name1", Version: "v2.2.0"},
			{Scheme: "test", Name: "name1", Version: "v3.2.0"},
			{Scheme: "test", Name: "name2", Version: "v2.2.1"},
			{Scheme: "test", Name: "name2", Version: "v3.2.2"},
			{Scheme: "test", Name: "name2", Version: "v4.2.3"},
		}
		if diff := cmp.Diff(expectedPackages, packages); diff != "" {
			t.Errorf("unexpected packages (-want +got):\n%s", diff)
		}
	}
}

func TestDependencyIndexingSchedulerHandlerShouldSkipRepository(t *testing.T) {
	mockDBStore := NewMockDBStore()
	mockExtSvcStore := NewMockExternalServiceStore()
	mockScanner := NewMockPackageReferenceScanner()
	mockDBStore.WithFunc.SetDefaultReturn(mockDBStore)
	mockDBStore.GetUploadByIDFunc.SetDefaultReturn(dbstore.Upload{ID: 42, RepositoryID: 51, Indexer: "lsif-tsc"}, true, nil)
	mockDBStore.ReferencesForUploadFunc.SetDefaultReturn(mockScanner, nil)

	indexEnqueuer := NewMockIndexEnqueuer()

	handler := &dependencyIndexingSchedulerHandler{
		dbStore:       mockDBStore,
		indexEnqueuer: indexEnqueuer,
		extsvcStore:   mockExtSvcStore,
	}

	job := dbstore.DependencyIndexingJob{
		UploadID: 42,
	}
	if err := handler.Handle(context.Background(), job); err != nil {
		t.Fatalf("unexpected error performing update: %s", err)
	}

	if len(indexEnqueuer.QueueIndexesForPackageFunc.History()) != 0 {
		t.Errorf("unexpected number of calls to QueueIndexesForPackage. want=%d have=%d", 0, len(indexEnqueuer.QueueIndexesFunc.History()))
	}
}
