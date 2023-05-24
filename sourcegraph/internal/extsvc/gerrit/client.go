//nolint:bodyclose // Body is closed in Client.Do, but the response is still returned to provide access to the headers
package gerrit

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"

	"github.com/sourcegraph/sourcegraph/internal/extsvc/auth"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/ratelimit"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// Client access a Gerrit via the REST API.
type client struct {
	// HTTP Client used to communicate with the API
	httpClient httpcli.Doer

	// URL is the base URL of Gerrit.
	URL *url.URL

	// RateLimit is the self-imposed rate limiter (since Gerrit does not have a concept
	// of rate limiting in HTTP response headers).
	rateLimit *ratelimit.InstrumentedLimiter

	// Authenticator used to authenticate HTTP requests.
	auther auth.Authenticator
}

type Client interface {
	GetURL() *url.URL
	WithAuthenticator(a auth.Authenticator) (Client, error)
	Authenticator() auth.Authenticator
	GetAuthenticatedUserAccount(ctx context.Context) (*Account, error)
	GetGroup(ctx context.Context, groupName string) (Group, error)
	ListProjects(ctx context.Context, opts ListProjectsArgs) (projects ListProjectsResponse, nextPage bool, err error)
	GetChange(ctx context.Context, changeID string) (*Change, error)
	AbandonChange(ctx context.Context, changeID string) (*Change, error)
	SubmitChange(ctx context.Context, changeID string) (*Change, error)
	RestoreChange(ctx context.Context, changeID string) (*Change, error)
	WriteReviewComment(ctx context.Context, changeID string, comment ChangeReviewComment) error
}

// NewClient returns an authenticated Gerrit API client with
// the provided configuration. If a nil httpClient is provided, httpcli.ExternalDoer
// will be used.
func NewClient(urn string, url *url.URL, creds *AccountCredentials, httpClient httpcli.Doer) (Client, error) {
	if httpClient == nil {
		httpClient = httpcli.ExternalDoer
	}

	auther := &auth.BasicAuth{
		Username: creds.Username,
		Password: creds.Password,
	}

	return &client{
		httpClient: httpClient,
		URL:        url,
		rateLimit:  ratelimit.DefaultRegistry.Get(urn),
		auther:     auther,
	}, nil
}

func (c *client) WithAuthenticator(a auth.Authenticator) (Client, error) {
	switch a.(type) {
	case *auth.BasicAuth, *auth.BasicAuthWithSSH:
		break
	default:
		return nil, errors.Errorf("authenticator type unsupported for Azure DevOps clients: %s", a)
	}

	return &client{
		httpClient: c.httpClient,
		URL:        c.URL,
		rateLimit:  c.rateLimit,
		auther:     a,
	}, nil
}

func (c *client) Authenticator() auth.Authenticator {
	return c.auther
}

func (c *client) GetAuthenticatedUserAccount(ctx context.Context) (*Account, error) {
	req, err := http.NewRequest("GET", "a/accounts/self", nil)
	if err != nil {
		return nil, err
	}

	var account Account
	if _, err = c.do(ctx, req, &account); err != nil {
		if httpErr := (&httpError{}); errors.As(err, &httpErr) {
			if httpErr.Unauthorized() {
				return nil, errors.New("Invalid username or password.")
			}
		}

		return nil, err
	}

	return &account, nil
}

func (c *client) GetGroup(ctx context.Context, groupName string) (Group, error) {

	urlGroup := url.URL{Path: fmt.Sprintf("a/groups/%s", groupName)}

	reqAllAccounts, err := http.NewRequest("GET", urlGroup.String(), nil)

	if err != nil {
		return Group{}, err
	}

	respGetGroup := Group{}
	if _, err = c.do(ctx, reqAllAccounts, &respGetGroup); err != nil {
		return respGetGroup, err
	}
	return respGetGroup, nil
}

func (c *client) do(ctx context.Context, req *http.Request, result any) (*http.Response, error) { //nolint:unparam // http.Response is never used, but it makes sense API wise.
	req.URL = c.URL.ResolveReference(req.URL)

	// Authenticate request with auther
	if c.auther != nil {
		if err := c.auther.Authenticate(req); err != nil {
			return nil, err
		}
	}

	if err := c.rateLimit.Wait(ctx); err != nil {
		return nil, err
	}

	resp, err := c.httpClient.Do(req)

	if err != nil {
		return nil, err
	}

	defer resp.Body.Close()

	bs, err := io.ReadAll(resp.Body)

	if err != nil {
		return nil, err
	}

	if resp.StatusCode < 200 || resp.StatusCode >= 400 {
		return nil, &httpError{
			URL:        req.URL,
			StatusCode: resp.StatusCode,
			Body:       bs,
		}
	}

	// The first 4 characters of the Gerrit API responses need to be stripped, see: https://gerrit-review.googlesource.com/Documentation/rest-api.html#output .
	if len(bs) < 4 {
		return nil, &httpError{
			URL:        req.URL,
			StatusCode: resp.StatusCode,
			Body:       bs,
		}
	}
	if result == nil {
		return resp, nil
	}
	return resp, json.Unmarshal(bs[4:], result)
}

func (c *client) GetURL() *url.URL {
	return c.URL
}
