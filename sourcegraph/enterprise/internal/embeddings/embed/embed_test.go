package embed

import (
	"context"
	"strings"
	"testing"

	"github.com/sourcegraph/log"
	"github.com/stretchr/testify/require"

	codeintelContext "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/context"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func mockFile(lines ...string) []byte {
	return []byte(strings.Join(lines, "\n"))
}

func defaultSplitter(ctx context.Context, text, fileName string, splitOptions codeintelContext.SplitOptions) ([]codeintelContext.EmbeddableChunk, error) {
	return codeintelContext.SplitIntoEmbeddableChunks(text, fileName, splitOptions), nil
}

func TestEmbedRepo(t *testing.T) {
	ctx := context.Background()
	repoName := api.RepoName("repo/name")
	revision := api.CommitID("deadbeef")
	client := NewMockEmbeddingsClient()
	contextService := NewMockContextService()
	contextService.SplitIntoEmbeddableChunksFunc.SetDefaultHook(defaultSplitter)
	splitOptions := codeintelContext.SplitOptions{ChunkTokensThreshold: 8}
	mockFiles := map[string][]byte{
		// 2 embedding chunks (based on split options above)
		"a.go": mockFile(
			strings.Repeat("a", 32),
			"",
			strings.Repeat("b", 32),
		),
		// 2 embedding chunks
		"b.md": mockFile(
			"# "+strings.Repeat("a", 32),
			"",
			"## "+strings.Repeat("b", 32),
		),
		// 3 embedding chunks
		"c.java": mockFile(
			strings.Repeat("a", 32),
			"",
			strings.Repeat("b", 32),
			"",
			strings.Repeat("c", 32),
		),
		// Should be excluded
		"autogen.py": mockFile(
			"# "+strings.Repeat("a", 32),
			"// Do not edit",
		),
		// Should be excluded
		"lines_too_long.c": mockFile(
			strings.Repeat("a", 2049),
			strings.Repeat("b", 2049),
			strings.Repeat("c", 2049),
		),
		// Should be excluded
		"empty.rb": mockFile(""),
		// Should be excluded (binary file),
		"binary.bin": {0xFF, 0xF, 0xF, 0xF, 0xFF, 0xF, 0xF, 0xA},
	}

	mockRanks := map[string]float64{
		"a.go":             0.1,
		"b.md":             0.2,
		"c.java":           0.3,
		"autogen.py":       0.4,
		"lines_too_long.c": 0.5,
		"empty.rb":         0.6,
		"binary.bin":       0.7,
	}

	mockRepoPathRanks := types.RepoPathRanks{
		MeanRank: 0,
		Paths:    mockRanks,
	}

	reader := funcReader(func(_ context.Context, fileName string) ([]byte, error) {
		content, ok := mockFiles[fileName]
		if !ok {
			return nil, errors.Newf("file %s not found", fileName)
		}
		return content, nil
	})

	newReadLister := func(fileNames ...string) FileReadLister {
		fileEntries := make([]FileEntry, len(fileNames))
		for i, fileName := range fileNames {
			fileEntries[i] = FileEntry{Name: fileName, Size: 350}
		}
		return listReader{
			FileReader: reader,
			FileLister: staticLister(fileEntries),
		}
	}

	excludedGlobPatterns := GetDefaultExcludedFilePathPatterns()

	opts := EmbedRepoOpts{
		RepoName:          repoName,
		Revision:          revision,
		ExcludePatterns:   excludedGlobPatterns,
		SplitOptions:      splitOptions,
		MaxCodeEmbeddings: 100000,
		MaxTextEmbeddings: 100000,
	}

	logger := log.NoOp()

	t.Run("no files", func(t *testing.T) {
		index, _, stats, err := EmbedRepo(ctx, client, contextService, newReadLister(), mockRepoPathRanks, opts, logger)
		require.NoError(t, err)
		require.Len(t, index.CodeIndex.Embeddings, 0)
		require.Len(t, index.TextIndex.Embeddings, 0)

		expectedStats := &embeddings.EmbedRepoStats{
			HasRanks: true,
			CodeIndexStats: embeddings.EmbedFilesStats{
				SkippedByteCounts: map[string]int{},
				SkippedCounts:     map[string]int{},
			},
			TextIndexStats: embeddings.EmbedFilesStats{
				SkippedByteCounts: map[string]int{},
				SkippedCounts:     map[string]int{},
			},
		}
		// ignore durations
		stats.Duration = 0
		stats.CodeIndexStats.Duration = 0
		stats.TextIndexStats.Duration = 0
		require.Equal(t, expectedStats, stats)
	})

	t.Run("code files only", func(t *testing.T) {
		index, _, stats, err := EmbedRepo(ctx, client, contextService, newReadLister("a.go"), mockRepoPathRanks, opts, logger)
		require.NoError(t, err)
		require.Len(t, index.TextIndex.Embeddings, 0)
		require.Len(t, index.CodeIndex.Embeddings, 6)
		require.Len(t, index.CodeIndex.RowMetadata, 2)
		require.Len(t, index.CodeIndex.Ranks, 2)

		expectedStats := &embeddings.EmbedRepoStats{
			HasRanks: true,
			CodeIndexStats: embeddings.EmbedFilesStats{
				EmbeddedFileCount:  1,
				EmbeddedChunkCount: 2,
				EmbeddedBytes:      65,
				SkippedByteCounts:  map[string]int{},
				SkippedCounts:      map[string]int{},
			},
			TextIndexStats: embeddings.EmbedFilesStats{
				SkippedByteCounts: map[string]int{},
				SkippedCounts:     map[string]int{},
			},
		}
		// ignore durations
		stats.Duration = 0
		stats.CodeIndexStats.Duration = 0
		stats.TextIndexStats.Duration = 0
		require.Equal(t, expectedStats, stats)
	})

	t.Run("text files only", func(t *testing.T) {
		index, _, stats, err := EmbedRepo(ctx, client, contextService, newReadLister("b.md"), mockRepoPathRanks, opts, logger)
		require.NoError(t, err)
		require.Len(t, index.CodeIndex.Embeddings, 0)
		require.Len(t, index.TextIndex.Embeddings, 6)
		require.Len(t, index.TextIndex.RowMetadata, 2)
		require.Len(t, index.TextIndex.Ranks, 2)

		expectedStats := &embeddings.EmbedRepoStats{
			HasRanks: true,
			CodeIndexStats: embeddings.EmbedFilesStats{
				SkippedByteCounts: map[string]int{},
				SkippedCounts:     map[string]int{},
			},
			TextIndexStats: embeddings.EmbedFilesStats{
				EmbeddedFileCount:  1,
				EmbeddedChunkCount: 2,
				EmbeddedBytes:      70,
				SkippedByteCounts:  map[string]int{},
				SkippedCounts:      map[string]int{},
			},
		}
		// ignore durations
		stats.Duration = 0
		stats.CodeIndexStats.Duration = 0
		stats.TextIndexStats.Duration = 0
		require.Equal(t, expectedStats, stats)
	})

	t.Run("mixed code and text files", func(t *testing.T) {
		rl := newReadLister("a.go", "b.md", "c.java", "autogen.py", "empty.rb", "lines_too_long.c", "binary.bin")
		index, _, stats, err := EmbedRepo(ctx, client, contextService, rl, mockRepoPathRanks, opts, logger)
		require.NoError(t, err)
		require.Len(t, index.CodeIndex.Embeddings, 15)
		require.Len(t, index.CodeIndex.RowMetadata, 5)
		require.Len(t, index.CodeIndex.Ranks, 5)
		require.Len(t, index.TextIndex.Embeddings, 6)
		require.Len(t, index.TextIndex.RowMetadata, 2)
		require.Len(t, index.TextIndex.Ranks, 2)

		expectedStats := &embeddings.EmbedRepoStats{
			HasRanks: true,
			CodeIndexStats: embeddings.EmbedFilesStats{
				EmbeddedFileCount:  2,
				EmbeddedChunkCount: 5,
				EmbeddedBytes:      163,
				SkippedByteCounts: map[string]int{
					"autogenerated": 49,
					"binary":        8,
					"longLine":      6149,
					"small":         0,
				},
				SkippedCounts: map[string]int{
					"autogenerated": 1,
					"binary":        1,
					"longLine":      1,
					"small":         1,
				},
			},
			TextIndexStats: embeddings.EmbedFilesStats{
				EmbeddedFileCount:  1,
				EmbeddedChunkCount: 2,
				EmbeddedBytes:      70,
				SkippedByteCounts:  map[string]int{},
				SkippedCounts:      map[string]int{},
			},
		}
		// ignore durations
		stats.Duration = 0
		stats.CodeIndexStats.Duration = 0
		stats.TextIndexStats.Duration = 0
		require.Equal(t, expectedStats, stats)
	})

	t.Run("embeddings limited", func(t *testing.T) {
		optsCopy := opts
		optsCopy.MaxCodeEmbeddings = 3
		optsCopy.MaxTextEmbeddings = 1

		rl := newReadLister("a.go", "b.md", "c.java", "autogen.py", "empty.rb", "lines_too_long.c", "binary.bin")
		index, _, _, err := EmbedRepo(ctx, client, contextService, rl, mockRepoPathRanks, optsCopy, logger)
		require.NoError(t, err)

		// a.md has 2 chunks, c.java has 3 chunks
		require.Len(t, index.CodeIndex.Embeddings, index.CodeIndex.ColumnDimension*5)
		// b.md has 2 chunks
		require.Len(t, index.TextIndex.Embeddings, index.CodeIndex.ColumnDimension*2)
	})
}

func NewMockEmbeddingsClient() EmbeddingsClient {
	return &mockEmbeddingsClient{}
}

type mockEmbeddingsClient struct{}

func (c *mockEmbeddingsClient) GetDimensions() (int, error) {
	return 3, nil
}

func (c *mockEmbeddingsClient) GetEmbeddingsWithRetries(_ context.Context, texts []string, maxRetries int) ([]float32, error) {
	dimensions, err := c.GetDimensions()
	if err != nil {
		return nil, err
	}
	return make([]float32, len(texts)*dimensions), nil
}

type funcReader func(ctx context.Context, fileName string) ([]byte, error)

func (f funcReader) Read(ctx context.Context, fileName string) ([]byte, error) {
	return f(ctx, fileName)
}

type staticLister []FileEntry

func (l staticLister) List(_ context.Context) ([]FileEntry, error) {
	return l, nil
}

type listReader struct {
	FileReader
	FileLister
	FileDiffer
}
