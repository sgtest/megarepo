package resolvers

import (
	"context"
	"fmt"
	"time"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/background"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/scheduler"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/store"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

var (
	_ graphqlbackend.InsightsResolver            = &Resolver{}
	_ graphqlbackend.InsightsAggregationResolver = &AggregationResolver{}
)

// baseInsightResolver is a "super" resolver for all other insights resolvers. Since insights interacts with multiple
// database and multiple Stores, this is a convenient way to propagate those stores without having to drill individual
// references all over the place, but still allow interfaces at the individual resolver level for mocking.
type baseInsightResolver struct {
	insightStore    *store.InsightStore
	timeSeriesStore *store.Store
	dashboardStore  *store.DBDashboardStore
	workerBaseStore *basestore.Store
	scheduler       *scheduler.Scheduler

	// including the DB references for any one off stores that may need to be created.
	insightsDB edb.InsightsDB
	postgresDB database.DB
}

func WithBase(insightsDB edb.InsightsDB, primaryDB database.DB, clock func() time.Time) *baseInsightResolver {
	insightStore := store.NewInsightStore(insightsDB)
	timeSeriesStore := store.NewWithClock(insightsDB, store.NewInsightPermissionStore(primaryDB), clock)
	dashboardStore := store.NewDashboardStore(insightsDB)
	insightsScheduler := scheduler.NewScheduler(insightsDB)
	workerBaseStore := basestore.NewWithHandle(primaryDB.Handle())

	return &baseInsightResolver{
		insightStore:    insightStore,
		timeSeriesStore: timeSeriesStore,
		dashboardStore:  dashboardStore,
		workerBaseStore: workerBaseStore,
		scheduler:       insightsScheduler,
		insightsDB:      insightsDB,
		postgresDB:      primaryDB,
	}
}

// Resolver is the GraphQL resolver of all things related to Insights.
type Resolver struct {
	logger               log.Logger
	timeSeriesStore      store.Interface
	insightMetadataStore store.InsightMetadataStore
	dataSeriesStore      store.DataSeriesStore
	insightEnqueuer      *background.InsightEnqueuer

	baseInsightResolver
}

// New returns a new Resolver whose store uses the given Postgres DBs.
func New(db edb.InsightsDB, postgres database.DB) graphqlbackend.InsightsResolver {
	return newWithClock(db, postgres, timeutil.Now)
}

// newWithClock returns a new Resolver whose store uses the given Postgres DBs and the given clock
// for timestamps.
func newWithClock(db edb.InsightsDB, postgres database.DB, clock func() time.Time) *Resolver {
	base := WithBase(db, postgres, clock)
	return &Resolver{
		logger:               log.Scoped("Resolver", ""),
		baseInsightResolver:  *base,
		timeSeriesStore:      base.timeSeriesStore,
		insightMetadataStore: base.insightStore,
		dataSeriesStore:      base.insightStore,
		insightEnqueuer:      background.NewInsightEnqueuer(clock, base.workerBaseStore),
	}
}

func (r *Resolver) InsightsDashboards(ctx context.Context, args *graphqlbackend.InsightsDashboardsArgs) (graphqlbackend.InsightsDashboardConnectionResolver, error) {
	return &dashboardConnectionResolver{
		baseInsightResolver: r.baseInsightResolver,
		orgStore:            r.postgresDB.Orgs(),
		args:                args,
	}, nil
}

// 🚨 SECURITY
// only add users / orgs if the user is non-anonymous. This will restrict anonymous users to only see
// dashboards with a global grant.
func getUserPermissions(ctx context.Context, orgStore database.OrgStore) (userIds []int, orgIds []int, err error) {
	userId := actor.FromContext(ctx).UID
	if userId != 0 {
		var orgs []*types.Org
		orgs, err = orgStore.GetByUserID(ctx, userId)
		if err != nil {
			return
		}
		userIds = []int{int(userId)}
		orgIds = make([]int, 0, len(orgs))
		for _, org := range orgs {
			orgIds = append(orgIds, int(org.ID))
		}
	}
	return
}

// AggregationResolver is the GraphQL resolver for insights aggregations.
type AggregationResolver struct {
	postgresDB database.DB
	logger     log.Logger
	operations *aggregationsOperations
}

func NewAggregationResolver(observationCtx *observation.Context, postgres database.DB) graphqlbackend.InsightsAggregationResolver {
	return &AggregationResolver{
		logger:     log.Scoped("AggregationResolver", ""),
		postgresDB: postgres,
		operations: newAggregationsOperations(observationCtx),
	}
}

func (r *AggregationResolver) SearchQueryAggregate(ctx context.Context, args graphqlbackend.SearchQueryArgs) (graphqlbackend.SearchQueryAggregateResolver, error) {
	return &searchAggregateResolver{
		postgresDB:  r.postgresDB,
		searchQuery: args.Query,
		patternType: args.PatternType,
		operations:  r.operations,
	}, nil
}

type aggregationsOperations struct {
	aggregations *observation.Operation
}

func newAggregationsOperations(observationCtx *observation.Context) *aggregationsOperations {
	redM := metrics.NewREDMetrics(
		observationCtx.Registerer,
		"insights_aggregations",
		metrics.WithLabels("op", "extended_mode", "aggregation_mode"),
	)

	op := func(name string) *observation.Operation {
		return observationCtx.Operation(observation.Op{
			Name:              fmt.Sprintf("insights_aggregations.%s", name),
			MetricLabelValues: []string{name},
			Metrics:           redM,
			ErrorFilter: func(err error) observation.ErrorFilterBehaviour {
				return observation.EmitForTraces | observation.EmitForMetrics // silence logging for these errors
			},
		})
	}

	return &aggregationsOperations{
		aggregations: op("Aggregations"),
	}
}
