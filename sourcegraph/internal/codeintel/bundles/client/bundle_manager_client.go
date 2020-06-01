package client

import (
	"compress/gzip"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strings"

	"github.com/hashicorp/go-multierror"
	"github.com/inconshreveable/log15"
	"github.com/mxk/go-flowrate/flowrate"
	"github.com/neelance/parallel"
	"github.com/opentracing-contrib/go-stdlib/nethttp"
	"github.com/opentracing/opentracing-go/ext"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/sourcegraph/codeintelutils"
	"github.com/sourcegraph/sourcegraph/internal/metrics"
	"github.com/sourcegraph/sourcegraph/internal/tar"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
	"golang.org/x/net/context/ctxhttp"
)

// ErrNotFound occurs when the requested upload or bundle was evicted from disk.
var ErrNotFound = errors.New("data does not exist")

// ErrNoDownloadProgress occurs when there are multiple transient errors in a row that prevent
// the client from receiving a raw upload payload from the bundle manager.
var ErrNoDownloadProgress = errors.New("no download progress")

// The maximum number of iterations where we make no progress while fetching an upload.
const MaxZeroPayloadIterations = 3

// BundleManagerClient is the interface to the precise-code-intel-bundle-manager service.
type BundleManagerClient interface {
	// BundleClient creates a client that can answer intelligence queries for a single dump.
	BundleClient(bundleID int) BundleClient

	// SendUpload transfers a raw LSIF upload to the bundle manager to be stored on disk.
	SendUpload(ctx context.Context, bundleID int, r io.Reader) error

	// SendUploadPart transfers a partial LSIF upload to the bundle manager to be stored on disk.
	SendUploadPart(ctx context.Context, bundleID, partIndex int, r io.Reader) error

	// StitchParts instructs the bundle manager to collapse multipart uploads into a single file.
	StitchParts(ctx context.Context, bundleID int) error

	// DeleteUpload removes the upload file with the given identifier from disk.
	DeleteUpload(ctx context.Context, bundleID int) error

	// GetUpload retrieves a reader containing the content of a raw, uncompressed LSIF upload
	// from the bundle manager.
	GetUpload(ctx context.Context, bundleID int) (io.ReadCloser, error)

	// SendDB transfers a converted database archive to the bundle manager to be stored on disk.
	SendDB(ctx context.Context, bundleID int, path string) error

	// Exists determines if a file exists on disk for all the supplied identifiers.
	Exists(ctx context.Context, bundleIDs []int) (map[int]bool, error)
}

type baseClient interface {
	QueryBundle(ctx context.Context, bundleID int, op string, qs map[string]interface{}, target interface{}) error
}

var requestMeter = metrics.NewRequestMeter("precise_code_intel_bundle_manager", "Total number of requests sent to precise code intel bundle manager.")

const MaxIdleConnectionsPerHost = 500

// defaultTransport is the default transport used in the default client and the
// default reverse proxy. ot.Transport will propagate opentracing spans.
var defaultTransport = &ot.Transport{
	RoundTripper: requestMeter.Transport(&http.Transport{
		// Default is 2, but we can send many concurrent requests
		MaxIdleConnsPerHost: MaxIdleConnectionsPerHost,
	}, func(u *url.URL) string {
		// Extract the operation from a path like `/dbs/{id}/{operation}`
		if segments := strings.Split(u.Path, "/"); len(segments) == 4 {
			return segments[3]
		}

		// All other methods are uploading/downloading upload or bundle files.
		// These can all go into a single bucket which  are meant not necessarily
		// meant to have low latency.
		return "transfer"
	}),
}

type bundleManagerClientImpl struct {
	httpClient          *http.Client
	httpLimiter         *parallel.Run
	bundleManagerURL    string
	userAgent           string
	maxPayloadSizeBytes int
	ioCopy              func(io.Writer, io.Reader) (int64, error)
}

var _ BundleManagerClient = &bundleManagerClientImpl{}
var _ baseClient = &bundleManagerClientImpl{}

func New(bundleManagerURL string) BundleManagerClient {
	return &bundleManagerClientImpl{
		httpClient:          &http.Client{Transport: defaultTransport},
		httpLimiter:         parallel.NewRun(500),
		bundleManagerURL:    bundleManagerURL,
		userAgent:           filepath.Base(os.Args[0]),
		maxPayloadSizeBytes: 100 * 1000 * 1000, // 100Mb
		ioCopy:              io.Copy,
	}
}

// BundleClient creates a client that can answer intelligence queries for a single dump.
func (c *bundleManagerClientImpl) BundleClient(bundleID int) BundleClient {
	return &bundleClientImpl{
		base:     c,
		bundleID: bundleID,
	}
}

// SendUpload transfers a raw LSIF upload to the bundle manager to be stored on disk.
func (c *bundleManagerClientImpl) SendUpload(ctx context.Context, bundleID int, r io.Reader) error {
	url, err := makeURL(c.bundleManagerURL, fmt.Sprintf("uploads/%d", bundleID), nil)
	if err != nil {
		return err
	}

	body, err := c.do(ctx, "POST", url, r)
	if err != nil {
		return err
	}
	body.Close()
	return nil
}

// SendUploadPart transfers a partial LSIF upload to the bundle manager to be stored on disk.
func (c *bundleManagerClientImpl) SendUploadPart(ctx context.Context, bundleID, partIndex int, r io.Reader) error {
	url, err := makeURL(c.bundleManagerURL, fmt.Sprintf("uploads/%d/%d", bundleID, partIndex), nil)
	if err != nil {
		return err
	}

	body, err := c.do(ctx, "POST", url, r)
	if err != nil {
		return err
	}
	body.Close()
	return nil
}

// StitchParts instructs the bundle manager to collapse multipart uploads into a single file.
func (c *bundleManagerClientImpl) StitchParts(ctx context.Context, bundleID int) error {
	url, err := makeURL(c.bundleManagerURL, fmt.Sprintf("uploads/%d/stitch", bundleID), nil)
	if err != nil {
		return err
	}

	body, err := c.do(ctx, "POST", url, nil)
	if err != nil {
		return err
	}
	body.Close()
	return nil
}

// DeleteUpload removes the upload file with the given identifier from disk.
func (c *bundleManagerClientImpl) DeleteUpload(ctx context.Context, bundleID int) error {
	url, err := makeURL(c.bundleManagerURL, fmt.Sprintf("uploads/%d", bundleID), nil)
	if err != nil {
		return err
	}

	body, err := c.do(ctx, "DELETE", url, nil)
	if err != nil {
		return err
	}
	body.Close()
	return nil
}

// GetUpload retrieves a reader containing the content of a raw, uncompressed LSIF upload
// from the bundle manager.
func (c *bundleManagerClientImpl) GetUpload(ctx context.Context, bundleID int) (io.ReadCloser, error) {
	url, err := makeURL(c.bundleManagerURL, fmt.Sprintf("uploads/%d", bundleID), nil)
	if err != nil {
		return nil, err
	}

	pr, pw := io.Pipe()

	go func() {
		defer pw.Close()

		seek := int64(0)
		zeroPayloadIterations := 0

		for {
			n, err := c.getUploadChunk(ctx, pw, url, seek)
			if err != nil {
				if !isConnectionError(err) {
					_ = pw.CloseWithError(err)
					return
				}

				if n == 0 {
					zeroPayloadIterations++

					// Ensure that we don't spin infinitely when when a reset error
					// happens at the beginning of the requested payload. We'll just
					// give up if this happens a few times in a row, which should be
					// very unlikely.
					if zeroPayloadIterations > MaxZeroPayloadIterations {
						_ = pw.CloseWithError(ErrNoDownloadProgress)
						return
					}
				} else {
					zeroPayloadIterations = 0
				}

				// We have a transient error. Make another request but skip the
				// first seek + n bytes as we've already written these to disk.
				seek += n
				log15.Warn("Transient error while reading payload", "error", err)
				continue
			}

			return
		}
	}()

	return gzip.NewReader(pr)
}

// getUploadChunk retrieves a raw LSIF upload from the bundle manager starting from the offset as
// indicated by seek. The number of bytes written to the given writer is returned, along with any
// error.
func (c *bundleManagerClientImpl) getUploadChunk(ctx context.Context, w io.Writer, url *url.URL, seek int64) (n int64, err error) {
	q := url.Query()
	q.Set("seek", fmt.Sprintf("%d", seek))
	url.RawQuery = q.Encode()

	body, err := c.do(ctx, "GET", url, nil)
	if err != nil {
		return 0, err
	}
	defer body.Close()

	return c.ioCopy(w, body)
}

// SendDB transfers a converted database archive to the bundle manager to be stored on disk.
func (c *bundleManagerClientImpl) SendDB(ctx context.Context, bundleID int, path string) (err error) {
	files, cleanup, err := codeintelutils.SplitReader(tar.Archive(path), c.maxPayloadSizeBytes)
	if err != nil {
		return err
	}
	defer func() {
		err = cleanup(err)
	}()

	for i, file := range files {
		if err := c.sendPart(ctx, bundleID, file, i); err != nil {
			return err
		}
	}

	// We've uploaded all of our parts, signal the bundle manager to concatenate all
	// of the part files together so it can begin to serve queries with the new database.
	url, err := makeURL(c.bundleManagerURL, fmt.Sprintf("dbs/%d/stitch", bundleID), nil)
	if err != nil {
		return err
	}

	body, err := c.do(ctx, "POST", url, nil)
	if err != nil {
		return err
	}
	body.Close()
	return nil
}

// sendPart sends a portion of the database to the bundle manager.
func (c *bundleManagerClientImpl) sendPart(ctx context.Context, bundleID int, filename string, index int) (err error) {
	f, err := os.Open(filename)
	if err != nil {
		return err
	}
	defer func() {
		if closeErr := f.Close(); closeErr != nil {
			err = multierror.Append(err, closeErr)
		}
	}()

	url, err := makeURL(c.bundleManagerURL, fmt.Sprintf("dbs/%d/%d", bundleID, index), nil)
	if err != nil {
		return err
	}

	body, err := c.do(ctx, "POST", url, codeintelutils.Gzip(f))
	if err != nil {
		return err
	}
	defer body.Close()

	return nil
}

// Exists determines if a file exists on disk for all the supplied identifiers.
func (c *bundleManagerClientImpl) Exists(ctx context.Context, bundleIDs []int) (target map[int]bool, err error) {
	var bundleIDStrings []string
	for _, bundleID := range bundleIDs {
		bundleIDStrings = append(bundleIDStrings, fmt.Sprintf("%d", bundleID))
	}

	url, err := makeURL(c.bundleManagerURL, "exists", map[string]interface{}{
		"ids": strings.Join(bundleIDStrings, ","),
	})
	if err != nil {
		return nil, err
	}

	body, err := c.do(ctx, "GET", url, nil)
	if err != nil {
		return nil, err
	}
	defer body.Close()

	if err := json.NewDecoder(body).Decode(&target); err != nil {
		return nil, err
	}

	return target, nil
}

func (c *bundleManagerClientImpl) QueryBundle(ctx context.Context, bundleID int, op string, qs map[string]interface{}, target interface{}) (err error) {
	url, err := makeBundleURL(c.bundleManagerURL, bundleID, op, qs)
	if err != nil {
		return err
	}

	body, err := c.do(ctx, "GET", url, nil)
	if err != nil {
		return err
	}
	defer body.Close()

	return json.NewDecoder(body).Decode(&target)
}

func (c *bundleManagerClientImpl) do(ctx context.Context, method string, url *url.URL, body io.Reader) (_ io.ReadCloser, err error) {
	span, ctx := ot.StartSpanFromContext(ctx, "BundleManagerClient.do")
	defer func() {
		if err != nil {
			ext.Error.Set(span, true)
			span.SetTag("err", err.Error())
		}
		span.Finish()
	}()

	req, err := http.NewRequest(method, url.String(), limitTransferRate(body))
	if err != nil {
		return nil, err
	}

	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("User-Agent", c.userAgent)
	req = req.WithContext(ctx)

	if c.httpLimiter != nil {
		span.LogKV("event", "Waiting on HTTP limiter")
		c.httpLimiter.Acquire()
		defer c.httpLimiter.Release()
		span.LogKV("event", "Acquired HTTP limiter")
	}

	req, ht := nethttp.TraceRequest(
		span.Tracer(),
		req,
		nethttp.OperationName("Precise Code Intel Bundle Manager Client"),
		nethttp.ClientTrace(false),
	)
	defer ht.Finish()

	resp, err := ctxhttp.Do(req.Context(), c.httpClient, req)
	if err != nil {
		return nil, err
	}

	if resp.StatusCode == http.StatusNotFound {
		resp.Body.Close()
		return nil, ErrNotFound
	}

	if resp.StatusCode != http.StatusOK {
		resp.Body.Close()
		return nil, fmt.Errorf("unexpected status code %d", resp.StatusCode)
	}

	return resp.Body, nil
}

func makeURL(baseURL, path string, qs map[string]interface{}) (*url.URL, error) {
	values := url.Values{}
	for k, v := range qs {
		values[k] = []string{fmt.Sprintf("%v", v)}
	}

	url, err := url.Parse(fmt.Sprintf("%s/%s", baseURL, path))
	if err != nil {
		return nil, err
	}
	url.RawQuery = values.Encode()
	return url, nil
}

func makeBundleURL(baseURL string, bundleID int, op string, qs map[string]interface{}) (*url.URL, error) {
	return makeURL(baseURL, fmt.Sprintf("dbs/%d/%s", bundleID, op), qs)
}

// limitTransferRate applies a transfer limit to the given reader.
//
// In the case that the bundle manager is running on the same host as this service, an unbounded
// transfer rate can end up being so fast that we harm our own network connectivity. In order to
// prevent the disruption of other in-flight requests, we cap the transfer rate of r to 1Gbps.
func limitTransferRate(r io.Reader) io.ReadCloser {
	if r == nil {
		return nil
	}

	return flowrate.NewReader(r, 1000*1000*1000)
}

//
// Temporary network debugging code

var connectionErrors = prometheus.NewCounter(prometheus.CounterOpts{
	Name: "src_bundle_manager_connection_reset_by_peer_read",
	Help: "The total number connection reset by peer errors (client) when trying to transfer upload payloads.",
})

func init() {
	prometheus.MustRegister(connectionErrors)
}

func isConnectionError(err error) bool {
	if err != nil && strings.Contains(err.Error(), "read: connection reset by peer") {
		connectionErrors.Inc()
		return true
	}

	return false
}
