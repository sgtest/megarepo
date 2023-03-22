package shared

import (
	lru "github.com/hashicorp/golang-lru/v2"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/embeddings/embed"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const QUERY_EMBEDDING_RETRIES = 3
const QUERY_EMBEDDINGS_CACHE_MAX_ENTRIES = 128

func getCachedQueryEmbeddingFn(client embed.EmbeddingsClient) (getQueryEmbeddingFn, error) {
	cache, err := lru.New[string, []float32](QUERY_EMBEDDINGS_CACHE_MAX_ENTRIES)
	if err != nil {
		return nil, errors.Wrap(err, "creating query embeddings cache")
	}

	return func(query string) (queryEmbedding []float32, err error) {
		if cachedQueryEmbedding, ok := cache.Get(query); ok {
			queryEmbedding = cachedQueryEmbedding
		} else {
			queryEmbedding, err = client.GetEmbeddingsWithRetries([]string{query}, QUERY_EMBEDDING_RETRIES)
			if err != nil {
				return nil, err
			}
			cache.Add(query, queryEmbedding)
		}
		return queryEmbedding, err
	}, nil
}
