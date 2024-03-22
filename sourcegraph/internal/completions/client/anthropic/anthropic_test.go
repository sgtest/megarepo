package anthropic

import (
	"bytes"
	"context"
	"io"
	"net/http"
	"testing"

	"github.com/hexops/autogold/v2"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/internal/completions/types"
)

type mockDoer struct {
	do func(*http.Request) (*http.Response, error)
}

func (c *mockDoer) Do(r *http.Request) (*http.Response, error) {
	return c.do(r)
}

func linesToResponse(lines []string, separator string) []byte {
	responseBytes := []byte{}
	for _, line := range lines {
		responseBytes = append(responseBytes, []byte(line)...)
		responseBytes = append(responseBytes, []byte(separator)...)
	}
	return responseBytes
}

func getMockClient(responseBody []byte) types.CompletionsClient {
	return NewClient(&mockDoer{
		func(r *http.Request) (*http.Response, error) {
			return &http.Response{StatusCode: http.StatusOK, Body: io.NopCloser(bytes.NewReader(responseBody))}, nil
		},
	}, "", "", false)
}

func TestValidAnthropicMessagesStream(t *testing.T) {
	var mockAnthropicMessagesResponseLines = []string{
		`event: message_start
		data: {"type": "message_start", "message": {"id": "msg_1nZdL29xx5MUA1yADyHTEsnR8uuvGzszyY", "type": "message", "role": "assistant", "content": [], "model": "claude-3-opus-20240229", "stop_reason": null, "stop_sequence": null, "usage": {"input_tokens": 25, "output_tokens": 1}}}`,
		`event: content_block_start
		data: {"type": "content_block_start", "index":0, "content_block": {"type": "text", "text": ""}}`,
		`event: ping
		data: {"type": "ping"}`,
		`event: content_block_delta
		data: {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "He"}}`,
		`event: content_block_delta
		data: {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "llo"}}`,
		`event: content_block_delta
		data: {"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "!"}}`,
		`event: content_block_stop
		data: {"type": "content_block_stop", "index": 0}`,
		`event: message_delta
		data: {"type": "message_delta", "delta": {"stop_reason": "end_turn", "stop_sequence":null, "usage":{"output_tokens": 15}}}`,
		`event: message_stop
		data: {"type": "message_stop"}`,
	}

	mockClient := getMockClient(linesToResponse(mockAnthropicMessagesResponseLines, "\n\n"))
	events := []types.CompletionResponse{}
	stream := true
	err := mockClient.Stream(context.Background(), types.CompletionsFeatureChat, types.CompletionsVersionLegacy, types.CompletionRequestParameters{
		Stream: &stream,
	}, func(event types.CompletionResponse) error {
		events = append(events, event)
		return nil
	})
	if err != nil {
		t.Fatal(err)
	}
	autogold.ExpectFile(t, events)
}

func TestInvalidAnthropicMessagesStream(t *testing.T) {
	var mockAnthropicInvalidResponseLines = []string{`data:{]`}

	mockClient := getMockClient(linesToResponse(mockAnthropicInvalidResponseLines, "\r\n\r\n"))
	err := mockClient.Stream(context.Background(), types.CompletionsFeatureChat, types.CompletionsVersionLegacy, types.CompletionRequestParameters{}, func(event types.CompletionResponse) error { return nil })
	if err == nil {
		t.Fatal("expected error, got nil")
	}
	assert.Contains(t, err.Error(), "failed to decode event payload")
}

func TestErrStatusNotOK(t *testing.T) {
	mockClient := NewClient(&mockDoer{
		func(r *http.Request) (*http.Response, error) {
			return &http.Response{
				StatusCode: http.StatusTooManyRequests,
				Body:       io.NopCloser(bytes.NewReader([]byte("oh no, please slow down!"))),
			}, nil
		},
	}, "", "", false)

	t.Run("Complete", func(t *testing.T) {
		resp, err := mockClient.Complete(context.Background(), types.CompletionsFeatureChat, types.CompletionsVersionLegacy, types.CompletionRequestParameters{})
		require.Error(t, err)
		assert.Nil(t, resp)

		autogold.Expect("Anthropic: unexpected status code 429: oh no, please slow down!").Equal(t, err.Error())
		_, ok := types.IsErrStatusNotOK(err)
		assert.True(t, ok)
	})

	t.Run("Stream", func(t *testing.T) {
		err := mockClient.Stream(context.Background(), types.CompletionsFeatureChat, types.CompletionsVersionLegacy, types.CompletionRequestParameters{}, func(event types.CompletionResponse) error { return nil })
		require.Error(t, err)

		autogold.Expect("Anthropic: unexpected status code 429: oh no, please slow down!").Equal(t, err.Error())
		_, ok := types.IsErrStatusNotOK(err)
		assert.True(t, ok)
	})
}

func TestCompleteApiToMessages(t *testing.T) {
	var response *http.Request
	mockClient := NewClient(&mockDoer{
		func(r *http.Request) (*http.Response, error) {
			response = r
			return &http.Response{
				StatusCode: http.StatusTooManyRequests,
				Body:       io.NopCloser(bytes.NewReader([]byte("oh no, please slow down!"))),
			}, nil
		},
	}, "", "", false)
	messages := []types.Message{
		{Speaker: "human", Text: "¡Hola!"},
		// /complete prompts can have human messages without an assistant response. These should
		// be ignored.
		{Speaker: "assistant", Text: ""},
		{Speaker: "human", Text: "Servus!"},
		// /complete prompts might end with an empty assistant message
		{Speaker: "assistant"},
	}

	t.Run("Complete", func(t *testing.T) {
		resp, err := mockClient.Complete(context.Background(), types.CompletionsFeatureChat, types.CompletionsVersionLegacy, types.CompletionRequestParameters{Messages: messages})
		require.Error(t, err)
		assert.Nil(t, resp)

		assert.NotNil(t, response)
		body, err := io.ReadAll(response.Body)
		assert.NoError(t, err)

		autogold.Expect(body).Equal(t, []byte(`{"messages":[{"role":"user","content":[{"type":"text","text":"Servus!"}]}],"model":""}`))
	})

	t.Run("Stream", func(t *testing.T) {
		stream := true
		err := mockClient.Stream(context.Background(), types.CompletionsFeatureChat, types.CompletionsVersionLegacy, types.CompletionRequestParameters{Messages: messages, Stream: &stream}, func(event types.CompletionResponse) error { return nil })
		require.Error(t, err)

		assert.NotNil(t, response)
		body, err := io.ReadAll(response.Body)
		assert.NoError(t, err)

		autogold.Expect(body).Equal(t, []byte(`{"messages":[{"role":"user","content":[{"type":"text","text":"Servus!"}]}],"model":"","stream":true}`))
	})
}

func TestPinModel(t *testing.T) {
	t.Run("Claude Instant", func(t *testing.T) {
		assert.Equal(t, pinModel("claude-instant-1"), "claude-instant-1.2")
		assert.Equal(t, pinModel("claude-instant-v1"), "claude-instant-1.2")
	})

	t.Run("Claude 2", func(t *testing.T) {
		assert.Equal(t, pinModel("claude-2"), "claude-2.0")
	})
}
