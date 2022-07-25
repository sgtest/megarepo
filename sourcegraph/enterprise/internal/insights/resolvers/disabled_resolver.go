package resolvers

import (
	"context"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type disabledResolver struct {
	reason string
}

func NewDisabledResolver(reason string) graphqlbackend.InsightsResolver {
	return &disabledResolver{reason}
}

func (r *disabledResolver) Insights(ctx context.Context, args *graphqlbackend.InsightsArgs) (graphqlbackend.InsightConnectionResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) InsightsDashboards(ctx context.Context, args *graphqlbackend.InsightsDashboardsArgs) (graphqlbackend.InsightsDashboardConnectionResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) CreateInsightsDashboard(ctx context.Context, args *graphqlbackend.CreateInsightsDashboardArgs) (graphqlbackend.InsightsDashboardPayloadResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) UpdateInsightsDashboard(ctx context.Context, args *graphqlbackend.UpdateInsightsDashboardArgs) (graphqlbackend.InsightsDashboardPayloadResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) DeleteInsightsDashboard(ctx context.Context, args *graphqlbackend.DeleteInsightsDashboardArgs) (*graphqlbackend.EmptyResponse, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) AddInsightViewToDashboard(ctx context.Context, args *graphqlbackend.AddInsightViewToDashboardArgs) (graphqlbackend.InsightsDashboardPayloadResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) RemoveInsightViewFromDashboard(ctx context.Context, args *graphqlbackend.RemoveInsightViewFromDashboardArgs) (graphqlbackend.InsightsDashboardPayloadResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) UpdateInsightSeries(ctx context.Context, args *graphqlbackend.UpdateInsightSeriesArgs) (graphqlbackend.InsightSeriesMetadataPayloadResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) InsightSeriesQueryStatus(ctx context.Context) ([]graphqlbackend.InsightSeriesQueryStatusResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) CreateLineChartSearchInsight(ctx context.Context, args *graphqlbackend.CreateLineChartSearchInsightArgs) (graphqlbackend.InsightViewPayloadResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) UpdateLineChartSearchInsight(ctx context.Context, args *graphqlbackend.UpdateLineChartSearchInsightArgs) (graphqlbackend.InsightViewPayloadResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) CreatePieChartSearchInsight(ctx context.Context, args *graphqlbackend.CreatePieChartSearchInsightArgs) (graphqlbackend.InsightViewPayloadResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) UpdatePieChartSearchInsight(ctx context.Context, args *graphqlbackend.UpdatePieChartSearchInsightArgs) (graphqlbackend.InsightViewPayloadResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) InsightViews(ctx context.Context, args *graphqlbackend.InsightViewQueryArgs) (graphqlbackend.InsightViewConnectionResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) DeleteInsightView(ctx context.Context, args *graphqlbackend.DeleteInsightViewArgs) (*graphqlbackend.EmptyResponse, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) SearchInsightLivePreview(ctx context.Context, args graphqlbackend.SearchInsightLivePreviewArgs) ([]graphqlbackend.SearchInsightLivePreviewSeriesResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) SearchInsightPreview(ctx context.Context, args graphqlbackend.SearchInsightPreviewArgs) ([]graphqlbackend.SearchInsightLivePreviewSeriesResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) RelatedInsightsInline(ctx context.Context, args graphqlbackend.RelatedInsightsArgs) ([]graphqlbackend.RelatedInsightsInlineResolver, error) {
	return nil, errors.New(r.reason)
}

func (r *disabledResolver) RelatedInsightsForFile(ctx context.Context, args graphqlbackend.RelatedInsightsArgs) ([]graphqlbackend.RelatedInsightsForFileResolver, error) {
	return nil, errors.New(r.reason)
}
