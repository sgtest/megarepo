package codenav

import (
	"context"
	"testing"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/codenav/shared"
	uploadsshared "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/shared"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	sgtypes "github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/codeintel/precise"
)

var (
	testRange1 = shared.Range{Start: shared.Position{Line: 11, Character: 21}, End: shared.Position{Line: 31, Character: 41}}
	testRange2 = shared.Range{Start: shared.Position{Line: 12, Character: 22}, End: shared.Position{Line: 32, Character: 42}}
	testRange3 = shared.Range{Start: shared.Position{Line: 13, Character: 23}, End: shared.Position{Line: 33, Character: 43}}
	testRange4 = shared.Range{Start: shared.Position{Line: 14, Character: 24}, End: shared.Position{Line: 34, Character: 44}}
	testRange5 = shared.Range{Start: shared.Position{Line: 15, Character: 25}, End: shared.Position{Line: 35, Character: 45}}
	testRange6 = shared.Range{Start: shared.Position{Line: 16, Character: 26}, End: shared.Position{Line: 36, Character: 46}}

	mockPath   = "s1/main.go"
	mockCommit = "deadbeef"
)

func TestReferences(t *testing.T) {
	// Set up mocks
	mockRepoStore := defaultMockRepoStore()
	mockLsifStore := NewMockLsifStore()
	mockUploadSvc := NewMockUploadService()
	mockGitserverClient := gitserver.NewMockClient()
	hunkCache, _ := NewHunkCache(50)

	// Init service
	svc := newService(&observation.TestContext, mockRepoStore, mockLsifStore, mockUploadSvc, mockGitserverClient)

	// Set up request state
	mockRequestState := RequestState{}
	mockRequestState.SetLocalCommitCache(mockRepoStore, mockGitserverClient)
	mockRequestState.SetLocalGitTreeTranslator(mockGitserverClient, &sgtypes.Repo{}, mockCommit, mockPath, hunkCache)
	uploads := []uploadsshared.Dump{
		{ID: 50, Commit: "deadbeef", Root: "sub1/"},
		{ID: 51, Commit: "deadbeef", Root: "sub2/"},
		{ID: 52, Commit: "deadbeef", Root: "sub3/"},
		{ID: 53, Commit: "deadbeef", Root: "sub4/"},
	}
	mockRequestState.SetUploadsDataLoader(uploads)

	// Empty result set (prevents nil pointer as scanner is always non-nil)
	mockUploadSvc.GetUploadIDsWithReferencesFunc.PushReturn([]int{}, 0, 0, nil)

	locations := []shared.Location{
		{DumpID: 51, Path: "a.go", Range: testRange1},
		{DumpID: 51, Path: "b.go", Range: testRange2},
		{DumpID: 51, Path: "a.go", Range: testRange3},
		{DumpID: 51, Path: "b.go", Range: testRange4},
		{DumpID: 51, Path: "c.go", Range: testRange5},
	}
	mockLsifStore.GetReferenceLocationsFunc.PushReturn(locations[:1], 1, nil)
	mockLsifStore.GetReferenceLocationsFunc.PushReturn(locations[1:4], 3, nil)
	mockLsifStore.GetReferenceLocationsFunc.PushReturn(locations[4:], 1, nil)

	mockCursor := ReferencesCursor{Phase: "local"}
	mockRequest := RequestArgs{
		RepositoryID: 42,
		Commit:       mockCommit,
		Path:         mockPath,
		Line:         10,
		Character:    20,
		Limit:        50,
	}
	adjustedLocations, _, err := svc.GetReferences(context.Background(), mockRequest, mockRequestState, mockCursor)
	if err != nil {
		t.Fatalf("unexpected error querying references: %s", err)
	}

	expectedLocations := []shared.UploadLocation{
		{Dump: uploads[1], Path: "sub2/a.go", TargetCommit: "deadbeef", TargetRange: testRange1},
		{Dump: uploads[1], Path: "sub2/b.go", TargetCommit: "deadbeef", TargetRange: testRange2},
		{Dump: uploads[1], Path: "sub2/a.go", TargetCommit: "deadbeef", TargetRange: testRange3},
		{Dump: uploads[1], Path: "sub2/b.go", TargetCommit: "deadbeef", TargetRange: testRange4},
		{Dump: uploads[1], Path: "sub2/c.go", TargetCommit: "deadbeef", TargetRange: testRange5},
	}
	if diff := cmp.Diff(expectedLocations, adjustedLocations); diff != "" {
		t.Errorf("unexpected locations (-want +got):\n%s", diff)
	}
}

func TestReferencesWithSubRepoPermissions(t *testing.T) {
	// Set up mocks
	mockRepoStore := defaultMockRepoStore()
	mockLsifStore := NewMockLsifStore()
	mockUploadSvc := NewMockUploadService()
	mockGitserverClient := gitserver.NewMockClient()
	hunkCache, _ := NewHunkCache(50)

	// Init service
	svc := newService(&observation.TestContext, mockRepoStore, mockLsifStore, mockUploadSvc, mockGitserverClient)

	// Set up request state
	mockRequestState := RequestState{}
	mockRequestState.SetLocalCommitCache(mockRepoStore, mockGitserverClient)
	mockRequestState.SetLocalGitTreeTranslator(mockGitserverClient, &sgtypes.Repo{}, mockCommit, mockPath, hunkCache)
	uploads := []uploadsshared.Dump{
		{ID: 50, Commit: "deadbeef", Root: "sub1/"},
		{ID: 51, Commit: "deadbeef", Root: "sub2/"},
		{ID: 52, Commit: "deadbeef", Root: "sub3/"},
		{ID: 53, Commit: "deadbeef", Root: "sub4/"},
	}
	mockRequestState.SetUploadsDataLoader(uploads)

	// Applying sub-repo permissions
	checker := authz.NewMockSubRepoPermissionChecker()
	checker.EnabledFunc.SetDefaultHook(func() bool {
		return true
	})
	checker.PermissionsFunc.SetDefaultHook(func(ctx context.Context, i int32, content authz.RepoContent) (authz.Perms, error) {
		if content.Path == "sub2/a.go" {
			return authz.Read, nil
		}
		return authz.None, nil
	})
	mockRequestState.SetAuthChecker(checker)

	// Empty result set (prevents nil pointer as scanner is always non-nil)
	mockUploadSvc.GetUploadIDsWithReferencesFunc.PushReturn([]int{}, 0, 0, nil)

	locations := []shared.Location{
		{DumpID: 51, Path: "a.go", Range: testRange1},
		{DumpID: 51, Path: "b.go", Range: testRange2},
		{DumpID: 51, Path: "a.go", Range: testRange3},
		{DumpID: 51, Path: "b.go", Range: testRange4},
		{DumpID: 51, Path: "c.go", Range: testRange5},
	}
	mockLsifStore.GetReferenceLocationsFunc.PushReturn(locations[:1], 1, nil)
	mockLsifStore.GetReferenceLocationsFunc.PushReturn(locations[1:4], 3, nil)
	mockLsifStore.GetReferenceLocationsFunc.PushReturn(locations[4:], 1, nil)

	ctx := actor.WithActor(context.Background(), &actor.Actor{UID: 1})
	mockCursor := ReferencesCursor{Phase: "local"}
	mockRequest := RequestArgs{
		RepositoryID: 42,
		Commit:       mockCommit,
		Path:         mockPath,
		Line:         10,
		Character:    20,
		Limit:        50,
	}

	adjustedLocations, _, err := svc.GetReferences(ctx, mockRequest, mockRequestState, mockCursor)
	if err != nil {
		t.Fatalf("unexpected error querying references: %s", err)
	}
	expectedLocations := []shared.UploadLocation{
		{Dump: uploads[1], Path: "sub2/a.go", TargetCommit: "deadbeef", TargetRange: testRange1},
		{Dump: uploads[1], Path: "sub2/a.go", TargetCommit: "deadbeef", TargetRange: testRange3},
	}
	if diff := cmp.Diff(expectedLocations, adjustedLocations); diff != "" {
		t.Errorf("unexpected locations (-want +got):\n%s", diff)
	}
}

func TestReferencesRemote(t *testing.T) {
	// Set up mocks
	mockRepoStore := defaultMockRepoStore()
	mockLsifStore := NewMockLsifStore()
	mockUploadSvc := NewMockUploadService()
	mockGitserverClient := gitserver.NewMockClient()
	hunkCache, _ := NewHunkCache(50)

	// Init service
	svc := newService(&observation.TestContext, mockRepoStore, mockLsifStore, mockUploadSvc, mockGitserverClient)

	// Set up request state
	mockRequestState := RequestState{}
	mockRequestState.SetLocalCommitCache(mockRepoStore, mockGitserverClient)
	mockRequestState.SetLocalGitTreeTranslator(mockGitserverClient, &sgtypes.Repo{}, mockCommit, mockPath, hunkCache)
	uploads := []uploadsshared.Dump{
		{ID: 50, Commit: "deadbeef", Root: "sub1/"},
		{ID: 51, Commit: "deadbeef", Root: "sub2/"},
		{ID: 52, Commit: "deadbeef", Root: "sub3/"},
		{ID: 53, Commit: "deadbeef", Root: "sub4/"},
	}
	mockRequestState.SetUploadsDataLoader(uploads)

	definitionUploads := []uploadsshared.Dump{
		{ID: 150, Commit: "deadbeef1", Root: "sub1/"},
		{ID: 151, Commit: "deadbeef2", Root: "sub2/"},
		{ID: 152, Commit: "deadbeef3", Root: "sub3/"},
		{ID: 153, Commit: "deadbeef4", Root: "sub4/"},
	}
	mockUploadSvc.GetDumpsWithDefinitionsForMonikersFunc.PushReturn(definitionUploads, nil)

	referenceUploads := []uploadsshared.Dump{
		{ID: 250, Commit: "deadbeef1", Root: "sub1/"},
		{ID: 251, Commit: "deadbeef2", Root: "sub2/"},
		{ID: 252, Commit: "deadbeef3", Root: "sub3/"},
		{ID: 253, Commit: "deadbeef4", Root: "sub4/"},
	}
	mockUploadSvc.GetDumpsByIDsFunc.PushReturn(nil, nil) // empty
	mockUploadSvc.GetDumpsByIDsFunc.PushReturn(referenceUploads[:2], nil)
	mockUploadSvc.GetDumpsByIDsFunc.PushReturn(referenceUploads[2:], nil)

	mockUploadSvc.GetUploadIDsWithReferencesFunc.PushReturn([]int{250, 251}, 0, 4, nil)
	mockUploadSvc.GetUploadIDsWithReferencesFunc.PushReturn([]int{252, 253}, 0, 2, nil)

	// upload #150/#250's commits no longer exists; all others do
	mockGitserverClient.CommitsExistFunc.SetDefaultHook(func(ctx context.Context, _ authz.SubRepoPermissionChecker, rcs []api.RepoCommit) (exists []bool, _ error) {
		for _, rc := range rcs {
			exists = append(exists, rc.CommitID != "deadbeef1")
		}
		return
	})

	monikers := []precise.MonikerData{
		{Kind: "import", Scheme: "tsc", Identifier: "padLeft", PackageInformationID: "51"},
		{Kind: "export", Scheme: "tsc", Identifier: "pad_left", PackageInformationID: "52"},
		{Kind: "import", Scheme: "tsc", Identifier: "pad-left", PackageInformationID: "53"},
		{Kind: "import", Scheme: "tsc", Identifier: "left_pad"},
	}
	mockLsifStore.GetMonikersByPositionFunc.PushReturn([][]precise.MonikerData{{monikers[0]}}, nil)
	mockLsifStore.GetMonikersByPositionFunc.PushReturn([][]precise.MonikerData{{monikers[1]}}, nil)
	mockLsifStore.GetMonikersByPositionFunc.PushReturn([][]precise.MonikerData{{monikers[2]}}, nil)
	mockLsifStore.GetMonikersByPositionFunc.PushReturn([][]precise.MonikerData{{monikers[3]}}, nil)

	packageInformation1 := precise.PackageInformationData{Name: "leftpad", Version: "0.1.0"}
	packageInformation2 := precise.PackageInformationData{Name: "leftpad", Version: "0.2.0"}
	packageInformation3 := precise.PackageInformationData{Name: "leftpad", Version: "0.3.0"}
	mockLsifStore.GetPackageInformationFunc.PushReturn(packageInformation1, true, nil)
	mockLsifStore.GetPackageInformationFunc.PushReturn(packageInformation2, true, nil)
	mockLsifStore.GetPackageInformationFunc.PushReturn(packageInformation3, true, nil)

	locations := []shared.Location{
		{DumpID: 51, Path: "a.go", Range: testRange1},
		{DumpID: 51, Path: "b.go", Range: testRange2},
		{DumpID: 51, Path: "a.go", Range: testRange3},
		{DumpID: 51, Path: "b.go", Range: testRange4},
		{DumpID: 51, Path: "c.go", Range: testRange5},
	}
	mockLsifStore.GetReferenceLocationsFunc.PushReturn(locations[:1], 1, nil)
	mockLsifStore.GetReferenceLocationsFunc.PushReturn(locations[1:4], 3, nil)
	mockLsifStore.GetReferenceLocationsFunc.PushReturn(locations[4:5], 1, nil)

	monikerLocations := []shared.Location{
		{DumpID: 53, Path: "a.go", Range: testRange1},
		{DumpID: 53, Path: "b.go", Range: testRange2},
		{DumpID: 53, Path: "a.go", Range: testRange3},
		{DumpID: 53, Path: "b.go", Range: testRange4},
		{DumpID: 53, Path: "c.go", Range: testRange5},
	}
	mockLsifStore.GetBulkMonikerLocationsFunc.PushReturn(monikerLocations[0:1], 1, nil) // defs
	mockLsifStore.GetBulkMonikerLocationsFunc.PushReturn(monikerLocations[1:2], 1, nil) // refs batch 1
	mockLsifStore.GetBulkMonikerLocationsFunc.PushReturn(monikerLocations[2:], 3, nil)  // refs batch 2

	// uploads := []dbstore.Dump{
	// 	{ID: 50, Commit: "deadbeef", Root: "sub1/"},
	// 	{ID: 51, Commit: "deadbeef", Root: "sub2/"},
	// 	{ID: 52, Commit: "deadbeef", Root: "sub3/"},
	// 	{ID: 53, Commit: "deadbeef", Root: "sub4/"},
	// }
	// resolver.SetUploadsDataLoader(uploads)

	mockCursor := ReferencesCursor{Phase: "local"}
	mockRequest := RequestArgs{
		RepositoryID: 42,
		Commit:       mockCommit,
		Path:         mockPath,
		Line:         10,
		Character:    20,
		Limit:        50,
	}
	adjustedLocations, _, err := svc.GetReferences(context.Background(), mockRequest, mockRequestState, mockCursor)
	if err != nil {
		t.Fatalf("unexpected error querying references: %s", err)
	}

	expectedLocations := []shared.UploadLocation{
		{Dump: uploads[1], Path: "sub2/a.go", TargetCommit: "deadbeef", TargetRange: testRange1},
		{Dump: uploads[1], Path: "sub2/b.go", TargetCommit: "deadbeef", TargetRange: testRange2},
		{Dump: uploads[1], Path: "sub2/a.go", TargetCommit: "deadbeef", TargetRange: testRange3},
		{Dump: uploads[1], Path: "sub2/b.go", TargetCommit: "deadbeef", TargetRange: testRange4},
		{Dump: uploads[1], Path: "sub2/c.go", TargetCommit: "deadbeef", TargetRange: testRange5},
		{Dump: uploads[3], Path: "sub4/a.go", TargetCommit: "deadbeef", TargetRange: testRange1},
		{Dump: uploads[3], Path: "sub4/b.go", TargetCommit: "deadbeef", TargetRange: testRange2},
		{Dump: uploads[3], Path: "sub4/a.go", TargetCommit: "deadbeef", TargetRange: testRange3},
		{Dump: uploads[3], Path: "sub4/b.go", TargetCommit: "deadbeef", TargetRange: testRange4},
		{Dump: uploads[3], Path: "sub4/c.go", TargetCommit: "deadbeef", TargetRange: testRange5},
	}
	if diff := cmp.Diff(expectedLocations, adjustedLocations); diff != "" {
		t.Errorf("unexpected locations (-want +got):\n%s", diff)
	}

	if history := mockUploadSvc.GetDumpsWithDefinitionsForMonikersFunc.History(); len(history) != 1 {
		t.Fatalf("unexpected call count for dbstore.DefinitionDump. want=%d have=%d", 1, len(history))
	} else {
		expectedMonikers := []precise.QualifiedMonikerData{
			{MonikerData: monikers[0], PackageInformationData: packageInformation1},
			{MonikerData: monikers[1], PackageInformationData: packageInformation2},
			{MonikerData: monikers[2], PackageInformationData: packageInformation3},
		}
		if diff := cmp.Diff(expectedMonikers, history[0].Arg1); diff != "" {
			t.Errorf("unexpected monikers (-want +got):\n%s", diff)
		}
	}

	if history := mockLsifStore.GetBulkMonikerLocationsFunc.History(); len(history) != 3 {
		t.Fatalf("unexpected call count for lsifstore.BulkMonikerResults. want=%d have=%d", 3, len(history))
	} else {
		if diff := cmp.Diff([]int{151, 152, 153}, history[0].Arg2); diff != "" {
			t.Errorf("unexpected ids (-want +got):\n%s", diff)
		}

		expectedMonikers := []precise.MonikerData{
			monikers[0],
			monikers[1],
			monikers[2],
		}
		if diff := cmp.Diff(expectedMonikers, history[0].Arg3); diff != "" {
			t.Errorf("unexpected monikers (-want +got):\n%s", diff)
		}

		if diff := cmp.Diff([]int{251}, history[1].Arg2); diff != "" {
			t.Errorf("unexpected ids (-want +got):\n%s", diff)
		}
		if diff := cmp.Diff(expectedMonikers, history[1].Arg3); diff != "" {
			t.Errorf("unexpected monikers (-want +got):\n%s", diff)
		}

		if diff := cmp.Diff([]int{252, 253}, history[2].Arg2); diff != "" {
			t.Errorf("unexpected ids (-want +got):\n%s", diff)
		}
		if diff := cmp.Diff(expectedMonikers, history[2].Arg3); diff != "" {
			t.Errorf("unexpected monikers (-want +got):\n%s", diff)
		}
	}
}

func TestReferencesRemoteWithSubRepoPermissions(t *testing.T) {
	// Set up mocks
	mockRepoStore := defaultMockRepoStore()
	mockLsifStore := NewMockLsifStore()
	mockUploadSvc := NewMockUploadService()
	mockGitserverClient := gitserver.NewMockClient()
	hunkCache, _ := NewHunkCache(50)

	// Init service
	svc := newService(&observation.TestContext, mockRepoStore, mockLsifStore, mockUploadSvc, mockGitserverClient)

	// Set up request state
	mockRequestState := RequestState{}
	mockRequestState.SetLocalCommitCache(mockRepoStore, mockGitserverClient)
	mockRequestState.SetLocalGitTreeTranslator(mockGitserverClient, &sgtypes.Repo{}, mockCommit, mockPath, hunkCache)
	uploads := []uploadsshared.Dump{
		{ID: 50, Commit: "deadbeef", Root: "sub1/"},
		{ID: 51, Commit: "deadbeef", Root: "sub2/"},
		{ID: 52, Commit: "deadbeef", Root: "sub3/"},
		{ID: 53, Commit: "deadbeef", Root: "sub4/"},
	}
	mockRequestState.SetUploadsDataLoader(uploads)

	// Applying sub-repo permissions
	checker := authz.NewMockSubRepoPermissionChecker()
	checker.EnabledFunc.SetDefaultHook(func() bool {
		return true
	})
	checker.PermissionsFunc.SetDefaultHook(func(ctx context.Context, i int32, content authz.RepoContent) (authz.Perms, error) {
		if content.Path == "sub2/b.go" || content.Path == "sub4/b.go" {
			return authz.Read, nil
		}
		return authz.None, nil
	})
	mockRequestState.SetAuthChecker(checker)

	definitionUploads := []uploadsshared.Dump{
		{ID: 150, Commit: "deadbeef1", Root: "sub1/"},
		{ID: 151, Commit: "deadbeef2", Root: "sub2/"},
		{ID: 152, Commit: "deadbeef3", Root: "sub3/"},
		{ID: 153, Commit: "deadbeef4", Root: "sub4/"},
	}
	mockUploadSvc.GetDumpsWithDefinitionsForMonikersFunc.PushReturn(definitionUploads, nil)

	referenceUploads := []uploadsshared.Dump{
		{ID: 250, Commit: "deadbeef1", Root: "sub1/"},
		{ID: 251, Commit: "deadbeef2", Root: "sub2/"},
		{ID: 252, Commit: "deadbeef3", Root: "sub3/"},
		{ID: 253, Commit: "deadbeef4", Root: "sub4/"},
	}
	mockUploadSvc.GetDumpsByIDsFunc.PushReturn(nil, nil) // empty
	mockUploadSvc.GetDumpsByIDsFunc.PushReturn(referenceUploads[:2], nil)
	mockUploadSvc.GetDumpsByIDsFunc.PushReturn(referenceUploads[2:], nil)

	mockUploadSvc.GetUploadIDsWithReferencesFunc.PushReturn([]int{250, 251}, 0, 4, nil)
	mockUploadSvc.GetUploadIDsWithReferencesFunc.PushReturn([]int{252, 253}, 0, 2, nil)

	// upload #150/#250's commits no longer exists; all others do
	mockGitserverClient.CommitsExistFunc.SetDefaultHook(func(ctx context.Context, _ authz.SubRepoPermissionChecker, rcs []api.RepoCommit) (exists []bool, _ error) {
		for _, rc := range rcs {
			exists = append(exists, rc.CommitID != "deadbeef1")
		}
		return
	})

	monikers := []precise.MonikerData{
		{Kind: "import", Scheme: "tsc", Identifier: "padLeft", PackageInformationID: "51"},
		{Kind: "export", Scheme: "tsc", Identifier: "pad_left", PackageInformationID: "52"},
		{Kind: "import", Scheme: "tsc", Identifier: "pad-left", PackageInformationID: "53"},
		{Kind: "import", Scheme: "tsc", Identifier: "left_pad"},
	}
	mockLsifStore.GetMonikersByPositionFunc.PushReturn([][]precise.MonikerData{{monikers[0]}}, nil)
	mockLsifStore.GetMonikersByPositionFunc.PushReturn([][]precise.MonikerData{{monikers[1]}}, nil)
	mockLsifStore.GetMonikersByPositionFunc.PushReturn([][]precise.MonikerData{{monikers[2]}}, nil)
	mockLsifStore.GetMonikersByPositionFunc.PushReturn([][]precise.MonikerData{{monikers[3]}}, nil)

	packageInformation1 := precise.PackageInformationData{Name: "leftpad", Version: "0.1.0"}
	packageInformation2 := precise.PackageInformationData{Name: "leftpad", Version: "0.2.0"}
	packageInformation3 := precise.PackageInformationData{Name: "leftpad", Version: "0.3.0"}
	mockLsifStore.GetPackageInformationFunc.PushReturn(packageInformation1, true, nil)
	mockLsifStore.GetPackageInformationFunc.PushReturn(packageInformation2, true, nil)
	mockLsifStore.GetPackageInformationFunc.PushReturn(packageInformation3, true, nil)

	locations := []shared.Location{
		{DumpID: 51, Path: "a.go", Range: testRange1},
		{DumpID: 51, Path: "b.go", Range: testRange2},
		{DumpID: 51, Path: "a.go", Range: testRange3},
		{DumpID: 51, Path: "b.go", Range: testRange4},
		{DumpID: 51, Path: "c.go", Range: testRange5},
	}
	mockLsifStore.GetReferenceLocationsFunc.PushReturn(locations[:1], 1, nil)
	mockLsifStore.GetReferenceLocationsFunc.PushReturn(locations[1:4], 3, nil)
	mockLsifStore.GetReferenceLocationsFunc.PushReturn(locations[4:5], 1, nil)

	monikerLocations := []shared.Location{
		{DumpID: 53, Path: "a.go", Range: testRange1},
		{DumpID: 53, Path: "b.go", Range: testRange2},
		{DumpID: 53, Path: "a.go", Range: testRange3},
		{DumpID: 53, Path: "b.go", Range: testRange4},
		{DumpID: 53, Path: "c.go", Range: testRange5},
	}
	mockLsifStore.GetBulkMonikerLocationsFunc.PushReturn(monikerLocations[0:1], 1, nil) // defs
	mockLsifStore.GetBulkMonikerLocationsFunc.PushReturn(monikerLocations[1:2], 1, nil) // refs batch 1
	mockLsifStore.GetBulkMonikerLocationsFunc.PushReturn(monikerLocations[2:], 3, nil)  // refs batch 2

	ctx := actor.WithActor(context.Background(), &actor.Actor{UID: 1})
	mockCursor := ReferencesCursor{Phase: "local"}
	mockRequest := RequestArgs{
		RepositoryID: 42,
		Commit:       mockCommit,
		Path:         mockPath,
		Line:         10,
		Character:    20,
		Limit:        50,
	}
	adjustedLocations, _, err := svc.GetReferences(ctx, mockRequest, mockRequestState, mockCursor)
	if err != nil {
		t.Fatalf("unexpected error querying references: %s", err)
	}

	expectedLocations := []shared.UploadLocation{
		{Dump: uploads[1], Path: "sub2/b.go", TargetCommit: "deadbeef", TargetRange: testRange2},
		{Dump: uploads[1], Path: "sub2/b.go", TargetCommit: "deadbeef", TargetRange: testRange4},
		{Dump: uploads[3], Path: "sub4/b.go", TargetCommit: "deadbeef", TargetRange: testRange2},
		{Dump: uploads[3], Path: "sub4/b.go", TargetCommit: "deadbeef", TargetRange: testRange4},
	}
	if diff := cmp.Diff(expectedLocations, adjustedLocations); diff != "" {
		t.Errorf("unexpected locations (-want +got):\n%s", diff)
	}

	if history := mockUploadSvc.GetDumpsWithDefinitionsForMonikersFunc.History(); len(history) != 1 {
		t.Fatalf("unexpected call count for dbstore.DefinitionDump. want=%d have=%d", 1, len(history))
	} else {
		expectedMonikers := []precise.QualifiedMonikerData{
			{MonikerData: monikers[0], PackageInformationData: packageInformation1},
			{MonikerData: monikers[1], PackageInformationData: packageInformation2},
			{MonikerData: monikers[2], PackageInformationData: packageInformation3},
		}
		if diff := cmp.Diff(expectedMonikers, history[0].Arg1); diff != "" {
			t.Errorf("unexpected monikers (-want +got):\n%s", diff)
		}
	}

	if history := mockLsifStore.GetBulkMonikerLocationsFunc.History(); len(history) != 3 {
		t.Fatalf("unexpected call count for mockSvc.GetBulkMonikerLocationsFunc. want=%d have=%d", 3, len(history))
	} else {
		if diff := cmp.Diff([]int{151, 152, 153}, history[0].Arg2); diff != "" {
			t.Errorf("unexpected ids (-want +got):\n%s", diff)
		}

		expectedMonikers := []precise.MonikerData{
			monikers[0],
			monikers[1],
			monikers[2],
		}
		if diff := cmp.Diff(expectedMonikers, history[0].Arg3); diff != "" {
			t.Errorf("unexpected monikers (-want +got):\n%s", diff)
		}

		if diff := cmp.Diff([]int{251}, history[1].Arg2); diff != "" {
			t.Errorf("unexpected ids (-want +got):\n%s", diff)
		}
		if diff := cmp.Diff(expectedMonikers, history[1].Arg3); diff != "" {
			t.Errorf("unexpected monikers (-want +got):\n%s", diff)
		}

		if diff := cmp.Diff([]int{252, 253}, history[2].Arg2); diff != "" {
			t.Errorf("unexpected ids (-want +got):\n%s", diff)
		}
		if diff := cmp.Diff(expectedMonikers, history[2].Arg3); diff != "" {
			t.Errorf("unexpected monikers (-want +got):\n%s", diff)
		}
	}
}
