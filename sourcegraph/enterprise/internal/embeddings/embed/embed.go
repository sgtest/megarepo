package embed

import (
	"context"

	"github.com/sourcegraph/sourcegraph/lib/errors"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings/split"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/binary"
)

const GET_EMBEDDINGS_MAX_RETRIES = 5
const MAX_CODE_EMBEDDING_VECTORS = 768_000
const MAX_TEXT_EMBEDDING_VECTORS = 128_000

const EMBEDDING_BATCHES = 5
const EMBEDDING_BATCH_SIZE = 512

type readFile func(fileName string) ([]byte, error)

// EmbedRepo embeds file contents from the given file names for a repository.
// It separates the file names into code files and text files and embeds them separately.
// It returns a RepoEmbeddingIndex containing the embeddings and metadata.
func EmbedRepo(
	ctx context.Context,
	repoName api.RepoName,
	revision api.CommitID,
	fileNames []string,
	client EmbeddingsClient,
	splitOptions split.SplitOptions,
	readFile readFile,
) (*embeddings.RepoEmbeddingIndex, error) {
	codeFileNames, textFileNames := []string{}, []string{}
	for _, fileName := range fileNames {
		if isValidTextFile(fileName) {
			textFileNames = append(textFileNames, fileName)
		} else if isValidCodeFile(fileName) {
			codeFileNames = append(codeFileNames, fileName)
		}
	}

	codeIndex, err := embedFiles(codeFileNames, client, splitOptions, readFile, MAX_CODE_EMBEDDING_VECTORS)
	if err != nil {
		return nil, err
	}

	textIndex, err := embedFiles(textFileNames, client, splitOptions, readFile, MAX_TEXT_EMBEDDING_VECTORS)
	if err != nil {
		return nil, err
	}

	return &embeddings.RepoEmbeddingIndex{RepoName: repoName, Revision: revision, CodeIndex: codeIndex, TextIndex: textIndex}, nil
}

func createEmptyEmbeddingIndex(columnDimension int) embeddings.EmbeddingIndex[embeddings.RepoEmbeddingRowMetadata] {
	return embeddings.EmbeddingIndex[embeddings.RepoEmbeddingRowMetadata]{
		Embeddings:      []float32{},
		RowMetadata:     []embeddings.RepoEmbeddingRowMetadata{},
		ColumnDimension: columnDimension,
	}
}

// embedFiles embeds file contents from the given file names. Since embedding models can only handle a certain amount of text (tokens) we cannot embed
// entire files. So we split the file contents into chunks and get embeddings for the chunks in batches. Functions returns an EmbeddingIndex containing
// the embeddings and metadata about the chunks the embeddings correspond to.
func embedFiles(
	fileNames []string,
	client EmbeddingsClient,
	splitOptions split.SplitOptions,
	readFile readFile,
	maxEmbeddingVectors int,
) (embeddings.EmbeddingIndex[embeddings.RepoEmbeddingRowMetadata], error) {
	dimensions, err := client.GetDimensions()
	if err != nil {
		return createEmptyEmbeddingIndex(dimensions), err
	}

	if len(fileNames) == 0 {
		return createEmptyEmbeddingIndex(dimensions), nil
	}

	index := embeddings.EmbeddingIndex[embeddings.RepoEmbeddingRowMetadata]{
		Embeddings:      make([]float32, 0, len(fileNames)*dimensions),
		RowMetadata:     make([]embeddings.RepoEmbeddingRowMetadata, 0, len(fileNames)),
		ColumnDimension: dimensions,
	}

	// addEmbeddableChunks batches embeddable chunks, gets embeddings for the batches, and appends them to the index above.
	addEmbeddableChunks := func(embeddableChunks []split.EmbeddableChunk, batchSize int) error {
		// The embeddings API operates with batches up to a certain size, so we can't send all embeddable chunks for embedding at once.
		// We batch them according to `batchSize`, and embed one by one.
		for i := 0; i < len(embeddableChunks); i += batchSize {
			end := min(len(embeddableChunks), i+batchSize)
			batch := embeddableChunks[i:end]
			batchChunks := make([]string, len(batch))
			for idx, chunk := range batch {
				batchChunks[idx] = chunk.Content
				index.RowMetadata = append(index.RowMetadata, embeddings.RepoEmbeddingRowMetadata{FileName: chunk.FileName, StartLine: chunk.StartLine, EndLine: chunk.EndLine})
			}

			batchEmbeddings, err := client.GetEmbeddingsWithRetries(batchChunks, GET_EMBEDDINGS_MAX_RETRIES)
			if err != nil {
				return errors.Wrap(err, "error while getting embeddings")
			}
			index.Embeddings = append(index.Embeddings, batchEmbeddings...)
		}
		return nil
	}

	embeddableChunks := []split.EmbeddableChunk{}
	for _, fileName := range fileNames {
		// This is a fail-safe measure to prevent producing an extremely large index for large repositories.
		if len(index.RowMetadata) > maxEmbeddingVectors {
			break
		}

		contentBytes, err := readFile(fileName)
		if err != nil {
			return createEmptyEmbeddingIndex(dimensions), errors.Wrap(err, "error while reading a file")
		}
		if binary.IsBinary(contentBytes) {
			continue
		}
		content := string(contentBytes)
		if !isEmbeddableFile(fileName, content) {
			continue
		}

		embeddableChunks = append(embeddableChunks, split.SplitIntoEmbeddableChunks(content, fileName, splitOptions)...)

		if len(embeddableChunks) > EMBEDDING_BATCHES*EMBEDDING_BATCH_SIZE {
			err := addEmbeddableChunks(embeddableChunks, EMBEDDING_BATCH_SIZE)
			if err != nil {
				return createEmptyEmbeddingIndex(dimensions), err
			}
			embeddableChunks = []split.EmbeddableChunk{}
		}
	}

	if len(embeddableChunks) > 0 {
		err := addEmbeddableChunks(embeddableChunks, EMBEDDING_BATCH_SIZE)
		if err != nil {
			return createEmptyEmbeddingIndex(dimensions), err
		}
	}

	return index, nil
}
