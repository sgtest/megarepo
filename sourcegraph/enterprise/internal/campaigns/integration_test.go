package campaigns

import (
	"flag"
	"testing"

	"github.com/sourcegraph/sourcegraph/internal/db/dbtest"
)

var dsn = flag.String("dsn", "", "Database connection string to use in integration tests")

func TestIntegration(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	t.Parallel()

	db, cleanup := dbtest.NewDB(t, *dsn)
	defer cleanup()

	t.Run("Store", testStore(db))
	t.Run("GitHubWebhook", testGitHubWebhook(db))

	// The following tests need to be separate because testStore above wraps everything in a global transaction
	t.Run("StoreLocking", testStoreLocking(db))
	t.Run("ProcessCampaignJob", testProcessCampaignJob(db))
}
