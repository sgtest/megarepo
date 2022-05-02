// Package pypi
//
// A client for PyPI's simple project API as described in
// https://peps.python.org/pep-0503/.
//
// Nomenclature:
//
// A "project" on PyPI is the name of a collection of releases and files, and
// information about them. Projects on PyPI are made and shared by other members
// of the Python community so that you can use them.
//
// A "release" on PyPI is a specific version of a project. For example, the
// requests project has many releases, like "requests 2.10" and "requests 1.2.1".
// A release consists of one or more "files".
//
// A "file", also known as a "package", on PyPI is something that you can
// download and install. Because of different hardware, operating systems, and
// file formats, a release may have several files (packages), like an archive
// containing source code or a binary
//
// https://pypi.org/help/#packages
//
package pypi

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"path"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"github.com/inconshreveable/log15"
	"golang.org/x/net/html"
	"golang.org/x/time/rate"

	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"
	"github.com/sourcegraph/sourcegraph/internal/ratelimit"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// rateLimitingWaitThreshold is maximum rate limiting wait duration after which
// a warning log is produced to help site admins debug why syncing may be taking
// longer than expected.
const rateLimitingWaitThreshold = 200 * time.Millisecond

type Client struct {
	// A list of PyPI proxies. Each url should point to the root of the simple-API.
	// For example for pypi.org the url should be https://pypi.org/simple with or
	// without a trailing slash.
	urls []string
	cli  httpcli.Doer

	// Self-imposed rate-limiter. pypi.org does not impose a rate limiting policy.
	limiter *rate.Limiter
}

func NewClient(urn string, urls []string) *Client {
	return &Client{
		urls: urls,

		// ExternalDoer sets the user-agent as suggested by PyPI
		// https://warehouse.pypa.io/api-reference/.
		cli:     httpcli.ExternalDoer,
		limiter: ratelimit.DefaultRegistry.Get(urn),
	}
}

// Project returns the content of the simple-API /<project>/ endpoint.
func (c *Client) Project(ctx context.Context, project string) ([]byte, error) {
	data, err := c.get(ctx, normalize(project))
	if err != nil {
		return nil, errors.Wrap(err, "PyPI")
	}
	return data, nil
}

// File represents one anchor element in the response from /<project>/.
//
// https://peps.python.org/pep-0503/
type File struct {
	// The file format for tarballs is <package>-<version>.tar.gz.
	//
	// The file format for wheels (.whl) is described in
	// https://peps.python.org/pep-0491/#file-format.
	Name string

	// URLs may be either absolute or relative as long as they point to the correct
	// location.
	URL string

	// Optional. A repository MAY include a data-gpg-sig attribute on a file link
	// with a value of either true or false to indicate whether or not there is a
	// GPG signature. Repositories that do this SHOULD include it on every link.
	DataGPGSig *bool

	// A repository MAY include a data-requires-python attribute on a file link.
	// This exposes the Requires-Python metadata field, specified in PEP 345, for
	// the corresponding release.
	DataRequiresPython string
}

// Parse parses the output of Client.Project into a list of files. Anchor tags
// without href are ignored.
func Parse(b []byte) ([]File, error) {
	var files []File

	z := html.NewTokenizer(bytes.NewReader(b))

	// We want to iterate over the anchor tags. Quoting from PEP503: "[The project]
	// URL must respond with a valid HTML5 page with a single anchor element per
	// file for the project".
	nextAnchor := func() bool {
		for {
			switch z.Next() {
			case html.ErrorToken:
				return false
			case html.StartTagToken:
				if name, _ := z.TagName(); string(name) == "a" {
					return true
				}
			}
		}
	}

OUTER:
	for nextAnchor() {
		cur := File{}

		// Parse attributes.
		for {
			k, v, more := z.TagAttr()
			switch string(k) {
			case "href":
				cur.URL = string(v)
			case "data-requires-python":
				cur.DataRequiresPython = string(v)
			case "data-gpg-sig":
				w, err := strconv.ParseBool(string(v))
				if err != nil {
					continue
				}
				cur.DataGPGSig = &w
			}
			if !more {
				break
			}
		}

		if cur.URL == "" {
			continue
		}

	INNER:
		for {
			switch z.Next() {
			case html.ErrorToken:
				break OUTER
			case html.TextToken:
				cur.Name = string(z.Text())

				// the text of the anchor tag MUST match the final path component (the filename)
				// of the URL. The URL SHOULD include a hash in the form of a URL fragment with
				// the following syntax: #<hashname>=<hashvalue>
				u, err := url.Parse(cur.URL)
				if err != nil {
					return nil, err
				}
				if base := filepath.Base(u.Path); base != cur.Name {
					return nil, errors.Newf("%s != %s: text does not match final path component", cur.Name, base)
				}

				files = append(files, cur)
				break INNER
			}
		}
	}
	if err := z.Err(); err != nil && err != io.EOF {
		return nil, err
	}
	return files, nil
}

// Download downloads a file located at url, respecting the rate limit.
func (c *Client) Download(ctx context.Context, url string) ([]byte, error) {
	startWait := time.Now()
	if err := c.limiter.Wait(ctx); err != nil {
		return nil, err
	}
	if d := time.Since(startWait); d > rateLimitingWaitThreshold {
		log15.Warn("client self-enforced API rate limit: request delayed longer than expected due to rate limit", "delay", d)
	}

	req, err := http.NewRequestWithContext(ctx, "GET", url, nil)
	if err != nil {
		return nil, err
	}

	b, err := c.do(req)
	if err != nil {
		return nil, errors.Wrap(err, "PyPI")
	}
	return b, nil
}

// https://peps.python.org/pep-0491/#file-format
type Wheel struct {
	Distribution string
	Version      string
	BuildTag     string
	PythonTag    string
	ABITag       string
	PlatformTag  string
}

// ToWheel parses a filename of a wheel according to the format specified in
// https://peps.python.org/pep-0491/#file-format
func ToWheel(name string) (*Wheel, error) {
	if e := path.Ext(name); e != ".whl" {
		return nil, errors.Errorf("%s does not conform to pep 491", name)
	} else {
		name = name[:len(name)-len(e)]
	}
	pcs := strings.Split(name, "-")
	switch len(pcs) {
	case 5:
		return &Wheel{
			Distribution: pcs[0],
			Version:      pcs[1],
			BuildTag:     "",
			PythonTag:    pcs[2],
			ABITag:       pcs[3],
			PlatformTag:  pcs[4],
		}, nil
	case 6:
		return &Wheel{
			Distribution: pcs[0],
			Version:      pcs[1],
			BuildTag:     pcs[2],
			PythonTag:    pcs[3],
			ABITag:       pcs[4],
			PlatformTag:  pcs[5],
		}, nil
	default:
		return nil, errors.Errorf("%s does not conform to pep 491", name)
	}
}

func (c *Client) get(ctx context.Context, project string) (respBody []byte, err error) {
	var (
		reqURL *url.URL
		req    *http.Request
	)

	for _, baseURL := range c.urls {
		startWait := time.Now()
		if err = c.limiter.Wait(ctx); err != nil {
			return nil, err
		}

		if d := time.Since(startWait); d > rateLimitingWaitThreshold {
			log15.Warn("client self-enforced API rate limit: request delayed longer than expected due to rate limit", "delay", d)
		}

		reqURL, err = url.Parse(baseURL)
		if err != nil {
			return nil, errors.Errorf("invalid proxy URL %q", baseURL)
		}

		// Go-http-client User-Agents are currently blocked from accessing /simple
		// resources without a trailing slash. This causes a redirect to the
		// canonicalized URL with the trailing slash. PyPI maintainers have been
		// struggling to handle a piece of software with this User-Agent overloading our
		// backends with requests resulting in redirects.
		reqURL.Path = path.Join(reqURL.Path, project) + "/"

		req, err = http.NewRequestWithContext(ctx, "GET", reqURL.String(), nil)
		if err != nil {
			return nil, err
		}

		respBody, err = c.do(req)
		if err == nil || !errcode.IsNotFound(err) {
			break
		}
	}

	return respBody, err
}

func (c *Client) do(req *http.Request) ([]byte, error) {
	resp, err := c.cli.Do(req)
	if err != nil {
		return nil, err
	}

	defer resp.Body.Close()

	bs, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	if resp.StatusCode != http.StatusOK {
		return nil, &Error{path: req.URL.Path, code: resp.StatusCode, message: string(bs)}
	}

	return bs, nil
}

type Error struct {
	path    string
	code    int
	message string
}

func (e *Error) Error() string {
	return fmt.Sprintf("bad proxy response with status code %d for %s: %s", e.code, e.path, e.message)
}

func (e *Error) NotFound() bool {
	return e.code == http.StatusNotFound
}

// https://peps.python.org/pep-0503/#normalized-names
var normalizer = lazyregexp.New("[-_.]+")

func normalize(path string) string {
	return strings.ToLower(normalizer.ReplaceAllLiteralString(path, "-"))
}
