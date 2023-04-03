package repo

import (
	"context"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/lib/errors"

	"github.com/grafana/regexp"

	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings"
	repoembeddingsbg "github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings/background/repo"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings/embed"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings/split"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/uploadstore"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
)

type handler struct {
	db              edb.EnterpriseDB
	uploadStore     uploadstore.Store
	gitserverClient gitserver.Client
}

var _ workerutil.Handler[*repoembeddingsbg.RepoEmbeddingJob] = &handler{}

var matchEverythingRegexp = regexp.MustCompile(``)

const MAX_FILE_SIZE = 1000000 // 1MB

// The threshold to embed the entire file is slightly larger than the chunk threshold to
// avoid splitting small files unnecessarily.
const (
	embedEntireFileTokensThreshold          = 384
	embeddingChunkTokensThreshold           = 256
	embeddingChunkEarlySplitTokensThreshold = embeddingChunkTokensThreshold - 32
)

var splitOptions = split.SplitOptions{
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

	files, err := h.gitserverClient.ListFiles(ctx, nil, repo.Name, record.Revision, matchEverythingRegexp)
	if err != nil {
		return err
	}

	validFiles := []string{}
	for _, file := range files {
		stat, err := h.gitserverClient.Stat(ctx, nil, repo.Name, record.Revision, file)
		if err != nil {
			return err
		}

		if !stat.IsDir() && stat.Size() <= MAX_FILE_SIZE {
			validFiles = append(validFiles, file)
		}
	}

	embeddingsClient := embed.NewEmbeddingsClient()

	config := conf.Get().Embeddings
	excludedGlobPatterns := embed.GetDefaultExcludedFilePathPatterns()
	excludedGlobPatterns = append(excludedGlobPatterns, embed.CompileGlobPatterns(config.ExcludedFilePathPatterns)...)

	repoEmbeddingIndex, err := embed.EmbedRepo(
		ctx,
		repo.Name,
		record.Revision,
		validFiles,
		excludedGlobPatterns,
		embeddingsClient,
		splitOptions,
		func(fileName string) ([]byte, error) {
			return h.gitserverClient.ReadFile(ctx, nil, repo.Name, record.Revision, fileName)
		},
		getDocumentRanks,
	)
	if err != nil {
		return err
	}

	return embeddings.UploadRepoEmbeddingIndex(ctx, h.uploadStore, string(embeddings.GetRepoEmbeddingIndexName(repo.Name)), repoEmbeddingIndex)
}
