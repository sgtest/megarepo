package resolvers

import (
	"context"
	"time"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/store"
	"github.com/sourcegraph/sourcegraph/internal/gqlutil"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/query"
)

var _ graphqlbackend.InsightSeriesResolver = &dynamicInsightSeriesResolver{}
var _ graphqlbackend.InsightStatusResolver = &emptyInsightStatusResolver{}

// dynamicInsightSeriesResolver is a series resolver that expands based on matches from a search query.
type dynamicInsightSeriesResolver struct {
	generated *query.GeneratedTimeSeries
}

func (d *dynamicInsightSeriesResolver) SeriesId() string {
	return d.generated.SeriesId
}

func (d *dynamicInsightSeriesResolver) Label() string {
	return d.generated.Label
}

func (d *dynamicInsightSeriesResolver) Points(ctx context.Context, _ *graphqlbackend.InsightsPointsArgs) ([]graphqlbackend.InsightsDataPointResolver, error) {
	var resolvers []graphqlbackend.InsightsDataPointResolver
	for i := 0; i < len(d.generated.Points); i++ {
		point := store.SeriesPoint{
			SeriesID: d.generated.SeriesId,
			Time:     d.generated.Points[i].Time,
			Value:    float64(d.generated.Points[i].Count),
		}
		// This resolver is no longer used and about to be removed
		resolvers = append(resolvers, &insightsDataPointResolver{p: point, diffInfo: nil})
	}

	return resolvers, nil
}

func (d *dynamicInsightSeriesResolver) Status(ctx context.Context) (graphqlbackend.InsightStatusResolver, error) {
	return &emptyInsightStatusResolver{}, nil
}

type emptyInsightStatusResolver struct{}

func (e emptyInsightStatusResolver) TotalPoints(ctx context.Context) (int32, error) {
	return 0, nil
}

func (e emptyInsightStatusResolver) PendingJobs(ctx context.Context) (int32, error) {
	return 0, nil
}

func (e emptyInsightStatusResolver) CompletedJobs(ctx context.Context) (int32, error) {
	return 0, nil
}

func (e emptyInsightStatusResolver) FailedJobs(ctx context.Context) (int32, error) {
	return 0, nil
}

func (e emptyInsightStatusResolver) IsLoadingData(ctx context.Context) (*bool, error) {
	// beacuse this resolver is created when dynamic data exists
	// it means it's not loading data.
	loading := false
	return &loading, nil
}

func (e emptyInsightStatusResolver) BackfillQueuedAt(ctx context.Context) *gqlutil.DateTime {
	current := time.Now().AddDate(-1, 0, 0)
	return gqlutil.DateTimeOrNil(&current)
}

func (e emptyInsightStatusResolver) IncompleteDatapoints(ctx context.Context) (resolvers []graphqlbackend.IncompleteDatapointAlert, err error) {
	return nil, nil
}
