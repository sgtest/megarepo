package gitlab

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"path"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/inconshreveable/log15"
	"github.com/pkg/errors"
	"golang.org/x/time/rate"

	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/auth"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/ratelimit"
	"github.com/sourcegraph/sourcegraph/internal/rcache"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
)

var (
	requestCounter = metrics.NewRequestMeter("gitlab", "Total number of requests sent to the GitLab API.")

	// Whether debug logging is turned on
	traceEnabled int32 = 0
)

func init() {
	go func() {
		conf.Watch(func() {
			exp := conf.Get().ExperimentalFeatures
			if exp == nil {
				atomic.StoreInt32(&traceEnabled, 0)
				return
			}
			if debugLog := exp.DebugLog; debugLog == nil || !debugLog.ExtsvcGitlab {
				atomic.StoreInt32(&traceEnabled, 0)
				return
			}
			atomic.StoreInt32(&traceEnabled, 1)
		})
	}()
}

func trace(msg string, ctx ...interface{}) {
	if atomic.LoadInt32(&traceEnabled) == 1 {
		log15.Info(fmt.Sprintf("TRACE %s", msg), ctx...)
	}
}

// ClientProvider creates GitLab API clients. Each client has separate authentication creds and a
// separate cache, but they share an underlying HTTP client and rate limiter. Callers who want a simple
// unauthenticated API client should use `NewClientProvider(baseURL, transport).GetClient()`.
type ClientProvider struct {
	// baseURL is the base URL of GitLab; e.g., https://gitlab.com or https://gitlab.example.com
	baseURL *url.URL

	// httpClient is the underlying the HTTP client to use
	httpClient httpcli.Doer

	gitlabClients   map[string]*Client
	gitlabClientsMu sync.Mutex
}

type CommonOp struct {
	// NoCache, if true, will bypass any caching done in this package
	NoCache bool
}

func NewClientProvider(baseURL *url.URL, cli httpcli.Doer) *ClientProvider {
	if cli == nil {
		cli = httpcli.ExternalDoer()
	}
	cli = requestCounter.Doer(cli, func(u *url.URL) string {
		// The 3rd component of the Path (/api/v4/XYZ) mostly maps to the type of API
		// request we are making.
		var category string
		if parts := strings.SplitN(u.Path, "/", 3); len(parts) >= 4 {
			category = parts[3]
		}
		return category
	})

	return &ClientProvider{
		baseURL:       baseURL.ResolveReference(&url.URL{Path: path.Join(baseURL.Path, "api/v4") + "/"}),
		httpClient:    cli,
		gitlabClients: make(map[string]*Client),
	}
}

// GetAuthenticatorClient returns a client authenticated by the given
// authenticator.
func (p *ClientProvider) GetAuthenticatorClient(a auth.Authenticator) *Client {
	return p.getClient(a)
}

// GetPATClient returns a client authenticated by the personal access token.
func (p *ClientProvider) GetPATClient(personalAccessToken, sudo string) *Client {
	if personalAccessToken == "" {
		return p.getClient(nil)
	}
	return p.getClient(&SudoableToken{Token: personalAccessToken, Sudo: sudo})
}

// GetOAuthClient returns a client authenticated by the OAuth token.
func (p *ClientProvider) GetOAuthClient(oauthToken string) *Client {
	if oauthToken == "" {
		return p.getClient(nil)
	}
	return p.getClient(&auth.OAuthBearerToken{Token: oauthToken})
}

// GetClient returns an unauthenticated client.
func (p *ClientProvider) GetClient() *Client {
	return p.getClient(nil)
}

func (p *ClientProvider) getClient(a auth.Authenticator) *Client {
	p.gitlabClientsMu.Lock()
	defer p.gitlabClientsMu.Unlock()

	key := "<nil>"
	if a != nil {
		key = a.Hash()
	}
	if c, ok := p.gitlabClients[key]; ok {
		return c
	}

	c := p.newClient(p.baseURL, a, p.httpClient)
	p.gitlabClients[key] = c
	return c
}

// Client is a GitLab API client. Clients are associated with a particular user
// identity, which is defined by the Auth implementation. In addition to the
// generic types provided by the auth package, Client also supports
// SudoableToken: if this is used and its Sudo field is non-empty, then the user
// identity will be the user ID specified by Sudo (rather than the user that
// owns the token).
//
// The Client's cache is keyed by Auth.Hash(). It is NOT keyed by the actual
// user ID that is defined by the authentication method. So if an OAuth token
// and personal access token belong to the same user and there are two
// corresponding Client instances, those Client instances will NOT share the
// same cache. However, two Client instances sharing the exact same values for
// those fields WILL share a cache.
type Client struct {
	baseURL          *url.URL
	httpClient       httpcli.Doer
	projCache        *rcache.Cache
	Auth             auth.Authenticator
	rateLimitMonitor *ratelimit.Monitor
	rateLimiter      *rate.Limiter // Our internal rate limiter
}

// newClient creates a new GitLab API client with an optional personal access token to authenticate requests.
//
// The URL must point to the base URL of the GitLab instance. This is https://gitlab.com for GitLab.com and
// http[s]://[gitlab-hostname] for self-hosted GitLab instances.
//
// See the docstring of Client for the meaning of the parameters.
func (p *ClientProvider) newClient(baseURL *url.URL, a auth.Authenticator, httpClient httpcli.Doer) *Client {
	// Cache for GitLab project metadata.
	var cacheTTL time.Duration
	if isGitLabDotComURL(baseURL) && a == nil {
		cacheTTL = 10 * time.Minute // cache for longer when unauthenticated
	} else {
		cacheTTL = 30 * time.Second
	}
	key := "gl_proj:"
	var tokenHash string
	if a != nil {
		tokenHash = a.Hash()
		key += tokenHash
	}
	projCache := rcache.NewWithTTL(key, int(cacheTTL/time.Second))

	rl := ratelimit.DefaultRegistry.Get(baseURL.String())
	rlm := ratelimit.DefaultMonitorRegistry.GetOrSet(baseURL.String(), tokenHash, &ratelimit.Monitor{})

	return &Client{
		baseURL:          baseURL,
		httpClient:       httpClient,
		projCache:        projCache,
		Auth:             a,
		rateLimiter:      rl,
		rateLimitMonitor: rlm,
	}
}

func isGitLabDotComURL(baseURL *url.URL) bool {
	hostname := strings.ToLower(baseURL.Hostname())
	return hostname == "gitlab.com" || hostname == "www.gitlab.com"
}

func (c *Client) do(ctx context.Context, req *http.Request, result interface{}) (responseHeader http.Header, responseCode int, err error) {
	req.URL = c.baseURL.ResolveReference(req.URL)
	req.Header.Set("Content-Type", "application/json; charset=utf-8")
	if c.Auth != nil {
		if err := c.Auth.Authenticate(req); err != nil {
			return nil, 0, errors.Wrap(err, "authenticating request")
		}
	}
	var resp *http.Response

	span, ctx := ot.StartSpanFromContext(ctx, "GitLab")
	span.SetTag("URL", req.URL.String())
	defer func() {
		if err != nil {
			span.SetTag("error", err.Error())
		}
		if resp != nil {
			span.SetTag("status", resp.Status)
		}
		span.Finish()
	}()

	if c.rateLimiter != nil {
		err = c.rateLimiter.Wait(ctx)
		if err != nil {
			return nil, 0, errors.Wrap(err, "rate limit")
		}
	}

	resp, err = c.httpClient.Do(req.WithContext(ctx))
	if err != nil {
		trace("GitLab API error", "method", req.Method, "url", req.URL.String(), "err", err)
		return nil, 0, err
	}
	defer resp.Body.Close()
	trace("GitLab API", "method", req.Method, "url", req.URL.String(), "respCode", resp.StatusCode)

	c.rateLimitMonitor.Update(resp.Header)
	if resp.StatusCode < 200 || resp.StatusCode >= 400 {
		return nil, resp.StatusCode, errors.Wrap(HTTPError(resp.StatusCode), fmt.Sprintf("unexpected response from GitLab API (%s)", req.URL))
	}

	return resp.Header, resp.StatusCode, json.NewDecoder(resp.Body).Decode(result)
}

// RateLimitMonitor exposes the rate limit monitor.
func (c *Client) RateLimitMonitor() *ratelimit.Monitor {
	return c.rateLimitMonitor
}

func (c *Client) WithAuthenticator(a auth.Authenticator) *Client {
	tokenHash := a.Hash()

	cc := *c
	cc.rateLimiter = ratelimit.DefaultRegistry.Get(cc.baseURL.String())
	cc.rateLimitMonitor = ratelimit.DefaultMonitorRegistry.GetOrSet(cc.baseURL.String(), tokenHash, &ratelimit.Monitor{})
	cc.Auth = a

	return &cc
}

type HTTPError int

func (err HTTPError) Error() string {
	return fmt.Sprintf("HTTP error status %d", err)
}

// HTTPErrorCode returns err's HTTP status code, if it is an HTTP error from
// this package. Otherwise it returns 0.
func HTTPErrorCode(err error) int {
	e, ok := err.(HTTPError)
	if !ok {
		// Try one level deeper.
		err = errors.Cause(err)
		e, ok = err.(HTTPError)
	}
	if ok {
		return int(e)
	}
	return 0
}

// ErrNotFound is when the requested GitLab project is not found.
var ErrNotFound = errors.New("GitLab project not found")

// IsNotFound reports whether err is a GitLab API error of type NOT_FOUND, the equivalent cached
// response error, or HTTP 404.
func IsNotFound(err error) bool {
	if err == ErrNotFound || errors.Cause(err) == ErrNotFound {
		return true
	}
	if HTTPErrorCode(err) == http.StatusNotFound {
		return true
	}
	return false
}
