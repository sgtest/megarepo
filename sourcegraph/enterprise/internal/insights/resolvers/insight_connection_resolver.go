package resolvers

import (
	"context"
	"strconv"
	"sync"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/insights/store"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	"github.com/sourcegraph/sourcegraph/schema"
)

var _ graphqlbackend.InsightConnectionResolver = &insightConnectionResolver{}

type insightConnectionResolver struct {
	store        store.Interface
	settingStore *database.SettingStore

	// We use our own mock here because database.Mocks.Settings.GetLatest is a global which means
	// we could not run our tests in parallel.
	mocksSettingsGetLatest func(ctx context.Context, subject api.SettingsSubject) (*api.Settings, error)

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
		settingsGetLatest := r.settingStore.GetLatest
		if r.mocksSettingsGetLatest != nil {
			settingsGetLatest = r.mocksSettingsGetLatest
		}

		// Get latest Global user settings.
		//
		// FUTURE: include user/org settings.
		subject := api.SettingsSubject{Site: true}
		globalSettingsRaw, err := settingsGetLatest(ctx, subject)
		if err != nil {
			r.err = err
			return
		}
		globalSettings, err := parseUserSettings(globalSettingsRaw)
		r.insights = globalSettings.Insights
	})
	return r.insights, r.next, r.err
}

func parseUserSettings(settings *api.Settings) (*schema.Settings, error) {
	if settings == nil {
		// Settings have never been saved for this subject; equivalent to `{}`.
		return &schema.Settings{}, nil
	}
	var v schema.Settings
	if err := jsonc.Unmarshal(settings.Contents, &v); err != nil {
		return nil, err
	}
	return &v, nil
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
