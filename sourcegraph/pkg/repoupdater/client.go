package repoupdater

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"

	"github.com/opentracing-contrib/go-stdlib/nethttp"
	"github.com/opentracing/opentracing-go"
	"github.com/opentracing/opentracing-go/ext"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/env"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver"
	"github.com/sourcegraph/sourcegraph/pkg/repoupdater/protocol"
)

var repoupdaterURL = env.Get("REPO_UPDATER_URL", "http://repo-updater:3182", "repo-updater server URL")

var (
	// ErrNotFound is when a repository is not found.
	ErrNotFound = errors.New("repository not found")

	// ErrUnauthorized is when an authorization error occurred.
	ErrUnauthorized = errors.New("not authorized")

	// ErrTemporarilyUnavailable is when the repository was reported as being temporarily
	// unavailable.
	ErrTemporarilyUnavailable = errors.New("repository temporarily unavailable")
)

// DefaultClient is the default Client. Unless overwritten, it is connected to the server specified by the
// REPO_UPDATER_URL environment variable.
var DefaultClient = &Client{
	URL: repoupdaterURL,
	HTTPClient: &http.Client{
		// nethttp.Transport will propagate opentracing spans
		Transport: &nethttp.Transport{
			RoundTripper: &http.Transport{
				// Default is 2, but we can send many concurrent requests
				MaxIdleConnsPerHost: 500,
			},
		},
	},
}

// Client is a repoupdater client.
type Client struct {
	// URL to repoupdater server.
	URL string

	// HTTP client to use
	HTTPClient *http.Client
}

// MockRepoLookup mocks (*Client).RepoLookup for tests.
var MockRepoLookup func(protocol.RepoLookupArgs) (*protocol.RepoLookupResult, error)

// RepoLookup retrieves information about the repository on repoupdater.
func (c *Client) RepoLookup(ctx context.Context, args protocol.RepoLookupArgs) (result *protocol.RepoLookupResult, err error) {
	if MockRepoLookup != nil {
		return MockRepoLookup(args)
	}

	span, ctx := opentracing.StartSpanFromContext(ctx, "Client.RepoLookup")
	defer func() {
		if result != nil {
			span.SetTag("found", result.Repo != nil)
		}
		if err != nil {
			ext.Error.Set(span, true)
			span.SetTag("err", err.Error())
		}
		span.Finish()
	}()
	if args.ExternalRepo != nil {
		span.SetTag("ExternalRepo.ID", args.ExternalRepo.ID)
		span.SetTag("ExternalRepo.ServiceType", args.ExternalRepo.ServiceType)
		span.SetTag("ExternalRepo.ServiceID", args.ExternalRepo.ServiceID)
	}
	if args.Repo != "" {
		span.SetTag("Repo", string(args.Repo))
	}

	resp, err := c.httpPost(ctx, "repo-lookup", args)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	stack := fmt.Sprintf("RepoLookup: %+v", args)
	if resp.StatusCode != http.StatusOK {
		return nil, errors.Wrap(fmt.Errorf("http status %d", resp.StatusCode), stack)
	}

	err = json.NewDecoder(resp.Body).Decode(&result)
	if err == nil && result != nil {
		switch {
		case result.ErrorNotFound:
			err = ErrNotFound
		case result.ErrorUnauthorized:
			err = ErrUnauthorized
		case result.ErrorTemporarilyUnavailable:
			err = ErrTemporarilyUnavailable
		}
	}
	return result, err
}

// Repo represents a repository on gitserver. It contains the information necessary to identify and
// create/clone it.
type Repo struct {
	Name api.RepoURI // the repository's URI

	// URL is the repository's Git remote URL. If the gitserver already has cloned the repository,
	// this field is optional (it will use the last-used Git remote URL). If the repository is not
	// cloned on the gitserver, the request will fail.
	URL string
}

// MockEnqueueRepoUpdate mocks (*Client).EnqueueRepoUpdate for tests.
var MockEnqueueRepoUpdate func(ctx context.Context, repo gitserver.Repo) error

// EnqueueRepoUpdate requests that the named repository be updated in the near
// future. It does not wait for the update.
func (c *Client) EnqueueRepoUpdate(ctx context.Context, repo gitserver.Repo) error {
	if MockEnqueueRepoUpdate != nil {
		return MockEnqueueRepoUpdate(ctx, repo)
	}

	req := &protocol.RepoUpdateRequest{
		Repo: repo.Name,
		URL:  repo.URL,
	}
	resp, err := c.httpPost(ctx, "enqueue-repo-update", req)
	if err != nil {
		return err
	}
	resp.Body.Close()
	return nil
}

func (c *Client) httpPost(ctx context.Context, method string, payload interface{}) (resp *http.Response, err error) {
	span, ctx := opentracing.StartSpanFromContext(ctx, "Client.httpPost")
	defer func() {
		if err != nil {
			ext.Error.Set(span, true)
			span.SetTag("err", err.Error())
		}
		span.Finish()
	}()

	reqBody, err := json.Marshal(payload)
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequest("POST", c.URL+"/"+method, bytes.NewReader(reqBody))
	if err != nil {
		return nil, err
	}

	req.Header.Set("Content-Type", "application/json")
	req = req.WithContext(ctx)
	req, ht := nethttp.TraceRequest(opentracing.GlobalTracer(), req,
		nethttp.OperationName("RepoUpdater Client"),
		nethttp.ClientTrace(false))
	defer ht.Finish()

	if c.HTTPClient != nil {
		return c.HTTPClient.Do(req)
	}
	return http.DefaultClient.Do(req)
}
