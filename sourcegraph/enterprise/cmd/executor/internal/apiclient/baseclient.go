package apiclient

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/url"

	"github.com/inconshreveable/log15"
	"golang.org/x/net/context/ctxhttp"

	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// BaseClient is an abstract HTTP API-backed data access layer. Instances of this
// struct should not be used directly, but should be used compositionally by other
// stores that implement logic specific to a domain.
//
// The following is a minimal example of decorating the base client, making the
// actual logic of the decorated client extremely lean:
//
//     type SprocketClient struct {
//         *httpcli.BaseClient
//
//         baseURL *url.URL
//     }
//
//     func (c *SprocketClient) Fabricate(ctx context.Context(), spec SprocketSpec) (Sprocket, error) {
//         url := c.baseURL.ResolveReference(&url.URL{Path: "/new"})
//
//         req, err := httpcli.MakeJSONRequest("POST", url.String(), spec)
//         if err != nil {
//             return Sprocket{}, err
//         }
//
//         var s Sprocket
//         err := c.client.DoAndDecode(ctx, req, &s)
//         return s, err
//     }
type BaseClient struct {
	httpClient *http.Client
	options    BaseClientOptions
}

type BaseClientOptions struct {
	// UserAgent specifies the user agent string to supply on requests.
	UserAgent string
}

// NewBaseClient creates a new BaseClient with the given transport.
func NewBaseClient(options BaseClientOptions) *BaseClient {
	return &BaseClient{
		httpClient: httpcli.InternalClient,
		options:    options,
	}
}

// Do performs the given HTTP request and returns the body. If there is no content
// to be read due to a 204 response, then a false-valued flag is returned.
func (c *BaseClient) Do(ctx context.Context, req *http.Request) (hasContent bool, _ io.ReadCloser, err error) {
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("User-Agent", c.options.UserAgent)
	req = req.WithContext(ctx)

	resp, err := ctxhttp.Do(req.Context(), c.httpClient, req)
	if err != nil {
		return false, nil, err
	}

	if resp.StatusCode != http.StatusOK {
		defer resp.Body.Close()

		if resp.StatusCode == http.StatusNoContent {
			return false, nil, nil
		}

		if content, err := io.ReadAll(resp.Body); err != nil {
			log15.Error("Failed to read response body", "error", err)
		} else {
			log15.Error("apiclient got unexpected status code", "code", resp.StatusCode, "body", string(content))
		}

		return false, nil, errors.Errorf("unexpected status code %d", resp.StatusCode)
	}

	return true, resp.Body, nil
}

// DoAndDecode performs the given HTTP request and unmarshals the response body into the
// given interface pointer. If the response body was empty due to a 204 response, then a
// false-valued flag is returned.
func (c *BaseClient) DoAndDecode(ctx context.Context, req *http.Request, payload any) (decoded bool, _ error) {
	hasContent, body, err := c.Do(ctx, req)
	if err == nil && hasContent {
		defer body.Close()
		return true, json.NewDecoder(body).Decode(&payload)
	}

	return false, err
}

// DoAndDrop performs the given HTTP request and ignores the response body.
func (c *BaseClient) DoAndDrop(ctx context.Context, req *http.Request) error {
	hasContent, body, err := c.Do(ctx, req)
	if hasContent {
		defer body.Close()
	}

	return err
}

// MakeJSONRequest creates an HTTP request with the given payload serialized as JSON. This
// will also ensure that the proper content type header (which is necessary, not pedantic).
func MakeJSONRequest(method string, url *url.URL, payload any) (*http.Request, error) {
	contents, err := json.Marshal(payload)
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequest(method, url.String(), bytes.NewReader(contents))
	if err != nil {
		return nil, err
	}

	req.Header.Set("Content-Type", "application/json")
	return req, nil
}
