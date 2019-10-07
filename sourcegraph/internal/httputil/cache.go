package httputil

import (
	"net/http"

	"github.com/sourcegraph/sourcegraph/internal/rcache"

	"github.com/gregjones/httpcache"
)

var (
	// Cache is a HTTP cache backed by Redis. The TTL of a week is a
	// balance between caching values for a useful amount of time versus
	// growing the cache too large.
	Cache = rcache.NewWithTTL("http", 604800)

	// CachingClient is an HTTP client that caches responses backed by
	// Redis (using Cache).
	CachingClient = &http.Client{Transport: httpcache.NewTransport(Cache)}
)
