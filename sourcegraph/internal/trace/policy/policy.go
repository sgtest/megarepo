// Package policy exports functionality related to whether or not to trace.
package policy

import (
	"context"
	"net/http"
	"strconv"
	"strings"

	"github.com/opentracing-contrib/go-stdlib/nethttp"
	"go.uber.org/atomic"
)

type TracePolicy string

const (
	// TraceNone turns off tracing.
	TraceNone TracePolicy = "none"

	// TraceSelective turns on tracing only for requests with the X-Sourcegraph-Should-Trace header
	// set to a truthy value.
	TraceSelective TracePolicy = "selective"

	// TraceAll turns on tracing for all requests.
	TraceAll TracePolicy = "all"
)

var trPolicy = atomic.NewString(string(TraceNone))

func SetTracePolicy(newTracePolicy TracePolicy) {
	trPolicy.Store(string(newTracePolicy))
}

func GetTracePolicy() TracePolicy {
	return TracePolicy(trPolicy.Load())
}

type key int

const shouldTraceKey key = iota

// ShouldTrace returns true if the shouldTraceKey context value is true.
func ShouldTrace(ctx context.Context) bool {
	v, ok := ctx.Value(shouldTraceKey).(bool)
	if !ok {
		return false
	}
	return v
}

// WithShouldTrace sets the shouldTraceKey context value.
func WithShouldTrace(ctx context.Context, shouldTrace bool) context.Context {
	return context.WithValue(ctx, shouldTraceKey, shouldTrace)
}

const (
	traceHeader = "X-Sourcegraph-Should-Trace"
	traceQuery  = "trace"
)

// Transport wraps an underlying HTTP RoundTripper, injecting the X-Sourcegraph-Should-Trace header
// into outgoing requests whenever the shouldTraceKey context value is true.
type Transport struct {
	http.RoundTripper
}

func (r *Transport) RoundTrip(req *http.Request) (*http.Response, error) {
	req.Header.Set(traceHeader, strconv.FormatBool(ShouldTrace(req.Context())))
	t := nethttp.Transport{RoundTripper: r.RoundTripper}
	return t.RoundTrip(req)
}

// requestWantsTrace returns true if a request is opting into tracing either
// via our HTTP Header or our URL Query.
func RequestWantsTracing(r *http.Request) bool {
	// Prefer header over query param.
	if v := r.Header.Get(traceHeader); v != "" {
		b, _ := strconv.ParseBool(v)
		return b
	}
	// PERF: Avoid parsing RawQuery if "trace=" is not present
	if strings.Contains(r.URL.RawQuery, "trace=") {
		v := r.URL.Query().Get(traceQuery)
		b, _ := strconv.ParseBool(v)
		return b
	}
	return false
}
