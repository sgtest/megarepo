package gitserver

import (
	"bytes"
	"context"
	"crypto/md5"
	"encoding/binary"
	"encoding/gob"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/cespare/xxhash/v2"
	"github.com/inconshreveable/log15"
	"github.com/neelance/parallel"
	"github.com/opentracing-contrib/go-stdlib/nethttp"
	"github.com/opentracing/opentracing-go/ext"
	otlog "github.com/opentracing/opentracing-go/log"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"

	"github.com/sourcegraph/go-rendezvous"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitolite"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/protocol"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var (
	clientFactory  = httpcli.NewInternalClientFactory("gitserver")
	defaultDoer, _ = clientFactory.Doer()
	defaultLimiter = parallel.NewRun(500)

	// DefaultClient is the default Client. Unless overwritten it is connected to servers specified by SRC_GIT_SERVERS.
	DefaultClient Client = NewClient(defaultDoer)
)

var ClientMocks, emptyClientMocks struct {
	GetObject func(repo api.RepoName, objectName string) (*gitdomain.GitObject, error)
}

// ResetClientMocks clears the mock functions set on Mocks (so that subsequent
// tests don't inadvertently use them).
func ResetClientMocks() {
	ClientMocks = emptyClientMocks
}

// NewClient returns a new gitserver.Client instantiated with default arguments
// and httpcli.Doer.
func NewClient(cli httpcli.Doer) *ClientImplementor {
	return &ClientImplementor{
		addrs: func() []string {
			return conf.Get().ServiceConnections().GitServers
		},
		HTTPClient:  cli,
		HTTPLimiter: defaultLimiter,
		// Use the binary name for UserAgent. This should effectively identify
		// which service is making the request (excluding requests proxied via the
		// frontend internal API)
		UserAgent: filepath.Base(os.Args[0]),
	}
}

func NewTestClient(cli httpcli.Doer, addrs []string) *ClientImplementor {
	return &ClientImplementor{
		addrs: func() []string {
			return addrs
		},
		HTTPClient:  cli,
		HTTPLimiter: parallel.NewRun(500),
		// Use the binary name for UserAgent. This should effectively identify
		// which service is making the request (excluding requests proxied via the
		// frontend internal API)
		UserAgent: filepath.Base(os.Args[0]),
	}
}

// ClientImplementor is a gitserver client.
type ClientImplementor struct {
	// HTTP client to use
	HTTPClient httpcli.Doer

	// Limits concurrency of outstanding HTTP posts
	HTTPLimiter *parallel.Run

	// addrs is a function which should return the addresses for gitservers. It
	// is called each time a request is made. The function must be safe for
	// concurrent use. It may return different results at different times.
	addrs func() []string

	// UserAgent is a string identifying who the client is. It will be logged in
	// the telemetry in gitserver.
	UserAgent string
}

//go:generate ../../dev/mockgen.sh github.com/sourcegraph/sourcegraph/internal/gitserver -i Client -o mock_client.go
type Client interface {
	// AddrForRepo returns the gitserver address to use for the given repo name.
	AddrForRepo(api.RepoName) string

	Addrs() []string

	// Archive produces an archive from a Git repository.
	Archive(context.Context, api.RepoName, ArchiveOptions) (io.ReadCloser, error)

	// ArchiveURL returns a URL from which an archive of the given Git repository can
	// be downloaded from.
	ArchiveURL(api.RepoName, ArchiveOptions) *url.URL

	// Command creates a new Cmd. Command name must be 'git', otherwise it panics.
	Command(name string, args ...string) *Cmd

	// CreateCommitFromPatch will attempt to create a commit from a patch
	// If possible, the error returned will be of type protocol.CreateCommitFromPatchError
	CreateCommitFromPatch(context.Context, protocol.CreateCommitFromPatchRequest) (string, error)

	// GetGitolitePhabricatorMetadata returns Phabricator metadata for a Gitolite repository fetched via
	// a user-provided command.
	GetGitolitePhabricatorMetadata(_ context.Context, gitoliteHost string, _ api.RepoName) (*protocol.GitolitePhabricatorMetadataResponse, error)

	// GetObject fetches git object data in the supplied repo
	GetObject(_ context.Context, _ api.RepoName, objectName string) (*gitdomain.GitObject, error)

	// IsRepoCloneable returns nil if the repository is cloneable.
	IsRepoCloneable(context.Context, api.RepoName) error

	IsRepoCloned(context.Context, api.RepoName) (bool, error)

	// ListCloned lists all cloned repositories
	ListCloned(context.Context) ([]string, error)

	// ListGitolite lists Gitolite repositories.
	ListGitolite(_ context.Context, gitoliteHost string) ([]*gitolite.Repo, error)

	// P4Exec sends a p4 command with given arguments and returns an io.ReadCloser for the output.
	P4Exec(_ context.Context, host, user, password string, args ...string) (io.ReadCloser, http.Header, error)

	// Remove removes the repository clone from gitserver.
	Remove(context.Context, api.RepoName) error

	// RendezvousAddrForRepo returns the gitserver address to use for the given
	// repo name using the Rendezvous hashing scheme.
	RendezvousAddrForRepo(api.RepoName) string

	RepoCloneProgress(context.Context, ...api.RepoName) (*protocol.RepoCloneProgressResponse, error)

	// RepoInfo retrieves information about one or more repositories on gitserver.
	//
	// The repository not existing is not an error; in that case, RepoInfoResponse.Results[i].Cloned
	// will be false and the error will be nil.
	//
	// If multiple errors occurred, an incomplete result is returned along with an
	// error.errors.
	RepoInfo(context.Context, ...api.RepoName) (*protocol.RepoInfoResponse, error)

	// ReposStats will return a map of the ReposStats for each gitserver in a
	// map. If we fail to fetch a stat from a gitserver, it won't be in the
	// returned map and will be appended to the error. If no errors occur err will
	// be nil.
	//
	// Note: If the statistics for a gitserver have not been computed, the
	// UpdatedAt field will be zero. This can happen for new gitservers.
	ReposStats(context.Context) (map[string]*protocol.ReposStats, error)

	// RequestRepoMigrate is effectively RequestRepoUpdate but with some additional metadata to aid our
	// migration of gitserver repos to the rendezvous hashing scheme.
	RequestRepoMigrate(context.Context, api.RepoName) (*protocol.RepoUpdateResponse, error)

	// RequestRepoUpdate is the new protocol endpoint for synchronous requests
	// with more detailed responses. Do not use this if you are not repo-updater.
	//
	// Repo updates are not guaranteed to occur. If a repo has been updated
	// recently (within the Since duration specified in the request), the
	// update won't happen.
	RequestRepoUpdate(context.Context, api.RepoName, time.Duration) (*protocol.RepoUpdateResponse, error)

	// Search executes a search as specified by args, streaming the results as
	// it goes by calling onMatches with each set of results it receives in
	// response.
	Search(_ context.Context, _ *protocol.SearchRequest, onMatches func([]protocol.CommitMatch)) (limitHit bool, _ error)
}

func (c *ClientImplementor) Addrs() []string {
	return c.addrs()
}

func (c *ClientImplementor) AddrForRepo(repo api.RepoName) string {
	addrs := c.addrs()
	if len(addrs) == 0 {
		panic("unexpected state: no gitserver addresses")
	}
	return AddrForRepo(repo, addrs)
}

func (c *ClientImplementor) RendezvousAddrForRepo(repo api.RepoName) string {
	addrs := c.addrs()
	if len(addrs) == 0 {
		panic("unexpected state: no gitserver addresses")
	}

	return RendezvousAddrForRepo(repo, addrs)
}

// addrForKey returns the gitserver address to use for the given string key,
// which is hashed for sharding purposes.
func (c *ClientImplementor) addrForKey(key string) string {
	addrs := c.addrs()
	if len(addrs) == 0 {
		panic("unexpected state: no gitserver addresses")
	}
	return addrForKey(key, addrs)
}

var addForRepoInvoked = promauto.NewCounter(prometheus.CounterOpts{
	Name: "src_gitserver_addr_for_repo_invoked",
	Help: "Number of times gitserver.AddrForRepo was invoked",
})

// AddrForRepo returns the gitserver address to use for the given repo name.
// It should never be called with an empty slice.
func AddrForRepo(repo api.RepoName, addrs []string) string {
	addForRepoInvoked.Inc()

	repo = protocol.NormalizeRepo(repo) // in case the caller didn't already normalize it
	return addrForKey(string(repo), addrs)
}

// RendezvousAddrForRepo returns the gitserver address to use for the given repo name using the
// Rendezvous hashing scheme.
//
// It should never be called with an empty slice.
func RendezvousAddrForRepo(repo api.RepoName, addrs []string) string {
	r := rendezvous.New(addrs, xxhash.Sum64String)
	return r.Lookup(string(protocol.NormalizeRepo(repo)))
}

// addrForKey returns the gitserver address to use for the given string key,
// which is hashed for sharding purposes.
func addrForKey(key string, addrs []string) string {
	sum := md5.Sum([]byte(key))
	serverIndex := binary.BigEndian.Uint64(sum[:]) % uint64(len(addrs))
	return addrs[serverIndex]
}

// ArchiveOptions contains options for the Archive func.
type ArchiveOptions struct {
	Treeish string   // the tree or commit to produce an archive for
	Format  string   // format of the resulting archive (usually "tar" or "zip")
	Paths   []string // if nonempty, only include these paths
}

// archiveReader wraps the StdoutReader yielded by gitserver's
// Cmd.StdoutReader with one that knows how to report a repository-not-found
// error more carefully.
type archiveReader struct {
	base io.ReadCloser
	repo api.RepoName
	spec string
}

// Read checks the known output behavior of the StdoutReader.
func (a *archiveReader) Read(p []byte) (int, error) {
	n, err := a.base.Read(p)
	if err != nil {
		// handle the special case where git archive failed because of an invalid spec
		if strings.Contains(err.Error(), "Not a valid object") {
			return 0, &gitdomain.RevisionNotFoundError{Repo: a.repo, Spec: a.spec}
		}
	}
	return n, err
}

func (a *archiveReader) Close() error {
	return a.base.Close()
}

// ArchiveURL returns a URL from which an archive of the given Git repository can
// be downloaded from.
func (c *ClientImplementor) ArchiveURL(repo api.RepoName, opt ArchiveOptions) *url.URL {
	q := url.Values{
		"repo":    {string(repo)},
		"treeish": {opt.Treeish},
		"format":  {opt.Format},
	}

	for _, path := range opt.Paths {
		q.Add("path", path)
	}

	return &url.URL{
		Scheme:   "http",
		Host:     c.AddrForRepo(repo),
		Path:     "/archive",
		RawQuery: q.Encode(),
	}
}

func (c *ClientImplementor) Archive(ctx context.Context, repo api.RepoName, opt ArchiveOptions) (_ io.ReadCloser, err error) {
	span, ctx := ot.StartSpanFromContext(ctx, "Git: Archive")
	span.SetTag("Repo", repo)
	span.SetTag("Treeish", opt.Treeish)
	defer func() {
		if err != nil {
			ext.Error.Set(span, true)
			span.LogFields(otlog.Error(err))
		}
		span.Finish()
	}()

	// Check that ctx is not expired.
	if err := ctx.Err(); err != nil {
		deadlineExceededCounter.Inc()
		return nil, err
	}

	u := c.ArchiveURL(repo, opt)
	resp, err := c.do(ctx, repo, "POST", u.String(), nil)
	if err != nil {
		return nil, err
	}

	switch resp.StatusCode {
	case http.StatusOK:
		return &archiveReader{
			base: &cmdReader{
				rc:      resp.Body,
				trailer: resp.Trailer,
			},
			repo: repo,
			spec: opt.Treeish,
		}, nil
	case http.StatusNotFound:
		var payload protocol.NotFoundPayload
		if err := json.NewDecoder(resp.Body).Decode(&payload); err != nil {
			resp.Body.Close()
			return nil, err
		}
		resp.Body.Close()
		return nil, &badRequestError{
			error: &gitdomain.RepoNotExistError{
				Repo:            repo,
				CloneInProgress: payload.CloneInProgress,
				CloneProgress:   payload.CloneProgress,
			},
		}
	default:
		resp.Body.Close()
		return nil, errors.Errorf("unexpected status code: %d", resp.StatusCode)
	}
}

type badRequestError struct{ error }

func (e badRequestError) BadRequest() bool { return true }

func (c *Cmd) sendExec(ctx context.Context) (_ io.ReadCloser, _ http.Header, errRes error) {
	repoName := protocol.NormalizeRepo(c.Repo)

	span, ctx := ot.StartSpanFromContext(ctx, "Client.sendExec")
	defer func() {
		if errRes != nil {
			ext.Error.Set(span, true)
			span.SetTag("err", errRes.Error())
		}
		span.Finish()
	}()
	span.SetTag("request", "Exec")
	span.SetTag("repo", c.Repo)
	span.SetTag("args", c.Args[1:])

	// Check that ctx is not expired.
	if err := ctx.Err(); err != nil {
		deadlineExceededCounter.Inc()
		return nil, nil, err
	}

	req := &protocol.ExecRequest{
		Repo:           repoName,
		EnsureRevision: c.EnsureRevision,
		Args:           c.Args[1:],
		NoTimeout:      c.NoTimeout,
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
		return nil, nil, &gitdomain.RepoNotExistError{Repo: repoName, CloneInProgress: payload.CloneInProgress, CloneProgress: payload.CloneProgress}

	default:
		resp.Body.Close()
		return nil, nil, errors.Errorf("unexpected status code: %d", resp.StatusCode)
	}
}

func (c *ClientImplementor) Search(ctx context.Context, args *protocol.SearchRequest, onMatches func([]protocol.CommitMatch)) (limitHit bool, err error) {
	span, ctx := ot.StartSpanFromContext(ctx, "GitserverClient.Search")
	span.SetTag("repo", string(args.Repo))
	span.SetTag("query", args.Query.String())
	span.SetTag("diff", args.IncludeDiff)
	span.SetTag("limit", args.Limit)
	defer func() {
		if err != nil {
			ext.Error.Set(span, true)
			span.SetTag("err", err.Error())
		}
		span.Finish()
	}()

	repoName := protocol.NormalizeRepo(args.Repo)

	protocol.RegisterGob()
	var buf bytes.Buffer
	enc := gob.NewEncoder(&buf)
	if err := enc.Encode(args); err != nil {
		return false, err
	}

	uri := "http://" + c.AddrForRepo(repoName) + "/search"
	resp, err := c.do(ctx, repoName, "POST", uri, buf.Bytes())
	if err != nil {
		return false, err
	}
	defer resp.Body.Close()

	var (
		decodeErr error
		eventDone protocol.SearchEventDone
	)
	dec := StreamSearchDecoder{
		OnMatches: func(e protocol.SearchEventMatches) {
			onMatches(e)
		},
		OnDone: func(e protocol.SearchEventDone) {
			eventDone = e
		},
		OnUnknown: func(event, _ []byte) {
			decodeErr = errors.Errorf("unknown event %s", event)
		},
	}

	if err := dec.ReadAll(resp.Body); err != nil {
		return false, err
	}

	if decodeErr != nil {
		return false, decodeErr
	}

	return eventDone.LimitHit, eventDone.Err()
}

func (c *ClientImplementor) P4Exec(ctx context.Context, host, user, password string, args ...string) (_ io.ReadCloser, _ http.Header, errRes error) {
	span, ctx := ot.StartSpanFromContext(ctx, "Client.P4Exec")
	defer func() {
		if errRes != nil {
			ext.Error.Set(span, true)
			span.SetTag("err", errRes.Error())
		}
		span.Finish()
	}()
	span.SetTag("request", "P4Exec")
	span.SetTag("host", host)
	span.SetTag("args", args)

	// Check that ctx is not expired.
	if err := ctx.Err(); err != nil {
		deadlineExceededCounter.Inc()
		return nil, nil, err
	}

	req := &protocol.P4ExecRequest{
		P4Port:   host,
		P4User:   user,
		P4Passwd: password,
		Args:     args,
	}
	resp, err := c.httpPost(ctx, "", "p4-exec", req)
	if err != nil {
		return nil, nil, err
	}

	switch resp.StatusCode {
	case http.StatusOK:
		return resp.Body, resp.Trailer, nil

	default:
		// Read response body at best effort
		body, _ := io.ReadAll(resp.Body)
		_ = resp.Body.Close()
		return nil, nil, errors.Errorf("unexpected status code: %d - %s", resp.StatusCode, body)
	}
}

var deadlineExceededCounter = promauto.NewCounter(prometheus.CounterOpts{
	Name: "src_gitserver_client_deadline_exceeded",
	Help: "Times that Client.sendExec() returned context.DeadlineExceeded",
})

// Cmd represents a command to be executed remotely.
type Cmd struct {
	client *ClientImplementor

	Args           []string
	Repo           api.RepoName // the repository to execute the command in
	EnsureRevision string
	ExitStatus     int
	NoTimeout      bool
}

func (c *ClientImplementor) Command(name string, arg ...string) *Cmd {
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

	stdout, err := io.ReadAll(rc)
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

func (c *Cmd) DisableTimeout() {
	c.NoTimeout = true
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
			return 0, errors.Errorf("%s (stderr: %q)", errorMsg, stderr)
		}
		if exitStatus := c.trailer.Get("X-Exec-Exit-Status"); exitStatus != "0" {
			return 0, errors.Errorf("non-zero exit status: %s (stderr: %q)", exitStatus, stderr)
		}
	}
	return n, err
}

func (c *cmdReader) Close() error {
	return c.rc.Close()
}

func (c *ClientImplementor) ListGitolite(ctx context.Context, gitoliteHost string) (list []*gitolite.Repo, err error) {
	// The gitserver calls the shared Gitolite server in response to this request, so
	// we need to only call a single gitserver (or else we'd get duplicate results).
	addr := c.addrForKey(gitoliteHost)
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

func (c *ClientImplementor) ListCloned(ctx context.Context) ([]string, error) {
	var (
		wg    sync.WaitGroup
		mu    sync.Mutex
		err   error
		repos []string
	)
	addrs := c.addrs()
	for _, addr := range addrs {
		wg.Add(1)
		go func(addr string) {
			defer wg.Done()
			r, e := c.doListOne(ctx, "?cloned", addr)

			// Only include repos that belong on addr.
			if len(r) > 0 {
				filtered := r[:0]
				for _, repo := range r {
					if addrForKey(repo, addrs) == addr {
						filtered = append(filtered, repo)
					}
				}
				r = filtered
			}
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

func (c *ClientImplementor) GetGitolitePhabricatorMetadata(ctx context.Context, gitoliteHost string, repoName api.RepoName) (*protocol.GitolitePhabricatorMetadataResponse, error) {
	u := "http://" + c.addrForKey(gitoliteHost) +
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

func (c *ClientImplementor) doListOne(ctx context.Context, urlSuffix, addr string) ([]string, error) {
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

func (c *ClientImplementor) RequestRepoUpdate(ctx context.Context, repo api.RepoName, since time.Duration) (*protocol.RepoUpdateResponse, error) {
	req := &protocol.RepoUpdateRequest{
		Repo:  repo,
		Since: since,
	}
	resp, err := c.httpPost(ctx, repo, "repo-update", req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(io.LimitReader(resp.Body, 200))
		return nil, &url.Error{URL: resp.Request.URL.String(), Op: "RepoInfo", Err: errors.Errorf("RepoInfo: http status %d: %s", resp.StatusCode, body)}
	}

	var info *protocol.RepoUpdateResponse
	err = json.NewDecoder(resp.Body).Decode(&info)
	return info, err
}

func (c *ClientImplementor) RequestRepoMigrate(ctx context.Context, repo api.RepoName) (*protocol.RepoUpdateResponse, error) {
	// We do not need to set a value for the attribute "Since" because the repo is not expected to
	// be cloned at the new gitserver instance. And for not cloned repos, this attribute is already
	// ignored.
	req := &protocol.RepoUpdateRequest{
		Repo:        repo,
		MigrateFrom: c.AddrForRepo(repo),
	}

	// We set "uri" to the HTTP URL of the gitserver instance that should be the new owner of this
	// "repo" based on the rendezvous hashing scheme. This way, when the gitserver instance receives
	// the request at /repo-update, it will treat it as a new clone operation and attempt to clone
	// the repo from the URL set in MigrateFrom - the gitserver instance that owns this repo based
	// on the existing hashing scheme.
	uri := "http://" + c.RendezvousAddrForRepo(repo) + "/repo-update"
	resp, err := c.httpPostWithURI(ctx, repo, uri, req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(io.LimitReader(resp.Body, 200))
		return nil, &url.Error{
			URL: resp.Request.URL.String(),
			Op:  "RepoMigrate",
			Err: errors.Errorf("RepoMigrate: http status %d: %s", resp.StatusCode, body),
		}
	}

	var info *protocol.RepoUpdateResponse
	err = json.NewDecoder(resp.Body).Decode(&info)

	return info, err
}

// MockIsRepoCloneable mocks (*Client).IsRepoCloneable for tests.
var MockIsRepoCloneable func(api.RepoName) error

func (c *ClientImplementor) IsRepoCloneable(ctx context.Context, repo api.RepoName) error {
	if MockIsRepoCloneable != nil {
		return MockIsRepoCloneable(repo)
	}

	req := &protocol.IsRepoCloneableRequest{
		Repo: repo,
	}
	r, err := c.httpPost(ctx, repo, "is-repo-cloneable", req)
	if err != nil {
		return err
	}
	defer r.Body.Close()
	body, err := io.ReadAll(r.Body)
	if err != nil {
		return err
	}
	if r.StatusCode != http.StatusOK {
		return errors.Errorf("gitserver error (status code %d): %s", r.StatusCode, string(body))
	}

	var resp protocol.IsRepoCloneableResponse
	if err := json.Unmarshal(body, &resp); err != nil {
		return err
	}

	if resp.Cloneable {
		return nil
	}

	// Treat all 4xx errors as not found, since we have more relaxed
	// requirements on what a valid URL is we should treat bad requests,
	// etc as not found.
	notFound := strings.Contains(resp.Reason, "not found") || strings.Contains(resp.Reason, "The requested URL returned error: 4")
	return &RepoNotCloneableErr{repo: repo, reason: resp.Reason, notFound: notFound}
}

// RepoNotCloneableErr is the error that happens when a repository can not be cloned.
type RepoNotCloneableErr struct {
	repo     api.RepoName
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
	return fmt.Sprintf("repo not found (name=%s notfound=%v) because %s", e.repo, e.notFound, e.reason)
}

func (c *ClientImplementor) IsRepoCloned(ctx context.Context, repo api.RepoName) (bool, error) {
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

func (c *ClientImplementor) RepoCloneProgress(ctx context.Context, repos ...api.RepoName) (*protocol.RepoCloneProgressResponse, error) {
	numPossibleShards := len(c.addrs())
	shards := make(map[string]*protocol.RepoCloneProgressRequest, (len(repos)/numPossibleShards)*2) // 2x because it may not be a perfect division

	for _, r := range repos {
		addr := c.AddrForRepo(r)
		shard := shards[addr]

		if shard == nil {
			shard = new(protocol.RepoCloneProgressRequest)
			shards[addr] = shard
		}

		shard.Repos = append(shard.Repos, r)
	}

	type op struct {
		req *protocol.RepoCloneProgressRequest
		res *protocol.RepoCloneProgressResponse
		err error
	}

	ch := make(chan op, len(shards))
	for _, req := range shards {
		go func(o op) {
			var resp *http.Response
			resp, o.err = c.httpPost(ctx, o.req.Repos[0], "repo-clone-progress", o.req)
			if o.err != nil {
				ch <- o
				return
			}

			defer resp.Body.Close()
			if resp.StatusCode != http.StatusOK {
				o.err = &url.Error{
					URL: resp.Request.URL.String(),
					Op:  "RepoCloneProgress",
					Err: errors.Errorf("RepoCloneProgress: http status %d", resp.StatusCode),
				}
				ch <- o
				return // we never get an error status code AND result
			}

			o.res = new(protocol.RepoCloneProgressResponse)
			o.err = json.NewDecoder(resp.Body).Decode(o.res)
			ch <- o
		}(op{req: req})
	}

	var err error
	res := protocol.RepoCloneProgressResponse{
		Results: make(map[api.RepoName]*protocol.RepoCloneProgress),
	}

	for i := 0; i < cap(ch); i++ {
		o := <-ch

		if o.err != nil {
			err = errors.Append(err, o.err)
			continue
		}

		for repo, info := range o.res.Results {
			res.Results[repo] = info
		}
	}

	return &res, err
}

func (c *ClientImplementor) RepoInfo(ctx context.Context, repos ...api.RepoName) (*protocol.RepoInfoResponse, error) {
	numPossibleShards := len(c.addrs())
	shards := make(map[string]*protocol.RepoInfoRequest, (len(repos)/numPossibleShards)*2) // 2x because it may not be a perfect division

	for _, r := range repos {
		addr := c.AddrForRepo(r)
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

	var err error
	res := protocol.RepoInfoResponse{
		Results: make(map[api.RepoName]*protocol.RepoInfo),
	}

	for i := 0; i < cap(ch); i++ {
		o := <-ch

		if o.err != nil {
			err = errors.Append(err, o.err)
			continue
		}

		for repo, info := range o.res.Results {
			res.Results[repo] = info
		}
	}

	return &res, err
}

func (c *ClientImplementor) ReposStats(ctx context.Context) (map[string]*protocol.ReposStats, error) {
	stats := map[string]*protocol.ReposStats{}
	var allErr error
	for _, addr := range c.addrs() {
		stat, err := c.doReposStats(ctx, addr)
		if err != nil {
			allErr = errors.Append(allErr, err)
		} else {
			stats[addr] = stat
		}
	}
	return stats, allErr
}

func (c *ClientImplementor) doReposStats(ctx context.Context, addr string) (*protocol.ReposStats, error) {
	req, err := http.NewRequestWithContext(ctx, "GET", "http://"+addr+"/repos-stats", nil)
	if err != nil {
		return nil, err
	}

	resp, err := c.HTTPClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	var stats protocol.ReposStats
	err = json.NewDecoder(resp.Body).Decode(&stats)
	if err != nil {
		return nil, err
	}

	return &stats, nil
}

func (c *ClientImplementor) Remove(ctx context.Context, repo api.RepoName) error {
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
		body, _ := io.ReadAll(io.LimitReader(resp.Body, 200))
		return &url.Error{URL: resp.Request.URL.String(), Op: "RepoRemove", Err: errors.Errorf("RepoRemove: http status %d: %s", resp.StatusCode, string(body))}
	}
	return nil
}

// httpPost will apply the MD5 hashing scheme on the repo name to determine the gitserver instance
// to which the HTTP POST request is sent. To use the rendezvous hashing scheme, see
// httpPostWithURI.
func (c *ClientImplementor) httpPost(ctx context.Context, repo api.RepoName, op string, payload interface{}) (resp *http.Response, err error) {
	b, err := json.Marshal(payload)
	if err != nil {
		return nil, err
	}

	uri := "http://" + c.AddrForRepo(repo) + "/" + op
	return c.do(ctx, repo, "POST", uri, b)
}

// httpPostWithURI does not apply any transformations to the given URI. This allows the consumer to
// use the predetermined hashing scheme (md5 or rendezvous) of their choice to derive the gitserver
// instance to which the HTTP POST request is sent.
func (c *ClientImplementor) httpPostWithURI(ctx context.Context, repo api.RepoName, uri string, payload interface{}) (resp *http.Response, err error) {
	b, err := json.Marshal(payload)
	if err != nil {
		return nil, err
	}

	return c.do(ctx, repo, "POST", uri, b)
}

//nolint:unparam // unparam complains that `method` always has same value across call-sites, but that's OK
// do performs a request to a gitserver instance based on the address in the uri argument.
func (c *ClientImplementor) do(ctx context.Context, repo api.RepoName, method, uri string, payload []byte) (resp *http.Response, err error) {
	parsedURL, err := url.ParseRequestURI(uri)
	if err != nil {
		return nil, errors.Wrap(err, "do")
	}

	span, ctx := ot.StartSpanFromContext(ctx, "Client.do")
	defer func() {
		span.LogKV("repo", string(repo), "method", method, "path", parsedURL.Path)
		if err != nil {
			ext.Error.Set(span, true)
			span.SetTag("err", err.Error())
		}
		span.Finish()
	}()

	req, err := http.NewRequest(method, uri, bytes.NewReader(payload))
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

func (c *ClientImplementor) CreateCommitFromPatch(ctx context.Context, req protocol.CreateCommitFromPatchRequest) (string, error) {
	resp, err := c.httpPost(ctx, req.Repo, "create-commit-from-patch", req)
	if err != nil {
		return "", err
	}
	defer resp.Body.Close()

	data, err := io.ReadAll(resp.Body)
	if err != nil {
		log15.Warn("reading gitserver create-commit-from-patch response", "err", err.Error())
		return "", &url.Error{URL: resp.Request.URL.String(), Op: "CreateCommitFromPatch", Err: errors.Errorf("CreateCommitFromPatch: http status %d %s", resp.StatusCode, err.Error())}
	}

	var res protocol.CreateCommitFromPatchResponse
	err = json.Unmarshal(data, &res)
	if err != nil {
		log15.Warn("decoding gitserver create-commit-from-patch response", "err", err.Error())
		return "", &url.Error{URL: resp.Request.URL.String(), Op: "CreateCommitFromPatch", Err: errors.Errorf("CreateCommitFromPatch: http status %d %s", resp.StatusCode, string(data))}
	}

	if res.Error != nil {
		return res.Rev, res.Error
	}
	return res.Rev, nil
}

func (c *ClientImplementor) GetObject(ctx context.Context, repo api.RepoName, objectName string) (*gitdomain.GitObject, error) {
	if ClientMocks.GetObject != nil {
		return ClientMocks.GetObject(repo, objectName)
	}

	req := protocol.GetObjectRequest{
		Repo:       repo,
		ObjectName: objectName,
	}
	resp, err := c.httpPost(ctx, req.Repo, "commands/get-object", req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	data, err := io.ReadAll(resp.Body)
	if err != nil {
		log15.Warn("reading gitserver get-object response", "err", err.Error())
		return nil, &url.Error{URL: resp.Request.URL.String(), Op: "GetObject", Err: errors.Errorf("GetObject: http status %d %s", resp.StatusCode, err.Error())}
	}

	var res protocol.GetObjectResponse
	err = json.Unmarshal(data, &res)
	if err != nil {
		log15.Warn("decoding gitserver get-object response", "err", err.Error())
		return nil, &url.Error{URL: resp.Request.URL.String(), Op: "GetObject", Err: errors.Errorf("GetObject: http status %d %s", resp.StatusCode, string(data))}
	}

	return &res.Object, nil
}
