package repo

import (
	"context"

	"github.com/sourcegraph/log"

	codeintelContext "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/context"
	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings"
	repoembeddingsbg "github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings/background/repo"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings/embed"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/uploadstore"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type handler struct {
	db              edb.EnterpriseDB
	uploadStore     uploadstore.Store
	gitserverClient gitserver.Client
	contextService  embed.ContextService
}

var _ workerutil.Handler[*repoembeddingsbg.RepoEmbeddingJob] = &handler{}

// The threshold to embed the entire file is slightly larger than the chunk threshold to
// avoid splitting small files unnecessarily.
const (
	embedEntireFileTokensThreshold          = 384
	embeddingChunkTokensThreshold           = 256
	embeddingChunkEarlySplitTokensThreshold = embeddingChunkTokensThreshold - 32

	defaultMaxCodeEmbeddingsPerRepo = 3_072_000
	defaultMaxTextEmbeddingsPerRepo = 512_000
)

var splitOptions = codeintelContext.SplitOptions{
	NoSplitTokensThreshold:         embedEntireFileTokensThreshold,
	ChunkTokensThreshold:           embeddingChunkTokensThreshold,
	ChunkEarlySplitTokensThreshold: embeddingChunkEarlySplitTokensThreshold,
}

func (h *handler) Handle(ctx context.Context, logger log.Logger, record *repoembeddingsbg.RepoEmbeddingJob) error {
	if !conf.EmbeddingsEnabled() {
		return errors.New("embeddings are not configured or disabled")
	}

	repo, err := h.db.Repos().Get(ctx, record.RepoID)
	if err != nil {
		return err
	}

	embeddingsClient := embed.NewEmbeddingsClient()
	fetcher := &revisionFetcher{
		repo:      repo.Name,
		revision:  record.Revision,
		gitserver: h.gitserverClient,
	}

	config := conf.Get().Embeddings
	excludedGlobPatterns := embed.GetDefaultExcludedFilePathPatterns()
	excludedGlobPatterns = append(excludedGlobPatterns, embed.CompileGlobPatterns(config.ExcludedFilePathPatterns)...)

	opts := embed.EmbedRepoOpts{
		RepoName:          repo.Name,
		Revision:          record.Revision,
		ExcludePatterns:   excludedGlobPatterns,
		SplitOptions:      splitOptions,
		MaxCodeEmbeddings: defaultTo(config.MaxCodeEmbeddingsPerRepo, defaultMaxCodeEmbeddingsPerRepo),
		MaxTextEmbeddings: defaultTo(config.MaxTextEmbeddingsPerRepo, defaultMaxTextEmbeddingsPerRepo),
	}

	repoEmbeddingIndex, stats, err := embed.EmbedRepo(
		ctx,
		embeddingsClient,
		h.contextService,
		fetcher,
		getDocumentRanks,
		opts,
	)
	if err != nil {
		return err
	}

	logger.Info(
		"finished generating repo embeddings",
		log.String("repoName", string(repo.Name)),
		log.String("revision", string(record.Revision)),
		log.Object("stats", stats.ToFields()...),
	)

	return embeddings.UploadRepoEmbeddingIndex(ctx, h.uploadStore, string(embeddings.GetRepoEmbeddingIndexName(repo.Name)), repoEmbeddingIndex)
}

func defaultTo(input, def int) int {
	if input == 0 {
		return def
	}
	return input
}

type revisionFetcher struct {
	repo      api.RepoName
	revision  api.CommitID
	gitserver gitserver.Client
}

func (r *revisionFetcher) Read(ctx context.Context, fileName string) ([]byte, error) {
	return r.gitserver.ReadFile(ctx, nil, r.repo, r.revision, fileName)
}

func (r *revisionFetcher) List(ctx context.Context) ([]embed.FileEntry, error) {
	fileInfos, err := r.gitserver.ReadDir(ctx, nil, r.repo, r.revision, "", true)
	if err != nil {
		return nil, err
	}

	entries := make([]embed.FileEntry, 0, len(fileInfos))
	for _, fileInfo := range fileInfos {
		if !fileInfo.IsDir() {
			entries = append(entries, embed.FileEntry{
				Name: fileInfo.Name(),
				Size: fileInfo.Size(),
			})
		}
	}
	return entries, nil
}
