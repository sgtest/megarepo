package resolvers

import (
	"context"
	"strconv"
	"sync"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/discovery"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/store"
	"github.com/sourcegraph/sourcegraph/schema"
)

var _ graphqlbackend.InsightConnectionResolver = &insightConnectionResolver{}

type insightConnectionResolver struct {
	store        store.Interface
	settingStore discovery.SettingStore

	// cache results because they are used by multiple fields
	once     sync.Once
	insights []*schema.Insight
	next     int64
	err      error
}

func (r *insightConnectionResolver) Nodes(ctx context.Context) ([]graphqlbackend.InsightResolver, error) {
	nodes, _, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}
	resolvers := make([]graphqlbackend.InsightResolver, 0, len(nodes))
	for _, insight := range nodes {
		resolvers = append(resolvers, &insightResolver{store: r.store, insight: insight})
	}
	return resolvers, nil
}

func (r *insightConnectionResolver) TotalCount(ctx context.Context) (int32, error) {
	insights, _, err := r.compute(ctx)
	return int32(len(insights)), err
}

func (r *insightConnectionResolver) PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error) {
	_, next, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}
	if next != 0 {
		return graphqlutil.NextPageCursor(strconv.Itoa(int(next))), nil
	}
	return graphqlutil.HasNextPage(false), nil
}

func (r *insightConnectionResolver) compute(ctx context.Context) ([]*schema.Insight, int64, error) {
	r.once.Do(func() {
		r.insights, r.err = discovery.Discover(ctx, r.settingStore)
	})
	return r.insights, r.next, r.err
}

// InsightResolver is also defined here as it is covered by the same tests.

var _ graphqlbackend.InsightResolver = &insightResolver{}

type insightResolver struct {
	store   store.Interface
	insight *schema.Insight
}

func (r *insightResolver) Title() string { return r.insight.Title }

func (r *insightResolver) Description() string { return r.insight.Description }

func (r *insightResolver) Series() []graphqlbackend.InsightSeriesResolver {
	series := r.insight.Series
	resolvers := make([]graphqlbackend.InsightSeriesResolver, 0, len(series))
	for _, series := range series {
		resolvers = append(resolvers, &insightSeriesResolver{store: r.store, series: series})
	}
	return resolvers
}
