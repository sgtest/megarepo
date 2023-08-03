package openai

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
)

func TestOpenAI(t *testing.T) {
	t.Run("errors on empty embedding string", func(t *testing.T) {
		client := NewClient(http.DefaultClient, &conftypes.EmbeddingsConfig{})
		invalidTexts := []string{"a", ""} // empty string is invalid
		_, err := client.GetDocumentEmbeddings(context.Background(), invalidTexts)
		require.ErrorContains(t, err, "empty string")
	})

	t.Run("retry on empty embedding", func(t *testing.T) {
		gotRequest1 := false
		gotRequest2 := false
		s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
			// On the first request, respond with a null embedding
			if !gotRequest1 {
				resp := openaiEmbeddingAPIResponse{
					Data: []openaiEmbeddingAPIResponseData{{
						Index:     0,
						Embedding: append(make([]float32, 1535), 1),
					}, {
						Index:     1,
						Embedding: nil,
					}},
				}
				json.NewEncoder(w).Encode(resp)
				gotRequest1 = true
				return
			}

			// The client should try that embedding once more. This time, actually return a value.
			if !gotRequest2 {
				resp := openaiEmbeddingAPIResponse{
					Data: []openaiEmbeddingAPIResponseData{{
						Index:     0,
						Embedding: append(make([]float32, 1535), 2),
					}},
				}
				json.NewEncoder(w).Encode(resp)
				gotRequest2 = true
				return
			}

			panic("only expected 2 responses")
		}))
		defer s.Close()

		httpClient := s.Client()

		// HACK: override the URL to always go to the test server
		oldTransport := httpClient.Transport
		httpClient.Transport = roundTripFunc(func(r *http.Request) (*http.Response, error) {
			r.URL, _ = url.Parse(s.URL)
			return oldTransport.RoundTrip(r)
		})

		client := NewClient(s.Client(), &conftypes.EmbeddingsConfig{})
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
		require.True(t, gotRequest1)
		require.True(t, gotRequest2)
	})

	t.Run("retry on empty embedding fails and returns failed indices no error", func(t *testing.T) {
		gotRequest1 := false
		s := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
			// On the first request, respond with a null embedding
			if !gotRequest1 {
				resp := openaiEmbeddingAPIResponse{
					Data: []openaiEmbeddingAPIResponseData{{
						Index:     0,
						Embedding: append(make([]float32, 1535), 1),
					}, {
						Index:     1,
						Embedding: nil,
					}, {
						Index:     2,
						Embedding: append(make([]float32, 1535), 2),
					}},
				}
				json.NewEncoder(w).Encode(resp)
				gotRequest1 = true
				return
			}

			// Always return an invalid response to all the retry requests
			resp := openaiEmbeddingAPIResponse{
				Data: []openaiEmbeddingAPIResponseData{{
					Index:     0,
					Embedding: nil,
				}},
			}
			json.NewEncoder(w).Encode(resp)
		}))
		defer s.Close()

		httpClient := s.Client()
		oldTransport := httpClient.Transport
		httpClient.Transport = roundTripFunc(func(r *http.Request) (*http.Response, error) {
			r.URL, _ = url.Parse(s.URL)
			return oldTransport.RoundTrip(r)
		})

		client := NewClient(s.Client(), &conftypes.EmbeddingsConfig{})
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
		require.True(t, gotRequest1)
	})
}

type roundTripFunc func(r *http.Request) (*http.Response, error)

func (f roundTripFunc) RoundTrip(r *http.Request) (*http.Response, error) { return f(r) }
