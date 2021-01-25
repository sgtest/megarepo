package graphqlbackend

import (
	"context"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/database"
)

type surveyResponseConnectionResolver struct {
	opt database.SurveyResponseListOptions
}

func (r *schemaResolver) SurveyResponses(args *struct {
	graphqlutil.ConnectionArgs
}) *surveyResponseConnectionResolver {
	var opt database.SurveyResponseListOptions
	args.ConnectionArgs.Set(&opt.LimitOffset)
	return &surveyResponseConnectionResolver{opt: opt}
}

func (r *surveyResponseConnectionResolver) Nodes(ctx context.Context) ([]*surveyResponseResolver, error) {
	// 🚨 SECURITY: Survey responses can only be viewed by site admins.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return nil, err
	}

	responses, err := database.GlobalSurveyResponses.GetAll(ctx)
	if err != nil {
		return nil, err
	}

	var surveyResponses []*surveyResponseResolver
	for _, resp := range responses {
		surveyResponses = append(surveyResponses, &surveyResponseResolver{surveyResponse: resp})
	}

	return surveyResponses, nil
}

func (r *surveyResponseConnectionResolver) TotalCount(ctx context.Context) (int32, error) {
	// 🚨 SECURITY: Only site admins can count survey responses.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return 0, err
	}

	count, err := database.GlobalSurveyResponses.Count(ctx)
	return int32(count), err
}

func (r *surveyResponseConnectionResolver) AverageScore(ctx context.Context) (float64, error) {
	// 🚨 SECURITY: Only site admins can see average scores.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return 0, err
	}
	return database.GlobalSurveyResponses.Last30DaysAverageScore(ctx)
}

func (r *surveyResponseConnectionResolver) NetPromoterScore(ctx context.Context) (int32, error) {
	// 🚨 SECURITY: Only site admins can see net promoter scores.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return 0, err
	}
	nps, err := database.GlobalSurveyResponses.Last30DaysNetPromoterScore(ctx)
	return int32(nps), err
}

func (r *surveyResponseConnectionResolver) Last30DaysCount(ctx context.Context) (int32, error) {
	// 🚨 SECURITY: Only site admins can count survey responses.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return 0, err
	}
	count, err := database.GlobalSurveyResponses.Last30DaysCount(ctx)
	return int32(count), err
}
