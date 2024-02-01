package gitserver

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"io/fs"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
	"go.opentelemetry.io/otel/attribute"
	"google.golang.org/grpc/status"

	"github.com/sourcegraph/conc/pool"
	"github.com/sourcegraph/go-diff/diff"
	sglog "github.com/sourcegraph/log"
	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitolite"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/protocol"
	proto "github.com/sourcegraph/sourcegraph/internal/gitserver/v1"
	"github.com/sourcegraph/sourcegraph/internal/grpc/streamio"
	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/perforce"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const git = "git"

var ClientMocks, emptyClientMocks struct {
	GetObject               func(repo api.RepoName, objectName string) (*gitdomain.GitObject, error)
	Archive                 func(ctx context.Context, repo api.RepoName, opt ArchiveOptions) (_ io.ReadCloser, err error)
	LocalGitserver          bool
	LocalGitCommandReposDir string
}

// conns is the global variable holding a reference to the gitserver connections.
var conns = &atomicGitServerConns{}

// ResetClientMocks clears the mock functions set on Mocks (so that subsequent
// tests don't inadvertently use them).
func ResetClientMocks() {
	ClientMocks = emptyClientMocks
}

var _ Client = &clientImplementor{}

// ClientSource is a source of gitserver.Client instances.
// It allows for mocking out the client source in tests.
type ClientSource interface {
	// ClientForRepo returns a Client for the given repo.
	ClientForRepo(ctx context.Context, repo api.RepoName) (proto.GitserverServiceClient, error)
	// AddrForRepo returns the address of the gitserver for the given repo.
	AddrForRepo(ctx context.Context, repo api.RepoName) string
	// Address the current list of gitserver addresses.
	Addresses() []AddressWithClient
	// GetAddressWithClient returns the address and client for a gitserver instance.
	// It returns nil if there's no server with that address
	GetAddressWithClient(addr string) AddressWithClient
}

// NewClient returns a new gitserver.Client.
// See Client.Scoped() for info on scoped clients.
func NewClient(scope string) Client {
	logger := sglog.Scoped("GitserverClient")
	return &clientImplementor{
		logger:              logger,
		scope:               scope,
		operations:          getOperations(),
		clientSource:        conns,
		subRepoPermsChecker: authz.DefaultSubRepoPermsChecker,
	}
}

// NewTestClient returns a test client that will us
func NewTestClient(t testing.TB) TestClient {
	logger := logtest.Scoped(t)

	return &clientImplementor{
		logger:              logger,
		scope:               fmt.Sprintf("gitserver.test.%s", t.Name()),
		operations:          newOperations(observation.ContextWithLogger(logger, &observation.TestContext)),
		clientSource:        NewTestClientSource(t, nil),
		subRepoPermsChecker: authz.DefaultSubRepoPermsChecker,
	}
}

type TestClient interface {
	Client
	WithChecker(authz.SubRepoPermissionChecker) TestClient
	WithClientSource(ClientSource) TestClient
}

func (c *clientImplementor) WithChecker(checker authz.SubRepoPermissionChecker) TestClient {
	c.subRepoPermsChecker = checker
	return c
}

func (c *clientImplementor) WithClientSource(cs ClientSource) TestClient {
	c.clientSource = cs
	return c
}

// NewMockClientWithExecReader return new MockClient with provided mocked
// behaviour of ExecReader function.
func NewMockClientWithExecReader(checker authz.SubRepoPermissionChecker, execReader func(context.Context, api.RepoName, []string) (io.ReadCloser, error)) *MockClient {
	client := NewMockClient()
	// NOTE: This hook is the same as DiffFunc, but with `execReader` used above
	client.DiffFunc.SetDefaultHook(func(ctx context.Context, opts DiffOptions) (*DiffFileIterator, error) {
		if opts.Base == DevNullSHA {
			opts.RangeType = ".."
		} else if opts.RangeType != ".." {
			opts.RangeType = "..."
		}

		rangeSpec := opts.Base + opts.RangeType + opts.Head
		if strings.HasPrefix(rangeSpec, "-") || strings.HasPrefix(rangeSpec, ".") {
			return nil, errors.Errorf("invalid diff range argument: %q", rangeSpec)
		}

		// Here is where all the mocking happens!
		rdr, err := execReader(ctx, opts.Repo, append([]string{
			"diff",
			"--find-renames",
			"--full-index",
			"--inter-hunk-context=3",
			"--no-prefix",
			rangeSpec,
			"--",
		}, opts.Paths...))
		if err != nil {
			return nil, errors.Wrap(err, "executing git diff")
		}

		return &DiffFileIterator{
			rdr:            rdr,
			mfdr:           diff.NewMultiFileDiffReader(rdr),
			fileFilterFunc: getFilterFunc(ctx, checker, opts.Repo),
		}, nil
	})

	// NOTE: This hook is the same as DiffPath, but with `execReader` used above
	client.DiffPathFunc.SetDefaultHook(func(ctx context.Context, repo api.RepoName, sourceCommit, targetCommit, path string) ([]*diff.Hunk, error) {
		a := actor.FromContext(ctx)
		if hasAccess, err := authz.FilterActorPath(ctx, checker, a, repo, path); err != nil {
			return nil, err
		} else if !hasAccess {
			return nil, os.ErrNotExist
		}
		// Here is where all the mocking happens!
		reader, err := execReader(ctx, repo, []string{"diff", sourceCommit, targetCommit, "--", path})
		if err != nil {
			return nil, err
		}
		defer reader.Close()

		output, err := io.ReadAll(reader)
		if err != nil {
			return nil, err
		}
		if len(output) == 0 {
			return nil, nil
		}

		d, err := diff.NewFileDiffReader(bytes.NewReader(output)).Read()
		if err != nil {
			return nil, err
		}
		return d.Hunks, nil
	})

	return client
}

// clientImplementor is a gitserver client.
type clientImplementor struct {
	// the current scope of the client.
	scope string

	// logger is used for all logging and logger creation
	logger sglog.Logger

	// operations are used for internal observability
	operations *operations

	// clientSource is used to get the corresponding gprc client or address for a given repository
	clientSource ClientSource

	// subRepoPermsChecker is sub-repository permissions checker. This will
	// usually be authz.DefaultSubRepoPermsChecker, at least until that global is removed.
	subRepoPermsChecker authz.SubRepoPermissionChecker
}

func (c *clientImplementor) Scoped(scope string) Client {
	return &clientImplementor{
		logger:       c.logger,
		scope:        appendScope(c.scope, scope),
		operations:   c.operations,
		clientSource: c.clientSource,
	}
}

func appendScope(existing, new string) string {
	if existing == "" {
		return new
	}
	return existing + "." + new
}

type HunkReader interface {
	Read() (*gitdomain.Hunk, error)
	Close() error
}

// BlameOptions configures a blame.
type BlameOptions struct {
	NewestCommit     api.CommitID `json:",omitempty" url:",omitempty"`
	IgnoreWhitespace bool         `json:",omitempty" url:",omitempty"`
	StartLine        int          `json:",omitempty" url:",omitempty"` // 1-indexed start line (or 0 for beginning of file)
	EndLine          int          `json:",omitempty" url:",omitempty"` // 1-indexed end line (or 0 for end of file)
}

func (o *BlameOptions) Attrs() []attribute.KeyValue {
	return []attribute.KeyValue{
		attribute.String("newestCommit", string(o.NewestCommit)),
		attribute.Int("startLine", o.StartLine),
		attribute.Int("endLine", o.EndLine),
		attribute.Bool("ignoreWhitespace", o.IgnoreWhitespace),
	}
}

type CommitLog struct {
	AuthorEmail  string
	AuthorName   string
	Timestamp    time.Time
	SHA          string
	ChangedFiles []string
}

type Client interface {
	// Scoped adds a usage scope to the client and returns a new client with that scope.
	// Usage scopes should be descriptive and be lowercase plaintext, eg. batches.reconciler.
	// Scopes should get more specific as they get nested, eg:
	// batches.reconciler
	// batches.reconciler.processor
	// The Scoped method adds a single scope, so for gitserver.NewClient("batches").Scoped("reconciler")
	// the scope name will be batches.reconciler.
	// You may also use gitserver.NewClient("batches.reconciler") directly, where
	// useful.
	// We use scopes to add context to logs and metrics.
	Scoped(scope string) Client

	// AddrForRepo returns the gitserver address to use for the given repo name.
	AddrForRepo(ctx context.Context, repoName api.RepoName) string

	// ArchiveReader streams back the file contents of an archived git repo.
	ArchiveReader(ctx context.Context, repo api.RepoName, options ArchiveOptions) (io.ReadCloser, error)

	// StreamBlameFile returns Git blame information about a file in a streaming fashion.
	StreamBlameFile(ctx context.Context, repo api.RepoName, path string, opt *BlameOptions) (HunkReader, error)

	// CreateCommitFromPatch will attempt to create a commit from a patch
	// If possible, the error returned will be of type protocol.CreateCommitFromPatchError
	CreateCommitFromPatch(context.Context, protocol.CreateCommitFromPatchRequest) (*protocol.CreateCommitFromPatchResponse, error)

	// GetDefaultBranch returns the name of the default branch and the commit it's
	// currently at from the given repository. If short is true, then `main` instead
	// of `refs/heads/main` would be returned.
	//
	// If the repository is empty or currently being cloned, empty values and no
	// error are returned.
	GetDefaultBranch(ctx context.Context, repo api.RepoName, short bool) (refName string, commit api.CommitID, err error)

	// GetObject fetches git object data in the supplied repo
	GetObject(ctx context.Context, repo api.RepoName, objectName string) (*gitdomain.GitObject, error)

	// HasCommitAfter indicates the staleness of a repository. It returns a boolean indicating if a repository
	// contains a commit past a specified date.
	HasCommitAfter(ctx context.Context, repo api.RepoName, date string, revspec string) (bool, error)

	// IsRepoCloneable returns nil if the repository is cloneable.
	IsRepoCloneable(context.Context, api.RepoName) error

	// ListRefs returns a list of all refs in the repository.
	ListRefs(ctx context.Context, repo api.RepoName) ([]gitdomain.Ref, error)

	// ListBranches returns a list of all branches in the repository.
	ListBranches(ctx context.Context, repo api.RepoName) ([]*gitdomain.Branch, error)

	// MergeBase returns the merge base commit sha for the specified revspecs.
	MergeBase(ctx context.Context, repo api.RepoName, base, head string) (api.CommitID, error)

	// Remove removes the repository clone from gitserver.
	Remove(context.Context, api.RepoName) error

	RepoCloneProgress(context.Context, ...api.RepoName) (*protocol.RepoCloneProgressResponse, error)

	// ResolveRevision will return the absolute commit for a commit-ish spec. If spec is empty, HEAD is
	// used.
	//
	// Error cases:
	// * Repo does not exist: gitdomain.RepoNotExistError
	// * Commit does not exist: gitdomain.RevisionNotFoundError
	// * Empty repository: gitdomain.RevisionNotFoundError
	// * Other unexpected errors.
	ResolveRevision(ctx context.Context, repo api.RepoName, spec string, opt ResolveRevisionOptions) (api.CommitID, error)

	// ResolveRevisions expands a set of RevisionSpecifiers into an equivalent set of commit hashes.
	ResolveRevisions(_ context.Context, repo api.RepoName, _ []protocol.RevisionSpecifier) ([]string, error)

	// RequestRepoUpdate is the new protocol endpoint for synchronous requests
	// with more detailed responses. Do not use this if you are not repo-updater.
	//
	// Repo updates are not guaranteed to occur. If a repo has been updated
	// recently (within the Since duration specified in the request), the
	// update won't happen.
	RequestRepoUpdate(context.Context, api.RepoName, time.Duration) (*protocol.RepoUpdateResponse, error)

	// RequestRepoClone is an asynchronous request to clone a repository.
	RequestRepoClone(context.Context, api.RepoName) (*protocol.RepoCloneResponse, error)

	// Search executes a search as specified by args, streaming the results as
	// it goes by calling onMatches with each set of results it receives in
	// response.
	Search(_ context.Context, _ *protocol.SearchRequest, onMatches func([]protocol.CommitMatch)) (limitHit bool, _ error)

	// Stat returns a FileInfo describing the named file at commit.
	Stat(ctx context.Context, repo api.RepoName, commit api.CommitID, path string) (fs.FileInfo, error)

	// DiffPath returns a position-ordered slice of changes (additions or deletions)
	// of the given path between the given source and target commits.
	DiffPath(ctx context.Context, repo api.RepoName, sourceCommit, targetCommit, path string) ([]*diff.Hunk, error)

	// ReadDir reads the contents of the named directory at commit.
	ReadDir(ctx context.Context, repo api.RepoName, commit api.CommitID, path string, recurse bool) ([]fs.FileInfo, error)

	// NewFileReader returns an io.ReadCloser reading from the named file at commit.
	// The caller should always close the reader after use.
	//
	// If you just need to check a file's existence, use Stat, not a file reader.
	//
	// If the file doesn't exist, the returned error will pass the os.IsNotExist()
	// check.
	//
	// If the path points to a submodule, a reader for an empty file is returned
	// (ie. io.EOF is returned immediately).
	NewFileReader(ctx context.Context, repo api.RepoName, commit api.CommitID, name string) (io.ReadCloser, error)

	// DiffSymbols performs a diff command which is expected to be parsed by our symbols package
	DiffSymbols(ctx context.Context, repo api.RepoName, commitA, commitB api.CommitID) ([]byte, error)

	// Commits returns all commits matching the options.
	Commits(ctx context.Context, repo api.RepoName, opt CommitsOptions) ([]*gitdomain.Commit, error)

	// FirstEverCommit returns the first commit ever made to the repository.
	FirstEverCommit(ctx context.Context, repo api.RepoName) (*gitdomain.Commit, error)

	// ListTags returns a list of all tags in the repository. If commitObjs is non-empty, only all tags pointing at those commits are returned.
	ListTags(ctx context.Context, repo api.RepoName, commitObjs ...string) ([]*gitdomain.Tag, error)

	// ListDirectoryChildren fetches the list of children under the given directory
	// names. The result is a map keyed by the directory names with the list of files
	// under each.
	ListDirectoryChildren(ctx context.Context, repo api.RepoName, commit api.CommitID, dirnames []string) (map[string][]string, error)

	// Diff returns an iterator that can be used to access the diff between two
	// commits on a per-file basis. The iterator must be closed with Close when no
	// longer required.
	Diff(ctx context.Context, opts DiffOptions) (*DiffFileIterator, error)

	// BranchesContaining returns a map from branch names to branch tip hashes for
	// each branch containing the given commit.
	// The returned branches will be in short form (e.g. "master" instead of
	// "refs/heads/master").
	BranchesContaining(ctx context.Context, repo api.RepoName, commit api.CommitID) ([]string, error)

	// RefDescriptions returns a map from commits to descriptions of the tip of each
	// branch and tag of the given repository.
	RefDescriptions(ctx context.Context, repo api.RepoName, gitObjs ...string) (map[string][]gitdomain.RefDescription, error)

	// CommitGraph returns the commit graph for the given repository as a mapping
	// from a commit to its parents. If a commit is supplied, the returned graph will
	// be rooted at the given commit. If a non-zero limit is supplied, at most that
	// many commits will be returned.
	CommitGraph(ctx context.Context, repo api.RepoName, opts CommitGraphOptions) (_ *gitdomain.CommitGraph, err error)

	// CommitLog returns the repository commit log, including the file paths that were changed. The general approach to parsing
	// is to separate the first line (the metadata line) from the remaining lines (the files), and then parse the metadata line
	// into component parts separately.
	CommitLog(ctx context.Context, repo api.RepoName, after time.Time) ([]CommitLog, error)

	// CommitsUniqueToBranch returns a map from commits that exist on a particular
	// branch in the given repository to their committer date. This set of commits is
	// determined by listing `{branchName} ^HEAD`, which is interpreted as: all
	// commits on {branchName} not also on the tip of the default branch. If the
	// supplied branch name is the default branch, then this method instead returns
	// all commits reachable from HEAD.
	CommitsUniqueToBranch(ctx context.Context, repo api.RepoName, branchName string, isDefaultBranch bool, maxAge *time.Time) (map[string]time.Time, error)

	// LsFiles returns the output of `git ls-files`.
	LsFiles(ctx context.Context, repo api.RepoName, commit api.CommitID, pathspecs ...gitdomain.Pathspec) ([]string, error)

	// GetCommit returns the commit with the given commit ID, or RevisionNotFoundError if no such commit
	// exists.
	GetCommit(ctx context.Context, repo api.RepoName, id api.CommitID) (*gitdomain.Commit, error)

	// GetBehindAhead returns the behind/ahead commit counts information for right vs. left (both Git
	// revspecs).
	GetBehindAhead(ctx context.Context, repo api.RepoName, left, right string) (*gitdomain.BehindAhead, error)

	// ContributorCount returns the number of commits grouped by contributor
	ContributorCount(ctx context.Context, repo api.RepoName, opt ContributorOptions) ([]*gitdomain.ContributorCount, error)

	// LogReverseEach runs git log in reverse order and calls the given callback for each entry.
	LogReverseEach(ctx context.Context, repo string, commit string, n int, onLogEntry func(entry gitdomain.LogEntry) error) error

	// RevList makes a git rev-list call and iterates through the resulting commits, calling the provided
	// onCommit function for each.
	RevList(ctx context.Context, repo string, commit string, onCommit func(commit string) (bool, error)) error

	// SystemsInfo returns information about all gitserver instances associated with a Sourcegraph instance.
	SystemsInfo(ctx context.Context) ([]protocol.SystemInfo, error)

	// SystemInfo returns information about the gitserver instance at the given address.
	SystemInfo(ctx context.Context, addr string) (protocol.SystemInfo, error)

	// IsPerforcePathCloneable checks if the given Perforce depot path is cloneable by
	// checking if it is a valid depot and the given user has permission to access it.
	IsPerforcePathCloneable(ctx context.Context, conn protocol.PerforceConnectionDetails, depotPath string) error

	// CheckPerforceCredentials checks if the given Perforce credentials are valid
	CheckPerforceCredentials(ctx context.Context, conn protocol.PerforceConnectionDetails) error

	// PerforceUsers lists all the users known to the given Perforce server.
	PerforceUsers(ctx context.Context, conn protocol.PerforceConnectionDetails) ([]*perforce.User, error)

	// PerforceProtectsForUser returns all protects that apply to the given Perforce user.
	PerforceProtectsForUser(ctx context.Context, conn protocol.PerforceConnectionDetails, username string) ([]*perforce.Protect, error)

	// PerforceProtectsForDepot returns all protects that apply to the given Perforce depot.
	PerforceProtectsForDepot(ctx context.Context, conn protocol.PerforceConnectionDetails, depot string) ([]*perforce.Protect, error)

	// PerforceGroupMembers returns the members of the given Perforce group.
	PerforceGroupMembers(ctx context.Context, conn protocol.PerforceConnectionDetails, group string) ([]string, error)

	// IsPerforceSuperUser checks if the given Perforce user is a super user, and otherwise returns an error.
	IsPerforceSuperUser(ctx context.Context, conn protocol.PerforceConnectionDetails) error

	// PerforceGetChangelist gets the perforce changelist details for the given changelist ID.
	PerforceGetChangelist(ctx context.Context, conn protocol.PerforceConnectionDetails, changelist string) (*perforce.Changelist, error)

	// ListGitoliteRepos returns a list of Gitolite repositories. Gitserver owns the SSH keys
	// so is the only service able to talk to gitolite.
	ListGitoliteRepos(ctx context.Context, host string) ([]*gitolite.Repo, error)
}

func (c *clientImplementor) SystemsInfo(ctx context.Context) (_ []protocol.SystemInfo, err error) {
	ctx, _, endObservation := c.operations.systemsInfo.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
	})
	defer endObservation(1, observation.Args{})

	addresses := c.clientSource.Addresses()

	wg := pool.NewWithResults[protocol.SystemInfo]().WithErrors().WithContext(ctx)

	for _, addr := range addresses {
		addr := addr // capture addr
		wg.Go(func(ctx context.Context) (protocol.SystemInfo, error) {
			response, err := c.getDiskInfo(ctx, addr)
			if err != nil {
				return protocol.SystemInfo{}, err
			}
			return protocol.SystemInfo{
				Address:     addr.Address(),
				FreeSpace:   response.GetFreeSpace(),
				TotalSpace:  response.GetTotalSpace(),
				PercentUsed: response.GetPercentUsed(),
			}, nil
		})
	}

	return wg.Wait()
}

func (c *clientImplementor) SystemInfo(ctx context.Context, addr string) (_ protocol.SystemInfo, err error) {
	ctx, _, endObservation := c.operations.systemInfo.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			attribute.String("addr", addr),
		},
	})
	defer endObservation(1, observation.Args{})

	ac := c.clientSource.GetAddressWithClient(addr)
	if ac == nil {
		return protocol.SystemInfo{}, errors.Newf("no client for address: %s", addr)
	}

	response, err := c.getDiskInfo(ctx, ac)
	if err != nil {
		return protocol.SystemInfo{}, err
	}

	return protocol.SystemInfo{
		Address:    ac.Address(),
		FreeSpace:  response.FreeSpace,
		TotalSpace: response.TotalSpace,
	}, nil
}

func (c *clientImplementor) getDiskInfo(ctx context.Context, addr AddressWithClient) (*proto.DiskInfoResponse, error) {
	client, err := addr.GRPCClient()
	if err != nil {
		return nil, err
	}
	resp, err := client.DiskInfo(ctx, &proto.DiskInfoRequest{})
	if err != nil {
		return nil, err
	}
	return resp, nil
}

func (c *clientImplementor) AddrForRepo(ctx context.Context, repo api.RepoName) string {
	return c.clientSource.AddrForRepo(ctx, repo)
}

func (c *clientImplementor) ClientForRepo(ctx context.Context, repo api.RepoName) (proto.GitserverServiceClient, error) {
	return c.clientSource.ClientForRepo(ctx, repo)
}

// ArchiveOptions contains options for the Archive func.
type ArchiveOptions struct {
	Treeish   string               // the tree or commit to produce an archive for
	Format    ArchiveFormat        // format of the resulting archive (usually "tar" or "zip")
	Pathspecs []gitdomain.Pathspec // if nonempty, only include these pathspecs.
}

func (a *ArchiveOptions) Attrs() []attribute.KeyValue {
	specs := make([]string, len(a.Pathspecs))
	for i, pathspec := range a.Pathspecs {
		specs[i] = string(pathspec)
	}
	return []attribute.KeyValue{
		attribute.String("treeish", a.Treeish),
		attribute.String("format", string(a.Format)),
		attribute.StringSlice("pathspecs", specs),
	}
}

func (o *ArchiveOptions) FromProto(x *proto.ArchiveRequest) {
	protoPathSpecs := x.GetPathspecs()
	pathSpecs := make([]gitdomain.Pathspec, 0, len(protoPathSpecs))

	for _, path := range protoPathSpecs {
		pathSpecs = append(pathSpecs, gitdomain.Pathspec(path))
	}

	*o = ArchiveOptions{
		Treeish:   x.GetTreeish(),
		Format:    ArchiveFormat(x.GetFormat()),
		Pathspecs: pathSpecs,
	}
}

func (o *ArchiveOptions) ToProto(repo string) *proto.ArchiveRequest {
	protoPathSpecs := make([]string, 0, len(o.Pathspecs))

	for _, path := range o.Pathspecs {
		protoPathSpecs = append(protoPathSpecs, string(path))
	}

	return &proto.ArchiveRequest{
		Repo:      repo,
		Treeish:   o.Treeish,
		Format:    string(o.Format),
		Pathspecs: protoPathSpecs,
	}
}

// archiveReader wraps the StdoutReader yielded by gitserver's
// RemoteGitCommand.StdoutReader with one that knows how to report a repository-not-found
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
		if isRevisionNotFound(err.Error()) {
			return 0, &gitdomain.RevisionNotFoundError{Repo: a.repo, Spec: a.spec}
		}
	}
	return n, err
}

func (a *archiveReader) Close() error {
	return a.base.Close()
}

func (c *RemoteGitCommand) sendExec(ctx context.Context) (_ io.ReadCloser, err error) {
	ctx, cancel := context.WithCancel(ctx)
	ctx, _, endObservation := c.execOp.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			c.repo.Attr(),
			attribute.StringSlice("args", c.args[1:]),
		},
	})
	done := func() {
		cancel()
		endObservation(1, observation.Args{})
	}
	defer func() {
		if err != nil {
			done()
		}
	}()

	repoName := protocol.NormalizeRepo(c.repo)

	// Check that ctx is not expired.
	if err := ctx.Err(); err != nil {
		deadlineExceededCounter.Inc()
		return nil, err
	}

	client, err := c.execer.ClientForRepo(ctx, repoName)
	if err != nil {
		return nil, err
	}

	req := &proto.ExecRequest{
		Repo:      string(repoName),
		Args:      stringsToByteSlices(c.args[1:]),
		NoTimeout: c.noTimeout,

		// 🚨Warning🚨: There is no guarantee that EnsureRevision is a valid utf-8 string.
		EnsureRevision: []byte(c.EnsureRevision()),
	}

	stream, err := client.Exec(ctx, req)
	if err != nil {
		return nil, err
	}
	r := streamio.NewReader(func() ([]byte, error) {
		msg, err := stream.Recv()
		if err != nil {
			return nil, err
		}
		return msg.GetData(), nil
	})

	return &readCloseWrapper{Reader: r, closeFn: done}, nil
}

type readCloseWrapper struct {
	io.Reader
	closeFn func()
}

func (r *readCloseWrapper) Close() error {
	r.closeFn()
	return nil
}

func isRevisionNotFound(err string) bool {
	// error message is lowercased in to handle case insensitive error messages
	loweredErr := strings.ToLower(err)
	return strings.Contains(loweredErr, "not a valid object")
}

func (c *clientImplementor) Search(ctx context.Context, args *protocol.SearchRequest, onMatches func([]protocol.CommitMatch)) (_ bool, err error) {
	ctx, _, endObservation := c.operations.search.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			args.Repo.Attr(),
			attribute.Stringer("query", args.Query),
			attribute.Bool("diff", args.IncludeDiff),
			attribute.Int("limit", args.Limit),
		},
	})
	defer endObservation(1, observation.Args{})

	repoName := protocol.NormalizeRepo(args.Repo)

	client, err := c.ClientForRepo(ctx, repoName)
	if err != nil {
		return false, err
	}

	cs, err := client.Search(ctx, args.ToProto())
	if err != nil {
		return false, err
	}

	limitHit := false
	for {
		msg, err := cs.Recv()
		if err != nil {
			if errors.Is(err, io.EOF) {
				return limitHit, nil
			}
			return limitHit, err
		}

		switch m := msg.Message.(type) {
		case *proto.SearchResponse_LimitHit:
			limitHit = limitHit || m.LimitHit
		case *proto.SearchResponse_Match:
			onMatches([]protocol.CommitMatch{protocol.CommitMatchFromProto(m.Match)})
		default:
			return false, errors.Newf("unknown message type %T", m)
		}
	}
}

var deadlineExceededCounter = promauto.NewCounter(prometheus.CounterOpts{
	Name: "src_gitserver_client_deadline_exceeded",
	Help: "Times that Client.sendExec() returned context.DeadlineExceeded",
})

func (c *clientImplementor) gitCommand(repo api.RepoName, arg ...string) GitCommand {
	if ClientMocks.LocalGitserver {
		cmd := NewLocalGitCommand(repo, arg...)
		if ClientMocks.LocalGitCommandReposDir != "" {
			cmd.ReposDir = ClientMocks.LocalGitCommandReposDir
		}
		return cmd
	}
	return &RemoteGitCommand{
		repo:   repo,
		execer: c,
		args:   append([]string{git}, arg...),
		execOp: c.operations.exec,
		scope:  c.scope,
	}
}

func (c *clientImplementor) RequestRepoUpdate(ctx context.Context, repo api.RepoName, since time.Duration) (_ *protocol.RepoUpdateResponse, err error) {
	ctx, _, endObservation := c.operations.requestRepoUpdate.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			attribute.Stringer("since", since),
		},
	})
	defer endObservation(1, observation.Args{})

	req := &protocol.RepoUpdateRequest{
		Repo:  repo,
		Since: since,
	}

	client, err := c.ClientForRepo(ctx, repo)
	if err != nil {
		return nil, err
	}

	resp, err := client.RepoUpdate(ctx, req.ToProto())
	if err != nil {
		return nil, err
	}

	var info protocol.RepoUpdateResponse
	info.FromProto(resp)

	return &info, nil
}

// RequestRepoClone requests that the gitserver does an asynchronous clone of the repository.
func (c *clientImplementor) RequestRepoClone(ctx context.Context, repo api.RepoName) (_ *protocol.RepoCloneResponse, err error) {
	ctx, _, endObservation := c.operations.requestRepoClone.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
		},
	})
	defer endObservation(1, observation.Args{})

	client, err := c.ClientForRepo(ctx, repo)
	if err != nil {
		return nil, err
	}

	req := proto.RepoCloneRequest{
		Repo: string(repo),
	}

	resp, err := client.RepoClone(ctx, &req)
	if err != nil {
		return nil, err
	}

	var info protocol.RepoCloneResponse
	info.FromProto(resp)
	return &info, nil
}

// MockIsRepoCloneable mocks (*Client).IsRepoCloneable for tests.
var MockIsRepoCloneable func(api.RepoName) error

func (c *clientImplementor) IsRepoCloneable(ctx context.Context, repo api.RepoName) (err error) {
	ctx, _, endObservation := c.operations.isRepoCloneable.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
		},
	})
	defer endObservation(1, observation.Args{})

	if MockIsRepoCloneable != nil {
		return MockIsRepoCloneable(repo)
	}

	var resp protocol.IsRepoCloneableResponse

	client, err := c.ClientForRepo(ctx, repo)
	if err != nil {
		return err
	}

	req := &proto.IsRepoCloneableRequest{
		Repo: string(repo),
	}

	r, err := client.IsRepoCloneable(ctx, req)
	if err != nil {
		return err
	}

	resp.FromProto(r)

	if resp.Cloneable {
		return nil
	}

	// Treat all 4xx errors as not found, since we have more relaxed
	// requirements on what a valid URL is we should treat bad requests,
	// etc as not found.
	notFound := strings.Contains(resp.Reason, "not found") || strings.Contains(resp.Reason, "The requested URL returned error: 4")
	return &RepoNotCloneableErr{
		repo:     repo,
		reason:   resp.Reason,
		notFound: notFound,
		cloned:   resp.Cloned,
	}
}

// RepoNotCloneableErr is the error that happens when a repository can not be cloned.
type RepoNotCloneableErr struct {
	repo     api.RepoName
	reason   string
	notFound bool
	cloned   bool // Has the repo ever been cloned in the past
}

// NotFound returns true if the repo could not be cloned because it wasn't found.
// This may be because the repo doesn't exist, or because the repo is private and
// there are insufficient permissions.
func (e *RepoNotCloneableErr) NotFound() bool {
	return e.notFound
}

func (e *RepoNotCloneableErr) Error() string {
	msg := "unable to clone repo"
	if e.cloned {
		msg = "unable to update repo"
	}
	return fmt.Sprintf("%s (name=%q notfound=%v) because %s", msg, e.repo, e.notFound, e.reason)
}

func (c *clientImplementor) RepoCloneProgress(ctx context.Context, repos ...api.RepoName) (_ *protocol.RepoCloneProgressResponse, err error) {
	ctx, _, endObservation := c.operations.repoCloneProgress.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			attribute.Int("repos", len(repos)),
		},
	})
	defer endObservation(1, observation.Args{})

	numPossibleShards := len(c.clientSource.Addresses())

	shards := make(map[proto.GitserverServiceClient]*proto.RepoCloneProgressRequest, (len(repos)/numPossibleShards)*2) // 2x because it may not be a perfect division
	for _, r := range repos {
		client, err := c.ClientForRepo(ctx, r)
		if err != nil {
			return nil, err
		}

		shard := shards[client]
		if shard == nil {
			shard = new(proto.RepoCloneProgressRequest)
			shards[client] = shard
		}

		shard.Repos = append(shard.Repos, string(r))
	}

	p := pool.NewWithResults[*proto.RepoCloneProgressResponse]().WithContext(ctx)

	for client, req := range shards {
		client := client
		req := req
		p.Go(func(ctx context.Context) (*proto.RepoCloneProgressResponse, error) {
			return client.RepoCloneProgress(ctx, req)
		})
	}

	res, err := p.Wait()
	if err != nil {
		return nil, err
	}

	result := &protocol.RepoCloneProgressResponse{
		Results: make(map[api.RepoName]*protocol.RepoCloneProgress),
	}
	for _, r := range res {
		for repo, info := range r.Results {
			var rp protocol.RepoCloneProgress
			rp.FromProto(info)
			result.Results[api.RepoName(repo)] = &rp
		}
	}

	return result, nil
}

func (c *clientImplementor) Remove(ctx context.Context, repo api.RepoName) (err error) {
	ctx, _, endObservation := c.operations.remove.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
		},
	})
	defer endObservation(1, observation.Args{})

	// In case the repo has already been deleted from the database we need to pass
	// the old name in order to land on the correct gitserver instance
	undeletedName := api.UndeletedRepoName(repo)

	client, err := c.ClientForRepo(ctx, undeletedName)
	if err != nil {
		return err
	}
	_, err = client.RepoDelete(ctx, &proto.RepoDeleteRequest{
		Repo: string(repo),
	})
	return err
}

func (c *clientImplementor) IsPerforcePathCloneable(ctx context.Context, conn protocol.PerforceConnectionDetails, depotPath string) (err error) {
	ctx, _, endObservation := c.operations.isPerforcePathCloneable.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
	})
	defer endObservation(1, observation.Args{})

	// depotPath is not actually a repo name, but it will spread the load of isPerforcePathCloneable
	// a bit over the different gitserver instances. It's really just used as a consistent hashing
	// key here.
	client, err := c.ClientForRepo(ctx, api.RepoName(depotPath))
	if err != nil {
		return err
	}
	_, err = client.IsPerforcePathCloneable(ctx, &proto.IsPerforcePathCloneableRequest{
		ConnectionDetails: conn.ToProto(),
		DepotPath:         depotPath,
	})
	if err != nil {
		// Unwrap proto errors for nicer error messages.
		if s, ok := status.FromError(err); ok {
			return errors.New(s.Message())
		}
		return err
	}

	return nil
}

func (c *clientImplementor) CheckPerforceCredentials(ctx context.Context, conn protocol.PerforceConnectionDetails) (err error) {
	ctx, _, endObservation := c.operations.checkPerforceCredentials.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
	})
	defer endObservation(1, observation.Args{})

	// p4port is not actually a repo name, but it will spread the load of CheckPerforceCredentials
	// a bit over the different gitserver instances. It's really just used as a consistent hashing
	// key here.
	client, err := c.ClientForRepo(ctx, api.RepoName(conn.P4Port))
	if err != nil {
		return err
	}
	_, err = client.CheckPerforceCredentials(ctx, &proto.CheckPerforceCredentialsRequest{
		ConnectionDetails: conn.ToProto(),
	})
	if err != nil {
		// Unwrap proto errors for nicer error messages.
		if s, ok := status.FromError(err); ok {
			return errors.New(s.Message())
		}
		return err
	}

	return nil
}

func (c *clientImplementor) PerforceUsers(ctx context.Context, conn protocol.PerforceConnectionDetails) (_ []*perforce.User, err error) {
	ctx, _, endObservation := c.operations.perforceUsers.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
	})
	defer endObservation(1, observation.Args{})

	// p4port is not actually a repo name, but it will spread the load of CheckPerforceCredentials
	// a bit over the different gitserver instances. It's really just used as a consistent hashing
	// key here.
	client, err := c.ClientForRepo(ctx, api.RepoName(conn.P4Port))
	if err != nil {
		return nil, err
	}
	resp, err := client.PerforceUsers(ctx, &proto.PerforceUsersRequest{
		ConnectionDetails: conn.ToProto(),
	})
	if err != nil {
		return nil, err
	}

	users := make([]*perforce.User, len(resp.GetUsers()))
	for i, u := range resp.GetUsers() {
		users[i] = perforce.UserFromProto(u)
	}
	return users, nil
}

func (c *clientImplementor) PerforceProtectsForUser(ctx context.Context, conn protocol.PerforceConnectionDetails, username string) (_ []*perforce.Protect, err error) {
	ctx, _, endObservation := c.operations.perforceProtectsForUser.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			attribute.String("username", username),
		},
	})
	defer endObservation(1, observation.Args{})

	// p4port is not actually a repo name, but it will spread the load of CheckPerforceCredentials
	// a bit over the different gitserver instances. It's really just used as a consistent hashing
	// key here.
	client, err := c.ClientForRepo(ctx, api.RepoName(conn.P4Port))
	if err != nil {
		return nil, err
	}
	resp, err := client.PerforceProtectsForUser(ctx, &proto.PerforceProtectsForUserRequest{
		ConnectionDetails: conn.ToProto(),
		Username:          username,
	})
	if err != nil {
		return nil, err
	}

	protects := make([]*perforce.Protect, len(resp.GetProtects()))
	for i, p := range resp.GetProtects() {
		protects[i] = perforce.ProtectFromProto(p)
	}
	return protects, nil
}

func (c *clientImplementor) PerforceProtectsForDepot(ctx context.Context, conn protocol.PerforceConnectionDetails, depot string) (_ []*perforce.Protect, err error) {
	ctx, _, endObservation := c.operations.perforceProtectsForDepot.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			attribute.String("depot", depot),
		},
	})
	defer endObservation(1, observation.Args{})

	// p4port is not actually a repo name, but it will spread the load of CheckPerforceCredentials
	// a bit over the different gitserver instances. It's really just used as a consistent hashing
	// key here.
	client, err := c.ClientForRepo(ctx, api.RepoName(conn.P4Port))
	if err != nil {
		return nil, err
	}
	resp, err := client.PerforceProtectsForDepot(ctx, &proto.PerforceProtectsForDepotRequest{
		ConnectionDetails: conn.ToProto(),
		Depot:             depot,
	})
	if err != nil {
		return nil, err
	}

	protects := make([]*perforce.Protect, len(resp.GetProtects()))
	for i, p := range resp.GetProtects() {
		protects[i] = perforce.ProtectFromProto(p)
	}
	return protects, nil
}

func (c *clientImplementor) PerforceGroupMembers(ctx context.Context, conn protocol.PerforceConnectionDetails, group string) (_ []string, err error) {
	ctx, _, endObservation := c.operations.perforceGroupMembers.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			attribute.String("group", group),
		},
	})
	defer endObservation(1, observation.Args{})

	// p4port is not actually a repo name, but it will spread the load of CheckPerforceCredentials
	// a bit over the different gitserver instances. It's really just used as a consistent hashing
	// key here.
	client, err := c.ClientForRepo(ctx, api.RepoName(conn.P4Port))
	if err != nil {
		return nil, err
	}
	resp, err := client.PerforceGroupMembers(ctx, &proto.PerforceGroupMembersRequest{
		ConnectionDetails: conn.ToProto(),
		Group:             group,
	})
	if err != nil {
		return nil, err
	}

	return resp.GetUsernames(), nil
}

func (c *clientImplementor) IsPerforceSuperUser(ctx context.Context, conn protocol.PerforceConnectionDetails) (err error) {
	ctx, _, endObservation := c.operations.isPerforceSuperUser.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
	})
	defer endObservation(1, observation.Args{})

	// p4port is not actually a repo name, but it will spread the load of CheckPerforceCredentials
	// a bit over the different gitserver instances. It's really just used as a consistent hashing
	// key here.
	client, err := c.ClientForRepo(ctx, api.RepoName(conn.P4Port))
	if err != nil {
		return err
	}
	_, err = client.IsPerforceSuperUser(ctx, &proto.IsPerforceSuperUserRequest{
		ConnectionDetails: conn.ToProto(),
	})
	return err
}

func (c *clientImplementor) PerforceGetChangelist(ctx context.Context, conn protocol.PerforceConnectionDetails, changelist string) (_ *perforce.Changelist, err error) {
	ctx, _, endObservation := c.operations.perforceGetChangelist.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			attribute.String("changelist", changelist),
		},
	})
	defer endObservation(1, observation.Args{})

	// p4port is not actually a repo name, but it will spread the load of CheckPerforceCredentials
	// a bit over the different gitserver instances. It's really just used as a consistent hashing
	// key here.
	client, err := c.ClientForRepo(ctx, api.RepoName(conn.P4Port))
	if err != nil {
		return nil, err
	}
	resp, err := client.PerforceGetChangelist(ctx, &proto.PerforceGetChangelistRequest{
		ConnectionDetails: conn.ToProto(),
		ChangelistId:      changelist,
	})
	if err != nil {
		return nil, err
	}

	return perforce.ChangelistFromProto(resp.GetChangelist()), nil
}

func (c *clientImplementor) CreateCommitFromPatch(ctx context.Context, req protocol.CreateCommitFromPatchRequest) (_ *protocol.CreateCommitFromPatchResponse, err error) {
	ctx, _, endObservation := c.operations.createCommitFromPatch.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			req.Repo.Attr(),
		},
	})
	defer endObservation(1, observation.Args{})

	client, err := c.ClientForRepo(ctx, req.Repo)
	if err != nil {
		return nil, err
	}

	cc, err := client.CreateCommitFromPatchBinary(ctx)
	if err != nil {
		st, ok := status.FromError(err)
		if ok {
			for _, detail := range st.Details() {
				switch dt := detail.(type) {
				case *proto.CreateCommitFromPatchError:
					var e protocol.CreateCommitFromPatchError
					e.FromProto(dt)
					return nil, &e
				}
			}
		}
		return nil, err
	}

	// Send the metadata event first.
	if err := cc.Send(&proto.CreateCommitFromPatchBinaryRequest{Payload: &proto.CreateCommitFromPatchBinaryRequest_Metadata_{
		Metadata: req.ToMetadataProto(),
	}}); err != nil {
		return nil, errors.Wrap(err, "sending metadata")
	}

	// Then create a writer that sends data in chunks that won't exceed the maximum
	// message size of gRPC of the patch in separate events.
	w := streamio.NewWriter(func(p []byte) error {
		req := &proto.CreateCommitFromPatchBinaryRequest{
			Payload: &proto.CreateCommitFromPatchBinaryRequest_Patch_{
				Patch: &proto.CreateCommitFromPatchBinaryRequest_Patch{
					Data: p,
				},
			},
		}
		return cc.Send(req)
	})

	if _, err := w.Write(req.Patch); err != nil {
		return nil, errors.Wrap(err, "writing chunk of patch")
	}

	resp, err := cc.CloseAndRecv()
	if err != nil {
		st, ok := status.FromError(err)
		if !ok {
			return nil, err
		}

		for _, detail := range st.Details() {
			switch dt := detail.(type) {
			case *proto.CreateCommitFromPatchError:
				var e protocol.CreateCommitFromPatchError
				e.FromProto(dt)
				return nil, &e
			}
		}

		return nil, err
	}

	var res protocol.CreateCommitFromPatchResponse
	res.FromProto(resp, nil)

	return &res, nil
}

func (c *clientImplementor) GetObject(ctx context.Context, repo api.RepoName, objectName string) (_ *gitdomain.GitObject, err error) {
	ctx, _, endObservation := c.operations.getObject.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			attribute.String("name", objectName),
		},
	})
	defer endObservation(1, observation.Args{})

	if ClientMocks.GetObject != nil {
		return ClientMocks.GetObject(repo, objectName)
	}

	req := protocol.GetObjectRequest{
		Repo:       repo,
		ObjectName: objectName,
	}
	client, err := c.ClientForRepo(ctx, req.Repo)
	if err != nil {
		return nil, err
	}

	grpcResp, err := client.GetObject(ctx, req.ToProto())
	if err != nil {
		return nil, err
	}

	var res protocol.GetObjectResponse
	res.FromProto(grpcResp)

	return &res.Object, nil
}

var ambiguousArgPattern = lazyregexp.New(`ambiguous argument '([^']+)'`)

func (c *clientImplementor) ResolveRevisions(ctx context.Context, repo api.RepoName, revs []protocol.RevisionSpecifier) (_ []string, err error) {
	ctx, _, endObservation := c.operations.resolveRevisions.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			attribute.Int("revs", len(revs)),
		},
	})
	defer endObservation(1, observation.Args{})

	args := append([]string{"rev-parse"}, revsToGitArgs(revs)...)

	cmd := c.gitCommand(repo, args...)
	stdout, stderr, err := cmd.DividedOutput(ctx)
	if err != nil {
		if gitdomain.IsRepoNotExist(err) {
			return nil, err
		}
		if match := ambiguousArgPattern.FindSubmatch(stderr); match != nil {
			return nil, &gitdomain.RevisionNotFoundError{Repo: repo, Spec: string(match[1])}
		}
		return nil, errors.WithMessage(err, fmt.Sprintf("git command %v failed (stderr: %q)", cmd.Args(), stderr))
	}

	return strings.Fields(string(stdout)), nil
}

func revsToGitArgs(revSpecs []protocol.RevisionSpecifier) []string {
	args := make([]string, 0, len(revSpecs))
	for _, r := range revSpecs {
		if r.RevSpec != "" {
			args = append(args, r.RevSpec)
		} else {
			args = append(args, "HEAD")
		}
	}

	// If revSpecs is empty, git treats it as equivalent to HEAD
	if len(revSpecs) == 0 {
		args = append(args, "HEAD")
	}
	return args
}

// readResponseBody will attempt to read the body of the HTTP response and return it as a
// string. However, in the unlikely scenario that it fails to read the body, it will encode and
// return the error message as a string.
//
// This allows us to use this function directly without yet another if err != nil check. As a
// result, this function should **only** be used when we're attempting to return the body's content
// as part of an error. In such scenarios we don't need to return the potential error from reading
// the body, but can get away with returning that error as a string itself.
//
// This is an unusual pattern of not returning an error. Be careful of replicating this in other
// parts of the code.
func readResponseBody(body io.Reader) string {
	content, err := io.ReadAll(body)
	if err != nil {
		return fmt.Sprintf("failed to read response body, error: %v", err)
	}

	// strings.TrimSpace is needed to remove trailing \n characters that is added by the
	// server. We use http.Error in the server which in turn uses fmt.Fprintln to format
	// the error message. And in translation that newline gets escaped into a \n
	// character.  For what the error message would look in the UI without
	// strings.TrimSpace, see attached screenshots in this pull request:
	// https://github.com/sourcegraph/sourcegraph/pull/39358.
	return strings.TrimSpace(string(content))
}

func stringsToByteSlices(in []string) [][]byte {
	res := make([][]byte, len(in))
	for i, s := range in {
		res[i] = []byte(s)
	}
	return res
}

func (c *clientImplementor) ListGitoliteRepos(ctx context.Context, gitoliteHost string) (list []*gitolite.Repo, err error) {
	client, err := c.ClientForRepo(ctx, api.RepoName(gitoliteHost))
	if err != nil {
		return nil, err
	}

	req := &proto.ListGitoliteRequest{
		GitoliteHost: gitoliteHost,
	}

	grpcResp, err := client.ListGitolite(ctx, req)
	if err != nil {
		return nil, err
	}

	list = make([]*gitolite.Repo, len(grpcResp.Repos))
	for i, r := range grpcResp.GetRepos() {
		list[i] = &gitolite.Repo{
			Name: r.GetName(),
			URL:  r.GetUrl(),
		}
	}

	return list, nil
}
