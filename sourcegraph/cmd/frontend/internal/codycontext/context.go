package codycontext

import (
	"context"
	"fmt"
	"strings"
	"sync"

	"github.com/grafana/regexp"
	"github.com/sourcegraph/conc/pool"
	"github.com/sourcegraph/log"
	"go.opentelemetry.io/otel/attribute"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/cody"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/embeddings"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/client"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/pointers"
)

type FileChunkContext struct {
	RepoName  api.RepoName
	RepoID    api.RepoID
	CommitID  api.CommitID
	Path      string
	StartLine int
}

func NewCodyContextClient(obsCtx *observation.Context, db database.DB, embeddingsClient embeddings.Client, searchClient client.SearchClient, gitserverClient gitserver.Client) *CodyContextClient {
	redMetrics := metrics.NewREDMetrics(
		obsCtx.Registerer,
		"codycontext_client",
		metrics.WithLabels("op"),
	)

	op := func(name string) *observation.Operation {
		return obsCtx.Operation(observation.Op{
			Name:              fmt.Sprintf("codycontext.client.%s", name),
			MetricLabelValues: []string{name},
			Metrics:           redMetrics,
			ErrorFilter: func(err error) observation.ErrorFilterBehaviour {
				return observation.EmitForAllExceptLogs
			},
		})
	}

	return &CodyContextClient{
		db:               db,
		embeddingsClient: embeddingsClient,
		searchClient:     searchClient,
		contentFilter:    newRepoContentFilter(obsCtx.Logger, db, gitserverClient),

		obsCtx:                 obsCtx,
		getCodyContextOp:       op("getCodyContext"),
		getEmbeddingsContextOp: op("getEmbeddingsContext"),
		getKeywordContextOp:    op("getKeywordContext"),
	}
}

type CodyContextClient struct {
	db               database.DB
	embeddingsClient embeddings.Client
	searchClient     client.SearchClient
	contentFilter    repoContentFilter

	obsCtx                 *observation.Context
	getCodyContextOp       *observation.Operation
	getEmbeddingsContextOp *observation.Operation
	getKeywordContextOp    *observation.Operation
}

type GetContextArgs struct {
	Repos            []types.RepoIDName
	Query            string
	CodeResultsCount int32
	TextResultsCount int32
}

func (a *GetContextArgs) RepoIDs() []api.RepoID {
	res := make([]api.RepoID, 0, len(a.Repos))
	for _, repo := range a.Repos {
		res = append(res, repo.ID)
	}
	return res
}

func (a *GetContextArgs) Attrs() []attribute.KeyValue {
	return []attribute.KeyValue{
		attribute.Int("numRepos", len(a.Repos)),
		attribute.String("query", a.Query),
		attribute.Int("codeResultsCount", int(a.CodeResultsCount)),
		attribute.Int("textResultsCount", int(a.TextResultsCount)),
	}
}

func (c *CodyContextClient) GetCodyContext(ctx context.Context, args GetContextArgs) (_ []FileChunkContext, err error) {
	ctx, _, endObservation := c.getCodyContextOp.With(ctx, &err, observation.Args{Attrs: args.Attrs()})
	defer endObservation(1, observation.Args{})

	if isEnabled, reason := cody.IsCodyEnabled(ctx, c.db); !isEnabled {
		return nil, errors.Newf("cody is not enabled: %s", reason)
	}

	if err := cody.CheckVerifiedEmailRequirement(ctx, c.db, c.obsCtx.Logger); err != nil {
		return nil, err
	}

	// Generating the content filter removes any repos where the filter can not
	// be determined
	filterableRepos, contextFilter, err := c.contentFilter.getMatcher(ctx, args.Repos)
	if err != nil {
		return nil, err
	}
	args.Repos = filterableRepos

	embeddingRepos, keywordRepos, err := c.partitionRepos(ctx, args.Repos)
	if err != nil {
		return nil, err
	}

	// NOTE: We use a pretty simple heuristic for combining results from
	// embeddings and keyword search. We use the ratio of repos with embeddings
	// to decide how many results out of our limit should be reserved for
	// embeddings results. We can't easily compare the scores between embeddings
	// and keyword search.
	embeddingsResultRatio := float32(len(embeddingRepos)) / float32(len(filterableRepos))

	embeddingsArgs := GetContextArgs{
		Repos:            embeddingRepos,
		Query:            args.Query,
		CodeResultsCount: int32(float32(args.CodeResultsCount) * embeddingsResultRatio),
		TextResultsCount: int32(float32(args.TextResultsCount) * embeddingsResultRatio),
	}
	keywordArgs := GetContextArgs{
		Repos: keywordRepos,
		Query: args.Query,
		// Assign the remaining result budget to keyword search
		CodeResultsCount: args.CodeResultsCount - embeddingsArgs.CodeResultsCount,
		TextResultsCount: args.TextResultsCount - embeddingsArgs.TextResultsCount,
	}

	var embeddingsResults, keywordResults []FileChunkContext

	// Fetch keyword results and embeddings results concurrently
	p := pool.New().WithErrors()
	p.Go(func() (err error) {
		embeddingsResults, err = c.getEmbeddingsContext(ctx, embeddingsArgs, contextFilter)
		return err
	})
	p.Go(func() (err error) {
		keywordResults, err = c.getKeywordContext(ctx, keywordArgs, contextFilter)
		return err
	})

	err = p.Wait()
	if err != nil {
		return nil, err
	}

	return append(embeddingsResults, keywordResults...), nil
}

// partitionRepos splits a set of repos into repos with embeddings and repos without embeddings
func (c *CodyContextClient) partitionRepos(ctx context.Context, input []types.RepoIDName) (embedded, notEmbedded []types.RepoIDName, err error) {
	// if embeddings are disabled , return all repos in the notEmbedded slice
	if !conf.EmbeddingsEnabled() {
		return nil, input, nil
	}
	for _, repo := range input {
		exists, err := c.db.Repos().RepoEmbeddingExists(ctx, repo.ID)
		if err != nil {
			return nil, nil, err
		}

		if exists {
			embedded = append(embedded, repo)
		} else {
			notEmbedded = append(notEmbedded, repo)
		}
	}
	return embedded, notEmbedded, nil
}

func (c *CodyContextClient) getEmbeddingsContext(ctx context.Context, args GetContextArgs, matcher fileMatcher) (_ []FileChunkContext, err error) {
	ctx, _, endObservation := c.getEmbeddingsContextOp.With(ctx, &err, observation.Args{Attrs: args.Attrs()})
	defer endObservation(1, observation.Args{})

	if len(args.Repos) == 0 || (args.CodeResultsCount == 0 && args.TextResultsCount == 0) {
		// Don't bother doing an API request if we can't actually have any results.
		return nil, nil
	}

	repoNames := make([]api.RepoName, len(args.Repos))
	repoIDs := make([]api.RepoID, len(args.Repos))
	for i, repo := range args.Repos {
		repoNames[i] = repo.Name
		repoIDs[i] = repo.ID
	}

	results, err := c.embeddingsClient.Search(ctx, embeddings.EmbeddingsSearchParameters{
		RepoNames:        repoNames,
		RepoIDs:          repoIDs,
		Query:            args.Query,
		CodeResultsCount: int(args.CodeResultsCount),
		TextResultsCount: int(args.TextResultsCount),
	})
	if err != nil {
		return nil, err
	}

	idsByName := make(map[api.RepoName]api.RepoID)
	for i, repoName := range repoNames {
		idsByName[repoName] = repoIDs[i]
	}

	res := make([]FileChunkContext, 0, len(results.CodeResults)+len(results.TextResults))
	for _, result := range append(results.CodeResults, results.TextResults...) {
		res = append(res, FileChunkContext{
			RepoName:  result.RepoName,
			RepoID:    idsByName[result.RepoName],
			CommitID:  result.Revision,
			Path:      result.FileName,
			StartLine: result.StartLine,
		})
	}

	filtered := make([]FileChunkContext, 0, len(res))
	for _, chunk := range res {
		if !matcher(chunk.RepoID, chunk.Path) {
			filtered = append(filtered, chunk)
		}
	}
	return filtered, nil
}

func getKeywordContextExcludeFilePathsQuery() string {
	var excludeFilePaths = []string{
		"\\.min\\.js$",
		"\\.map$",
		"\\.tsbuildinfo$",
		"(\\/|^)umd\\/",
		"(\\/|^)amd\\/",
		"(\\/|^)cjs\\/",
	}

	filters := []string{}
	for _, filePath := range excludeFilePaths {
		filters = append(filters, fmt.Sprintf("-file:%v", filePath))
	}

	return strings.Join(filters, " ")
}

// getKeywordContext uses keyword search to find relevant bits of context for Cody
func (c *CodyContextClient) getKeywordContext(ctx context.Context, args GetContextArgs, matcher fileMatcher) (_ []FileChunkContext, err error) {
	ctx, _, endObservation := c.getKeywordContextOp.With(ctx, &err, observation.Args{Attrs: args.Attrs()})
	defer endObservation(1, observation.Args{})

	if len(args.Repos) == 0 {
		// TODO(camdencheek): for some reason the search query `repo:^$`
		// returns all repos, not zero repos, causing searches over zero repos
		// to break in unexpected ways.
		return nil, nil
	}

	// mini-HACK: pass in the scope using repo: filters. In an ideal world, we
	// would not be using query text manipulation for this and would be using
	// the job structs directly.
	regexEscapedRepoNames := make([]string, len(args.Repos))
	for i, repo := range args.Repos {
		regexEscapedRepoNames[i] = regexp.QuoteMeta(string(repo.Name))
	}

	keywordQuery := fmt.Sprintf(`repo:^%s$ %s %s`, query.UnionRegExps(regexEscapedRepoNames), getKeywordContextExcludeFilePathsQuery(), args.Query)

	ctx, cancel := context.WithCancel(ctx)
	defer cancel()

	patternType := "codycontext"
	plan, err := c.searchClient.Plan(
		ctx,
		"V3",
		&patternType,
		keywordQuery,
		search.Precise,
		search.Streaming,
		pointers.Ptr(int32(0)),
	)

	if err != nil {
		return nil, err
	}

	addLimitsAndFilter(plan, matcher, args)

	var (
		mu        sync.Mutex
		collected []FileChunkContext
	)

	stream := streaming.StreamFunc(func(e streaming.SearchEvent) {
		mu.Lock()
		defer mu.Unlock()

		for _, res := range e.Results {
			if fm, ok := res.(*result.FileMatch); ok {
				collected = append(collected, fileMatchToContextMatch(fm))
			}
		}
	})

	alert, err := c.searchClient.Execute(ctx, stream, plan)
	if err != nil {
		return nil, err
	}

	if alert != nil {
		c.obsCtx.Logger.Warn("received alert from keyword search execution",
			log.String("title", alert.Title),
			log.String("description", alert.Description),
		)
	}

	return collected, nil
}

func addLimitsAndFilter(plan *search.Inputs, filter fileMatcher, args GetContextArgs) {
	if plan.Features == nil {
		plan.Features = &search.Features{}
	}

	plan.Features.CodyContextCodeCount = int(args.CodeResultsCount)
	plan.Features.CodyContextTextCount = int(args.TextResultsCount)
	plan.Features.CodyFileMatcher = filter
}

func fileMatchToContextMatch(fm *result.FileMatch) FileChunkContext {
	if len(fm.ChunkMatches) == 0 {
		// If this is a filename-only match, return a single chunk at the start of the file
		return FileChunkContext{
			RepoName:  fm.Repo.Name,
			RepoID:    fm.Repo.ID,
			CommitID:  fm.CommitID,
			Path:      fm.Path,
			StartLine: 0,
		}
	}

	// To provide some context variety, we just use the top-ranked
	// chunk (the first chunk) from each file

	// 5 lines of leading context, clamped to zero
	startLine := max(0, fm.ChunkMatches[0].ContentStart.Line-5)

	return FileChunkContext{
		RepoName:  fm.Repo.Name,
		RepoID:    fm.Repo.ID,
		CommitID:  fm.CommitID,
		Path:      fm.Path,
		StartLine: startLine,
	}
}
