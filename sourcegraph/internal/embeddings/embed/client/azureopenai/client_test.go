package azureopenai

import (
	"context"
	"testing"

	"github.com/Azure/azure-sdk-for-go/sdk/ai/azopenai"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type mockResponse struct {
	resp azopenai.GetEmbeddingsResponse
	err  error
}

type mockAzureClient struct {
	current   int
	responses []mockResponse
}

func (c *mockAzureClient) GetEmbeddings(ctx context.Context, body azopenai.EmbeddingsOptions, options *azopenai.GetEmbeddingsOptions) (azopenai.GetEmbeddingsResponse, error) {
	if c.current < len(c.responses) {
		i := c.current
		c.current++
		return c.responses[i].resp, c.responses[i].err
	}

	return azopenai.GetEmbeddingsResponse{}, errors.New("no more mock responses")

}

func newMockAPIClient(responses []mockResponse) GetEmbeddingsAPIClientFunc {
	return func(accessToken, endpoint string) (EmbeddingsClient, error) {
		return &mockAzureClient{
			responses: responses,
		}, nil
	}
}

func TestAzureOpenAI(t *testing.T) {
	t.Run("errors on empty embedding string", func(t *testing.T) {
		client, _ := NewClient(
			newMockAPIClient([]mockResponse{{resp: azopenai.GetEmbeddingsResponse{}, err: nil}}),
			&conftypes.EmbeddingsConfig{},
		)
		invalidTexts := []string{"a", ""} // empty string is invalid
		_, err := client.GetDocumentEmbeddings(context.Background(), invalidTexts)
		require.ErrorContains(t, err, "empty string")
	})

	t.Run("retry on empty embedding", func(t *testing.T) {
		client, _ := NewClient(
			newMockAPIClient(
				[]mockResponse{
					{resp: azopenai.GetEmbeddingsResponse{Embeddings: azopenai.Embeddings{Data: []azopenai.EmbeddingItem{
						{Embedding: append(make([]float32, 1535), 1), Index: int32Ptr(0)},
					}}}, err: nil},
					{resp: azopenai.GetEmbeddingsResponse{Embeddings: azopenai.Embeddings{Data: []azopenai.EmbeddingItem{
						{Embedding: nil, Index: int32Ptr(1)},
					}}}, err: nil},
					{resp: azopenai.GetEmbeddingsResponse{Embeddings: azopenai.Embeddings{Data: []azopenai.EmbeddingItem{
						{Embedding: append(make([]float32, 1535), 2), Index: int32Ptr(0)},
					}}}, err: nil},
				}),
			&conftypes.EmbeddingsConfig{Dimensions: 1536},
		)
		resp, err := client.GetDocumentEmbeddings(context.Background(), []string{"a", "b"})
		require.NoError(t, err)
		var expected []float32
		{
			expected = append(expected, make([]float32, 1535)...)
			expected = append(expected, 1)
			expected = append(expected, make([]float32, 1535)...)
			expected = append(expected, 2)
		}
		require.Equal(t, expected, resp.Embeddings)
		require.Empty(t, resp.Failed)
	})

	t.Run("retry on empty embedding fails and returns failed indices no error", func(t *testing.T) {
		client, _ := NewClient(
			newMockAPIClient(
				[]mockResponse{
					// First success
					{resp: azopenai.GetEmbeddingsResponse{Embeddings: azopenai.Embeddings{Data: []azopenai.EmbeddingItem{
						{Embedding: append(make([]float32, 1535), 1), Index: int32Ptr(0)},
					}}}, err: nil},
					// Initial Failure
					{resp: azopenai.GetEmbeddingsResponse{Embeddings: azopenai.Embeddings{Data: []azopenai.EmbeddingItem{
						{Embedding: nil, Index: int32Ptr(1)},
					}}}, err: nil},
					// Retry 1
					{resp: azopenai.GetEmbeddingsResponse{Embeddings: azopenai.Embeddings{Data: []azopenai.EmbeddingItem{
						{Embedding: nil, Index: int32Ptr(1)},
					}}}, err: nil},
					// Retry 2
					{resp: azopenai.GetEmbeddingsResponse{Embeddings: azopenai.Embeddings{Data: []azopenai.EmbeddingItem{
						{Embedding: nil, Index: int32Ptr(1)},
					}}}, err: nil},
					// Final success
					{resp: azopenai.GetEmbeddingsResponse{Embeddings: azopenai.Embeddings{Data: []azopenai.EmbeddingItem{
						{Embedding: append(make([]float32, 1535), 2), Index: int32Ptr(2)},
					}}}, err: nil},
				}),
			&conftypes.EmbeddingsConfig{Dimensions: 1536},
		)
		resp, err := client.GetDocumentEmbeddings(context.Background(), []string{"a", "b", "c"})
		require.NoError(t, err)
		var expected []float32
		{
			expected = append(expected, make([]float32, 1535)...)
			expected = append(expected, 1)

			// zero value embedding when chunk fails to generate embeddings
			expected = append(expected, make([]float32, 1536)...)

			expected = append(expected, make([]float32, 1535)...)
			expected = append(expected, 2)
		}

		failed := []int{1}
		require.Equal(t, expected, resp.Embeddings)
		require.Equal(t, failed, resp.Failed)

	})

}

func int32Ptr(i int32) *int32 { return &i }
