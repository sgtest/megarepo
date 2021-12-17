package symbols

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"sync"

	"github.com/cockroachdb/errors"
	"github.com/neelance/parallel"
	"github.com/opentracing-contrib/go-stdlib/nethttp"
	"github.com/opentracing/opentracing-go/ext"
	otlog "github.com/opentracing/opentracing-go/log"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/endpoint"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/search"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
)

var symbolsURL = env.Get("SYMBOLS_URL", "k8s+http://symbols:3184", "symbols service URL")

var defaultDoer = func() httpcli.Doer {
	d, err := httpcli.NewInternalClientFactory("symbols").Doer()
	if err != nil {
		panic(err)
	}
	return d
}()

// DefaultClient is the default Client. Unless overwritten, it is connected to the server specified by the
// SYMBOLS_URL environment variable.
var DefaultClient = &Client{
	URL:         symbolsURL,
	HTTPClient:  defaultDoer,
	HTTPLimiter: parallel.NewRun(500),
}

// Client is a symbols service client.
type Client struct {
	// URL to symbols service.
	URL string

	// HTTP client to use
	HTTPClient httpcli.Doer

	// Limits concurrency of outstanding HTTP posts
	HTTPLimiter *parallel.Run

	once     sync.Once
	endpoint *endpoint.Map
}

func (c *Client) url(repo api.RepoName) (string, error) {
	c.once.Do(func() {
		if len(strings.Fields(c.URL)) == 0 {
			c.endpoint = endpoint.Empty(errors.New("a symbols service has not been configured"))
		} else {
			c.endpoint = endpoint.New(c.URL)
		}
	})
	return c.endpoint.Get(string(repo))
}

// Search performs a symbol search on the symbols service.
func (c *Client) Search(ctx context.Context, args search.SymbolsParameters) (result result.Symbols, err error) {
	span, ctx := ot.StartSpanFromContext(ctx, "symbols.Client.Search")
	defer func() {
		if err != nil {
			ext.Error.Set(span, true)
			span.LogFields(otlog.Error(err))
		}
		span.Finish()
	}()
	span.SetTag("Repo", string(args.Repo))
	span.SetTag("CommitID", string(args.CommitID))

	resp, err := c.httpPost(ctx, "search", args.Repo, args)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		// best-effort inclusion of body in error message
		body, _ := io.ReadAll(io.LimitReader(resp.Body, 200))
		return nil, errors.Errorf(
			"Symbol.Search http status %d for %+v: %s",
			resp.StatusCode,
			resp.StatusCode,
			string(body),
		)
	}

	err = json.NewDecoder(resp.Body).Decode(&result)
	return result, err
}

func (c *Client) httpPost(
	ctx context.Context,
	method string,
	repo api.RepoName,
	payload interface{},
) (resp *http.Response, err error) {
	span, ctx := ot.StartSpanFromContext(ctx, "symbols.Client.httpPost")
	defer func() {
		if err != nil {
			ext.Error.Set(span, true)
			span.LogFields(otlog.Error(err))
		}
		span.Finish()
	}()

	url, err := c.url(repo)
	if err != nil {
		return nil, err
	}

	reqBody, err := json.Marshal(payload)
	if err != nil {
		return nil, err
	}

	if !strings.HasSuffix(url, "/") {
		url += "/"
	}
	req, err := http.NewRequest("POST", url+method, bytes.NewReader(reqBody))
	if err != nil {
		return nil, err
	}

	req.Header.Set("Content-Type", "application/json")
	req = req.WithContext(ctx)

	if c.HTTPLimiter != nil {
		span.LogKV("event", "Waiting on HTTP limiter")
		c.HTTPLimiter.Acquire()
		defer c.HTTPLimiter.Release()
		span.LogKV("event", "Acquired HTTP limiter")
	}

	req, ht := nethttp.TraceRequest(span.Tracer(), req,
		nethttp.OperationName("Symbols Client"),
		nethttp.ClientTrace(false))
	defer ht.Finish()

	return c.HTTPClient.Do(req)
}
