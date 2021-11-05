package resolvers

import (
	"context"
	"testing"
	"time"

	insightsdbtesting "github.com/sourcegraph/sourcegraph/enterprise/internal/insights/dbtesting"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtesting"
)

// TestResolver_Insights just checks that root resolver setup and getting an insights connection
// does not result in any errors. It is a pretty minimal test.
func TestResolver_Insights(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	//t.Parallel() // TODO: dbtesting.GetDB is not parallel-safe, yuck.

	ctx := actor.WithInternalActor(context.Background())
	now := time.Now().UTC().Truncate(time.Microsecond)
	clock := func() time.Time { return now }
	timescale, cleanup := insightsdbtesting.TimescaleDB(t)
	defer cleanup()
	postgres := dbtesting.GetDB(t)
	resolver := newWithClock(timescale, postgres, clock)

	insightsConnection, err := resolver.Insights(ctx, nil)
	if err != nil {
		t.Fatal(err)
	}
	if insightsConnection == nil {
		t.Fatal("got nil")
	}
}
