package worker

import (
	"context"
	"fmt"
	"io"
	"os"
	"strconv"
	"strings"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/metrics"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/bloomfilter"
	bundlemocks "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/bundles/client/mocks"
	bundletypes "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/bundles/types"
	gitservermocks "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/gitserver/mocks"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/store"
	storemocks "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/store/mocks"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/sqliteutil"
	"github.com/sourcegraph/sourcegraph/internal/vcs"
)

func init() {
	sqliteutil.SetLocalLibpath()
	sqliteutil.MustRegisterSqlite3WithPcre()
}

func TestProcess(t *testing.T) {
	setupRepoMocks(t)

	upload := store.Upload{
		ID:           42,
		Root:         "root/",
		Commit:       makeCommit(1),
		RepositoryID: 50,
		Indexer:      "lsif-go",
	}

	mockStore := storemocks.NewMockStore()
	bundleManagerClient := bundlemocks.NewMockBundleManagerClient()
	gitserverClient := gitservermocks.NewMockClient()

	// Set default transaction behavior
	mockStore.TransactFunc.SetDefaultReturn(mockStore, nil)
	mockStore.DoneFunc.SetDefaultHook(func(err error) error { return err })

	// Give correlation package a valid input dump
	bundleManagerClient.GetUploadFunc.SetDefaultHook(copyTestDump)

	// Allowlist all files in dump
	gitserverClient.DirectoryChildrenFunc.SetDefaultReturn(map[string][]string{
		"": {"foo.go", "bar.go"},
	}, nil)

	// Set a different tip commit
	gitserverClient.HeadFunc.SetDefaultReturn(makeCommit(30), nil)

	// Return some ancestors for each commit args
	gitserverClient.CommitsNearFunc.SetDefaultHook(func(ctx context.Context, store store.Store, repositoryID int, commit string) (map[string][]string, error) {
		offset, err := strconv.ParseInt(commit, 10, 64)
		if err != nil {
			return nil, err
		}

		commits := map[string][]string{}
		for i := 0; i < 10; i++ {
			commits[makeCommit(int(offset)+i)] = []string{makeCommit(int(offset) + i + 1)}
		}

		return commits, nil
	})

	processor := &processor{
		bundleManagerClient: bundleManagerClient,
		gitserverClient:     gitserverClient,
		metrics:             metrics.NewWorkerMetrics(&observation.TestContext),
	}

	requeued, err := processor.Process(context.Background(), mockStore, upload)
	if err != nil {
		t.Fatalf("unexpected error processing upload: %s", err)
	} else if requeued {
		t.Errorf("unexpected requeue")
	}

	expectedPackages := []bundletypes.Package{
		{DumpID: 42,
			Scheme:  "scheme B",
			Name:    "pkg B",
			Version: "v1.2.3",
		},
	}
	if len(mockStore.UpdatePackagesFunc.History()) != 1 {
		t.Errorf("unexpected number of UpdatePackages calls. want=%d have=%d", 1, len(mockStore.UpdatePackagesFunc.History()))
	} else if diff := cmp.Diff(expectedPackages, mockStore.UpdatePackagesFunc.History()[0].Arg1); diff != "" {
		t.Errorf("unexpected UpdatePackagesFuncargs (-want +got):\n%s", diff)
	}

	filter, err := bloomfilter.CreateFilter([]string{"ident A"})
	if err != nil {
		t.Fatalf("unexpected error creating filter: %s", err)
	}
	expectedPackageReferences := []bundletypes.PackageReference{
		{DumpID: 42,
			Scheme:  "scheme A",
			Name:    "pkg A",
			Version: "v0.1.0",
			Filter:  filter,
		},
	}
	if len(mockStore.UpdatePackageReferencesFunc.History()) != 1 {
		t.Errorf("unexpected number of UpdatePackageReferences calls. want=%d have=%d", 1, len(mockStore.UpdatePackageReferencesFunc.History()))
	} else if diff := cmp.Diff(expectedPackageReferences, mockStore.UpdatePackageReferencesFunc.History()[0].Arg1); diff != "" {
		t.Errorf("unexpected UpdatePackageReferencesFunc args (-want +got):\n%s", diff)
	}

	if len(mockStore.DeleteOverlappingDumpsFunc.History()) != 1 {
		t.Errorf("unexpected number of DeleteOverlappingDumps calls. want=%d have=%d", 1, len(mockStore.DeleteOverlappingDumpsFunc.History()))
	} else if mockStore.DeleteOverlappingDumpsFunc.History()[0].Arg1 != 50 {
		t.Errorf("unexpected value for repository id. want=%d have=%d", 50, mockStore.DeleteOverlappingDumpsFunc.History()[0].Arg1)
	} else if mockStore.DeleteOverlappingDumpsFunc.History()[0].Arg2 != makeCommit(1) {
		t.Errorf("unexpected value for commit. want=%s have=%s", makeCommit(1), mockStore.DeleteOverlappingDumpsFunc.History()[0].Arg2)
	} else if mockStore.DeleteOverlappingDumpsFunc.History()[0].Arg3 != "root/" {
		t.Errorf("unexpected value for root. want=%s have=%s", "root/", mockStore.DeleteOverlappingDumpsFunc.History()[0].Arg3)
	} else if mockStore.DeleteOverlappingDumpsFunc.History()[0].Arg4 != "lsif-go" {
		t.Errorf("unexpected value for indexer. want=%s have=%s", "lsif-go", mockStore.DeleteOverlappingDumpsFunc.History()[0].Arg4)
	}

	offsets := []int{1, 30}
	expectedCommits := map[string][]string{}
	for i := 0; i < 10; i++ {
		for _, offset := range offsets {
			expectedCommits[makeCommit(offset+i)] = []string{makeCommit(offset + i + 1)}
		}
	}
	if len(mockStore.UpdateCommitsFunc.History()) != 1 {
		t.Errorf("unexpected number of update UpdateCommits calls. want=%d have=%d", 1, len(mockStore.UpdateCommitsFunc.History()))
	} else if diff := cmp.Diff(expectedCommits, mockStore.UpdateCommitsFunc.History()[0].Arg2); diff != "" {
		t.Errorf("unexpected update UpdateCommitsFunc args (-want +got):\n%s", diff)
	}

	if len(mockStore.UpdateDumpsVisibleFromTipFunc.History()) != 1 {
		t.Errorf("unexpected number of UpdateDumpsVisibleFromTip calls. want=%d have=%d", 1, len(mockStore.UpdateDumpsVisibleFromTipFunc.History()))
	} else if mockStore.UpdateDumpsVisibleFromTipFunc.History()[0].Arg1 != 50 {
		t.Errorf("unexpected value for repository id. want=%d have=%d", 50, mockStore.UpdateDumpsVisibleFromTipFunc.History()[0].Arg1)
	} else if mockStore.UpdateDumpsVisibleFromTipFunc.History()[0].Arg2 != makeCommit(30) {
		t.Errorf("unexpected value for tip commit. want=%s have=%s", makeCommit(30), mockStore.UpdateDumpsVisibleFromTipFunc.History()[0].Arg2)
	}

	if len(bundleManagerClient.SendDBFunc.History()) != 1 {
		t.Errorf("unexpected number of SendDB calls. want=%d have=%d", 1, len(bundleManagerClient.SendDBFunc.History()))
	} else if bundleManagerClient.SendDBFunc.History()[0].Arg1 != 42 {
		t.Errorf("unexpected SendDBFunc args. want=%d have=%d", 42, bundleManagerClient.SendDBFunc.History()[0].Arg1)
	}
}

func TestProcessError(t *testing.T) {
	setupRepoMocks(t)

	upload := store.Upload{
		ID:           42,
		Root:         "root/",
		Commit:       makeCommit(1),
		RepositoryID: 50,
		Indexer:      "lsif-go",
	}

	mockStore := storemocks.NewMockStore()
	bundleManagerClient := bundlemocks.NewMockBundleManagerClient()
	gitserverClient := gitservermocks.NewMockClient()

	// Set default transaction behavior
	mockStore.TransactFunc.SetDefaultReturn(mockStore, nil)
	mockStore.DoneFunc.SetDefaultHook(func(err error) error { return err })

	// Give correlation package a valid input dump
	bundleManagerClient.GetUploadFunc.SetDefaultHook(copyTestDump)

	// Set a different tip commit
	gitserverClient.HeadFunc.SetDefaultReturn("", fmt.Errorf("uh-oh!"))

	processor := &processor{
		bundleManagerClient: bundleManagerClient,
		gitserverClient:     gitserverClient,
		metrics:             metrics.NewWorkerMetrics(&observation.TestContext),
	}

	requeued, err := processor.Process(context.Background(), mockStore, upload)
	if err == nil {
		t.Fatalf("unexpected nil error processing upload")
	} else if !strings.Contains(err.Error(), "uh-oh!") {
		t.Fatalf("unexpected error: %s", err)
	} else if requeued {
		t.Errorf("unexpected requeue")
	}

	if len(mockStore.DoneFunc.History()) != 1 {
		t.Errorf("unexpected number of Done calls. want=%d have=%d", 1, len(mockStore.DoneFunc.History()))
	}

	if len(bundleManagerClient.DeleteUploadFunc.History()) != 1 {
		t.Errorf("unexpected number of DeleteUpload calls. want=%d have=%d", 1, len(bundleManagerClient.DeleteUploadFunc.History()))
	}
}

func TestProcessCloneInProgress(t *testing.T) {
	t.Cleanup(func() {
		backend.Mocks.Repos.Get = nil
		backend.Mocks.Repos.ResolveRev = nil
	})

	backend.Mocks.Repos.Get = func(ctx context.Context, repoID api.RepoID) (*types.Repo, error) {
		if repoID != api.RepoID(50) {
			t.Errorf("unexpected repository name. want=%d have=%d", 50, repoID)
		}
		return &types.Repo{ID: repoID}, nil
	}

	backend.Mocks.Repos.ResolveRev = func(ctx context.Context, repo *types.Repo, rev string) (api.CommitID, error) {
		return api.CommitID(""), &vcs.RepoNotExistError{Repo: repo.Name, CloneInProgress: true}
	}

	upload := store.Upload{
		ID:           42,
		Root:         "root/",
		Commit:       makeCommit(1),
		RepositoryID: 50,
		Indexer:      "lsif-go",
	}

	mockStore := storemocks.NewMockStore()
	bundleManagerClient := bundlemocks.NewMockBundleManagerClient()
	gitserverClient := gitservermocks.NewMockClient()

	processor := &processor{
		bundleManagerClient: bundleManagerClient,
		gitserverClient:     gitserverClient,
		metrics:             metrics.NewWorkerMetrics(&observation.TestContext),
	}

	requeued, err := processor.Process(context.Background(), mockStore, upload)
	if err != nil {
		t.Fatalf("unexpected error processing upload: %s", err)
	} else if !requeued {
		t.Errorf("expected upload to be requeued")
	}

	if len(mockStore.RequeueFunc.History()) != 1 {
		t.Errorf("unexpected number of RequeueFunc calls. want=%d have=%d", 1, len(mockStore.RequeueFunc.History()))
	}
}

//
//

func makeCommit(i int) string {
	return fmt.Sprintf("%040d", i)
}

func copyTestDump(ctx context.Context, uploadID int) (io.ReadCloser, error) {
	return os.Open("../../testdata/dump1.lsif")
}

func setupRepoMocks(t *testing.T) {
	t.Cleanup(func() {
		backend.Mocks.Repos.Get = nil
		backend.Mocks.Repos.ResolveRev = nil
	})

	backend.Mocks.Repos.Get = func(ctx context.Context, repoID api.RepoID) (*types.Repo, error) {
		if repoID != api.RepoID(50) {
			t.Errorf("unexpected repository name. want=%d have=%d", 50, repoID)
		}
		return &types.Repo{ID: repoID}, nil
	}

	backend.Mocks.Repos.ResolveRev = func(ctx context.Context, repo *types.Repo, rev string) (api.CommitID, error) {
		if rev != makeCommit(1) {
			t.Errorf("unexpected commit. want=%s have=%s", makeCommit(1), rev)
		}
		return "", nil
	}
}
