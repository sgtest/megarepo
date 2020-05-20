package janitor

import (
	"context"
	"fmt"
	"path/filepath"
	"testing"

	"github.com/google/go-cmp/cmp"
	dbmocks "github.com/sourcegraph/sourcegraph/internal/codeintel/db/mocks"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
)

func TestEvictBundlesStopsAfterFreeingDesiredSpace(t *testing.T) {
	bundleDir := testRoot(t)
	sizes := map[int]int{
		1:  20,
		2:  20,
		3:  20,
		4:  20,
		5:  20,
		6:  20,
		7:  20,
		8:  20,
		9:  20,
		10: 20,
	}

	for id, size := range sizes {
		path := filepath.Join(bundleDir, "dbs", fmt.Sprintf("%d.lsif.db", id))
		if err := makeFileWithSize(path, size); err != nil {
			t.Fatalf("unexpected error creating file %s: %s", path, err)
		}
	}

	calls := 0
	mockDB := dbmocks.NewMockDB()
	mockDB.DeleteOldestDumpFunc.SetDefaultHook(func(ctx context.Context) (int, bool, error) {
		calls++
		return calls, true, nil
	})

	j := &Janitor{
		db:        mockDB,
		bundleDir: bundleDir,
		metrics:   NewJanitorMetrics(metrics.TestRegisterer),
	}

	if err := j.evictBundles(100); err != nil {
		t.Fatalf("unexpected error evicting bundles: %s", err)
	}

	names, err := getFilenames(filepath.Join(bundleDir, "dbs"))
	if err != nil {
		t.Fatalf("unexpected error listing directory: %s", err)
	}

	expected := []string{"10.lsif.db", "6.lsif.db", "7.lsif.db", "8.lsif.db", "9.lsif.db"}
	if diff := cmp.Diff(expected, names); diff != "" {
		t.Errorf("unexpected directory contents (-want +got):\n%s", diff)
	}
}

func TestEvictBundlesStopsWithNoPrunableDatabases(t *testing.T) {
	bundleDir := testRoot(t)
	sizes := map[int]int{
		1:  10,
		2:  10,
		3:  10,
		4:  10,
		5:  10,
		6:  10,
		7:  10,
		8:  10,
		9:  10,
		10: 10,
	}

	for id, size := range sizes {
		path := filepath.Join(bundleDir, "dbs", fmt.Sprintf("%d.lsif.db", id))
		if err := makeFileWithSize(path, size); err != nil {
			t.Fatalf("unexpected error creating file %s: %s", path, err)
		}
	}

	idsToPrune := []int{1, 2, 3, 4, 5}

	mockDB := dbmocks.NewMockDB()
	mockDB.DeleteOldestDumpFunc.SetDefaultHook(func(ctx context.Context) (int, bool, error) {
		if len(idsToPrune) == 0 {
			return 0, false, nil
		}

		id := idsToPrune[0]
		idsToPrune = idsToPrune[1:]
		return id, true, nil
	})

	j := &Janitor{
		db:        mockDB,
		bundleDir: bundleDir,
		metrics:   NewJanitorMetrics(metrics.TestRegisterer),
	}

	if err := j.evictBundles(100); err != nil {
		t.Fatalf("unexpected error evicting bundles: %s", err)
	}

	names, err := getFilenames(filepath.Join(bundleDir, "dbs"))
	if err != nil {
		t.Fatalf("unexpected error listing directory: %s", err)
	}

	expected := []string{"10.lsif.db", "6.lsif.db", "7.lsif.db", "8.lsif.db", "9.lsif.db"}
	if diff := cmp.Diff(expected, names); diff != "" {
		t.Errorf("unexpected directory contents (-want +got):\n%s", diff)
	}
}

func TestEvictBundlesNoBundleFile(t *testing.T) {
	bundleDir := testRoot(t)

	called := false
	mockDB := dbmocks.NewMockDB()
	mockDB.DeleteOldestDumpFunc.SetDefaultHook(func(ctx context.Context) (int, bool, error) {
		if !called {
			called = true
			return 42, true, nil
		}
		return 0, false, nil
	})

	j := &Janitor{
		db:        mockDB,
		bundleDir: bundleDir,
		metrics:   NewJanitorMetrics(metrics.TestRegisterer),
	}

	if err := j.evictBundles(100); err != nil {
		t.Fatalf("unexpected error evicting bundles: %s", err)
	}
}
