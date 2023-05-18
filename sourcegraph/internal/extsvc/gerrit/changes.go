package gerrit

import (
	"bytes"
	"context"
	"encoding/json"
	"net/http"
	"net/url"

	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func (c *client) GetChange(ctx context.Context, changeID string) (*Change, error) {
	pathStr, err := url.JoinPath("a/changes", url.PathEscape(changeID))
	if err != nil {
		return nil, err
	}
	reqURL := url.URL{Path: pathStr}
	req, err := http.NewRequest("GET", reqURL.String(), nil)
	if err != nil {
		return nil, err
	}

	var change Change
	resp, err := c.do(ctx, req, &change)
	if err != nil {
		return nil, err
	}

	if resp.StatusCode >= http.StatusBadRequest {
		return nil, errors.Errorf("unexpected status code: %d", resp.StatusCode)
	}

	return &change, nil
}

// AbandonChange abandons a Gerrit change.
func (c *client) AbandonChange(ctx context.Context, changeID string) (*Change, error) {
	pathStr, err := url.JoinPath("a/changes", url.PathEscape(changeID), "abandon")
	if err != nil {
		return nil, err
	}
	reqURL := url.URL{Path: pathStr}
	req, err := http.NewRequest("POST", reqURL.String(), nil)
	if err != nil {
		return nil, err
	}

	var change Change
	resp, err := c.do(ctx, req, &change)
	if err != nil {
		return nil, err
	}

	if resp.StatusCode >= http.StatusBadRequest {
		return nil, errors.Errorf("unexpected status code: %d", resp.StatusCode)
	}

	return &change, nil
}

// SubmitChange submits a Gerrit change.
func (c *client) SubmitChange(ctx context.Context, changeID string) (*Change, error) {
	pathStr, err := url.JoinPath("a/changes", url.PathEscape(changeID), "submit")
	if err != nil {
		return nil, err
	}
	reqURL := url.URL{Path: pathStr}
	req, err := http.NewRequest("POST", reqURL.String(), nil)
	if err != nil {
		return nil, err
	}

	var change Change
	resp, err := c.do(ctx, req, &change)
	if err != nil {
		return nil, err
	}

	if resp.StatusCode >= http.StatusBadRequest {
		return nil, errors.Errorf("unexpected status code: %d", resp.StatusCode)
	}

	return &change, nil
}

// WriteReviewComment writes a review comment on a Gerrit change.
func (c *client) WriteReviewComment(ctx context.Context, changeID string, comment ChangeReviewComment) error {
	pathStr, err := url.JoinPath("a/changes", url.PathEscape(changeID), "revisions/current/review")
	if err != nil {
		return err
	}
	reqURL := url.URL{Path: pathStr}
	data, err := json.Marshal(comment)
	if err != nil {
		return err
	}

	req, err := http.NewRequest("POST", reqURL.String(), bytes.NewBuffer(data))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "text/plain; charset=UTF-8")

	resp, err := c.do(ctx, req, nil)
	if err != nil {
		return err
	}

	if resp.StatusCode >= http.StatusBadRequest {
		return errors.Errorf("unexpected status code: %d", resp.StatusCode)
	}

	return nil
}
