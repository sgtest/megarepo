//nolint:bodyclose // Body is closed in Client.Do, but the response is still returned to provide access to the headers
package azuredevops

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"

	"github.com/goware/urlx"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/auth"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/ratelimit"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const (
	azureDevOpsServicesURL  = "https://dev.azure.com/"
	apiVersion              = "7.0"
	continuationTokenHeader = "x-ms-continuationtoken"
)

// Client used to access an AzureDevOps code host via the REST API.
type Client interface {
	WithAuthenticator(a auth.Authenticator) (Client, error)
	Authenticator() auth.Authenticator
	GetURL() *url.URL
	IsAzureDevOpsServices() bool
	AbandonPullRequest(ctx context.Context, args PullRequestCommonArgs) (PullRequest, error)
	CreatePullRequest(ctx context.Context, args OrgProjectRepoArgs, input CreatePullRequestInput) (PullRequest, error)
	GetPullRequest(ctx context.Context, args PullRequestCommonArgs) (PullRequest, error)
	GetPullRequestStatuses(ctx context.Context, args PullRequestCommonArgs) ([]PullRequestBuildStatus, error)
	UpdatePullRequest(ctx context.Context, args PullRequestCommonArgs, input PullRequestUpdateInput) (PullRequest, error)
	CreatePullRequestCommentThread(ctx context.Context, args PullRequestCommonArgs, input PullRequestCommentInput) (PullRequestCommentResponse, error)
	CompletePullRequest(ctx context.Context, args PullRequestCommonArgs, input PullRequestCompleteInput) (PullRequest, error)
	GetRepo(ctx context.Context, args OrgProjectRepoArgs) (Repository, error)
	ListRepositoriesByProjectOrOrg(ctx context.Context, args ListRepositoriesByProjectOrOrgArgs) ([]Repository, error)
	ForkRepository(ctx context.Context, org string, input ForkRepositoryInput) (Repository, error)
	GetRepositoryBranch(ctx context.Context, args OrgProjectRepoArgs, branchName string) (Ref, error)
	GetProject(ctx context.Context, org, project string) (Project, error)
	GetAuthorizedProfile(ctx context.Context) (Profile, error)
}

type client struct {
	// HTTP Client used to communicate with the API.
	httpClient httpcli.Doer

	// URL is the base URL of AzureDevOps.
	URL *url.URL

	// RateLimit is the self-imposed rate limiter (since AzureDevOps does not have a concept
	// of rate limiting in HTTP response headers).
	rateLimit *ratelimit.InstrumentedLimiter
	auth      auth.Authenticator
}

// NewClient returns an authenticated AzureDevOps API client with
// the provided configuration. If a nil httpClient is provided, http.DefaultClient
// will be used.
func NewClient(urn string, url string, auth auth.Authenticator, httpClient httpcli.Doer) (Client, error) {
	u, err := urlx.Parse(url)
	if err != nil {
		return nil, err
	}

	if httpClient == nil {
		httpClient = httpcli.ExternalDoer
	}

	return &client{
		httpClient: httpClient,
		URL:        u,
		rateLimit:  ratelimit.DefaultRegistry.Get(urn),
		auth:       auth,
	}, nil
}

// do performs the specified request, returning any errors and a continuationToken used for pagination (if the API supports it).
//
//nolint:unparam // http.Response is never used, but it makes sense API wise.
func (c *client) do(ctx context.Context, req *http.Request, urlOverride string, result any) (continuationToken string, err error) {
	u := c.URL
	if urlOverride != "" {
		u, err = url.Parse(urlOverride)
		if err != nil {
			return "", err
		}
	}

	queryParams := req.URL.Query()
	queryParams.Set("api-version", apiVersion)
	req.URL.RawQuery = queryParams.Encode()
	req.URL = u.ResolveReference(req.URL)
	if req.Body != nil {
		req.Header.Set("Content-Type", "application/json")
	}

	// Add Basic Auth headers for authenticated requests.
	c.auth.Authenticate(req)

	if err := c.rateLimit.Wait(ctx); err != nil {
		return "", err
	}

	resp, err := c.httpClient.Do(req)
	if err != nil {
		return "", err
	}

	bs, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", err
	}

	if resp.StatusCode < 200 || resp.StatusCode >= 400 {
		return "", &httpError{
			URL:        req.URL,
			StatusCode: resp.StatusCode,
			Body:       bs,
		}
	}

	return resp.Header.Get(continuationTokenHeader), json.Unmarshal(bs, result)
}

// WithAuthenticator returns a new Client that uses the same configuration,
// HTTPClient, and RateLimiter as the current Client, except authenticated with
// the given authenticator instance.
//
// Note that using an unsupported Authenticator implementation may result in
// unexpected behaviour, or (more likely) errors. At present, only BasicAuth is
// supported.
func (c *client) WithAuthenticator(a auth.Authenticator) (Client, error) {
	if _, ok := a.(*auth.BasicAuth); !ok {
		return nil, errors.Errorf("authenticator type unsupported for Azure DevOps clients: %s", a)
	}

	return &client{
		httpClient: c.httpClient,
		URL:        c.URL,
		auth:       a,
		rateLimit:  c.rateLimit,
	}, nil
}

func (c *client) Authenticator() auth.Authenticator {
	return c.auth
}

func (c *client) GetURL() *url.URL {
	return c.URL
}

// IsAzureDevOpsServices returns true if the client is configured to Azure DevOps
// Services (https://dev.azure.com
func (c *client) IsAzureDevOpsServices() bool {
	return c.URL.String() == azureDevOpsServicesURL
}

func (e *httpError) Error() string {
	return fmt.Sprintf("Azure DevOps API HTTP error: code=%d url=%q body=%q", e.StatusCode, e.URL, e.Body)
}

func (e *httpError) Unauthorized() bool {
	return e.StatusCode == http.StatusUnauthorized
}

func (e *httpError) NotFound() bool {
	return e.StatusCode == http.StatusNotFound
}
