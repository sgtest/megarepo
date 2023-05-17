package repoupdater

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"

	"github.com/opentracing-contrib/go-stdlib/nethttp"
	"github.com/opentracing/opentracing-go/ext"
	"github.com/sourcegraph/log"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf/deploy"
	internalgrpc "github.com/sourcegraph/sourcegraph/internal/grpc"
	"github.com/sourcegraph/sourcegraph/internal/grpc/defaults"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/repoupdater/protocol"
	proto "github.com/sourcegraph/sourcegraph/internal/repoupdater/v1"
	"github.com/sourcegraph/sourcegraph/internal/syncx"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var (
	defaultDoer, _ = httpcli.NewInternalClientFactory("repoupdater").Doer()

	// DefaultClient is the default Client. Unless overwritten, it is
	// connected to the server specified by the REPO_UPDATER_URL
	// environment variable.
	DefaultClient = NewClient(repoUpdaterURLDefault())
)

func repoUpdaterURLDefault() string {
	if u := os.Getenv("REPO_UPDATER_URL"); u != "" {
		return u
	}

	if deploy.IsApp() {
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

	// grpcClient is a function that lazily creates a grpc client.
	// Any implementation should not recreate the client more than once.
	grpcClient func() (proto.RepoUpdaterServiceClient, error)
}

// NewClient will initiate a new repoupdater Client with the given serverURL.
func NewClient(serverURL string) *Client {
	return &Client{
		URL:        serverURL,
		HTTPClient: defaultDoer,
		grpcClient: syncx.OnceValues(func() (proto.RepoUpdaterServiceClient, error) {
			u, err := url.Parse(serverURL)
			if err != nil {
				return nil, err
			}

			l := log.Scoped("repoUpdateGRPCClient", "gRPC client for repo-updater")
			conn, err := defaults.Dial(u.Host, l)
			if err != nil {
				return nil, err
			}

			return proto.NewRepoUpdaterServiceClient(conn), nil
		}),
	}
}

// RepoUpdateSchedulerInfo returns information about the state of the repo in the update scheduler.
func (c *Client) RepoUpdateSchedulerInfo(
	ctx context.Context,
	args protocol.RepoUpdateSchedulerInfoArgs,
) (result *protocol.RepoUpdateSchedulerInfoResult, err error) {
	if internalgrpc.IsGRPCEnabled(ctx) {
		client, err := c.grpcClient()
		if err != nil {
			return nil, err
		}
		req := &proto.RepoUpdateSchedulerInfoRequest{Id: int32(args.ID)}
		resp, err := client.RepoUpdateSchedulerInfo(ctx, req)
		if err != nil {
			return nil, err
		}
		return protocol.RepoUpdateSchedulerInfoResultFromProto(resp), nil
	}

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

	if internalgrpc.IsGRPCEnabled(ctx) {
		client, err := c.grpcClient()
		if err != nil {
			return nil, err
		}
		resp, err := client.RepoLookup(ctx, args.ToProto())
		if err != nil {
			return nil, errors.Wrapf(err, "RepoLookup for %+v failed", args)
		}
		res := protocol.RepoLookupResultFromProto(resp)
		switch {
		case resp.GetErrorNotFound():
			return res, &ErrNotFound{Repo: args.Repo, IsNotFound: true}
		case resp.GetErrorUnauthorized():
			return res, &ErrUnauthorized{Repo: args.Repo, NoAuthz: true}
		case resp.GetErrorTemporarilyUnavailable():
			return res, &ErrTemporary{Repo: args.Repo, IsTemporary: true}
		}
		return res, nil
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

	if internalgrpc.IsGRPCEnabled(ctx) {
		client, err := c.grpcClient()
		if err != nil {
			return nil, err
		}

		req := proto.EnqueueRepoUpdateRequest{Repo: string(repo)}
		resp, err := client.EnqueueRepoUpdate(ctx, &req)
		if err != nil {
			if st := status.Convert(err); st.Code() == codes.NotFound {
				return nil, &repoNotFoundError{repo: string(repo), responseBody: st.Message()}
			}
			return nil, err
		}

		return protocol.RepoUpdateResponseFromProto(resp), nil
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

	if internalgrpc.IsGRPCEnabled(ctx) {
		client, err := c.grpcClient()
		if err != nil {
			return err
		}

		// empty response can be ignored
		_, err = client.EnqueueChangesetSync(ctx, &proto.EnqueueChangesetSyncRequest{Ids: ids})
		return err
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

// MockSyncExternalService mocks (*Client).SyncExternalService for tests.
var MockSyncExternalService func(ctx context.Context, externalServiceID int64) (*protocol.ExternalServiceSyncResult, error)

// SyncExternalService requests the given external service to be synced.
func (c *Client) SyncExternalService(ctx context.Context, externalServiceID int64) (*protocol.ExternalServiceSyncResult, error) {
	if MockSyncExternalService != nil {
		return MockSyncExternalService(ctx, externalServiceID)
	}

	if internalgrpc.IsGRPCEnabled(ctx) {
		client, err := c.grpcClient()
		if err != nil {
			return nil, err
		}

		// empty response can be ignored
		_, err = client.SyncExternalService(ctx, &proto.SyncExternalServiceRequest{ExternalServiceId: externalServiceID})
		return nil, err
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

	if internalgrpc.IsGRPCEnabled(ctx) {
		client, err := c.grpcClient()
		if err != nil {
			return nil, err
		}

		resp, err := client.ExternalServiceNamespaces(ctx, args.ToProto())
		if err != nil {
			return nil, err
		}

		return protocol.ExternalServiceNamespacesResultFromProto(resp), nil
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

	if internalgrpc.IsGRPCEnabled(ctx) {
		client, err := c.grpcClient()
		if err != nil {
			return nil, err
		}

		resp, err := client.ExternalServiceRepositories(ctx, args.ToProto())
		if err != nil {
			return nil, err
		}

		return protocol.ExternalServiceRepositoriesResultFromProto(resp), nil
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
