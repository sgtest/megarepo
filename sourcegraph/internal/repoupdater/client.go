package repoupdater

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"

	"github.com/opentracing-contrib/go-stdlib/nethttp"
	"github.com/opentracing/opentracing-go/ext"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf/deploy"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/repoupdater/protocol"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// DefaultClient is the default Client. Unless overwritten, it is
// connected to the server specified by the REPO_UPDATER_URL
// environment variable.
var DefaultClient = NewClient(repoUpdaterURLDefault())

var defaultDoer, _ = httpcli.NewInternalClientFactory("repoupdater").Doer()

func repoUpdaterURLDefault() string {
	if u := os.Getenv("REPO_UPDATER_URL"); u != "" {
		return u
	}

	if deploy.IsDeployTypeSingleProgram(deploy.Type()) {
		return "http://127.0.0.1:3182"
	}

	return "http://repo-updater:3182"
}

// Client is a repoupdater client.
type Client struct {
	// URL to repoupdater server.
	URL string

	// HTTP client to use
	HTTPClient httpcli.Doer
}

// NewClient will initiate a new repoupdater Client with the given serverURL.
func NewClient(serverURL string) *Client {
	return &Client{
		URL:        serverURL,
		HTTPClient: defaultDoer,
	}
}

// RepoUpdateSchedulerInfo returns information about the state of the repo in the update scheduler.
func (c *Client) RepoUpdateSchedulerInfo(
	ctx context.Context,
	args protocol.RepoUpdateSchedulerInfoArgs,
) (result *protocol.RepoUpdateSchedulerInfoResult, err error) {
	resp, err := c.httpPost(ctx, "repo-update-scheduler-info", args)
	if err != nil {
		return nil, err
	}

	if resp.StatusCode != http.StatusOK {
		stack := fmt.Sprintf("RepoScheduleInfo: %+v", args)
		return nil, errors.Wrap(errors.Errorf("http status %d", resp.StatusCode), stack)
	}
	defer resp.Body.Close()
	err = json.NewDecoder(resp.Body).Decode(&result)
	return result, err
}

// MockRepoLookup mocks (*Client).RepoLookup for tests.
var MockRepoLookup func(protocol.RepoLookupArgs) (*protocol.RepoLookupResult, error)

// RepoLookup retrieves information about the repository on repoupdater.
func (c *Client) RepoLookup(
	ctx context.Context,
	args protocol.RepoLookupArgs,
) (result *protocol.RepoLookupResult, err error) {
	if MockRepoLookup != nil {
		return MockRepoLookup(args)
	}

	span, ctx := ot.StartSpanFromContext(ctx, "Client.RepoLookup") //nolint:staticcheck // OT is deprecated
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
	if args.Repo != "" {
		span.SetTag("Repo", string(args.Repo))
	}

	resp, err := c.httpPost(ctx, "repo-lookup", args)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		// best-effort inclusion of body in error message
		body, _ := io.ReadAll(io.LimitReader(resp.Body, 200))
		return nil, errors.Errorf(
			"RepoLookup for %+v failed with http status %d: %s",
			args,
			resp.StatusCode,
			string(body),
		)
	}

	err = json.NewDecoder(resp.Body).Decode(&result)
	if err == nil && result != nil {
		switch {
		case result.ErrorNotFound:
			err = &ErrNotFound{
				Repo:       args.Repo,
				IsNotFound: true,
			}
		case result.ErrorUnauthorized:
			err = &ErrUnauthorized{
				Repo:    args.Repo,
				NoAuthz: true,
			}
		case result.ErrorTemporarilyUnavailable:
			err = &ErrTemporary{
				Repo:        args.Repo,
				IsTemporary: true,
			}
		}
	}
	return result, err
}

// MockEnqueueRepoUpdate mocks (*Client).EnqueueRepoUpdate for tests.
var MockEnqueueRepoUpdate func(ctx context.Context, repo api.RepoName) (*protocol.RepoUpdateResponse, error)

// EnqueueRepoUpdate requests that the named repository be updated in the near
// future. It does not wait for the update.
func (c *Client) EnqueueRepoUpdate(ctx context.Context, repo api.RepoName) (*protocol.RepoUpdateResponse, error) {
	if MockEnqueueRepoUpdate != nil {
		return MockEnqueueRepoUpdate(ctx, repo)
	}

	req := &protocol.RepoUpdateRequest{
		Repo: repo,
	}

	resp, err := c.httpPost(ctx, "enqueue-repo-update", req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	bs, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, errors.Wrap(err, "failed to read response body")
	}

	var res protocol.RepoUpdateResponse
	if resp.StatusCode == http.StatusNotFound {
		return nil, &repoNotFoundError{string(repo), string(bs)}
	} else if resp.StatusCode < 200 || resp.StatusCode >= 400 {
		return nil, errors.New(string(bs))
	} else if err = json.Unmarshal(bs, &res); err != nil {
		return nil, err
	}

	return &res, nil
}

type repoNotFoundError struct {
	repo         string
	responseBody string
}

func (repoNotFoundError) NotFound() bool { return true }
func (e *repoNotFoundError) Error() string {
	return fmt.Sprintf("repo %v not found with response: %v", e.repo, e.responseBody)
}

// MockEnqueueChangesetSync mocks (*Client).EnqueueChangesetSync for tests.
var MockEnqueueChangesetSync func(ctx context.Context, ids []int64) error

func (c *Client) EnqueueChangesetSync(ctx context.Context, ids []int64) error {
	if MockEnqueueChangesetSync != nil {
		return MockEnqueueChangesetSync(ctx, ids)
	}

	req := protocol.ChangesetSyncRequest{IDs: ids}
	resp, err := c.httpPost(ctx, "enqueue-changeset-sync", req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	bs, err := io.ReadAll(resp.Body)
	if err != nil {
		return errors.Wrap(err, "failed to read response body")
	}

	var res protocol.ChangesetSyncResponse
	if resp.StatusCode < 200 || resp.StatusCode >= 400 {
		return errors.New(string(bs))
	} else if err = json.Unmarshal(bs, &res); err != nil {
		return err
	}

	if res.Error == "" {
		return nil
	}
	return errors.New(res.Error)
}

// MockSchedulePermsSync mocks (*Client).SchedulePermsSync for tests.
var MockSchedulePermsSync func(ctx context.Context, args protocol.PermsSyncRequest) error

func (c *Client) SchedulePermsSync(ctx context.Context, args protocol.PermsSyncRequest) error {
	if MockSchedulePermsSync != nil {
		return MockSchedulePermsSync(ctx, args)
	}

	resp, err := c.httpPost(ctx, "schedule-perms-sync", args)
	if err != nil {
		return err
	}
	defer func() { _ = resp.Body.Close() }()

	bs, err := io.ReadAll(resp.Body)
	if err != nil {
		return errors.Wrap(err, "read response body")
	}

	var res protocol.PermsSyncResponse
	if resp.StatusCode < 200 || resp.StatusCode >= 400 {
		return errors.New(string(bs))
	} else if err = json.Unmarshal(bs, &res); err != nil {
		return err
	}

	if res.Error == "" {
		return nil
	}
	return errors.New(res.Error)
}

// MockSyncExternalService mocks (*Client).SyncExternalService for tests.
var MockSyncExternalService func(ctx context.Context, externalServiceID int64) (*protocol.ExternalServiceSyncResult, error)

// SyncExternalService requests the given external service to be synced.
func (c *Client) SyncExternalService(ctx context.Context, externalServiceID int64) (*protocol.ExternalServiceSyncResult, error) {
	if MockSyncExternalService != nil {
		return MockSyncExternalService(ctx, externalServiceID)
	}
	req := &protocol.ExternalServiceSyncRequest{ExternalServiceID: externalServiceID}
	resp, err := c.httpPost(ctx, "sync-external-service", req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	bs, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, errors.Wrap(err, "failed to read response body")
	}

	var result protocol.ExternalServiceSyncResult
	if resp.StatusCode < 200 || resp.StatusCode >= 400 {
		return nil, errors.New(string(bs))
	} else if len(bs) == 0 {
		return &result, nil
	} else if err = json.Unmarshal(bs, &result); err != nil {
		return nil, err
	}

	if result.Error != "" {
		return nil, errors.New(result.Error)
	}
	return &result, nil
}

// MockExternalServiceNamespaces mocks (*Client).QueryExternalServiceNamespaces for tests.
var MockExternalServiceNamespaces func(ctx context.Context, args protocol.ExternalServiceNamespacesArgs) (*protocol.ExternalServiceNamespacesResult, error)

// ExternalServiceNamespaces retrieves a list of namespaces available to the given external service configuration
func (c *Client) ExternalServiceNamespaces(ctx context.Context, args protocol.ExternalServiceNamespacesArgs) (result *protocol.ExternalServiceNamespacesResult, err error) {
	if MockExternalServiceNamespaces != nil {
		return MockExternalServiceNamespaces(ctx, args)
	}

	resp, err := c.httpPost(ctx, "external-service-namespaces", args)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	err = json.NewDecoder(resp.Body).Decode(&result)
	if err == nil && result != nil && result.Error != "" {
		err = errors.New(result.Error)
	}
	return result, err
}

// MockExternalServiceRepositories mocks (*Client).ExternalServiceRepositories for tests.
var MockExternalServiceRepositories func(ctx context.Context, args protocol.ExternalServiceRepositoriesArgs) (*protocol.ExternalServiceRepositoriesResult, error)

// ExternalServiceRepositories retrieves a list of repositories sourced by the given external service configuration
func (c *Client) ExternalServiceRepositories(ctx context.Context, args protocol.ExternalServiceRepositoriesArgs) (result *protocol.ExternalServiceRepositoriesResult, err error) {
	if MockExternalServiceRepositories != nil {
		return MockExternalServiceRepositories(ctx, args)
	}

	resp, err := c.httpPost(ctx, "external-service-repositories", args)

	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	err = json.NewDecoder(resp.Body).Decode(&result)
	if err == nil && result != nil && result.Error != "" {
		err = errors.New(result.Error)
	}
	return result, err
}

func (c *Client) httpPost(ctx context.Context, method string, payload any) (resp *http.Response, err error) {
	reqBody, err := json.Marshal(payload)
	if err != nil {
		return nil, err
	}

	req, err := http.NewRequest("POST", c.URL+"/"+method, bytes.NewReader(reqBody))
	if err != nil {
		return nil, err
	}

	return c.do(ctx, req)
}

func (c *Client) do(ctx context.Context, req *http.Request) (_ *http.Response, err error) {
	span, ctx := ot.StartSpanFromContext(ctx, "Client.do") //nolint:staticcheck // OT is deprecated
	defer func() {
		if err != nil {
			ext.Error.Set(span, true)
			span.SetTag("err", err.Error())
		}
		span.Finish()
	}()

	req.Header.Set("Content-Type", "application/json")

	req = req.WithContext(ctx)
	req, ht := nethttp.TraceRequest(span.Tracer(), req,
		nethttp.OperationName("RepoUpdater Client"),
		nethttp.ClientTrace(false))
	defer ht.Finish()

	if c.HTTPClient != nil {
		return c.HTTPClient.Do(req)
	}
	return http.DefaultClient.Do(req)
}
