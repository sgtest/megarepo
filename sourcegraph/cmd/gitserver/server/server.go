package server

import (
	"bufio"
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"io/ioutil"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"syscall"
	"time"

	"github.com/opentracing/opentracing-go"
	"github.com/opentracing/opentracing-go/ext"
	otlog "github.com/opentracing/opentracing-go/log"
	"github.com/pkg/errors"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/conf"
	"github.com/sourcegraph/sourcegraph/pkg/env"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver/protocol"
	"github.com/sourcegraph/sourcegraph/pkg/honey"
	"github.com/sourcegraph/sourcegraph/pkg/mutablelimiter"
	"github.com/sourcegraph/sourcegraph/pkg/repotrackutil"
	"github.com/sourcegraph/sourcegraph/pkg/trace"
	"github.com/sourcegraph/sourcegraph/pkg/vcs/git"
	nettrace "golang.org/x/net/trace"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

// tempDirName is the name used for the temporary directory under ReposDir.
const tempDirName = ".tmp"

// traceLogs is controlled via the env SRC_GITSERVER_TRACE. If true we trace
// logs to stderr
var traceLogs bool

var lastCheckAt = make(map[api.RepoURI]time.Time)
var lastCheckMutex sync.Mutex

// debounce() provides some filtering to prevent spammy requests for the same
// repository. If the last fetch of the repository was within the given
// duration, returns false, otherwise returns true and updates the last
// fetch stamp.
func debounce(uri api.RepoURI, since time.Duration) bool {
	lastCheckMutex.Lock()
	defer lastCheckMutex.Unlock()
	if t, ok := lastCheckAt[uri]; ok && time.Now().Before(t.Add(since)) {
		return false
	}
	lastCheckAt[uri] = time.Now()
	return true
}

func init() {
	traceLogs, _ = strconv.ParseBool(env.Get("SRC_GITSERVER_TRACE", "false", "Toggles trace logging to stderr"))
}

// runCommandMock is set by tests. When non-nil it is run instead of
// runCommand
var runCommandMock func(context.Context, *exec.Cmd) (error, int)

// runCommand runs the command and returns the exit status. All clients of this function should set the context
// in cmd themselves, but we have to pass the context separately here for the sake of tracing.
func runCommand(ctx context.Context, cmd *exec.Cmd) (err error, exitCode int) {
	if runCommandMock != nil {
		return runCommandMock(ctx, cmd)
	}
	span, _ := opentracing.StartSpanFromContext(ctx, "runCommand")
	span.SetTag("path", cmd.Path)
	span.SetTag("args", cmd.Args)
	span.SetTag("dir", cmd.Dir)
	defer func() {
		if err != nil {
			ext.Error.Set(span, true)
			span.SetTag("err", err.Error())
			span.SetTag("exitCode", exitCode)
		}
		span.Finish()
	}()

	err = cmd.Run()
	exitStatus := -10810
	if cmd.ProcessState != nil { // is nil if process failed to start
		exitStatus = cmd.ProcessState.Sys().(syscall.WaitStatus).ExitStatus()
	}
	return err, exitStatus
}

// Server is a gitserver server.
type Server struct {
	// ReposDir is the path to the base directory for gitserver storage.
	ReposDir string

	// DeleteStaleRepositories when true will delete old repositories when the
	// Janitor job runs.
	DeleteStaleRepositories bool

	// skipCloneForTests is set by tests to avoid clones.
	skipCloneForTests bool

	// ctx is the context we use for all background jobs. It is done when the
	// server is stopped. Do not directly call this, rather call
	// Server.context()
	ctx      context.Context
	cancel   context.CancelFunc // used to shutdown background jobs
	cancelMu sync.Mutex         // protects canceled
	canceled bool
	wg       sync.WaitGroup // tracks running background jobs

	locker *RepositoryLocker

	// cloneLimiter and cloneableLimiter limits the number of concurrent
	// clones and ls-remotes respectively. Use s.acquireCloneLimiter() and
	// s.acquireClonableLimiter() instead of using these directly.
	cloneLimiter     *mutablelimiter.Limiter
	cloneableLimiter *mutablelimiter.Limiter

	updateRepo        chan<- updateRepoRequest
	repoUpdateLocksMu sync.Mutex // protects the map below and also updates to locks.once
	repoUpdateLocks   map[api.RepoURI]*locks
}

type locks struct {
	once *sync.Once  // consolidates multiple waiting updates
	mu   *sync.Mutex // prevents updates running in parallel
}

type updateRepoRequest struct {
	repo api.RepoURI
	url  string // remote URL
}

// shortGitCommandTimeout returns the timeout for git commands that should not
// take a long time. Some commands such as "git archive" are allowed more time
// than "git rev-parse", so this will return an appropriate timeout given the
// command.
func shortGitCommandTimeout(args []string) time.Duration {
	if len(args) < 1 {
		return time.Minute
	}
	switch args[0] {
	case "archive":
		// This is a long time, but this never blocks a user request for this
		// long. Even repos that are not that large can take a long time, for
		// example a search over all repos in an organization may have several
		// large repos. All of those repos will be competing for IO => we need
		// a larger timeout.
		return longGitCommandTimeout

	case "ls-remote":
		return 5 * time.Second

	default:
		return time.Minute
	}
}

// shortGitCommandSlow returns the threshold for regarding an git command as
// slow. Some commands such as "git archive" are inherently slower than "git
// rev-parse", so this will return an appropriate threshold given the command.
func shortGitCommandSlow(args []string) time.Duration {
	if len(args) < 1 {
		return time.Second
	}
	switch args[0] {
	case "archive":
		return 1 * time.Minute

	case "blame", "ls-tree", "log", "show":
		return 5 * time.Second

	default:
		return 2500 * time.Millisecond
	}
}

// This is a timeout for long git commands like clone or remote update.
// that may take a while for large repos. These types of commands should
// be run in the background.
var longGitCommandTimeout = time.Hour

// Handler returns the http.Handler that should be used to serve requests.
func (s *Server) Handler() http.Handler {
	s.ctx, s.cancel = context.WithCancel(context.Background())
	s.locker = &RepositoryLocker{}
	s.updateRepo = s.repoUpdateLoop()
	s.repoUpdateLocks = make(map[api.RepoURI]*locks)

	// GitMaxConcurrentClones controls the maximum number of clones that
	// can happen at once. Used to prevent throttle limits from a code
	// host. Defaults to 5.
	maxConcurrentClones := conf.Get().GitMaxConcurrentClones
	if maxConcurrentClones == 0 {
		maxConcurrentClones = 5
	}
	s.cloneLimiter = mutablelimiter.New(maxConcurrentClones)
	s.cloneableLimiter = mutablelimiter.New(maxConcurrentClones)
	conf.Watch(func() {
		limit := conf.Get().GitMaxConcurrentClones
		if limit == 0 {
			limit = 5
		}
		s.cloneLimiter.SetLimit(limit)
		s.cloneableLimiter.SetLimit(limit)
	})

	mux := http.NewServeMux()
	mux.HandleFunc("/exec", s.handleExec)
	mux.HandleFunc("/list", s.handleList)
	mux.HandleFunc("/is-repo-cloneable", s.handleIsRepoCloneable)
	mux.HandleFunc("/is-repo-cloned", s.handleIsRepoCloned)
	mux.HandleFunc("/repo", s.handleRepoInfo)
	mux.HandleFunc("/delete", s.handleRepoDelete)
	mux.HandleFunc("/enqueue-repo-update", s.handleEnqueueRepoUpdate)
	mux.HandleFunc("/repo-update", s.handleRepoUpdate)
	mux.HandleFunc("/upload-pack", s.handleUploadPack)
	mux.HandleFunc("/getGitolitePhabricatorMetadata", s.handleGetGitolitePhabricatorMetadata)
	mux.HandleFunc("/create-commit-from-patch", s.handleCreateCommitFromPatch)
	return mux
}

// Janitor does clean up tasks over s.ReposDir.
func (s *Server) Janitor() {
	// We may have clones which do not live in a directory named .git. Move
	// them.
	s.migrateGitDir()

	// Other janitorial tasks
	s.cleanupRepos()
}

// Stop cancels the running background jobs and returns when done.
func (s *Server) Stop() {
	// idempotent so we can just always set and cancel
	s.cancel()
	s.cancelMu.Lock()
	s.canceled = true
	s.cancelMu.Unlock()
	s.wg.Wait()
}

// serverContext returns a child context tied to the lifecycle of server.
func (s *Server) serverContext() (context.Context, context.CancelFunc) {
	// if we are already canceled don't increment our waitgroup. This is to
	// prevent a loop somewhere preventing us from ever finishing the
	// waitgroup, even though all calls fails instantly due to the canceled
	// context.
	s.cancelMu.Lock()
	if s.canceled {
		s.cancelMu.Unlock()
		return s.ctx, func() {}
	}
	s.wg.Add(1)
	s.cancelMu.Unlock()

	ctx, cancel := context.WithCancel(s.ctx)

	// we need to track if we have called cancel, since we are only allowed to
	// call wg.Done() once, but CancelFuncs can be called any number of times.
	var canceled int32
	return ctx, func() {
		ok := atomic.CompareAndSwapInt32(&canceled, 0, 1)
		if ok {
			cancel()
			s.wg.Done()
		}
	}
}

// acquireCloneLimiter() acquires a cancellable context associated with the
// clone limiter.
func (s *Server) acquireCloneLimiter(ctx context.Context) (context.Context, context.CancelFunc, error) {
	cloneQueue.Inc()
	defer cloneQueue.Dec()
	return s.cloneLimiter.Acquire(ctx)
}

// queryCloneLimiter reports the capacity and length of the clone limiter's queue
func (s *Server) queryCloneLimiter() (cap, len int) {
	return s.cloneLimiter.GetLimit()
}

func (s *Server) acquireCloneableLimiter(ctx context.Context) (context.Context, context.CancelFunc, error) {
	lsRemoteQueue.Inc()
	defer lsRemoteQueue.Dec()
	return s.cloneableLimiter.Acquire(ctx)
}

// tempDir is a wrapper around ioutil.TempDir, but using the server's
// temporary directory filepath.Join(s.ReposDir, tempDirName).
//
// This directory is cleaned up by gitserver and will be ignored by repository
// listing operations.
func (s *Server) tempDir(prefix string) (name string, err error) {
	dir := filepath.Join(s.ReposDir, tempDirName)

	// Create tmpdir directory if doesn't exist yet.
	if err := os.MkdirAll(dir, os.ModePerm); err != nil {
		return "", err
	}

	return ioutil.TempDir(dir, prefix)
}

func (s *Server) ignorePath(path string) bool {
	// We ignore any path which starts with .tmp in ReposDir
	if filepath.Dir(path) != s.ReposDir {
		return false
	}
	return strings.HasPrefix(filepath.Base(path), tempDirName)
}

func (s *Server) handleIsRepoCloneable(w http.ResponseWriter, r *http.Request) {
	var req protocol.IsRepoCloneableRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	req.Repo = protocol.NormalizeRepo(req.Repo)

	if req.URL == "" {
		req.URL = OriginMap(req.Repo)
	}
	if req.URL == "" {
		// BACKCOMPAT: Determine URL from the existing repo on disk if the client didn't send it.
		dir := path.Join(s.ReposDir, string(req.Repo))
		var err error
		req.URL, err = repoRemoteURL(r.Context(), dir)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
	}

	var resp protocol.IsRepoCloneableResponse
	if err := s.isCloneable(r.Context(), req.URL); err == nil {
		resp = protocol.IsRepoCloneableResponse{Cloneable: true}
	} else {
		resp = protocol.IsRepoCloneableResponse{
			Cloneable: false,
			Reason:    err.Error(),
		}
	}

	if err := json.NewEncoder(w).Encode(resp); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
}

func (s *Server) handleIsRepoCloned(w http.ResponseWriter, r *http.Request) {
	var req protocol.IsRepoClonedRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	req.Repo = protocol.NormalizeRepo(req.Repo)
	dir := path.Join(s.ReposDir, string(req.Repo))
	if repoCloned(dir) {
		w.WriteHeader(http.StatusOK)
	} else {
		w.WriteHeader(http.StatusNotFound)
	}
}

// handleEnqueueRepoUpdate: This is the old implementation, which is being
// deprecated.
func (s *Server) handleEnqueueRepoUpdate(w http.ResponseWriter, r *http.Request) {
	var req protocol.RepoUpdateRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	var resp protocol.RepoUpdateResponse
	req.Repo = protocol.NormalizeRepo(req.Repo)
	dir := path.Join(s.ReposDir, string(req.Repo))
	if !repoCloned(dir) && !s.skipCloneForTests {
		// optimistically, we assume that our cloning attempt might
		// succeed.
		resp.CloneInProgress = true
		go func() {
			ctx, cancel1 := s.serverContext()
			defer cancel1()
			ctx, cancel2 := context.WithTimeout(ctx, longGitCommandTimeout)
			defer cancel2()
			_, err := s.cloneRepo(ctx, req.Repo, req.URL, nil)
			if err != nil {
				log15.Warn("error cloning repo", "repo", req.Repo, "err", err)
			}
		}()
	} else {
		// Check the repo status before enqueuing
		var statusErr error
		lastFetched, err := repoLastFetched(dir)
		if err != nil {
			statusErr = err
		}
		lastChanged, err := repoLastChanged(dir)
		if err != nil {
			statusErr = err
		}

		// We always want to enqueue an update
		updateQueue.Inc()
		s.updateRepo <- updateRepoRequest{repo: req.Repo, url: req.URL}

		if statusErr != nil {
			log15.Error("failed to get status of repo", "repo", req.Repo, "error", statusErr)
			http.Error(w, statusErr.Error(), http.StatusInternalServerError)
			return
		}

		resp.Cloned = true
		resp.LastFetched = &lastFetched
		resp.LastChanged = &lastChanged
	}
	if err := json.NewEncoder(w).Encode(resp); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
}

// handleRepoUpdate is a synchronous (waits for update to complete or
// time out) method so it can yield errors. Updates are not
// unconditional; we debounce them based on the provided
// interval, to avoid spam.
func (s *Server) handleRepoUpdate(w http.ResponseWriter, r *http.Request) {
	var req protocol.RepoUpdateRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	var resp protocol.RepoUpdateResponse
	req.Repo = protocol.NormalizeRepo(req.Repo)
	dir := path.Join(s.ReposDir, string(req.Repo))

	// despite the existence of a context on the request, we don't want to
	// cancel the git commands partway through if the request terminates.
	ctx, cancel1 := s.serverContext()
	defer cancel1()
	ctx, cancel2 := context.WithTimeout(ctx, longGitCommandTimeout)
	defer cancel2()
	resp.QueueCap, resp.QueueLen = s.queryCloneLimiter()
	if !repoCloned(dir) && !s.skipCloneForTests {
		// optimistically, we assume that our cloning attempt might
		// succeed.
		resp.CloneInProgress = true
		_, err := s.cloneRepo(ctx, req.Repo, req.URL, nil)
		if err != nil {
			log15.Warn("error cloning repo", "repo", req.Repo, "err", err)
			resp.Error = err.Error()
		}
	} else {
		resp.Cloned = true
		var statusErr, updateErr error

		if debounce(req.Repo, req.Since) {
			updateErr = s.doRepoUpdate(ctx, req.Repo, req.URL)
		}

		// attempts to acquire these values are not contingent on the success of
		// the update.
		lastFetched, err := repoLastFetched(dir)
		if err != nil {
			statusErr = err
		} else {
			resp.LastFetched = &lastFetched
		}
		lastChanged, err := repoLastChanged(dir)
		if err != nil {
			statusErr = err
		} else {
			resp.LastChanged = &lastChanged
		}
		if statusErr != nil {
			log15.Error("failed to get status of repo", "repo", req.Repo, "error", statusErr)
			// report this error in-band, but still produce a valid response with the
			// other information.
			resp.Error = statusErr.Error()
		}
		// If an error occurred during update, report it but don't actually make
		// it into an http error; we want the client to get the information cleanly.
		// An update error "wins" over a status error.
		if updateErr != nil {
			resp.Error = updateErr.Error()
		}
	}
	if err := json.NewEncoder(w).Encode(resp); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
}

func (s *Server) handleExec(w http.ResponseWriter, r *http.Request) {
	span, ctx := opentracing.StartSpanFromContext(r.Context(), "Server.handleExec")
	defer span.Finish()

	var req protocol.ExecRequest
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	// Flush writes more aggressively than standard net/http so that clients
	// with a context deadline see as much partial response body as possible.
	if fw := newFlushingResponseWriter(w); fw != nil {
		w = fw
		defer fw.Close()
	}

	ctx, cancel := context.WithTimeout(ctx, shortGitCommandTimeout(req.Args))
	defer cancel()

	start := time.Now()
	var cmdStart time.Time // set once we have ensured commit
	exitStatus := -10810   // sentinel value to indicate not set
	var stdoutN, stderrN int64
	var status string
	var errStr string
	var ensureRevisionStatus string

	req.Repo = protocol.NormalizeRepo(req.Repo)

	// Instrumentation
	{
		repo := repotrackutil.GetTrackedRepo(req.Repo)
		cmd := ""
		if len(req.Args) > 0 {
			cmd = req.Args[0]
		}
		args := strings.Join(req.Args, " ")

		tr := nettrace.New("exec."+cmd, string(req.Repo))
		tr.LazyPrintf("args: %s", args)
		execRunning.WithLabelValues(cmd, repo).Inc()
		defer func() {
			tr.LazyPrintf("status=%s stdout=%d stderr=%d", status, stdoutN, stderrN)
			if errStr != "" {
				tr.LazyPrintf("error: %s", errStr)
				tr.SetError()
			}
			tr.Finish()

			duration := time.Since(start)
			execRunning.WithLabelValues(cmd, repo).Dec()
			execDuration.WithLabelValues(cmd, repo, status).Observe(duration.Seconds())

			var cmdDuration time.Duration
			var fetchDuration time.Duration
			if !cmdStart.IsZero() {
				cmdDuration = time.Since(cmdStart)
				fetchDuration = cmdStart.Sub(start)
			}

			if honey.Enabled() || traceLogs {
				ev := honey.Event("gitserver-exec")
				ev.AddField("repo", req.Repo)
				ev.AddField("remote_url", req.URL)
				ev.AddField("cmd", cmd)
				ev.AddField("args", args)
				ev.AddField("ensure_revision", req.EnsureRevision)
				ev.AddField("ensure_revision_status", ensureRevisionStatus)
				ev.AddField("client", r.UserAgent())
				ev.AddField("duration_ms", duration.Seconds()*1000)
				ev.AddField("stdout_size", stdoutN)
				ev.AddField("stderr_size", stderrN)
				ev.AddField("exit_status", exitStatus)
				ev.AddField("status", status)
				if errStr != "" {
					ev.AddField("error", errStr)
				}
				if !cmdStart.IsZero() {
					ev.AddField("cmd_duration_ms", cmdDuration.Seconds()*1000)
					ev.AddField("fetch_duration_ms", fetchDuration.Seconds()*1000)
				}

				if honey.Enabled() {
					ev.Send()
				}
				if traceLogs {
					log15.Debug("TRACE gitserver exec", mapToLog15Ctx(ev.Fields())...)
				}
			}

			if cmdDuration > shortGitCommandSlow(req.Args) {
				log15.Warn("Long exec request", "repo", req.Repo, "args", req.Args, "duration", cmdDuration.Round(time.Millisecond))
			}
			if fetchDuration > 10*time.Second {
				log15.Warn("Slow fetch/clone for exec request", "repo", req.Repo, "args", req.Args, "duration", fetchDuration)
			}
		}()
	}

	dir := path.Join(s.ReposDir, string(req.Repo))
	cloneProgress, cloneInProgress := s.locker.Status(dir)
	if strings.ToLower(string(req.Repo)) == "github.com/sourcegraphtest/alwayscloningtest" {
		cloneInProgress = true
		cloneProgress = "This will never finish cloning"
	}
	if cloneInProgress {
		status = "clone-in-progress"
		w.WriteHeader(http.StatusNotFound)
		json.NewEncoder(w).Encode(&protocol.NotFoundPayload{
			CloneInProgress: true,
			CloneProgress:   cloneProgress,
		})
		return
	}
	if !repoCloned(dir) {
		cloneProgress, err := s.cloneRepo(ctx, req.Repo, req.URL, nil)
		if err != nil {
			log15.Debug("error cloning repo", "repo", req.Repo, "err", err)
			status = "repo-not-found"
			w.WriteHeader(http.StatusNotFound)
			json.NewEncoder(w).Encode(&protocol.NotFoundPayload{CloneInProgress: false})
			return
		}
		status = "clone-in-progress"
		w.WriteHeader(http.StatusNotFound)
		json.NewEncoder(w).Encode(&protocol.NotFoundPayload{
			CloneInProgress: true,
			CloneProgress:   cloneProgress,
		})
		return
	}

	didUpdate := s.ensureRevision(ctx, req.Repo, req.URL, req.EnsureRevision, dir)
	if didUpdate {
		ensureRevisionStatus = "fetched"
	} else {
		ensureRevisionStatus = "noop"
	}

	w.Header().Set("Trailer", "X-Exec-Error")
	w.Header().Add("Trailer", "X-Exec-Exit-Status")
	w.Header().Add("Trailer", "X-Exec-Stderr")
	w.WriteHeader(http.StatusOK)

	// Special-case `git rev-parse HEAD` requests. These are invoked by search queries for every repo in scope.
	// For searches over large repo sets (> 1k), this leads to too many child process execs, which can lead
	// to a persistent failure mode where every exec takes > 10s, which is disastrous for gitserver performance.
	if len(req.Args) == 2 && req.Args[0] == "rev-parse" && req.Args[1] == "HEAD" {
		if resolved, err := quickRevParseHead(dir); err == nil && git.IsAbsoluteRevision(resolved) {
			w.Write([]byte(resolved))
			w.Header().Set("X-Exec-Error", "")
			w.Header().Set("X-Exec-Exit-Status", "0")
			w.Header().Set("X-Exec-Stderr", "")
			return
		}
	}

	var stderrBuf bytes.Buffer
	stdoutW := &writeCounter{w: w}
	stderrW := &writeCounter{w: &stderrBuf}

	cmdStart = time.Now()
	cmd := exec.CommandContext(ctx, "git", req.Args...)
	cmd.Dir = dir
	cmd.Stdout = stdoutW
	cmd.Stderr = stderrW

	var err error
	err, exitStatus = runCommand(ctx, cmd)
	if err != nil {
		errStr = err.Error()
	}

	status = strconv.Itoa(exitStatus)
	stdoutN = stdoutW.n
	stderrN = stderrW.n

	stderr := stderrBuf.String()
	if len(stderr) > 1024 {
		stderr = stderr[:1024]
	}

	// write trailer
	w.Header().Set("X-Exec-Error", errStr)
	w.Header().Set("X-Exec-Exit-Status", status)
	w.Header().Set("X-Exec-Stderr", string(stderr))
}

// setGitAttributes writes our global gitattributes to
// gitDir/info/attributes. This will override .gitattributes inside of
// repositories. It is used to unset attributes such as export-ignore.
func setGitAttributes(gitDir string) error {
	infoDir := filepath.Join(gitDir, "info")
	if err := os.Mkdir(infoDir, os.ModePerm); err != nil && !os.IsExist(err) {
		return errors.Wrap(err, "failed to set git attributes")
	}

	_, err := updateFileIfDifferent(
		filepath.Join(infoDir, "attributes"),
		[]byte(`# Managed by Sourcegraph gitserver.

# We want every file to be present in git archive.
* -export-ignore
`))
	if err != nil {
		return errors.Wrap(err, "failed to set git attributes")
	}
	return nil
}

// cloneOptions specify optional behaviour for the cloneRepo function.
type cloneOptions struct {
	// Block will wait for the clone to finish before returning. If the clone
	// fails, the error will be returned. The passed in context is
	// respected. When not blocking the clone is done with a server background
	// context.
	Block bool

	// Overwrite will overwrite the existing clone.
	Overwrite bool
}

// cloneRepo issues a git clone command for the given repo. It is
// non-blocking.
func (s *Server) cloneRepo(ctx context.Context, repo api.RepoURI, url string, opts *cloneOptions) (string, error) {
	dir := filepath.Join(s.ReposDir, string(protocol.NormalizeRepo(repo)))

	// PERF: Before doing the network request to check if isCloneable, lets
	// ensure we are not already cloning.
	if progress, cloneInProgress := s.locker.Status(dir); cloneInProgress {
		return progress, nil
	}

	if url == "" {
		// BACKCOMPAT: if URL is not specified in API request, look it up in the OriginMap.
		url = OriginMap(repo)
		if url == "" {
			return "", fmt.Errorf("error cloning repo: no URL provided and origin map entry found for %s", repo)
		}
	}

	// isCloneable causes a network request, so we limit the number that can
	// run at one time. We use a separate semaphore to cloning since these
	// checks being blocked by a few slow clones will lead to poor feedback to
	// users. We can defer since the rest of the function does not block this
	// goroutine.
	ctx, cancel, err := s.acquireCloneableLimiter(ctx)
	if err != nil {
		return "", err // err will be a context error
	}
	defer cancel()
	if err := s.isCloneable(ctx, url); err != nil {
		return "", fmt.Errorf("error cloning repo: repo %s (%s) not cloneable: %s", repo, url, err)
	}

	// Mark this repo as currently being cloned. We have to check again if someone else isn't already
	// cloning since we released the lock. We released the lock since isCloneable is a potentially
	// slow operation.
	lock, ok := s.locker.TryAcquire(dir, "starting clone")
	if !ok {
		// Someone else beat us to it
		status, _ := s.locker.Status(dir)
		return status, nil
	}

	if s.skipCloneForTests {
		lock.Release()
		return "", nil
	}

	// We clone to a temporary location first to avoid having incomplete
	// clones in the repo tree. This also avoids leaving behind corrupt clones
	// if the clone is interrupted.
	doClone := func(ctx context.Context) error {
		defer lock.Release()

		ctx, cancel1, err := s.acquireCloneLimiter(ctx)
		if err != nil {
			return err
		}
		defer cancel1()
		ctx, cancel2 := context.WithTimeout(ctx, longGitCommandTimeout)
		defer cancel2()

		dstPath := filepath.Join(dir, ".git")
		overwrite := opts != nil && opts.Overwrite
		if !overwrite {
			// We clone to a temporary directory first, so avoid wasting resources
			// if the directory already exists.
			if _, err := os.Stat(dstPath); err == nil {
				return &os.PathError{
					Op:   "cloneRepo",
					Path: dstPath,
					Err:  os.ErrExist,
				}
			}
		}

		tmpPath, err := s.tempDir("clone-")
		if err != nil {
			return err
		}
		defer os.RemoveAll(tmpPath)
		tmpPath = filepath.Join(tmpPath, ".git")

		cmd := exec.CommandContext(ctx, "git", "clone", "--mirror", "--progress", url, tmpPath)
		log15.Info("cloning repo", "repo", repo, "url", url, "tmp", tmpPath, "dst", dstPath)

		pr, pw := io.Pipe()
		defer pw.Close()
		go readCloneProgress(repo, url, lock, pr)

		if output, err := s.runWithRemoteOpts(ctx, cmd, pw); err != nil {
			return errors.Wrapf(err, "clone failed. Output: %s", string(output))
		}

		// Update the last-changed stamp.
		if err := setLastChanged(tmpPath); err != nil {
			return errors.Wrapf(err, "failed to update last changed time")
		}

		// Set gitattributes
		if err := setGitAttributes(tmpPath); err != nil {
			return err
		}

		if overwrite {
			// remove the current repo by putting it into our temporary directory
			err := os.Rename(dstPath, filepath.Join(filepath.Dir(tmpPath), "old"))
			if err != nil && !os.IsNotExist(err) {
				return errors.Wrapf(err, "failed to remove old clone")
			}
		}

		if err := os.MkdirAll(filepath.Dir(dstPath), os.ModePerm); err != nil {
			return err
		}
		if err := os.Rename(tmpPath, dstPath); err != nil {
			return err
		}

		log15.Info("repo cloned", "repo", repo)

		return nil
	}

	if opts != nil && opts.Block {
		// We are blocking, so use the passed in context.
		if err := doClone(ctx); err != nil {
			return "", errors.Wrapf(err, "failed to clone %s", repo)
		}
		return "", nil
	}

	go func() {
		// Create a new context because this is in a background goroutine.
		ctx, cancel := s.serverContext()
		defer cancel()
		if err := doClone(ctx); err != nil {
			log15.Error("failed to clone repo", "repo", repo, "error", err)
		}
	}()

	return "", nil
}

// readCloneProgress scans the reader and saves the most recent line of output
// as the lock status.
func readCloneProgress(repo api.RepoURI, url string, lock *RepositoryLock, pr io.Reader) {
	scan := bufio.NewScanner(pr)
	scan.Split(scanCRLF)
	redactor := newURLRedactor(url)
	for scan.Scan() {
		progress := scan.Text()

		// 🚨 SECURITY: The output could include the clone url with may contain a sensitive token.
		// Redact the full url and any found HTTP credentials to be safe.
		//
		// e.g.
		// $ git clone http://token@github.com/foo/bar
		// Cloning into 'nick'...
		// fatal: repository 'http://token@github.com/foo/bar/' not found
		redactedProgress := redactor.redact(progress)

		lock.SetStatus(redactedProgress)
	}
	if err := scan.Err(); err != nil {
		log15.Error("error reporting progress", "error", err)
	}
}

// urlRedactor redacts all sensitive strings from a message.
type urlRedactor struct {
	// sensitive are sensitive strings to be redacted.
	// The strings should not be empty.
	sensitive []string
}

// newURLRedactor returns a new urlRedactor that redacts
// credentials found in rawurl, and the rawurl itself.
func newURLRedactor(rawurl string) *urlRedactor {
	var sensitive []string
	parsedURL, _ := url.Parse(rawurl)
	if parsedURL != nil {
		if pw, _ := parsedURL.User.Password(); pw != "" {
			sensitive = append(sensitive, pw)
		}
		if u := parsedURL.User.Username(); u != "" {
			sensitive = append(sensitive, u)
		}
	}
	sensitive = append(sensitive, rawurl)
	return &urlRedactor{sensitive: sensitive}
}

// redact returns a redacted version of message.
// Sensitive strings are replaced with "<redacted>".
func (r *urlRedactor) redact(message string) string {
	for _, s := range r.sensitive {
		message = strings.Replace(message, s, "<redacted>", -1)
	}
	return message
}

// scanCRLF is similar to bufio.ScanLines except it splits on both '\r' and '\n'
// and it does not return tokens that contain only whitespace.
func scanCRLF(data []byte, atEOF bool) (advance int, token []byte, err error) {
	if atEOF && len(data) == 0 {
		return 0, nil, nil
	}
	trim := func(data []byte) []byte {
		data = bytes.TrimSpace(data)
		if len(data) == 0 {
			// Don't pass back a token that is all whitespace.
			return nil
		}
		return data
	}
	if i := bytes.IndexAny(data, "\r\n"); i >= 0 {
		// We have a full newline-terminated line.
		return i + 1, trim(data[:i]), nil
	}
	// If we're at EOF, we have a final, non-terminated line. Return it.
	if atEOF {
		return len(data), trim(data), nil
	}
	// Request more data.
	return 0, nil, nil
}

// testRepoExists is a test fixture that overrides the return value
// for isCloneable when it is set.
var testRepoExists func(ctx context.Context, url string) error

// isCloneable checks to see if the Git remote URL is cloneable.
func (s *Server) isCloneable(ctx context.Context, url string) error {
	ctx, cancel := context.WithTimeout(ctx, shortGitCommandTimeout([]string{"ls-remote"}))
	defer cancel()

	if strings.ToLower(string(protocol.NormalizeRepo(api.RepoURI(url)))) == "github.com/sourcegraphtest/alwayscloningtest" {
		return nil
	}
	if testRepoExists != nil {
		return testRepoExists(ctx, url)
	}

	cmd := exec.CommandContext(ctx, "git", "ls-remote", url, "HEAD")
	out, err := s.runWithRemoteOpts(ctx, cmd, nil)
	if err != nil {
		if len(out) > 0 {
			err = fmt.Errorf("%s (output follows)\n\n%s", err, out)
		}
		return err
	}
	return nil
}

var (
	execRunning = prometheus.NewGaugeVec(prometheus.GaugeOpts{
		Namespace: "src",
		Subsystem: "gitserver",
		Name:      "exec_running",
		Help:      "number of gitserver.Command running concurrently.",
	}, []string{"cmd", "repo"})
	execDuration = prometheus.NewHistogramVec(prometheus.HistogramOpts{
		Namespace: "src",
		Subsystem: "gitserver",
		Name:      "exec_duration_seconds",
		Help:      "gitserver.Command latencies in seconds.",
		Buckets:   trace.UserLatencyBuckets,
	}, []string{"cmd", "repo", "status"})
	cloneQueue = prometheus.NewGauge(prometheus.GaugeOpts{
		Namespace: "src",
		Subsystem: "gitserver",
		Name:      "clone_queue",
		Help:      "number of repos waiting to be cloned.",
	})
	lsRemoteQueue = prometheus.NewGauge(prometheus.GaugeOpts{
		Namespace: "src",
		Subsystem: "gitserver",
		Name:      "lsremote_queue",
		Help:      "number of repos waiting to check existence on remote code host (git ls-remote).",
	})
	updateQueue = prometheus.NewGauge(prometheus.GaugeOpts{
		Namespace: "src",
		Subsystem: "gitserver",
		Name:      "update_queue",
		Help:      "number of repos waiting to be updated (enqueue-repo-update)",
	})
)

func init() {
	prometheus.MustRegister(execRunning)
	prometheus.MustRegister(execDuration)
	prometheus.MustRegister(cloneQueue)
	prometheus.MustRegister(lsRemoteQueue)
	prometheus.MustRegister(updateQueue)
}

func (s *Server) repoUpdateLoop() chan<- updateRepoRequest {
	updateRepo := make(chan updateRepoRequest, 10)

	go func() {
		for req := range updateRepo {
			updateQueue.Dec()

			if !debounce(req.repo, 10*time.Second) {
				continue
			}
			go func(req updateRepoRequest) {
				// Create a new context with a new timeout (instead of passing one through updateRepoRequest)
				// because the ctx of the updateRepoRequest sender will get cancelled before this goroutine runs.
				ctx, cancel1 := s.serverContext()
				defer cancel1()
				ctx, cancel2 := context.WithTimeout(ctx, longGitCommandTimeout)
				defer cancel2()
				s.doRepoUpdate(ctx, req.repo, req.url)
			}(req)
		}
	}()

	return updateRepo
}

var headBranchPattern = regexp.MustCompile(`HEAD branch: (.+?)\n`)

func (s *Server) doRepoUpdate(ctx context.Context, repo api.RepoURI, url string) error {
	span, ctx := opentracing.StartSpanFromContext(ctx, "Server.doRepoUpdate")
	span.SetTag("repo", repo)
	span.SetTag("url", url)
	defer span.Finish()

	s.repoUpdateLocksMu.Lock()
	l, ok := s.repoUpdateLocks[repo]
	if !ok {
		l = &locks{
			once: new(sync.Once),
			mu:   new(sync.Mutex),
		}
		s.repoUpdateLocks[repo] = l
	}
	once := l.once
	mu := l.mu
	s.repoUpdateLocksMu.Unlock()

	// doRepoUpdate2 can block longer than our context deadline. done will
	// close when its done. We can return when either done is closed or our
	// deadline has passed.
	done := make(chan struct{})
	err := errors.New("another operation is already in progress")
	go func() {
		defer close(done)
		once.Do(func() {
			mu.Lock() // Prevent multiple updates in parallel. It works fine, but it wastes resources.
			defer mu.Unlock()

			s.repoUpdateLocksMu.Lock()
			l.once = new(sync.Once) // Make new requests wait for next update.
			s.repoUpdateLocksMu.Unlock()

			err = s.doRepoUpdate2(repo, url)
		})
	}()

	select {
	case <-done:
		return errors.Wrapf(err, "repo %s:", repo)
	case <-ctx.Done():
		span.LogFields(otlog.String("event", "context canceled"))
		return ctx.Err()
	}
}

// setLastChanged discerns an approximate last-changed timestamp for a
// repository. This can be approximate; it's used to determine how often we
// should run `git fetch`, but is not relied on strongly. The basic plan
// is as follows: If a repository has never had a timestamp before, we
// guess that the right stamp is *probably* the timestamp of the most
// chronologically-recent commit. If there are no commits, we just use the
// current time because that's probably usually a temporary state.
//
// If a timestamp already exists, we want to update it if and only if
// the set of references (as determined by `git show-ref`) has changed.
//
// To accomplish this, we assert that the file `sg_refhash` in the git
// directory should, if it exists, contain a hash of the output of
// `git show-ref`, and have a timestamp of "the last time this changed",
// except that if we're creating that file for the first time, we set
// it to the timestamp of the top commit. We then compute the hash of
// the show-ref output, and store it in the file if and only if it's
// different from the current contents.
//
// If show-ref fails, we use rev-list to determine whether that's just
// an empty repository (not an error) or some kind of actual error
// that is possibly causing our data to be incorrect, which should
// be reported.
func setLastChanged(dir string) error {
	// Handle two different locations for GIT_DIR :'(
	_, err := os.Stat(filepath.Join(dir, "HEAD"))
	if os.IsNotExist(err) {
		dir = filepath.Join(dir, ".git")
		_, err = os.Stat(filepath.Join(dir, "HEAD"))
	}
	if err != nil {
		return err
	}
	hashFile := filepath.Join(dir, "sg_refhash")

	hash, err := computeRefHash(dir)
	if err != nil {
		return errors.Wrapf(err, "computeRefHash failed for %s", dir)
	}

	var stamp time.Time
	if _, err := os.Stat(hashFile); os.IsNotExist(err) {
		// This is the first time we are calculating the hash. Give a more
		// approriate timestamp for sg_refhash than the current time.
		stamp, err = computeLatestCommitTimestamp(dir)
		if err != nil {
			return errors.Wrapf(err, "computeLatestCommitTimestamp failed for %s", dir)
		}
	}

	_, err = updateFileIfDifferent(hashFile, hash)
	if err != nil {
		return errors.Wrapf(err, "failed to update %s", hashFile)
	}

	// If stamp is non-zero we have a more approriate mtime.
	if !stamp.IsZero() {
		err = os.Chtimes(hashFile, stamp, stamp)
		if err != nil {
			return errors.Wrapf(err, "failed to set mtime to the lastest commit timestamp for %s", dir)
		}
	}

	return nil
}

// computeLatestCommitTimestamp returns the timestamp of the most recent
// commit if any. If there are no commits or the latest commit is in the
// future, time.Now is returned.
func computeLatestCommitTimestamp(dir string) (time.Time, error) {
	now := time.Now() // return current time if we don't find a more accurate time
	cmd := exec.Command("git", "rev-list", "--all", "--timestamp", "-n", "1")
	cmd.Dir = dir
	output, err := cmd.Output()

	// If we don't have a more specific stamp, we'll return the current time,
	// and possibly an error.
	if err != nil {
		return now, err
	}

	words := bytes.Split(output, []byte(" "))
	// An empty rev-list output, without an error, is okay.
	if len(words) < 2 {
		return now, nil
	}

	// We should have a timestamp and a commit hash; format is
	// 1521316105 ff03fac223b7f16627b301e03bf604e7808989be
	epoch, err := strconv.ParseInt(string(words[0]), 10, 64)
	if err != nil {
		return now, errors.Wrap(err, "invalid timestamp in rev-list output")
	}
	stamp := time.Unix(epoch, 0)
	if stamp.After(now) {
		return now, nil
	}
	return stamp, nil
}

// computeRefHash returns a hash of the refs for dir. The hash should only
// change if the set of refs and the commits they point to change.
func computeRefHash(dir string) ([]byte, error) {
	// Do not use CommandContext since this is a fast operation we do not want
	// to interrupt.
	cmd := exec.Command("git", "show-ref")
	cmd.Dir = dir
	output, err := cmd.Output()
	if err != nil {
		// Ignore the failure for an empty repository: show-ref fails with
		// empty output and an exit code of 1
		if e, ok := err.(*exec.ExitError); !ok || len(output) != 0 || len(e.Stderr) != 0 || e.Sys().(syscall.WaitStatus).ExitStatus() != 1 {
			return nil, err
		}
	}

	lines := bytes.Split(output, []byte("\n"))
	sort.Slice(lines, func(i, j int) bool {
		return bytes.Compare(lines[i], lines[j]) < 0
	})
	hasher := sha256.New()
	for _, b := range lines {
		hasher.Write(b)
		hasher.Write([]byte("\n"))
	}
	hash := make([]byte, hex.EncodedLen(hasher.Size()))
	hex.Encode(hash, hasher.Sum(nil))
	return hash, nil
}

func (s *Server) doRepoUpdate2(repo api.RepoURI, url string) error {
	// background context.
	ctx, cancel1 := s.serverContext()
	defer cancel1()

	ctx, cancel2, err := s.acquireCloneLimiter(ctx)
	if err != nil {
		return err
	}
	defer cancel2()

	repo = protocol.NormalizeRepo(repo)
	dir := path.Join(s.ReposDir, string(repo))

	// If URL is not set, we can also consult our deprecated OriginMap or the
	// last known working URL (set as the remote origin).
	var urlIsGitRemote bool
	if url == "" {
		// BACKCOMPAT: if URL is not specified in API request, look it up in the OriginMap.
		url = OriginMap(repo)
	}
	if url == "" {
		// log15.Warn("Deprecated: use of saved Git remote for repo updating (API client should set URL)", "repo", repo)
		var err error
		url, err = repoRemoteURL(ctx, dir)
		if err != nil || url == "" {
			log15.Error("Failed to determine Git remote URL", "repo", repo, "error", err, "url", url)
			return errors.Wrap(err, "failed to determine Git remote URL")
		}
		urlIsGitRemote = true
	}

	// url is now guaranteed to != "". Store the URL as the remote origin. If
	// a future call does not set the URL, we can fallback to this one. This
	// is best-effort, so we do not fail the repoUpdate if updating the remote
	// fails.
	if !urlIsGitRemote {
		// Note: We do not use CommandContext since it is a fast operation.
		var cmd *exec.Cmd
		if current, _ := repoRemoteURL(ctx, dir); current == "" {
			cmd = exec.Command("git", "remote", "add", "origin", url)
		} else if current != url {
			log15.Debug("repository remote URL changed", "repo", repo, "old", current, "new", url)
			cmd = exec.Command("git", "remote", "set-url", "origin", "--", url)
		}
		if cmd != nil {
			cmd.Dir = dir
			if err, _ := runCommand(ctx, cmd); err != nil {
				log15.Error("Failed to update repository's Git remote URL.", "repo", repo, "url", url, "error", err)
			}
		}
	}

	cmd := exec.CommandContext(ctx, "git", "fetch", "--prune", url, "+refs/heads/*:refs/heads/*", "+refs/tags/*:refs/tags/*", "+refs/pull/*:refs/pull/*")
	cmd.Dir = dir

	// drop temporary pack files after a fetch. this function won't
	// return until this fetch has completed or definitely-failed,
	// either way they can't still be in use. we don't care exactly
	// when the cleanup happens, just that it does.
	defer s.cleanTmpFiles(dir)

	if output, err := s.runWithRemoteOpts(ctx, cmd, nil); err != nil {
		log15.Error("Failed to update", "repo", repo, "error", err, "output", string(output))
		return errors.Wrap(err, "failed to update")
	}

	// Update the last-changed stamp.
	if err := setLastChanged(dir); err != nil {
		log15.Warn("Failed to update last changed time", "repo", repo, "error", err)
	}

	headBranch := "master"

	// try to fetch HEAD from origin
	cmd = exec.CommandContext(ctx, "git", "remote", "show", url)
	cmd.Dir = path.Join(s.ReposDir, string(repo))
	output, err := s.runWithRemoteOpts(ctx, cmd, nil)
	if err != nil {
		log15.Error("Failed to fetch remote info", "repo", repo, "url", url, "error", err, "output", string(output))
		return errors.Wrap(err, "failed to fetch remote info")
	}
	submatches := headBranchPattern.FindSubmatch(output)
	if len(submatches) == 2 {
		submatch := string(submatches[1])
		if submatch != "(unknown)" {
			headBranch = string(submatch)
		}
	}

	// check if branch pointed to by HEAD exists
	cmd = exec.CommandContext(ctx, "git", "rev-parse", headBranch, "--")
	cmd.Dir = path.Join(s.ReposDir, string(repo))
	if err := cmd.Run(); err != nil {
		// branch does not exist, pick first branch
		cmd := exec.CommandContext(ctx, "git", "branch")
		cmd.Dir = path.Join(s.ReposDir, string(repo))
		list, err := cmd.Output()
		if err != nil {
			log15.Error("Failed to list branches", "repo", repo, "error", err, "output", string(output))
			return errors.Wrap(err, "failed to list branches")
		}
		lines := strings.Split(string(list), "\n")
		branch := strings.TrimPrefix(strings.TrimPrefix(lines[0], "* "), "  ")
		if branch != "" {
			headBranch = branch
		}
	}

	// set HEAD
	cmd = exec.CommandContext(ctx, "git", "symbolic-ref", "HEAD", "refs/heads/"+headBranch)
	cmd.Dir = path.Join(s.ReposDir, string(repo))
	if output, err := cmd.CombinedOutput(); err != nil {
		log15.Error("Failed to set HEAD", "repo", repo, "error", err, "output", string(output))
		return errors.Wrap(err, "Failed to set HEAD")
	}
	return nil
}

func (s *Server) ensureRevision(ctx context.Context, repo api.RepoURI, url, rev, repoDir string) (didUpdate bool) {
	if rev == "" || rev == "HEAD" {
		return false
	}
	// rev-parse on an OID does not check if the commit actually exists, so it
	// is always works. So we append ^0 to force the check
	if git.IsAbsoluteRevision(rev) {
		rev = rev + "^0"
	}
	cmd := exec.Command("git", "rev-parse", rev, "--")
	cmd.Dir = repoDir
	if err := cmd.Run(); err == nil {
		return false
	}
	// Revision not found, update before returning.
	s.doRepoUpdate(ctx, repo, url)
	return true
}

// quickRevParseHead best-effort mimics the execution of `git rev-parse HEAD`, but doesn't exec a child process.
// It just reads the relevant files from the bare git repository directory.
func quickRevParseHead(dir string) (string, error) {
	// See if HEAD contains a commit hash and return it if so.
	head, err := ioutil.ReadFile(filepath.Join(dir, "HEAD"))
	if os.IsNotExist(err) {
		dir = filepath.Join(dir, ".git")
		head, err = ioutil.ReadFile(filepath.Join(dir, "HEAD"))
	}
	if err != nil {
		return "", err
	}
	head = bytes.TrimSpace(head)
	if git.IsAbsoluteRevision(string(head)) {
		return string(head), nil
	}

	// HEAD doesn't contain a commit hash. It contains something like "ref: refs/heads/master".
	if !bytes.HasPrefix(head, []byte("ref: ")) {
		return "", errors.New("unrecognized HEAD file format")
	}
	// Look for the file in refs/heads. If it exists, it contains the commit hash.
	headRef := bytes.TrimPrefix(head, []byte("ref: "))
	if bytes.HasPrefix(headRef, []byte("../")) || bytes.Contains(headRef, []byte("/../")) || bytes.HasSuffix(headRef, []byte("/..")) {
		// 🚨 SECURITY: prevent leakage of file contents outside repo dir
		return "", fmt.Errorf("invalid ref format: %s", headRef)
	}
	headRefFile := filepath.Join(dir, filepath.FromSlash(string(headRef)))
	if refs, err := ioutil.ReadFile(headRefFile); err == nil {
		return string(bytes.TrimSpace(refs)), nil
	}

	// File didn't exist in refs/heads. Look for it in packed-refs.
	f, err := os.Open(filepath.Join(dir, "packed-refs"))
	if err != nil {
		return "", err
	}
	scanner := bufio.NewScanner(f)
	for scanner.Scan() {
		fields := bytes.Fields(scanner.Bytes())
		if len(fields) != 2 {
			continue
		}
		commit, ref := fields[0], fields[1]
		if bytes.Equal(ref, headRef) {
			return string(commit), nil
		}
	}
	if err := scanner.Err(); err != nil {
		return "", err
	}

	// Didn't find the refs/heads/$HEAD_BRANCH in packed_refs
	return "", errors.New("could not compute `git rev-parse HEAD` in-process, try running `git` process")
}
