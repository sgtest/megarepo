package repos_test

import (
	"database/sql"
	"flag"
	"testing"

	"github.com/inconshreveable/log15"
	"github.com/opentracing/opentracing-go"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/repo-updater/repos"
	"github.com/sourcegraph/sourcegraph/internal/db/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/trace"
)

// This error is passed to txstore.Done in order to always
// roll-back the transaction a test case executes in.
// This is meant to ensure each test case has a clean slate.
var errRollback = errors.New("tx: rollback")

var dsn = flag.String("dsn", "", "Database connection string to use in integration tests")

func TestIntegration(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	t.Parallel()

	db := dbtest.NewDB(t, *dsn)

	dbstore := repos.NewDBStore(db, sql.TxOptions{
		Isolation: sql.LevelSerializable,
	})

	lg := log15.New()
	lg.SetHandler(log15.DiscardHandler())

	store := repos.NewObservedStore(
		dbstore,
		lg,
		repos.NewStoreMetrics(),
		trace.Tracer{Tracer: opentracing.GlobalTracer()},
	)

	for _, tc := range []struct {
		name string
		test func(*testing.T)
	}{
		{"DBStore/Transact", testDBStoreTransact(dbstore)},
		{"DBStore/ListExternalServices", testStoreListExternalServices(store)},
		{"DBStore/ListExternalServices/ByRepo", testStoreListExternalServicesByRepos(store)},
		{"DBStore/UpsertExternalServices", testStoreUpsertExternalServices(store)},
		{"DBStore/UpsertRepos", testStoreUpsertRepos(store)},
		{"DBStore/ListRepos", testStoreListRepos(store)},
		{"DBStore/ListRepos/Pagination", testStoreListReposPagination(store)},
		{"DBStore/SetClonedRepos", testStoreSetClonedRepos(store)},
		{"DBStore/CountNotClonedRepos", testStoreCountNotClonedRepos(store)},
		{"DBStore/Syncer/Sync", testSyncerSync(store)},
		{"DBStore/Syncer/SyncSubset", testSyncSubset(store)},
	} {
		t.Run(tc.name, tc.test)
	}
}
