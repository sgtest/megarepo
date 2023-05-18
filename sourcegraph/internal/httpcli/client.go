package httpcli

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"math"
	"math/rand"
	"net"
	"net/http"
	"net/url"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/PuerkitoBio/rehttp"
	"github.com/gregjones/httpcache"
	"github.com/opentracing-contrib/go-stdlib/nethttp"
	"github.com/opentracing/opentracing-go"
	otlog "github.com/opentracing/opentracing-go/log"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
	"github.com/sourcegraph/log"
	"go.opentelemetry.io/contrib/instrumentation/net/http/otelhttp"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/rcache"
	"github.com/sourcegraph/sourcegraph/internal/requestclient"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/trace/policy"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// A Doer captures the Do method of an http.Client. It facilitates decorating
// an http.Client with orthogonal concerns such as logging, metrics, retries,
// etc.
type Doer interface {
	Do(*http.Request) (*http.Response, error)
}

type MockDoer func(*http.Request) (*http.Response, error)

func (m MockDoer) Do(req *http.Request) (*http.Response, error) {
	return m(req)
}

// DoerFunc is function adapter that implements the http.RoundTripper
// interface by calling itself.
type DoerFunc func(*http.Request) (*http.Response, error)

// Do implements the Doer interface.
func (f DoerFunc) Do(req *http.Request) (*http.Response, error) {
	return f(req)
}

// A Middleware function wraps a Doer with a layer of behaviour. It's used
// to decorate an http.Client with orthogonal layers of behaviour such as
// logging, instrumentation, retries, etc.
type Middleware func(Doer) Doer

// NewMiddleware returns a Middleware stack composed of the given Middlewares.
func NewMiddleware(mws ...Middleware) Middleware {
	return func(bottom Doer) (stacked Doer) {
		stacked = bottom
		for _, mw := range mws {
			stacked = mw(stacked)
		}
		return stacked
	}
}

// Opt configures an aspect of a given *http.Client,
// returning an error in case of failure.
type Opt func(*http.Client) error

// A Factory constructs an http.Client with the given functional
// options applied, returning an aggregate error of the errors returned by
// all those options.
type Factory struct {
	stack  Middleware
	common []Opt
}

// redisCache is an HTTP cache backed by Redis. The TTL of a week is a balance
// between caching values for a useful amount of time versus growing the cache
// too large.
var redisCache = rcache.NewWithTTL("http", 604800)

// CachedTransportOpt is the default transport cache - it will return values from
// the cache where possible (avoiding a network request) and will additionally add
// validators (etag/if-modified-since) to repeated requests allowing servers to
// return 304 / Not Modified.
//
// Responses load from cache will have the 'X-From-Cache' header set.
var CachedTransportOpt = NewCachedTransportOpt(redisCache, true)

// ExternalClientFactory is a httpcli.Factory with common options
// and middleware pre-set for communicating with external services.
// WARN: Clients from this factory cache entire responses for etag matching. Do not
// use them for one-off requests if possible, and definitely not for larger payloads,
// like downloading arbitrarily sized files! See UncachedExternalClientFactory instead.
var ExternalClientFactory = NewExternalClientFactory()

// UncachedExternalClientFactory is a httpcli.Factory with common options
// and middleware pre-set for communicating with external services, but with caching
// responses disabled.
var UncachedExternalClientFactory = newExternalClientFactory(false)

var (
	externalTimeout, _               = time.ParseDuration(env.Get("SRC_HTTP_CLI_EXTERNAL_TIMEOUT", "5m", "Timeout for external HTTP requests"))
	externalRetryDelayBase, _        = time.ParseDuration(env.Get("SRC_HTTP_CLI_EXTERNAL_RETRY_DELAY_BASE", "200ms", "Base retry delay duration for external HTTP requests"))
	externalRetryDelayMax, _         = time.ParseDuration(env.Get("SRC_HTTP_CLI_EXTERNAL_RETRY_DELAY_MAX", "3s", "Max retry delay duration for external HTTP requests"))
	externalRetryMaxAttempts, _      = strconv.Atoi(env.Get("SRC_HTTP_CLI_EXTERNAL_RETRY_MAX_ATTEMPTS", "20", "Max retry attempts for external HTTP requests"))
	externalRetryAfterMaxDuration, _ = time.ParseDuration(env.Get("SRC_HTTP_CLI_EXTERNAL_RETRY_AFTER_MAX_DURATION", "3s", "Max duration to wait in retry-after header before we won't auto-retry"))
)

// NewExternalClientFactory returns a httpcli.Factory with common options
// and middleware pre-set for communicating with external services. Additional
// middleware can also be provided to e.g. enable logging with NewLoggingMiddleware.
// WARN: Clients from this factory cache entire responses for etag matching. Do not
// use them for one-off requests if possible, and definitely not for larger payloads,
// like downloading arbitrarily sized files!
func NewExternalClientFactory(middleware ...Middleware) *Factory {
	return newExternalClientFactory(true, middleware...)
}

// NewExternalClientFactory returns a httpcli.Factory with common options
// and middleware pre-set for communicating with external services. Additional
// middleware can also be provided to e.g. enable logging with NewLoggingMiddleware.
// If cache is true, responses will be cached in redis for improved rate limiting
// and reduced byte transfer sizes.
func newExternalClientFactory(cache bool, middleware ...Middleware) *Factory {
	mw := []Middleware{
		ContextErrorMiddleware,
		HeadersMiddleware("User-Agent", "Sourcegraph-Bot"),
		redisLoggerMiddleware(),
	}
	mw = append(mw, middleware...)

	opts := []Opt{
		NewTimeoutOpt(externalTimeout),
		// ExternalTransportOpt needs to be before TracedTransportOpt and
		// NewCachedTransportOpt since it wants to extract a http.Transport,
		// not a generic http.RoundTripper.
		ExternalTransportOpt,
		NewErrorResilientTransportOpt(
			NewRetryPolicy(MaxRetries(externalRetryMaxAttempts), externalRetryAfterMaxDuration),
			ExpJitterDelay(externalRetryDelayBase, externalRetryDelayMax),
		),
		TracedTransportOpt,
	}
	if cache {
		opts = append(opts, CachedTransportOpt)
	}

	return NewFactory(
		NewMiddleware(mw...),
		opts...,
	)
}

// ExternalDoer is a shared client for external communication. This is a
// convenience for existing uses of http.DefaultClient.
// WARN: This client caches entire responses for etag matching. Do not use it for
// one-off requests if possible, and definitely not for larger payloads, like
// downloading arbitrarily sized files! See UncachedExternalClient instead.
var ExternalDoer, _ = ExternalClientFactory.Doer()

// ExternalClient returns a shared client for external communication. This is
// a convenience for existing uses of http.DefaultClient.
// WARN: This client caches entire responses for etag matching. Do not use it for
// one-off requests if possible, and definitely not for larger payloads, like
// downloading arbitrarily sized files! See UncachedExternalClient instead.
var ExternalClient, _ = ExternalClientFactory.Client()

// InternalClientFactory is a httpcli.Factory with common options
// and middleware pre-set for communicating with internal services.
var InternalClientFactory = NewInternalClientFactory("internal")

var (
	internalTimeout, _               = time.ParseDuration(env.Get("SRC_HTTP_CLI_INTERNAL_TIMEOUT", "0", "Timeout for internal HTTP requests"))
	internalRetryDelayBase, _        = time.ParseDuration(env.Get("SRC_HTTP_CLI_INTERNAL_RETRY_DELAY_BASE", "50ms", "Base retry delay duration for internal HTTP requests"))
	internalRetryDelayMax, _         = time.ParseDuration(env.Get("SRC_HTTP_CLI_INTERNAL_RETRY_DELAY_MAX", "1s", "Max retry delay duration for internal HTTP requests"))
	internalRetryMaxAttempts, _      = strconv.Atoi(env.Get("SRC_HTTP_CLI_INTERNAL_RETRY_MAX_ATTEMPTS", "20", "Max retry attempts for internal HTTP requests"))
	internalRetryAfterMaxDuration, _ = time.ParseDuration(env.Get("SRC_HTTP_CLI_INTERNAL_RETRY_AFTER_MAX_DURATION", "3s", "Max duration to wait in retry-after header before we won't auto-retry"))
)

// NewInternalClientFactory returns a httpcli.Factory with common options
// and middleware pre-set for communicating with internal services. Additional
// middleware can also be provided to e.g. enable logging with NewLoggingMiddleware.
func NewInternalClientFactory(subsystem string, middleware ...Middleware) *Factory {
	mw := []Middleware{
		ContextErrorMiddleware,
	}
	mw = append(mw, middleware...)

	return NewFactory(
		NewMiddleware(mw...),
		NewTimeoutOpt(internalTimeout),
		NewMaxIdleConnsPerHostOpt(500),
		NewErrorResilientTransportOpt(
			NewRetryPolicy(MaxRetries(internalRetryMaxAttempts), internalRetryAfterMaxDuration),
			ExpJitterDelay(internalRetryDelayBase, internalRetryDelayMax),
		),
		MeteredTransportOpt(subsystem),
		ActorTransportOpt,
		RequestClientTransportOpt,
		TracedTransportOpt,
	)
}

// InternalDoer is a shared client for internal communication. This is a
// convenience for existing uses of http.DefaultClient.
var InternalDoer, _ = InternalClientFactory.Doer()

// InternalClient returns a shared client for external communication. This is
// a convenience for existing uses of http.DefaultClient.
var InternalClient, _ = InternalClientFactory.Client()

// Doer returns a new Doer wrapped with the middleware stack
// provided in the Factory constructor and with the given common
// and base opts applied to it.
func (f Factory) Doer(base ...Opt) (Doer, error) {
	cli, err := f.Client(base...)
	if err != nil {
		return nil, err
	}

	if f.stack != nil {
		return f.stack(cli), nil
	}

	return cli, nil
}

// Client returns a new http.Client configured with the
// given common and base opts, but not wrapped with any
// middleware.
func (f Factory) Client(base ...Opt) (*http.Client, error) {
	opts := make([]Opt, 0, len(f.common)+len(base))
	opts = append(opts, base...)
	opts = append(opts, f.common...)

	var cli http.Client
	var err error

	for _, opt := range opts {
		err = errors.Append(err, opt(&cli))
	}

	return &cli, err
}

// NewFactory returns a Factory that applies the given common
// Opts after the ones provided on each invocation of Client or Doer.
//
// If the given Middleware stack is not nil, the final configured client
// will be wrapped by it before being returned from a call to Doer, but not Client.
func NewFactory(stack Middleware, common ...Opt) *Factory {
	return &Factory{stack: stack, common: common}
}

//
// Common Middleware
//

// HeadersMiddleware returns a middleware that wraps a Doer
// and sets the given headers.
func HeadersMiddleware(headers ...string) Middleware {
	if len(headers)%2 != 0 {
		panic("missing header values")
	}
	return func(cli Doer) Doer {
		return DoerFunc(func(req *http.Request) (*http.Response, error) {
			for i := 0; i < len(headers); i += 2 {
				req.Header.Add(headers[i], headers[i+1])
			}
			return cli.Do(req)
		})
	}
}

// ContextErrorMiddleware wraps a Doer with context.Context error
// handling. It checks if the request context is done, and if so,
// returns its error. Otherwise, it returns the error from the inner
// Doer call.
func ContextErrorMiddleware(cli Doer) Doer {
	return DoerFunc(func(req *http.Request) (*http.Response, error) {
		resp, err := cli.Do(req)
		if err != nil {
			// If we got an error, and the context has been canceled,
			// the context's error is probably more useful.
			if e := req.Context().Err(); e != nil {
				err = e
			}
		}
		return resp, err
	})
}

// GitHubProxyRedirectMiddleware rewrites requests to the "github-proxy" host
// to "https://api.github.com".
func GitHubProxyRedirectMiddleware(cli Doer) Doer {
	return DoerFunc(func(req *http.Request) (*http.Response, error) {
		if req.URL.Hostname() == "github-proxy" {
			req.URL.Host = "api.github.com"
			req.URL.Scheme = "https"
		}
		return cli.Do(req)
	})
}

// requestContextKey is used to denote keys to fields that should be logged by the logging
// middleware. They should be set to the request context associated with a response.
type requestContextKey int

const (
	// requestRetryAttemptKey is the key to the rehttp.Attempt attached to a request, if
	// a request undergoes retries via NewRetryPolicy
	requestRetryAttemptKey requestContextKey = iota

	// redisLoggingMiddlewareErrorKey is the key to any errors that occurred when logging
	// a request to Redis via redisLoggerMiddleware
	redisLoggingMiddlewareErrorKey
)

// NewLoggingMiddleware logs basic diagnostics about requests made through this client at
// debug level. The provided logger is given the 'httpcli' subscope.
//
// It also logs metadata set by request context by other middleware, such as NewRetryPolicy.
func NewLoggingMiddleware(logger log.Logger) Middleware {
	logger = logger.Scoped("httpcli", "http client")

	return func(d Doer) Doer {
		return DoerFunc(func(r *http.Request) (*http.Response, error) {
			start := time.Now()
			resp, err := d.Do(r)

			// Gather fields about this request.
			fields := append(make([]log.Field, 0, 5), // preallocate some space
				log.String("host", r.URL.Host),
				log.String("path", r.URL.Path),
				log.Duration("duration", time.Since(start)))
			if err != nil {
				fields = append(fields, log.Error(err))
			}
			// Check incoming request context, unless a response is available, in which
			// case we check the request associated with the response in case it is not
			// the same as the original request (e.g. due to retries)
			ctx := r.Context()
			if resp != nil {
				ctx = resp.Request.Context()
				fields = append(fields, log.Int("code", resp.StatusCode))
			}
			// Gather fields from request context. When adding fields set into context,
			// make sure to test that the fields get propagated and picked up correctly
			// in TestLoggingMiddleware.
			if attempt, ok := ctx.Value(requestRetryAttemptKey).(rehttp.Attempt); ok {
				// Get fields from NewRetryPolicy
				fields = append(fields, log.Object("retry",
					log.Int("attempts", attempt.Index),
					log.Error(attempt.Error)))
			}
			if redisErr, ok := ctx.Value(redisLoggingMiddlewareErrorKey).(error); ok {
				// Get fields from redisLoggerMiddleware
				fields = append(fields, log.NamedError("redisLoggerErr", redisErr))
			}

			// Log results with link to trace if present
			trace.Logger(ctx, logger).
				Debug("request", fields...)

			return resp, err
		})
	}
}

//
// Common Opts
//

// ExternalTransportOpt returns an Opt that ensures the http.Client.Transport
// can contact non-Sourcegraph services. For example Admins can configure
// TLS/SSL settings.
func ExternalTransportOpt(cli *http.Client) error {
	tr, err := getTransportForMutation(cli)
	if err != nil {
		return errors.Wrap(err, "httpcli.ExternalTransportOpt")
	}

	cli.Transport = &externalTransport{base: tr}
	return nil
}

// NewCertPoolOpt returns an Opt that sets the RootCAs pool of an http.Client's
// transport.
func NewCertPoolOpt(certs ...string) Opt {
	return func(cli *http.Client) error {
		if len(certs) == 0 {
			return nil
		}

		tr, err := getTransportForMutation(cli)
		if err != nil {
			return errors.Wrap(err, "httpcli.NewCertPoolOpt")
		}

		if tr.TLSClientConfig == nil {
			tr.TLSClientConfig = new(tls.Config)
		}

		pool := x509.NewCertPool()
		tr.TLSClientConfig.RootCAs = pool

		for _, cert := range certs {
			if ok := pool.AppendCertsFromPEM([]byte(cert)); !ok {
				return errors.New("httpcli.NewCertPoolOpt: invalid certificate")
			}
		}

		return nil
	}
}

// NewCachedTransportOpt returns an Opt that wraps the existing http.Transport
// of an http.Client with caching using the given Cache.
//
// If markCachedResponses, responses returned from the cache will be given an extra header,
// X-From-Cache.
func NewCachedTransportOpt(c httpcache.Cache, markCachedResponses bool) Opt {
	return func(cli *http.Client) error {
		if cli.Transport == nil {
			cli.Transport = http.DefaultTransport
		}

		cli.Transport = &wrappedTransport{
			RoundTripper: &httpcache.Transport{
				Transport:           cli.Transport,
				Cache:               c,
				MarkCachedResponses: markCachedResponses,
			},
			Wrapped: cli.Transport,
		}

		return nil
	}
}

// TracedTransportOpt wraps an existing http.Transport of an http.Client with
// tracing functionality.
func TracedTransportOpt(cli *http.Client) error {
	if cli.Transport == nil {
		cli.Transport = http.DefaultTransport
	}

	// Propagate trace policy
	cli.Transport = &policy.Transport{RoundTripper: cli.Transport}

	// Keep the legacy nethttp transport for now that was used before - otelhttp
	// should propagate traces in the same way, but we keep this just in case.
	// This used to be in policy.Transport, but is clearer here.
	cli.Transport = &nethttp.Transport{RoundTripper: cli.Transport}

	// Collect and propagate OpenTelemetry trace (among other formats initialized
	// in internal/tracer)
	cli.Transport = otelhttp.NewTransport(cli.Transport)

	return nil
}

// MeteredTransportOpt returns an opt that wraps an existing http.Transport of a http.Client with
// metrics collection.
func MeteredTransportOpt(subsystem string) Opt {
	// This will generate a metric of the following format:
	// src_$subsystem_requests_total
	//
	// For example, if the subsystem is set to "internal", the metric being generated will be named
	// src_internal_requests_total
	meter := metrics.NewRequestMeter(
		subsystem,
		"Total number of requests sent to "+subsystem,
	)

	return func(cli *http.Client) error {
		if cli.Transport == nil {
			cli.Transport = http.DefaultTransport
		}

		cli.Transport = meter.Transport(cli.Transport, func(u *url.URL) string {
			// We don't have a way to return a low cardinality label here (for
			// the prometheus label "category"). Previously we returned u.Path
			// but that blew up prometheus. So we just return unknown.
			return "unknown"
		})

		return nil
	}
}

var metricRetry = promauto.NewCounter(prometheus.CounterOpts{
	Name: "src_httpcli_retry_total",
	Help: "Total number of times we retry HTTP requests.",
})

// A regular expression to match the error returned by net/http when the
// configured number of redirects is exhausted. This error isn't typed
// specifically so we resort to matching on the error string.
var redirectsErrorRe = lazyregexp.New(`stopped after \d+ redirects\z`)

// A regular expression to match the error returned by net/http when the
// scheme specified in the URL is invalid. This error isn't typed
// specifically so we resort to matching on the error string.
var schemeErrorRe = lazyregexp.New(`unsupported protocol scheme`)

// MaxRetries returns the max retries to be attempted, which should be passed
// to NewRetryPolicy. If we're in tests, it returns 1, otherwise it tries to
// parse SRC_HTTP_CLI_MAX_RETRIES and return that. If it can't, it defaults to 20.
func MaxRetries(n int) int {
	if strings.HasSuffix(os.Args[0], ".test") || strings.HasSuffix(os.Args[0], "_test") {
		return 0
	}
	return n
}

// NewRetryPolicy returns a retry policy used in any Doer or Client returned
// by NewExternalClientFactory.
func NewRetryPolicy(max int, retryAfterMaxSleepDuration time.Duration) rehttp.RetryFn {
	return func(a rehttp.Attempt) (retry bool) {
		status := 0
		var retryAfterHeader string

		defer func() {
			// Avoid trace log spam if we haven't invoked the retry policy.
			shouldTraceLog := retry || a.Index > 0
			if span := opentracing.SpanFromContext(a.Request.Context()); span != nil && shouldTraceLog {
				fields := []otlog.Field{
					otlog.Event("request-retry-decision"),
					otlog.Bool("retry", retry),
					otlog.Int("attempt", a.Index),
					otlog.String("method", a.Request.Method),
					otlog.String("url", a.Request.URL.String()),
					otlog.Int("status", status),
					otlog.String("retry-after", retryAfterHeader),
				}
				if a.Error != nil {
					fields = append(fields, otlog.Error(a.Error))
				}
				span.LogFields(fields...)
			}

			// Update request context with latest retry for logging middleware
			if shouldTraceLog {
				*a.Request = *a.Request.WithContext(
					context.WithValue(a.Request.Context(), requestRetryAttemptKey, a))
			}

			if retry {
				metricRetry.Inc()
			}

			if retry || a.Error == nil || a.Index == 0 {
				return
			}
		}()

		if a.Response != nil {
			status = a.Response.StatusCode
		}

		if a.Index >= max { // Max retries
			return false
		}

		switch a.Error {
		case nil:
		case context.DeadlineExceeded, context.Canceled:
			return false
		default:
			// Don't retry more than 3 times for no such host errors.
			// This affords some resilience to dns unreliability while
			// preventing 20 attempts with a non existing name.
			var dnsErr *net.DNSError
			if a.Index >= 3 && errors.As(a.Error, &dnsErr) && dnsErr.IsNotFound {
				return false
			}

			if v, ok := a.Error.(*url.Error); ok {
				e := v.Error()
				// Don't retry if the error was due to too many redirects.
				if redirectsErrorRe.MatchString(e) {
					return false
				}

				// Don't retry if the error was due to an invalid protocol scheme.
				if schemeErrorRe.MatchString(e) {
					return false
				}

				// Don't retry if the error was due to TLS cert verification failure.
				if _, ok := v.Err.(x509.UnknownAuthorityError); ok {
					return false
				}

			}
			// The error is likely recoverable so retry.
			return true
		}

		if status == 0 || (status >= 500 && status != http.StatusNotImplemented) {
			return true
		}

		if status == http.StatusTooManyRequests {
			// If a retry-after header exists, we only want to retry if it might resolve
			// the issue.
			if a.Response != nil {
				retryAfterHeader = a.Response.Header.Get("retry-after")
				if retryAfterHeader != "" {
					// There are two valid formats for retry-after headers: seconds
					// until retry in int, or a RFC1123 date string.
					// First, see if it is denoted in seconds.
					s, err := strconv.Atoi(retryAfterHeader)
					// If denoted in seconds, only retry if we will get access within
					// the next retryAfterMaxSleepDuration seconds.
					if err == nil {
						return s <= int(retryAfterMaxSleepDuration/time.Second)
					}

					// If we weren't able to parse as seconds, try to parse as RFC1123.
					if err != nil {
						after, err := time.Parse(time.RFC1123, retryAfterHeader)
						if err != nil {
							// We don't know how to parse this header, so let's just retry.
							return true
						}
						// Check if the date is either in the past, or if within the next
						// retryAfterMaxSleepDuration we would get access again.
						in := time.Until(after)
						return in <= retryAfterMaxSleepDuration
					}
				}

				// Otherwise, default to the behavior this function always had: retry 429 errors.
				return true
			}
		}

		return false
	}
}

// ExpJitterDelay returns a DelayFn that returns a delay between 0 and
// base * 2^attempt capped at max (an exponential backoff delay with
// jitter).
//
// See the full jitter algorithm in:
// http://www.awsarchitectureblog.com/2015/03/backoff.html
//
// This is adapted from rehttp.ExpJitterDelay to not use a non-thread-safe
// package level PRNG and to be safe against overflows. It assumes that
// max > base.
func ExpJitterDelay(base, max time.Duration) rehttp.DelayFn {
	var mu sync.Mutex
	prng := rand.New(rand.NewSource(time.Now().UnixNano()))
	return func(attempt rehttp.Attempt) time.Duration {
		exp := math.Pow(2, float64(attempt.Index))
		top := float64(base) * exp
		n := int64(math.Min(float64(max), top))
		if n <= 0 {
			return base
		}

		mu.Lock()
		delay := time.Duration(prng.Int63n(n))
		mu.Unlock()

		// Overflow handling
		switch {
		case delay < base:
			return base
		case delay > max:
			return max
		default:
			return delay
		}
	}
}

// NewErrorResilientTransportOpt returns an Opt that wraps an existing
// http.Transport of an http.Client with automatic retries.
func NewErrorResilientTransportOpt(retry rehttp.RetryFn, delay rehttp.DelayFn) Opt {
	return func(cli *http.Client) error {
		if cli.Transport == nil {
			cli.Transport = http.DefaultTransport
		}

		cli.Transport = rehttp.NewTransport(cli.Transport, retry, delay)
		return nil
	}
}

// NewIdleConnTimeoutOpt returns a Opt that sets the IdleConnTimeout of an
// http.Client's transport.
func NewIdleConnTimeoutOpt(timeout time.Duration) Opt {
	return func(cli *http.Client) error {
		tr, err := getTransportForMutation(cli)
		if err != nil {
			return errors.Wrap(err, "httpcli.NewIdleConnTimeoutOpt")
		}

		tr.IdleConnTimeout = timeout

		return nil
	}
}

// NewMaxIdleConnsPerHostOpt returns a Opt that sets the MaxIdleConnsPerHost field of an
// http.Client's transport.
func NewMaxIdleConnsPerHostOpt(max int) Opt {
	return func(cli *http.Client) error {
		tr, err := getTransportForMutation(cli)
		if err != nil {
			return errors.Wrap(err, "httpcli.NewMaxIdleConnsOpt")
		}

		tr.MaxIdleConnsPerHost = max

		return nil
	}
}

// NewTimeoutOpt returns a Opt that sets the Timeout field of an http.Client.
func NewTimeoutOpt(timeout time.Duration) Opt {
	return func(cli *http.Client) error {
		if timeout > 0 {
			cli.Timeout = timeout
		}
		return nil
	}
}

// getTransport returns the http.Transport for cli. If Transport is nil, it is
// set to a copy of the DefaultTransport. If it is the DefaultTransport, it is
// updated to a copy of the DefaultTransport.
//
// Use this function when you intend on mutating the transport.
func getTransportForMutation(cli *http.Client) (*http.Transport, error) {
	if cli.Transport == nil {
		cli.Transport = http.DefaultTransport
	}

	// Try to get the underlying, concrete *http.Transport implementation, copy it, and
	// replace it.
	var transport *http.Transport
	switch v := cli.Transport.(type) {
	case *http.Transport:
		transport = v.Clone()
		// Replace underlying implementation
		cli.Transport = transport

	case WrappedTransport:
		wrapped := unwrapAll(v)
		t, ok := (*wrapped).(*http.Transport)
		if !ok {
			return nil, errors.Errorf("http.Client.Transport cannot be unwrapped as *http.Transport: %T", cli.Transport)
		}
		transport = t.Clone()
		// Replace underlying implementation
		*wrapped = transport

	default:
		return nil, errors.Errorf("http.Client.Transport cannot be cast as a *http.Transport: %T", cli.Transport)
	}

	return transport, nil
}

// ActorTransportOpt wraps an existing http.Transport of an http.Client to pull the actor
// from the context and add it to each request's HTTP headers.
//
// Servers can use actor.HTTPMiddleware to populate actor context from incoming requests.
func ActorTransportOpt(cli *http.Client) error {
	if cli.Transport == nil {
		cli.Transport = http.DefaultTransport
	}

	cli.Transport = &wrappedTransport{
		RoundTripper: &actor.HTTPTransport{RoundTripper: cli.Transport},
		Wrapped:      cli.Transport,
	}

	return nil
}

// RequestClientTransportOpt wraps an existing http.Transport of an http.Client to pull
// the original client's IP from the context and add it to each request's HTTP headers.
//
// Servers can use requestclient.HTTPMiddleware to populate client context from incoming requests.
func RequestClientTransportOpt(cli *http.Client) error {
	if cli.Transport == nil {
		cli.Transport = http.DefaultTransport
	}

	cli.Transport = &wrappedTransport{
		RoundTripper: &requestclient.HTTPTransport{RoundTripper: cli.Transport},
		Wrapped:      cli.Transport,
	}

	return nil
}

// IsRiskyHeader returns true if the request or response header is likely to contain private data.
func IsRiskyHeader(name string, values []string) bool {
	return isRiskyHeaderName(name) || containsRiskyHeaderValue(values)
}

// isRiskyHeaderName returns true if the request or response header is likely to contain private data
// based on its name.
func isRiskyHeaderName(name string) bool {
	riskyHeaderKeys := []string{"auth", "cookie", "token"}
	for _, riskyKey := range riskyHeaderKeys {
		if strings.Contains(strings.ToLower(name), riskyKey) {
			return true
		}
	}
	return false
}

// ContainsRiskyHeaderValue returns true if the values array of a request or response header
// looks like it's likely to contain private data.
func containsRiskyHeaderValue(values []string) bool {
	riskyHeaderValues := []string{"bearer", "ghp_", "glpat-"}
	for _, value := range values {
		for _, riskyValue := range riskyHeaderValues {
			if strings.Contains(strings.ToLower(value), riskyValue) {
				return true
			}
		}
	}
	return false
}
