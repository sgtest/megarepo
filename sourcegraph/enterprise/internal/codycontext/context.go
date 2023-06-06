package context

import (
	"context"
	"fmt"
	"math"
	"regexp"
	"strconv"
	"strings"
	"sync"

	"github.com/sourcegraph/conc/pool"
	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings/embed"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/client"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/internal/settings"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

type FileChunkContext struct {
	RepoName  api.RepoName
	RepoID    api.RepoID
	CommitID  api.CommitID
	Path      string
	StartLine int
	EndLine   int
}

func NewCodyContextClient(logger log.Logger, db edb.EnterpriseDB, embeddingsClient embeddings.Client, searchClient client.SearchClient) *CodyContextClient {
	return &CodyContextClient{
		logger:           logger,
		db:               db,
		embeddingsClient: embeddingsClient,
		searchClient:     searchClient,
	}
}

type CodyContextClient struct {
	logger           log.Logger
	db               edb.EnterpriseDB
	embeddingsClient embeddings.Client
	searchClient     client.SearchClient
}

type GetContextArgs struct {
	Repos            []types.RepoIDName
	Query            string
	CodeResultsCount int32
	TextResultsCount int32
}

func (c *CodyContextClient) GetCodyContext(ctx context.Context, args GetContextArgs) ([]FileChunkContext, error) {
	embeddingRepos, keywordRepos, err := c.partitionRepos(ctx, args.Repos)
	if err != nil {
		return nil, err
	}

	// NOTE: We use a pretty simple heuristic for combining results from
	// embeddings and keyword search. We use the ratio of repos with embeddings
	// to decide how many results out of our limit should be reserved for
	// embeddings results. We can't easily compare the scores between embeddings
	// and keyword search.
	embeddingsResultRatio := float32(len(embeddingRepos)) / float32(len(args.Repos))

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

	// Fetch keyword results and embeddings results concurrently
	p := pool.NewWithResults[[]FileChunkContext]().WithErrors()
	p.Go(func() ([]FileChunkContext, error) {
		return c.getEmbeddingsContext(ctx, embeddingsArgs)
	})
	p.Go(func() ([]FileChunkContext, error) {
		return c.getKeywordContext(ctx, keywordArgs)
	})

	results, err := p.Wait()
	if err != nil {
		return nil, err
	}

	return append(results[0], results[1]...), nil
}

// partitionRepos splits a set of repos into repos with embeddings and repos without embeddings
func (c *CodyContextClient) partitionRepos(ctx context.Context, input []types.RepoIDName) (embedded, notEmbedded []types.RepoIDName, err error) {
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

func (c *CodyContextClient) getEmbeddingsContext(ctx context.Context, args GetContextArgs) (_ []FileChunkContext, err error) {
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
			EndLine:   result.EndLine,
		})
	}
	return res, nil
}

var textFileFilter = func() string {
	var extensions []string
	for extension := range embed.TextFileExtensions {
		extensions = append(extensions, extension)
	}
	return `file:(` + strings.Join(extensions, "|") + `)$`
}()

// getKeywordContext uses keyword search to find relevant bits of context for Cody
func (c *CodyContextClient) getKeywordContext(ctx context.Context, args GetContextArgs) (_ []FileChunkContext, err error) {
	if len(args.Repos) == 0 {
		// TODO(camdencheek): for some reason the search query `repo:^$`
		// returns all repos, not zero repos, causing searches over zero repos
		// to break in unexpected ways.
		return nil, nil
	}

	settings, err := settings.CurrentUserFinal(ctx, c.db)
	if err != nil {
		return nil, err
	}

	// mini-HACK: pass in the scope using repo: filters. In an ideal world, we
	// would not be using query text manipulation for this and would be using
	// the job structs directly.
	regexEscapedRepoNames := make([]string, len(args.Repos))
	for i, repo := range args.Repos {
		regexEscapedRepoNames[i] = regexp.QuoteMeta(string(repo.Name))
	}

	textQuery := fmt.Sprintf(`repo:^%s$ %s content:%s`, query.UnionRegExps(regexEscapedRepoNames), textFileFilter, strconv.Quote(args.Query))
	codeQuery := fmt.Sprintf(`repo:^%s$ -%s content:%s`, query.UnionRegExps(regexEscapedRepoNames), textFileFilter, strconv.Quote(args.Query))

	doSearch := func(ctx context.Context, query string, limit int) ([]FileChunkContext, error) {
		if limit == 0 {
			// Skip a search entirely if the limit is zero.
			return nil, nil
		}

		ctx, cancel := context.WithCancel(ctx)
		defer cancel()

		patternTypeKeyword := "keyword"
		plan, err := c.searchClient.Plan(
			ctx,
			"V3",
			&patternTypeKeyword,
			query,
			search.Precise,
			search.Streaming,
			settings,
			envvar.SourcegraphDotComMode(),
		)
		if err != nil {
			return nil, err
		}

		var (
			mu        sync.Mutex
			collected []FileChunkContext
		)
		stream := streaming.StreamFunc(func(e streaming.SearchEvent) {
			mu.Lock()
			defer mu.Unlock()

			for _, res := range e.Results {
				if fm, ok := res.(*result.FileMatch); ok {
					collected = append(collected, fileMatchToContextMatches(fm)...)
					if len(collected) >= limit {
						cancel()
						return
					}
				}
			}
		})

		alert, err := c.searchClient.Execute(ctx, stream, plan)
		if err != nil {
			return nil, err
		}
		if alert != nil {
			c.logger.Warn("received alert from keyword search execution",
				log.String("title", alert.Title),
				log.String("description", alert.Description),
			)
		}

		return collected, nil
	}

	p := pool.NewWithResults[[]FileChunkContext]().WithContext(ctx)
	p.Go(func(ctx context.Context) ([]FileChunkContext, error) {
		return doSearch(ctx, codeQuery, int(args.CodeResultsCount))
	})
	p.Go(func(ctx context.Context) ([]FileChunkContext, error) {
		return doSearch(ctx, textQuery, int(args.TextResultsCount))
	})
	results, err := p.Wait()
	if err != nil {
		return nil, err
	}

	return append(results[0], results[1]...), nil
}

func fileMatchToContextMatches(fm *result.FileMatch) []FileChunkContext {
	if len(fm.ChunkMatches) == 0 {
		return nil
	}

	// To provide some context variety, we just use the top-ranked
	// chunk (the first chunk) from each file

	// 4 lines of leading context, clamped to zero
	startLine := max(0, fm.ChunkMatches[0].ContentStart.Line-4)
	// depend on content fetching to trim to the end of the file
	endLine := startLine + 8

	return []FileChunkContext{{
		RepoName:  fm.Repo.Name,
		RepoID:    fm.Repo.ID,
		CommitID:  fm.CommitID,
		Path:      fm.Path,
		StartLine: startLine,
		EndLine:   endLine,
	}}
}

func max(vals ...int) int {
	res := math.MinInt32
	for _, val := range vals {
		if val > res {
			res = val
		}
	}
	return res
}

func min(vals ...int) int {
	res := math.MaxInt32
	for _, val := range vals {
		if val < res {
			res = val
		}
	}
	return res
}

func truncate[T any](input []T, size int) []T {
	return input[:min(len(input), size)]
}
