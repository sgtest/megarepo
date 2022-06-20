package store

import (
	"testing"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/encryption"
	et "github.com/sourcegraph/sourcegraph/internal/encryption/testing"
)

func TestIntegration(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	t.Parallel()

	logger := logtest.Scoped(t)

	db := dbtest.NewDB(logger, t)

	t.Run("Store", func(t *testing.T) {
		t.Run("BatchChanges", storeTest(db, nil, testStoreBatchChanges))
		t.Run("Changesets", storeTest(db, nil, testStoreChangesets))
		t.Run("ChangesetEvents", storeTest(db, nil, testStoreChangesetEvents))
		t.Run("ChangesetScheduling", storeTest(db, nil, testStoreChangesetScheduling))
		t.Run("ListChangesetSyncData", storeTest(db, nil, testStoreListChangesetSyncData))
		t.Run("ListChangesetsTextSearch", storeTest(db, nil, testStoreListChangesetsTextSearch))
		t.Run("BatchSpecs", storeTest(db, nil, testStoreBatchSpecs))
		t.Run("ChangesetSpecs", storeTest(db, nil, testStoreChangesetSpecs))
		t.Run("GetRewirerMappingWithArchivedChangesets", storeTest(db, nil, testStoreGetRewirerMappingWithArchivedChangesets))
		t.Run("ChangesetSpecsCurrentState", storeTest(db, nil, testStoreChangesetSpecsCurrentState))
		t.Run("ChangesetSpecsCurrentStateAndTextSearch", storeTest(db, nil, testStoreChangesetSpecsCurrentStateAndTextSearch))
		t.Run("ChangesetSpecsTextSearch", storeTest(db, nil, testStoreChangesetSpecsTextSearch))
		t.Run("CodeHosts", storeTest(db, nil, testStoreCodeHost))
		t.Run("UserDeleteCascades", storeTest(db, nil, testUserDeleteCascades))
		t.Run("ChangesetJobs", storeTest(db, nil, testStoreChangesetJobs))
		t.Run("BulkOperations", storeTest(db, nil, testStoreBulkOperations))
		t.Run("BatchSpecWorkspaces", storeTest(db, nil, testStoreBatchSpecWorkspaces))
		t.Run("BatchSpecWorkspaceExecutionJobs", storeTest(db, nil, testStoreBatchSpecWorkspaceExecutionJobs))
		t.Run("BatchSpecResolutionJobs", storeTest(db, nil, testStoreBatchSpecResolutionJobs))
		t.Run("BatchSpecExecutionCacheEntries", storeTest(db, nil, testStoreBatchSpecExecutionCacheEntries))

		for name, key := range map[string]encryption.Key{
			"no key":   nil,
			"test key": &et.TestKey{},
		} {
			t.Run(name, func(t *testing.T) {
				t.Run("SiteCredentials", storeTest(db, key, testStoreSiteCredentials))
			})
		}
	})
}
