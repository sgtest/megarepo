package api

import (
	"context"
	"testing"

	"github.com/google/go-cmp/cmp"
	bundles "github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/client"
	bundlemocks "github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/mocks"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/db"
	dbmocks "github.com/sourcegraph/sourcegraph/internal/codeintel/db/mocks"
)

func TestDefinitions(t *testing.T) {
	mockDB := dbmocks.NewMockDB()
	mockBundleManagerClient := bundlemocks.NewMockBundleManagerClient()
	mockBundleClient := bundlemocks.NewMockBundleClient()

	setMockDBGetDumpByID(t, mockDB, map[int]db.Dump{42: testDump1})
	setMockBundleManagerClientBundleClient(t, mockBundleManagerClient, map[int]bundles.BundleClient{42: mockBundleClient})
	setMockBundleClientDefinitions(t, mockBundleClient, "main.go", 10, 50, []bundles.Location{
		{DumpID: 42, Path: "foo.go", Range: testRange1},
		{DumpID: 42, Path: "bar.go", Range: testRange2},
		{DumpID: 42, Path: "baz.go", Range: testRange3},
	})

	api := New(mockDB, mockBundleManagerClient)
	definitions, err := api.Definitions(context.Background(), "sub1/main.go", 10, 50, 42)
	if err != nil {
		t.Fatalf("expected error getting definitions: %s", err)
	}

	expectedDefinitions := []ResolvedLocation{
		{Dump: testDump1, Path: "sub1/foo.go", Range: testRange1},
		{Dump: testDump1, Path: "sub1/bar.go", Range: testRange2},
		{Dump: testDump1, Path: "sub1/baz.go", Range: testRange3},
	}
	if diff := cmp.Diff(expectedDefinitions, definitions); diff != "" {
		t.Errorf("unexpected definitions (-want +got):\n%s", diff)
	}
}

func TestDefinitionsUnknownDump(t *testing.T) {
	mockDB := dbmocks.NewMockDB()
	mockBundleManagerClient := bundlemocks.NewMockBundleManagerClient()
	setMockDBGetDumpByID(t, mockDB, nil)

	api := New(mockDB, mockBundleManagerClient)
	if _, err := api.Definitions(context.Background(), "sub1/main.go", 10, 50, 25); err != ErrMissingDump {
		t.Fatalf("unexpected error getting definitions. want=%q have=%q", ErrMissingDump, err)
	}
}

func TestDefinitionViaSameDumpMoniker(t *testing.T) {
	mockDB := dbmocks.NewMockDB()
	mockBundleManagerClient := bundlemocks.NewMockBundleManagerClient()
	mockBundleClient := bundlemocks.NewMockBundleClient()

	setMockDBGetDumpByID(t, mockDB, map[int]db.Dump{42: testDump1})
	setMockBundleManagerClientBundleClient(t, mockBundleManagerClient, map[int]bundles.BundleClient{42: mockBundleClient})
	setMockBundleClientDefinitions(t, mockBundleClient, "main.go", 10, 50, nil)
	setMockBundleClientMonikersByPosition(t, mockBundleClient, "main.go", 10, 50, [][]bundles.MonikerData{{testMoniker2}})
	setMockBundleClientMonikerResults(t, mockBundleClient, "definition", "gomod", "pad", 0, 0, []bundles.Location{
		{DumpID: 42, Path: "foo.go", Range: testRange1},
		{DumpID: 42, Path: "bar.go", Range: testRange2},
		{DumpID: 42, Path: "baz.go", Range: testRange3},
	}, 3)

	api := New(mockDB, mockBundleManagerClient)
	definitions, err := api.Definitions(context.Background(), "sub1/main.go", 10, 50, 42)
	if err != nil {
		t.Fatalf("expected error getting definitions: %s", err)
	}

	expectedDefinitions := []ResolvedLocation{
		{Dump: testDump1, Path: "sub1/foo.go", Range: testRange1},
		{Dump: testDump1, Path: "sub1/bar.go", Range: testRange2},
		{Dump: testDump1, Path: "sub1/baz.go", Range: testRange3},
	}
	if diff := cmp.Diff(expectedDefinitions, definitions); diff != "" {
		t.Errorf("unexpected definitions (-want +got):\n%s", diff)
	}
}

func TestDefinitionViaRemoteDumpMoniker(t *testing.T) {
	mockDB := dbmocks.NewMockDB()
	mockBundleManagerClient := bundlemocks.NewMockBundleManagerClient()
	mockBundleClient1 := bundlemocks.NewMockBundleClient()
	mockBundleClient2 := bundlemocks.NewMockBundleClient()

	setMockDBGetDumpByID(t, mockDB, map[int]db.Dump{42: testDump1, 50: testDump2})
	setMockBundleManagerClientBundleClient(t, mockBundleManagerClient, map[int]bundles.BundleClient{42: mockBundleClient1, 50: mockBundleClient2})
	setMockBundleClientDefinitions(t, mockBundleClient1, "main.go", 10, 50, nil)
	setMockBundleClientMonikersByPosition(t, mockBundleClient1, "main.go", 10, 50, [][]bundles.MonikerData{{testMoniker1}})
	setMockBundleClientPackageInformation(t, mockBundleClient1, "main.go", "1234", testPackageInformation)
	setMockDBGetPackage(t, mockDB, "gomod", "leftpad", "0.1.0", testDump2, true)
	setMockBundleClientMonikerResults(t, mockBundleClient2, "definition", "gomod", "pad", 0, 0, []bundles.Location{
		{DumpID: 50, Path: "foo.go", Range: testRange1},
		{DumpID: 50, Path: "bar.go", Range: testRange2},
		{DumpID: 50, Path: "baz.go", Range: testRange3},
	}, 15)

	api := New(mockDB, mockBundleManagerClient)
	definitions, err := api.Definitions(context.Background(), "sub1/main.go", 10, 50, 42)
	if err != nil {
		t.Fatalf("expected error getting definitions: %s", err)
	}

	expectedDefinitions := []ResolvedLocation{
		{Dump: testDump2, Path: "sub2/foo.go", Range: testRange1},
		{Dump: testDump2, Path: "sub2/bar.go", Range: testRange2},
		{Dump: testDump2, Path: "sub2/baz.go", Range: testRange3},
	}
	if diff := cmp.Diff(expectedDefinitions, definitions); diff != "" {
		t.Errorf("unexpected definitions (-want +got):\n%s", diff)
	}
}
