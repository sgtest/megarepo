package janitor

import (
	"fmt"
	"path/filepath"
	"sort"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	storemocks "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/store/mocks"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
)

func TestRemoveProcessedUploadsWithoutBundleFile(t *testing.T) {
	bundleDir := testRoot(t)
	ids := []int{1, 2, 3, 4, 5}

	for _, id := range []int{1, 3, 5} {
		path := filepath.Join(bundleDir, "dbs", fmt.Sprintf("%d", id), "sqlite.db")
		if err := makeFile(path, time.Now().Local()); err != nil {
			t.Fatalf("unexpected error creating file %s: %s", path, err)
		}
	}

	mockStore := storemocks.NewMockStore()
	mockStore.GetDumpIDsFunc.SetDefaultReturn(ids, nil)

	j := &Janitor{
		store:     mockStore,
		bundleDir: bundleDir,
		metrics:   NewJanitorMetrics(metrics.TestRegisterer),
	}

	if err := j.removeProcessedUploadsWithoutBundleFile(); err != nil {
		t.Fatalf("unexpected error removing processed uploads without bundle files: %s", err)
	}

	if len(mockStore.DeleteUploadByIDFunc.History()) != 2 {
		t.Errorf("unexpected number of DeleteUploadByID calls. want=%d have=%d", 2, len(mockStore.DeleteUploadByIDFunc.History()))
	} else {
		ids := []int{
			mockStore.DeleteUploadByIDFunc.History()[0].Arg1,
			mockStore.DeleteUploadByIDFunc.History()[1].Arg1,
		}
		sort.Ints(ids)

		if diff := cmp.Diff([]int{2, 4}, ids); diff != "" {
			t.Errorf("unexpected dump ids (-want +got):\n%s", diff)
		}
	}
}
