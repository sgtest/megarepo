package hubspot

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/url"
	"path"
	"strings"
	"time"

	"github.com/google/go-querystring/query"

	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// Client is a HubSpot API client
type Client struct {
	portalID string
	hapiKey  string
}

// New returns a new HubSpot client using the given Portal ID.
func New(portalID, hapiKey string) *Client {
	return &Client{
		portalID: portalID,
		hapiKey:  hapiKey,
	}
}

// Send a POST request with form data to HubSpot APIs that accept
// application/x-www-form-urlencoded data (e.g. the Forms API)
func (c *Client) postForm(methodName string, baseURL *url.URL, suffix string, body any) error {
	var data url.Values
	switch body := body.(type) {
	case map[string]string:
		data = make(url.Values, len(body))
		for k, v := range body {
			data.Set(k, v)
		}
	default:
		var err error
		data, err = query.Values(body)
		if err != nil {
			return wrapError(methodName, err)
		}
	}

	baseURL.Path = path.Join(baseURL.Path, suffix)
	req, err := http.NewRequest("POST", baseURL.String(), strings.NewReader(data.Encode()))
	if err != nil {
		return wrapError(methodName, err)
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")

	resp, err := httpcli.ExternalDoer.Do(req)
	if err != nil {
		return wrapError(methodName, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusNoContent && resp.StatusCode != http.StatusFound {
		buf, err := io.ReadAll(resp.Body)
		if err != nil {
			return wrapError(methodName, err)
		}
		return wrapError(methodName, errors.Errorf("Code %v: %s", resp.StatusCode, string(buf)))
	}

	return nil
}

// Send a POST request with JSON data to HubSpot APIs that accept JSON
// (e.g. the Contacts, Lists, etc APIs)
func (c *Client) postJSON(methodName string, baseURL *url.URL, reqPayload, respPayload any) error {
	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	defer cancel()

	data, err := json.Marshal(reqPayload)
	if err != nil {
		return wrapError(methodName, err)
	}

	req, err := http.NewRequest("POST", baseURL.String(), bytes.NewBuffer(data))
	if err != nil {
		return wrapError(methodName, err)
	}

	req.Header.Set("Content-Type", "application/json")

	resp, err := httpcli.ExternalDoer.Do(req.WithContext(ctx))
	if err != nil {
		return wrapError(methodName, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		buf := new(bytes.Buffer)
		_, _ = buf.ReadFrom(resp.Body)
		return wrapError(methodName, errors.Errorf("Code %v: %s", resp.StatusCode, buf.String()))
	}

	return json.NewDecoder(resp.Body).Decode(respPayload)
}

// Send a GET request to HubSpot APIs that accept JSON in a querystring
// (e.g. the Events API)
func (c *Client) get(methodName string, baseURL *url.URL, suffix string, params map[string]string) error {
	q := make(url.Values, len(params))
	for k, v := range params {
		q.Set(k, v)
	}

	baseURL.Path = path.Join(baseURL.Path, suffix)
	baseURL.RawQuery = q.Encode()
	req, err := http.NewRequest("GET", baseURL.String(), nil)
	if err != nil {
		return wrapError(methodName, err)
	}
	req.Header.Set("Content-Type", "application/json")

	ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
	defer cancel()

	resp, err := httpcli.ExternalDoer.Do(req.WithContext(ctx))
	if err != nil {
		return wrapError(methodName, err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		buf := new(bytes.Buffer)
		_, _ = buf.ReadFrom(resp.Body)
		return wrapError(methodName, errors.Errorf("Code %v: %s", resp.StatusCode, buf.String()))
	}
	return nil
}

func wrapError(methodName string, err error) error {
	if err == nil {
		return nil
	}
	return errors.Errorf("hubspot.%s: %v", methodName, err)
}
