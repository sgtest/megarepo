package graphqlbackend

import (
	"context"
	"time"

	graphql "github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/siteid"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/actor"
	"github.com/sourcegraph/sourcegraph/pkg/hubspot/hubspotutil"
)

type surveyResponseResolver struct {
	surveyResponse *types.SurveyResponse
}

func (s *surveyResponseResolver) ID() graphql.ID {
	return marshalSurveyResponseID(s.surveyResponse.ID)
}
func marshalSurveyResponseID(id int32) graphql.ID { return relay.MarshalID("SurveyResponse", id) }

func (s *surveyResponseResolver) User(ctx context.Context) (*UserResolver, error) {
	if s.surveyResponse.UserID != nil {
		return UserByIDInt32(ctx, *s.surveyResponse.UserID)
	}
	return nil, nil
}

func (s *surveyResponseResolver) Email() *string {
	return s.surveyResponse.Email
}

func (s *surveyResponseResolver) Score() int32 {
	return s.surveyResponse.Score
}

func (s *surveyResponseResolver) Reason() *string {
	return s.surveyResponse.Reason
}

func (s *surveyResponseResolver) Better() *string {
	return s.surveyResponse.Better
}

func (s *surveyResponseResolver) CreatedAt() string {
	return s.surveyResponse.CreatedAt.Format(time.RFC3339)
}

// SurveySubmissionInput contains a satisfaction (NPS) survey response.
type SurveySubmissionInput struct {
	// Emails is an optional, user-provided email address, if there is no
	// currently authenticated user. If there is, this value will not be used.
	Email *string
	// Score is the user's likelihood of recommending Sourcegraph to a friend, from 0-10.
	Score int32
	// Reason is the answer to "What is the most important reason for the score you gave".
	Reason *string
	// Better is the answer to "What can Sourcegraph do to provide a better product"
	Better *string
}

type surveySubmissionForHubSpot struct {
	Email           *string `url:"email"`
	Score           int32   `url:"nps_score"`
	Reason          *string `url:"nps_reason"`
	Better          *string `url:"nps_improvement"`
	IsAuthenticated bool    `url:"user_is_authenticated"`
	SiteID          string  `url:"site_id"`
}

// SubmitSurvey records a new satisfaction (NPS) survey response by the current user.
func (r *schemaResolver) SubmitSurvey(ctx context.Context, args *struct {
	Input *SurveySubmissionInput
}) (*EmptyResponse, error) {
	input := args.Input
	var uid *int32
	email := input.Email

	// If user is authenticated, use their uid and overwrite the optional email field.
	actor := actor.FromContext(ctx)
	if actor.IsAuthenticated() {
		uid = &actor.UID
		e, _, err := db.UserEmails.GetPrimaryEmail(ctx, actor.UID)
		if err != nil {
			return nil, err
		}
		if e != "" {
			email = &e
		}
	}

	_, err := db.SurveyResponses.Create(ctx, uid, email, int(input.Score), input.Reason, input.Better)
	if err != nil {
		return nil, err
	}

	// Submit form to HubSpot
	if err := hubspotutil.Client().SubmitForm(hubspotutil.SurveyFormID, &surveySubmissionForHubSpot{
		Email:           email,
		Score:           args.Input.Score,
		Reason:          args.Input.Reason,
		Better:          args.Input.Better,
		IsAuthenticated: actor.IsAuthenticated(),
		SiteID:          siteid.Get(),
	}); err != nil {
		return nil, err
	}

	return &EmptyResponse{}, nil
}
