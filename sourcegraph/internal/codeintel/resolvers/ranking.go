package resolvers

import (
	"context"

	"github.com/sourcegraph/sourcegraph/internal/gqlutil"
)

type RankingServiceResolver interface {
	RankingSummary(ctx context.Context) (GlobalRankingSummaryResolver, error)
	BumpDerivativeGraphKey(ctx context.Context) (*EmptyResponse, error)
	DeleteRankingProgress(ctx context.Context, args *DeleteRankingProgressArgs) (*EmptyResponse, error)
}

type DeleteRankingProgressArgs struct {
	GraphKey string
}

type GlobalRankingSummaryResolver interface {
	RankingSummary() []RankingSummaryResolver
	NextJobStartsAt() *gqlutil.DateTime
}

type RankingSummaryResolver interface {
	GraphKey() string
	PathMapperProgress() RankingSummaryProgressResolver
	ReferenceMapperProgress() RankingSummaryProgressResolver
	ReducerProgress() RankingSummaryProgressResolver
}

type RankingSummaryProgressResolver interface {
	StartedAt() gqlutil.DateTime
	CompletedAt() *gqlutil.DateTime
	Processed() int32
	Total() int32
}
