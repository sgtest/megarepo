package queryrunnerapi

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"time"

	"github.com/pkg/errors"

	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/env"
)

var (
	queryRunnerURL = env.Get("QUERY_RUNNER_URL", "http://query-runner", "URL at which the query-runner service can be reached")

	Client = &client{
		client: &http.Client{
			Timeout: 5 * time.Second,
		},
	}
)

type SubjectAndConfig struct {
	Subject api.ConfigurationSubject
	Config  api.PartialConfigSavedQueries
}

type ErrorResponse struct {
	Message string
}

const (
	PathSavedQueryWasCreatedOrUpdated = "/saved-query-was-created-or-updated"
	PathSavedQueryWasDeleted          = "/saved-query-was-deleted"
	PathTestNotification              = "/test-notification"
)

type client struct {
	client *http.Client
}

type SavedQueryWasCreatedOrUpdatedArgs struct {
	SubjectAndConfig                 *SubjectAndConfig
	DisableSubscriptionNotifications bool
}

// SavedQueryWasCreated should be called whenever a saved query was created
// or updated after the server has started.
func (c *client) SavedQueryWasCreatedOrUpdated(ctx context.Context, subject api.ConfigurationSubject, config api.PartialConfigSavedQueries, disableSubscriptionNotifications bool) error {
	return c.post(PathSavedQueryWasCreatedOrUpdated, &SavedQueryWasCreatedOrUpdatedArgs{
		SubjectAndConfig: &SubjectAndConfig{
			Subject: subject,
			Config:  config,
		},
		DisableSubscriptionNotifications: disableSubscriptionNotifications,
	})
}

type SavedQueryWasDeletedArgs struct {
	Spec                             api.SavedQueryIDSpec
	DisableSubscriptionNotifications bool
}

// SavedQueryWasDeleted should be called whenever a saved query was deleted
// after the server has started.
func (c *client) SavedQueryWasDeleted(ctx context.Context, spec api.SavedQueryIDSpec, disableSubscriptionNotifications bool) error {
	return c.post(PathSavedQueryWasDeleted, &SavedQueryWasDeletedArgs{
		Spec:                             spec,
		DisableSubscriptionNotifications: disableSubscriptionNotifications,
	})
}

type TestNotificationArgs struct {
	Spec api.SavedQueryIDSpec
}

// TestNotification is called to send a test notification for a saved search. Users may perform this
// action to test that the configured notifications are working.
func (c *client) TestNotification(ctx context.Context, spec api.SavedQueryIDSpec) error {
	return c.post(PathTestNotification, &TestNotificationArgs{Spec: spec})
}

func (c *client) post(path string, data interface{}) error {
	var buf bytes.Buffer
	if err := json.NewEncoder(&buf).Encode(data); err != nil {
		return errors.Wrap(err, "Encoding request")
	}
	u, err := url.Parse(queryRunnerURL)
	if err != nil {
		return errors.Wrap(err, "Parse QUERY_RUNNER_URL")
	}
	u.Path = path
	resp, err := c.client.Post(u.String(), "application/json", &buf)
	if err != nil {
		return errors.Wrap(err, "Post "+u.String())
	}
	if resp.StatusCode == http.StatusOK {
		return nil
	}
	var errResp *ErrorResponse
	if err := json.NewDecoder(resp.Body).Decode(&errResp); err != nil {
		return errors.Wrap(err, "Decoding response")
	}
	return fmt.Errorf("Error from %s: %s", u.String(), errResp.Message)
}
