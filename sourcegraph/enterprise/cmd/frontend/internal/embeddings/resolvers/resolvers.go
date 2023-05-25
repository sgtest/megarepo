package resolvers

import (
	"bytes"
	"context"
	"os"

	"github.com/graph-gophers/graphql-go"
	"github.com/sourcegraph/conc/pool"
	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/lib/errors"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings"
	contextdetectionbg "github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings/background/contextdetection"
	repobg "github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings/background/repo"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/auth"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/cody"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
)

func NewResolver(
	db database.DB,
	logger log.Logger,
	gitserverClient gitserver.Client,
	embeddingsClient embeddings.Client,
	repoStore repobg.RepoEmbeddingJobsStore,
	contextDetectionStore contextdetectionbg.ContextDetectionEmbeddingJobsStore,
) graphqlbackend.EmbeddingsResolver {
	return &Resolver{
		db:                        db,
		logger:                    logger,
		gitserverClient:           gitserverClient,
		embeddingsClient:          embeddingsClient,
		repoEmbeddingJobsStore:    repoStore,
		contextDetectionJobsStore: contextDetectionStore,
	}
}

type Resolver struct {
	db                        database.DB
	logger                    log.Logger
	gitserverClient           gitserver.Client
	embeddingsClient          embeddings.Client
	repoEmbeddingJobsStore    repobg.RepoEmbeddingJobsStore
	contextDetectionJobsStore contextdetectionbg.ContextDetectionEmbeddingJobsStore
	emails                    backend.UserEmailsService
}

func (r *Resolver) EmbeddingsSearch(ctx context.Context, args graphqlbackend.EmbeddingsSearchInputArgs) (graphqlbackend.EmbeddingsSearchResultsResolver, error) {
	return r.EmbeddingsMultiSearch(ctx, graphqlbackend.EmbeddingsMultiSearchInputArgs{
		Repos:            []graphql.ID{args.Repo},
		Query:            args.Query,
		CodeResultsCount: args.CodeResultsCount,
		TextResultsCount: args.TextResultsCount,
	})
}

func (r *Resolver) EmbeddingsMultiSearch(ctx context.Context, args graphqlbackend.EmbeddingsMultiSearchInputArgs) (graphqlbackend.EmbeddingsSearchResultsResolver, error) {
	if !conf.EmbeddingsEnabled() {
		return nil, errors.New("embeddings are not configured or disabled")
	}

	if isEnabled := cody.IsCodyEnabled(ctx); !isEnabled {
		return nil, errors.New("cody experimental feature flag is not enabled for current user")
	}

	if err := cody.CheckVerifiedEmailRequirement(ctx, r.db, r.logger); err != nil {
		return nil, err
	}

	repoIDs := make([]api.RepoID, len(args.Repos))
	for i, repo := range args.Repos {
		repoID, err := graphqlbackend.UnmarshalRepositoryID(repo)
		if err != nil {
			return nil, err
		}
		repoIDs[i] = repoID
	}

	repos, err := r.db.Repos().GetByIDs(ctx, repoIDs...)
	if err != nil {
		return nil, err
	}

	repoNames := make([]api.RepoName, len(repos))
	for i, repo := range repos {
		repoNames[i] = repo.Name
	}

	results, err := r.embeddingsClient.Search(ctx, embeddings.EmbeddingsSearchParameters{
		RepoNames:        repoNames,
		RepoIDs:          repoIDs,
		Query:            args.Query,
		CodeResultsCount: int(args.CodeResultsCount),
		TextResultsCount: int(args.TextResultsCount),
	})
	if err != nil {
		return nil, err
	}

	return &embeddingsSearchResultsResolver{
		results:   results,
		gitserver: r.gitserverClient,
		logger:    r.logger,
	}, nil
}

func (r *Resolver) IsContextRequiredForChatQuery(ctx context.Context, args graphqlbackend.IsContextRequiredForChatQueryInputArgs) (bool, error) {
	if !conf.EmbeddingsEnabled() {
		return false, errors.New("embeddings are not configured or disabled")
	}
	if isEnabled := cody.IsCodyEnabled(ctx); !isEnabled {
		return false, errors.New("cody experimental feature flag is not enabled for current user")
	}

	if err := cody.CheckVerifiedEmailRequirement(ctx, r.db, r.logger); err != nil {
		return false, err
	}

	return r.embeddingsClient.IsContextRequiredForChatQuery(ctx, embeddings.IsContextRequiredForChatQueryParameters{Query: args.Query})
}

func (r *Resolver) RepoEmbeddingJobs(ctx context.Context, args graphqlbackend.ListRepoEmbeddingJobsArgs) (*graphqlutil.ConnectionResolver[graphqlbackend.RepoEmbeddingJobResolver], error) {
	if !conf.EmbeddingsEnabled() {
		return nil, errors.New("embeddings are not configured or disabled")
	}
	// 🚨 SECURITY: Only site admins may list repo embedding jobs.
	if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}
	return NewRepoEmbeddingJobsResolver(r.db, r.gitserverClient, r.repoEmbeddingJobsStore, args)
}

func (r *Resolver) ScheduleRepositoriesForEmbedding(ctx context.Context, args graphqlbackend.ScheduleRepositoriesForEmbeddingArgs) (_ *graphqlbackend.EmptyResponse, err error) {
	if !conf.EmbeddingsEnabled() {
		return nil, errors.New("embeddings are not configured or disabled")
	}

	// 🚨 SECURITY: Only site admins may schedule embedding jobs.
	if err = auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}

	var repoNames []api.RepoName
	for _, repo := range args.RepoNames {
		repoNames = append(repoNames, api.RepoName(repo))
	}

	err = embeddings.ScheduleRepositoriesForEmbedding(
		ctx,
		repoNames,
		r.db,
		r.repoEmbeddingJobsStore,
		r.gitserverClient,
	)
	if err != nil {
		return nil, err
	}

	return &graphqlbackend.EmptyResponse{}, nil
}

func (r *Resolver) ScheduleContextDetectionForEmbedding(ctx context.Context) (*graphqlbackend.EmptyResponse, error) {
	if !conf.EmbeddingsEnabled() {
		return nil, errors.New("embeddings are not configured or disabled")
	}

	// 🚨 SECURITY: Only site admins may schedule embedding jobs.
	if err := auth.CheckCurrentUserIsSiteAdmin(ctx, r.db); err != nil {
		return nil, err
	}
	_, err := r.contextDetectionJobsStore.CreateContextDetectionEmbeddingJob(ctx)
	if err != nil {
		return nil, err
	}
	return &graphqlbackend.EmptyResponse{}, nil
}

type embeddingsSearchResultsResolver struct {
	results   *embeddings.EmbeddingCombinedSearchResults
	gitserver gitserver.Client
	logger    log.Logger
}

func (r *embeddingsSearchResultsResolver) CodeResults(ctx context.Context) ([]graphqlbackend.EmbeddingsSearchResultResolver, error) {
	return embeddingsSearchResultsToResolvers(ctx, r.logger, r.gitserver, r.results.CodeResults)
}

func (r *embeddingsSearchResultsResolver) TextResults(ctx context.Context) ([]graphqlbackend.EmbeddingsSearchResultResolver, error) {
	return embeddingsSearchResultsToResolvers(ctx, r.logger, r.gitserver, r.results.TextResults)
}

func embeddingsSearchResultsToResolvers(
	ctx context.Context,
	logger log.Logger,
	gs gitserver.Client,
	results []embeddings.EmbeddingSearchResult,
) ([]graphqlbackend.EmbeddingsSearchResultResolver, error) {

	allContents := make([][]byte, len(results))
	allErrors := make([]error, len(results))
	{ // Fetch contents in parallel because fetching them serially can be slow.
		p := pool.New().WithMaxGoroutines(8)
		for i, result := range results {
			i, result := i, result
			p.Go(func() {
				content, err := gs.ReadFile(ctx, authz.DefaultSubRepoPermsChecker, result.RepoName, result.Revision, result.FileName)
				allContents[i] = content
				allErrors[i] = err
			})
		}
		p.Wait()
	}

	resolvers := make([]graphqlbackend.EmbeddingsSearchResultResolver, 0, len(results))
	{ // Merge the results with their contents, skipping any that errored when fetching the context.
		for i, result := range results {
			contents := allContents[i]
			err := allErrors[i]
			if err != nil {
				if !os.IsNotExist(err) {
					logger.Error(
						"error reading file",
						log.String("repoName", string(result.RepoName)),
						log.String("revision", string(result.Revision)),
						log.String("fileName", result.FileName),
						log.Error(err),
					)
				}
				continue
			}

			resolvers = append(resolvers, &embeddingsSearchResultResolver{
				result:  result,
				content: string(extractLineRange(contents, result.StartLine, result.EndLine)),
			})
		}
	}

	return resolvers, nil
}

func extractLineRange(content []byte, startLine, endLine int) []byte {
	lines := bytes.Split(content, []byte("\n"))

	// Sanity check: check that startLine and endLine are within 0 and len(lines).
	startLine = clamp(startLine, 0, len(lines))
	endLine = clamp(endLine, 0, len(lines))

	return bytes.Join(lines[startLine:endLine], []byte("\n"))
}

func clamp(input, min, max int) int {
	if input > max {
		return max
	} else if input < min {
		return min
	}
	return input
}

type embeddingsSearchResultResolver struct {
	result  embeddings.EmbeddingSearchResult
	content string
}

func (r *embeddingsSearchResultResolver) RepoName(ctx context.Context) string {
	return string(r.result.RepoName)
}

func (r *embeddingsSearchResultResolver) Revision(ctx context.Context) string {
	return string(r.result.Revision)
}

func (r *embeddingsSearchResultResolver) FileName(ctx context.Context) string {
	return r.result.FileName
}

func (r *embeddingsSearchResultResolver) StartLine(ctx context.Context) int32 {
	return int32(r.result.StartLine)
}

func (r *embeddingsSearchResultResolver) EndLine(ctx context.Context) int32 {
	return int32(r.result.EndLine)
}

func (r *embeddingsSearchResultResolver) Content(ctx context.Context) string {
	return r.content
}
