package gitserver

import (
	"bytes"
	"context"
	"crypto/md5"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"io/ioutil"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/hashicorp/go-multierror"
	"github.com/neelance/parallel"
	"github.com/opentracing-contrib/go-stdlib/nethttp"
	opentracing "github.com/opentracing/opentracing-go"
	"github.com/opentracing/opentracing-go/ext"
	"github.com/pkg/errors"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/conf"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/gitolite"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver/protocol"
	"github.com/sourcegraph/sourcegraph/pkg/httpcli"
	"github.com/sourcegraph/sourcegraph/pkg/vcs"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

// DefaultClient is the default Client. Unless overwritten it is connected to servers specified by SRC_GIT_SERVERS.
var DefaultClient = NewClient(&http.Client{
	// nethttp.Transport will propagate opentracing spans
	Transport: &nethttp.Transport{
		RoundTripper: &http.Transport{
			// Default is 2, but we can send many concurrent requests
			MaxIdleConnsPerHost: 500,
		},
	},
})

// NewClient returns a new gitserver.Client instantiated with default arguments
// and httpcli.Doer.
func NewClient(cli httpcli.Doer) *Client {
	return &Client{
		Addrs: func(ctx context.Context) []string {
			return conf.Get().ServiceConnections.GitServers
		},
		HTTPClient:  cli,
		HTTPLimiter: parallel.NewRun(500),
		// Use the binary name for UserAgent. This should effectively identify
		// which service is making the request (excluding requests proxied via the
		// frontend internal API)
		UserAgent: filepath.Base(os.Args[0]),
	}
}

// Client is a gitserver client.
type Client struct {
	// HTTP client to use
	HTTPClient httpcli.Doer

	// Limits concurrency of outstanding HTTP posts
	HTTPLimiter *parallel.Run

	// Addrs is a function which should return the addresses for gitservers. It
	// is called each time a request is made. The function must be safe for
	// concurrent use. It may return different results at different times.
	Addrs func(ctx context.Context) []string

	// UserAgent is a string identifing who the client is. It will be logged in
	// the telemetry in gitserver.
	UserAgent string
}

// addrForRepo returns the gitserver address to use for the given repo name.
func (c *Client) addrForRepo(ctx context.Context, repo api.RepoName) string {
	repo = protocol.NormalizeRepo(repo) // in case the caller didn't already normalize it
	return c.addrForKey(ctx, string(repo))
}

// addrForKey returns the gitserver address to use for the given string key,
// which is hashed for sharding purposes.
func (c *Client) addrForKey(ctx context.Context, key string) string {
	addrs := c.Addrs(ctx)
	if len(addrs) == 0 {
		panic("unexpected state: no gitserver addresses")
	}
	sum := md5.Sum([]byte(key))
	serverIndex := binary.BigEndian.Uint64(sum[:]) % uint64(len(addrs))
	return addrs[serverIndex]
}

func (c *Cmd) sendExec(ctx context.Context) (_ io.ReadCloser, _ http.Header, errRes error) {
	repoName := protocol.NormalizeRepo(c.Repo.Name)

	span, ctx := opentracing.StartSpanFromContext(ctx, "Client.sendExec")
	defer func() {
		if errRes != nil {
			ext.Error.Set(span, true)
			span.SetTag("err", errRes.Error())
		}
		span.Finish()
	}()
	span.SetTag("request", "Exec")
	span.SetTag("repo", c.Repo.Name)
	span.SetTag("remoteURL", c.Repo.URL)
	span.SetTag("args", c.Args[1:])

	// Check that ctx is not expired.
	if err := ctx.Err(); err != nil {
		deadlineExceededCounter.Inc()
		return nil, nil, err
	}

	req := &protocol.ExecRequest{
		Repo:           repoName,
		URL:            c.Repo.URL,
		EnsureRevision: c.EnsureRevision,
		Args:           c.Args[1:],
	}
	resp, err := c.client.httpPost(ctx, repoName, "exec", req)
	if err != nil {
		return nil, nil, err
	}

	switch resp.StatusCode {
	case http.StatusOK:
		return resp.Body, resp.Trailer, nil

	case http.StatusNotFound:
		var payload protocol.NotFoundPayload
		if err := json.NewDecoder(resp.Body).Decode(&payload); err != nil {
			resp.Body.Close()
			return nil, nil, err
		}
		resp.Body.Close()
		return nil, nil, &vcs.RepoNotExistError{Repo: repoName, CloneInProgress: payload.CloneInProgress, CloneProgress: payload.CloneProgress}

	default:
		resp.Body.Close()
		return nil, nil, fmt.Errorf("unexpected status code: %d", resp.StatusCode)
	}
}

var deadlineExceededCounter = prometheus.NewCounter(prometheus.CounterOpts{
	Namespace: "src",
	Subsystem: "gitserver",
	Name:      "client_deadline_exceeded",
	Help:      "Times that Client.sendExec() returned context.DeadlineExceeded",
})

func init() {
	prometheus.MustRegister(deadlineExceededCounter)
}

// Cmd represents a command to be executed remotely.
type Cmd struct {
	client *Client

	Args           []string
	Repo           // the repository to execute the command in
	EnsureRevision string
	ExitStatus     int
}

// Repo represents a repository on gitserver. It contains the information necessary to identify and
// create/clone it.
type Repo struct {
	Name api.RepoName // the repository's name

	// URL is the repository's Git remote URL. If the gitserver already has cloned the repository,
	// this field is optional (it will use the last-used Git remote URL). If the repository is not
	// cloned on the gitserver, the request will fail.
	URL string
}

// Command creates a new Cmd. Command name must be 'git',
// otherwise it panics.
func (c *Client) Command(name string, arg ...string) *Cmd {
	if name != "git" {
		panic("gitserver: command name must be 'git'")
	}
	return &Cmd{
		client: c,
		Args:   append([]string{"git"}, arg...),
	}
}

// DividedOutput runs the command and returns its standard output and standard error.
func (c *Cmd) DividedOutput(ctx context.Context) ([]byte, []byte, error) {
	rc, trailer, err := c.sendExec(ctx)
	if err != nil {
		return nil, nil, err
	}

	stdout, err := ioutil.ReadAll(rc)
	rc.Close()
	if err != nil {
		return nil, nil, err
	}

	c.ExitStatus, err = strconv.Atoi(trailer.Get("X-Exec-Exit-Status"))
	if err != nil {
		return nil, nil, err
	}

	stderr := []byte(trailer.Get("X-Exec-Stderr"))
	if errorMsg := trailer.Get("X-Exec-Error"); errorMsg != "" {
		return stdout, stderr, errors.New(errorMsg)
	}

	return stdout, stderr, nil
}

// Run starts the specified command and waits for it to complete.
func (c *Cmd) Run(ctx context.Context) error {
	_, _, err := c.DividedOutput(ctx)
	return err
}

// Output runs the command and returns its standard output.
func (c *Cmd) Output(ctx context.Context) ([]byte, error) {
	stdout, _, err := c.DividedOutput(ctx)
	return stdout, err
}

// CombinedOutput runs the command and returns its combined standard output and standard error.
func (c *Cmd) CombinedOutput(ctx context.Context) ([]byte, error) {
	stdout, stderr, err := c.DividedOutput(ctx)
	return append(stdout, stderr...), err
}

func (c *Cmd) String() string { return fmt.Sprintf("%q", c.Args) }

// StdoutReader returns an io.ReadCloser of stdout of c. If the command has a
// non-zero return value, Read returns a non io.EOF error. Do not pass in a
// started command.
func StdoutReader(ctx context.Context, c *Cmd) (io.ReadCloser, error) {
	rc, trailer, err := c.sendExec(ctx)
	if err != nil {
		return nil, err
	}

	return &cmdReader{
		rc:      rc,
		trailer: trailer,
	}, nil
}

type cmdReader struct {
	rc      io.ReadCloser
	trailer http.Header
}

func (c *cmdReader) Read(p []byte) (int, error) {
	n, err := c.rc.Read(p)
	if err == io.EOF {
		stderr := c.trailer.Get("X-Exec-Stderr")
		if len(stderr) > 100 {
			stderr = stderr[:100] + "... (truncated)"
		}
		if errorMsg := c.trailer.Get("X-Exec-Error"); errorMsg != "" {
			return 0, fmt.Errorf("%s (stderr: %q)", errorMsg, stderr)
		}
		if exitStatus := c.trailer.Get("X-Exec-Exit-Status"); exitStatus != "0" {
			return 0, fmt.Errorf("non-zero exit status: %s (stderr: %q)", exitStatus, stderr)
		}
	}
	return n, err
}

func (c *cmdReader) Close() error {
	return c.rc.Close()
}

// WaitForGitServers retries a noop request to all gitserver instances until
// getting back a successful response.
func (c *Client) WaitForGitServers(ctx context.Context) {
	for {
		if errs := c.pingAll(ctx); len(errs) == 0 {
			return
		}
		time.Sleep(250 * time.Millisecond)
	}
}

func (c *Client) pingAll(ctx context.Context) []error {
	addrs := c.Addrs(ctx)

	ch := make(chan error, len(addrs))
	for _, addr := range addrs {
		go func(addr string) {
			ch <- c.ping(ctx, addr)
		}(addr)
	}

	errs := make([]error, 0, len(addrs))
	for i := 0; i < cap(ch); i++ {
		if err := <-ch; err != nil {
			errs = append(errs, err)
		}
	}

	return errs
}

func (c *Client) ping(ctx context.Context, addr string) error {
	req, err := http.NewRequest("GET", "http://"+addr+"/ping", nil)
	if err != nil {
		return err
	}

	resp, err := c.HTTPClient.Do(req.WithContext(ctx))
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("ping: bad HTTP response status %d", resp.StatusCode)
	}

	return nil
}

// ListGitolite lists Gitolite repositories.
func (c *Client) ListGitolite(ctx context.Context, gitoliteHost string) (list []*gitolite.Repo, err error) {
	// The gitserver calls the shared Gitolite server in response to this request, so
	// we need to only call a single gitserver (or else we'd get duplicate results).
	addr := c.addrForKey(ctx, gitoliteHost)
	req, err := http.NewRequest("GET", "http://"+addr+"/list-gitolite?gitolite="+url.QueryEscape(gitoliteHost), nil)
	if err != nil {
		return nil, err
	}

	resp, err := c.HTTPClient.Do(req.WithContext(ctx))
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	err = json.NewDecoder(resp.Body).Decode(&list)
	return list, err
}

// ListCloned lists all cloned repositories
func (c *Client) ListCloned(ctx context.Context) ([]string, error) {
	var (
		wg    sync.WaitGroup
		mu    sync.Mutex
		err   error
		repos []string
	)
	for _, addr := range c.Addrs(ctx) {
		wg.Add(1)
		go func(addr string) {
			defer wg.Done()
			r, e := c.doListOne(ctx, "?cloned", addr)
			mu.Lock()
			if e != nil {
				err = e
			}
			repos = append(repos, r...)
			mu.Unlock()
		}(addr)
	}
	wg.Wait()
	return repos, err
}

// GetGitolitePhabricatorMetadata returns Phabricator metadata for a Gitolite repository fetched via
// a user-provided command.
func (c *Client) GetGitolitePhabricatorMetadata(ctx context.Context, gitoliteHost string, repoName api.RepoName) (*protocol.GitolitePhabricatorMetadataResponse, error) {
	u := "http://" + c.addrForKey(ctx, gitoliteHost) +
		"/getGitolitePhabricatorMetadata?gitolite=" + url.QueryEscape(gitoliteHost) +
		"&repo=" + url.QueryEscape(string(repoName))

	req, err := http.NewRequest("GET", u, nil)
	if err != nil {
		return nil, err
	}

	resp, err := c.HTTPClient.Do(req.WithContext(ctx))
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var metadata protocol.GitolitePhabricatorMetadataResponse
	err = json.NewDecoder(resp.Body).Decode(&metadata)
	return &metadata, err
}

func (c *Client) doListOne(ctx context.Context, urlSuffix, addr string) ([]string, error) {
	req, err := http.NewRequest("GET", "http://"+addr+"/list"+urlSuffix, nil)
	if err != nil {
		return nil, err
	}

	resp, err := c.HTTPClient.Do(req.WithContext(ctx))
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var list []string
	err = json.NewDecoder(resp.Body).Decode(&list)
	return list, err
}

// RequestRepoUpdate is the new protocol endpoint for synchronous requests
// with more detailed responses. Do not use this if you are not repo-updater.
//
// Repo updates are not guaranteed to occur. If a repo has been updated
// recently (within the Since duration specified in the request), the
// update won't happen.
func (c *Client) RequestRepoUpdate(ctx context.Context, repo Repo, since time.Duration) (*protocol.RepoUpdateResponse, error) {
	req := &protocol.RepoUpdateRequest{
		Repo:  repo.Name,
		URL:   repo.URL,
		Since: since,
	}
	resp, err := c.httpPost(ctx, repo.Name, "repo-update", req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		body, _ := ioutil.ReadAll(io.LimitReader(resp.Body, 200))
		return nil, &url.Error{URL: resp.Request.URL.String(), Op: "RepoInfo", Err: fmt.Errorf("RepoInfo: http status %d: %s", resp.StatusCode, body)}
	}

	var info *protocol.RepoUpdateResponse
	err = json.NewDecoder(resp.Body).Decode(&info)
	return info, err
}

// MockIsRepoCloneable mocks (*Client).IsRepoCloneable for tests.
var MockIsRepoCloneable func(Repo) error

// IsRepoCloneable returns nil if the repository is cloneable.
func (c *Client) IsRepoCloneable(ctx context.Context, repo Repo) error {
	if MockIsRepoCloneable != nil {
		return MockIsRepoCloneable(repo)
	}

	req := &protocol.IsRepoCloneableRequest{
		Repo: repo.Name,
		URL:  repo.URL,
	}
	r, err := c.httpPost(ctx, repo.Name, "is-repo-cloneable", req)
	if err != nil {
		return err
	}
	defer r.Body.Close()
	body, err := ioutil.ReadAll(r.Body)
	if err != nil {
		return err
	}
	if r.StatusCode != http.StatusOK {
		return fmt.Errorf("gitserver error (status code %d): %s", r.StatusCode, string(body))
	}

	// Try unmarshaling new response format (?v=2) first.
	var resp protocol.IsRepoCloneableResponse
	if err := json.Unmarshal(body, &resp); err == nil {
		if resp.Cloneable {
			return nil
		}
		// Treat all 4xx errors as not found, since we have more relaxed
		// requirements on what a valid URL is we should treat bad requests,
		// etc as not found.
		notFound := strings.Contains(resp.Reason, "not found") || strings.Contains(resp.Reason, "The requested URL returned error: 4")
		return &RepoNotCloneableErr{repo: repo, reason: resp.Reason, notFound: notFound}
	}

	// Backcompat (gitserver is old, does not recognize ?v=2)
	//
	// TODO(sqs): remove when unneeded
	var cloneable bool
	if err := json.Unmarshal(body, &cloneable); err != nil {
		return err
	}
	if cloneable {
		return nil
	}
	return &RepoNotCloneableErr{}
}

// RepoNotCloneableErr is the error that happens when a repository can not be cloned.
type RepoNotCloneableErr struct {
	repo     Repo
	reason   string
	notFound bool
}

// NotFound returns true if the repo could not be cloned because it wasn't found.
// This may be because the repo doesn't exist, or because the repo is private and
// there are insufficient permissions.
func (e *RepoNotCloneableErr) NotFound() bool {
	return e.notFound
}

func (e *RepoNotCloneableErr) Error() string {
	return fmt.Sprintf("repo not found (name=%s url=%s notfound=%v) because %s", e.repo.Name, e.repo.URL, e.notFound, e.reason)
}

func (c *Client) IsRepoCloned(ctx context.Context, repo api.RepoName) (bool, error) {
	req := &protocol.IsRepoClonedRequest{
		Repo: repo,
	}
	resp, err := c.httpPost(ctx, repo, "is-repo-cloned", req)
	if err != nil {
		return false, err
	}
	// no need to defer, we aren't using the body.
	resp.Body.Close()
	var cloned bool
	if resp.StatusCode == http.StatusOK {
		cloned = true
	}
	return cloned, nil
}

// RepoInfo retrieves information about one or more repositories on gitserver.
//
// The repository not existing is not an error; in that case, RepoInfoResponse.Results[i].Cloned
// will be false and the error will be nil.
//
// If multiple errors occurred, an incomplete result is returned along with a
// *multierror.Error.
func (c *Client) RepoInfo(ctx context.Context, repos ...api.RepoName) (*protocol.RepoInfoResponse, error) {
	numPossibleShards := len(c.Addrs(ctx))
	shards := make(map[string]*protocol.RepoInfoRequest, (len(repos)/numPossibleShards)*2) // 2x because it may not be a perfect division

	for _, r := range repos {
		addr := c.addrForRepo(ctx, r)
		shard := shards[addr]

		if shard == nil {
			shard = new(protocol.RepoInfoRequest)
			shards[addr] = shard
		}

		shard.Repos = append(shard.Repos, r)
	}

	type op struct {
		req *protocol.RepoInfoRequest
		res *protocol.RepoInfoResponse
		err error
	}

	ch := make(chan op, len(shards))
	for _, req := range shards {
		go func(o op) {
			var resp *http.Response
			resp, o.err = c.httpPost(ctx, o.req.Repos[0], "repos", o.req)
			if o.err != nil {
				ch <- o
				return
			}

			defer resp.Body.Close()
			if resp.StatusCode != http.StatusOK {
				o.err = &url.Error{
					URL: resp.Request.URL.String(),
					Op:  "RepoInfo",
					Err: errors.Errorf("RepoInfo: http status %d", resp.StatusCode),
				}
				ch <- o
				return // we never get an error status code AND result
			}

			o.res = new(protocol.RepoInfoResponse)
			o.err = json.NewDecoder(resp.Body).Decode(o.res)
			ch <- o
		}(op{req: req})
	}

	err := new(multierror.Error)
	res := protocol.RepoInfoResponse{
		Results: make(map[api.RepoName]*protocol.RepoInfo),
	}

	for i := 0; i < cap(ch); i++ {
		o := <-ch

		if o.err != nil {
			err = multierror.Append(err, o.err)
			continue
		}

		for repo, info := range o.res.Results {
			res.Results[repo] = info
		}
	}

	return &res, err.ErrorOrNil()
}

// Remove removes the repository clone from gitserver.
func (c *Client) Remove(ctx context.Context, repo api.RepoName) error {
	req := &protocol.RepoDeleteRequest{
		Repo: repo,
	}
	resp, err := c.httpPost(ctx, repo, "delete", req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		// best-effort inclusion of body in error message
		body, _ := ioutil.ReadAll(io.LimitReader(resp.Body, 200))
		return &url.Error{URL: resp.Request.URL.String(), Op: "RepoRemove", Err: fmt.Errorf("RepoRemove: http status %d: %s", resp.StatusCode, string(body))}
	}
	return nil
}

// httpPost performs a POST request to a gitserver, sharding based on the given
// repo name (the repo name is otherwise not used).
func (c *Client) httpPost(ctx context.Context, repo api.RepoName, method string, payload interface{}) (resp *http.Response, err error) {
	span, ctx := opentracing.StartSpanFromContext(ctx, "Client.httpPost")
	defer func() {
		if err != nil {
			ext.Error.Set(span, true)
			span.SetTag("err", err.Error())
		}
		span.Finish()
	}()

	reqBody, err := json.Marshal(payload)
	if err != nil {
		return nil, err
	}

	addr := c.addrForRepo(ctx, repo)
	req, err := http.NewRequest("POST", "http://"+addr+"/"+method, bytes.NewReader(reqBody))
	if err != nil {
		return nil, err
	}

	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("User-Agent", c.UserAgent)
	req = req.WithContext(ctx)

	if c.HTTPLimiter != nil {
		c.HTTPLimiter.Acquire()
		defer c.HTTPLimiter.Release()
		span.LogKV("event", "Acquired HTTP limiter")
	}

	req, ht := nethttp.TraceRequest(span.Tracer(), req,
		nethttp.OperationName("Gitserver Client"),
		nethttp.ClientTrace(false))
	defer ht.Finish()

	return c.HTTPClient.Do(req)
}

func (c *Client) CreateCommitFromPatch(ctx context.Context, req protocol.CreateCommitFromPatchRequest) (string, error) {
	resp, err := c.httpPost(ctx, req.Repo, "create-commit-from-patch", req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		b, _ := ioutil.ReadAll(resp.Body)
		log15.Warn("gitserver create-commit-from-patch error:", string(b))

		return "", &url.Error{URL: resp.Request.URL.String(), Op: "CreateCommitFromPatch", Err: fmt.Errorf("CreateCommitFromPatch: http status %d %s", resp.StatusCode, string(b))}
	}

	var res protocol.CreatePatchFromPatchResponse

	return res.Rev, json.NewDecoder(resp.Body).Decode(&res)
}
