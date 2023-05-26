package graphqlbackend

import (
	"context"

	"github.com/graph-gophers/graphql-go"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/internal/gqlutil"
)

type EmbeddingsResolver interface {
	EmbeddingsSearch(ctx context.Context, args EmbeddingsSearchInputArgs) (EmbeddingsSearchResultsResolver, error)
	EmbeddingsMultiSearch(ctx context.Context, args EmbeddingsMultiSearchInputArgs) (EmbeddingsSearchResultsResolver, error)
	IsContextRequiredForChatQuery(ctx context.Context, args IsContextRequiredForChatQueryInputArgs) (bool, error)
	RepoEmbeddingJobs(ctx context.Context, args ListRepoEmbeddingJobsArgs) (*graphqlutil.ConnectionResolver[RepoEmbeddingJobResolver], error)

	ScheduleRepositoriesForEmbedding(ctx context.Context, args ScheduleRepositoriesForEmbeddingArgs) (*EmptyResponse, error)
	ScheduleContextDetectionForEmbedding(ctx context.Context) (*EmptyResponse, error)
	CancelRepoEmbeddingJob(ctx context.Context, args CancelRepoEmbeddingJobArgs) (*EmptyResponse, error)
}

type ScheduleRepositoriesForEmbeddingArgs struct {
	RepoNames []string
}

type IsContextRequiredForChatQueryInputArgs struct {
	Query string
}

type EmbeddingsSearchInputArgs struct {
	Repo             graphql.ID
	Query            string
	CodeResultsCount int32
	TextResultsCount int32
}

type EmbeddingsMultiSearchInputArgs struct {
	Repos            []graphql.ID
	Query            string
	CodeResultsCount int32
	TextResultsCount int32
}

type EmbeddingsSearchResultsResolver interface {
	CodeResults(ctx context.Context) ([]EmbeddingsSearchResultResolver, error)
	TextResults(ctx context.Context) ([]EmbeddingsSearchResultResolver, error)
}

type EmbeddingsSearchResultResolver interface {
	RepoName(ctx context.Context) string
	Revision(ctx context.Context) string
	FileName(ctx context.Context) string
	StartLine(ctx context.Context) int32
	EndLine(ctx context.Context) int32
	Content(ctx context.Context) string
}

type ListRepoEmbeddingJobsArgs struct {
	graphqlutil.ConnectionResolverArgs
}

type CancelRepoEmbeddingJobArgs struct {
	Job graphql.ID
}

type RepoEmbeddingJobResolver interface {
	ID() graphql.ID
	State() string
	FailureMessage() *string
	QueuedAt() gqlutil.DateTime
	StartedAt() *gqlutil.DateTime
	FinishedAt() *gqlutil.DateTime
	ProcessAfter() *gqlutil.DateTime
	NumResets() int32
	NumFailures() int32
	LastHeartbeatAt() *gqlutil.DateTime
	WorkerHostname() string
	Cancel() bool
	Repo(ctx context.Context) (*RepositoryResolver, error)
	Revision(ctx context.Context) (*GitCommitResolver, error)
}
