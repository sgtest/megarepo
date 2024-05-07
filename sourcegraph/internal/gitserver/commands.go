package gitserver

import (
	"bufio"
	"bytes"
	"context"
	"encoding/hex"
	"fmt"
	"io"
	"io/fs"
	"os"
	stdlibpath "path"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/go-git/go-git/v5/plumbing/format/config"
	"github.com/golang/groupcache/lru"
	"github.com/sourcegraph/go-diff/diff"
	"go.opentelemetry.io/otel/attribute"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"
	"google.golang.org/protobuf/types/known/timestamppb"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/fileutil"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	proto "github.com/sourcegraph/sourcegraph/internal/gitserver/v1"
	"github.com/sourcegraph/sourcegraph/internal/grpc/streamio"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/pointers"
)

// Diff returns an iterator that can be used to access the diff between two
// commits on a per-file basis. The iterator must be closed with Close when no
// longer required.
func (c *clientImplementor) Diff(ctx context.Context, repo api.RepoName, opts DiffOptions) (_ *DiffFileIterator, err error) {
	ctx, _, endObservation := c.operations.diff.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
		},
	})

	req := &proto.RawDiffRequest{
		RepoName:    string(repo),
		BaseRevSpec: []byte(opts.Base),
		HeadRevSpec: []byte(opts.Head),
		Paths:       stringsToByteSlices(opts.Paths),
	}

	// Rare case: the base is the empty tree, in which case we must use ..
	// instead of ... as the latter only works for commits.
	if opts.Base == DevNullSHA {
		req.ComparisonType = proto.RawDiffRequest_COMPARISON_TYPE_ONLY_IN_HEAD
	} else if opts.RangeType != ".." {
		req.ComparisonType = proto.RawDiffRequest_COMPARISON_TYPE_INTERSECTION
	} else {
		req.ComparisonType = proto.RawDiffRequest_COMPARISON_TYPE_ONLY_IN_HEAD
	}

	client, err := c.clientSource.ClientForRepo(ctx, repo)
	if err != nil {
		endObservation(1, observation.Args{})
		return nil, err
	}

	ctx, cancel := context.WithCancel(ctx)

	cc, err := client.RawDiff(ctx, req)
	if err != nil {
		cancel()
		endObservation(1, observation.Args{})
		return nil, err
	}

	// We start by reading the first message to early-exit on potential errors.
	firstResp, firstRespErr := cc.Recv()
	if firstRespErr != nil {
		if errors.HasType(firstRespErr, &gitdomain.RevisionNotFoundError{}) {
			cancel()
			err = firstRespErr
			endObservation(1, observation.Args{})
			return nil, err
		}
	}

	firstRespRead := false
	r := streamio.NewReader(func() ([]byte, error) {
		if !firstRespRead {
			firstRespRead = true
			if firstRespErr != nil {
				return nil, firstRespErr
			}
			return firstResp.GetChunk(), nil
		}

		m, err := cc.Recv()
		if err != nil {
			return nil, err
		}
		return m.GetChunk(), nil
	})

	return &DiffFileIterator{
		onClose: func() {
			cancel()
			endObservation(1, observation.Args{})

		},
		mfdr:           diff.NewMultiFileDiffReader(r),
		fileFilterFunc: getFilterFunc(ctx, c.subRepoPermsChecker, repo),
	}, nil
}

type DiffFileIterator struct {
	onClose        func()
	mfdr           *diff.MultiFileDiffReader
	fileFilterFunc diffFileIteratorFilter
}

func NewDiffFileIterator(rdr io.ReadCloser) *DiffFileIterator {
	return &DiffFileIterator{
		mfdr: diff.NewMultiFileDiffReader(rdr),
	}
}

type diffFileIteratorFilter func(fileName string) (bool, error)

func getFilterFunc(ctx context.Context, checker authz.SubRepoPermissionChecker, repo api.RepoName) diffFileIteratorFilter {
	if !authz.SubRepoEnabled(checker) {
		return nil
	}
	return func(fileName string) (bool, error) {
		shouldFilter, err := authz.FilterActorPath(ctx, checker, actor.FromContext(ctx), repo, fileName)
		if err != nil {
			return false, err
		}
		return shouldFilter, nil
	}
}

func (i *DiffFileIterator) Close() error {
	if i.onClose != nil {
		i.onClose()
	}
	return nil
}

// Next returns the next file diff. If no more diffs are available, the diff
// will be nil and the error will be io.EOF.
func (i *DiffFileIterator) Next() (*diff.FileDiff, error) {
	fd, err := i.mfdr.ReadFile()
	if err != nil {
		return fd, err
	}
	if i.fileFilterFunc != nil {
		if canRead, err := i.fileFilterFunc(fd.NewName); err != nil {
			return nil, err
		} else if !canRead {
			// go to next
			return i.Next()
		}
	}
	return fd, err
}

// ContributorOptions contains options for filtering contributor commit counts
type ContributorOptions struct {
	Range string    // the range for which stats will be fetched
	After time.Time // the date after which to collect commits
	Path  string    // compute stats for commits that touch this path
}

func (o *ContributorOptions) Attrs() []attribute.KeyValue {
	return []attribute.KeyValue{
		attribute.String("range", o.Range),
		attribute.String("after", o.After.Format(time.RFC3339)),
		attribute.String("path", o.Path),
	}
}

func (c *clientImplementor) ContributorCount(ctx context.Context, repo api.RepoName, opt ContributorOptions) (_ []*gitdomain.ContributorCount, err error) {
	ctx, _, endObservation := c.operations.contributorCount.With(ctx,
		&err,
		observation.Args{
			Attrs:             opt.Attrs(),
			MetricLabelValues: []string{c.scope},
		},
	)
	defer endObservation(1, observation.Args{})

	client, err := c.clientSource.ClientForRepo(ctx, repo)
	if err != nil {
		return nil, err
	}

	res, err := client.ContributorCounts(ctx, &proto.ContributorCountsRequest{
		RepoName: string(repo),
		Range:    []byte(opt.Range),
		After:    timestamppb.New(opt.After),
		Path:     []byte(opt.Path),
	})
	if err != nil {
		return nil, err
	}

	counts := make([]*gitdomain.ContributorCount, len(res.GetCounts()))
	for i, c := range res.GetCounts() {
		counts[i] = gitdomain.ContributorCountFromProto(c)
	}

	return counts, nil
}

// checkSpecArgSafety returns a non-nil err if spec begins with a "-", which
// could cause it to be interpreted as a git command line argument.
func checkSpecArgSafety(spec string) error {
	if strings.HasPrefix(spec, "-") {
		return errors.Errorf("invalid git revision spec %q (begins with '-')", spec)
	}
	return nil
}

// CommitLog returns the repository commit log, including the file paths that were changed. The general approach to parsing
// is to separate the first line (the metadata line) from the remaining lines (the files), and then parse the metadata line
// into component parts separately.
func (c *clientImplementor) CommitLog(ctx context.Context, repo api.RepoName, after time.Time) (_ []CommitLog, err error) {
	ctx, _, endObservation := c.operations.commitLog.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			attribute.Stringer("after", after),
		},
	})
	defer endObservation(1, observation.Args{})

	args := []string{"log", "--pretty=format:%H<!>%ae<!>%an<!>%ad", "--name-only", "--topo-order", "--no-merges"}
	if !after.IsZero() {
		args = append(args, fmt.Sprintf("--after=%s", after.Format(time.RFC3339)))
	}

	cmd := c.gitCommand(repo, args...)
	out, err := cmd.CombinedOutput(ctx)
	if err != nil {
		return nil, errors.Wrapf(err, "gitCommand %s", string(out))
	}

	var ls []CommitLog
	lines := strings.Split(string(out), "\n\n")

	for _, logOutput := range lines {
		partitions := strings.Split(logOutput, "\n")
		if len(partitions) < 2 {
			continue
		}
		metaLine := partitions[0]
		var changedFiles []string
		for _, pt := range partitions[1:] {
			if pt != "" {
				changedFiles = append(changedFiles, pt)
			}
		}

		parts := strings.Split(metaLine, "<!>")
		if len(parts) != 4 {
			continue
		}
		sha, authorEmail, authorName, timestamp := parts[0], parts[1], parts[2], parts[3]
		t, err := parseTimestamp(timestamp)
		if err != nil {
			return nil, errors.Wrapf(err, "parseTimestamp %s", timestamp)
		}
		ls = append(ls, CommitLog{
			SHA:          sha,
			AuthorEmail:  authorEmail,
			AuthorName:   authorName,
			Timestamp:    t,
			ChangedFiles: changedFiles,
		})
	}
	return ls, nil
}

func parseTimestamp(timestamp string) (time.Time, error) {
	layout := "Mon Jan 2 15:04:05 2006 -0700"
	t, err := time.Parse(layout, timestamp)
	if err != nil {
		return time.Time{}, err
	}
	return t, nil
}

// DevNullSHA 4b825dc642cb6eb9a060e54bf8d69288fbee4904 is `git hash-object -t
// tree /dev/null`, which is used as the base when computing the `git diff` of
// the root commit.
const DevNullSHA = "4b825dc642cb6eb9a060e54bf8d69288fbee4904"

func (c *clientImplementor) ChangedFiles(ctx context.Context, repo api.RepoName, base, head string) (iterator ChangedFilesIterator, err error) {
	ctx, _, endObservation := c.operations.changedFiles.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			attribute.String("base", base),
			attribute.String("head", head),
		},
	})
	defer endObservation(1, observation.Args{})

	client, err := c.clientSource.ClientForRepo(ctx, repo)
	if err != nil {
		return nil, err
	}

	ctx, cancel := context.WithCancel(ctx)

	stream, err := client.ChangedFiles(ctx, &proto.ChangedFilesRequest{
		RepoName: string(repo),
		Base:     []byte(base),
		Head:     []byte(head),
	})
	if err != nil {
		cancel()
		return nil, err
	}

	fetchFunc := func() ([]gitdomain.PathStatus, error) {
		resp, err := stream.Recv()
		if err != nil {
			cancel()
			return nil, err
		}

		protoFiles := resp.GetFiles()

		changes := make([]gitdomain.PathStatus, 0, len(protoFiles))
		for _, p := range protoFiles {
			changes = append(changes, gitdomain.PathStatusFromProto(p))
		}

		return changes, nil
	}

	closeFunc := func() {
		cancel()
	}

	return newChangedFilesIterator(fetchFunc, closeFunc), nil
}

func newChangedFilesIterator(fetchFunc func() ([]gitdomain.PathStatus, error), closeFunc func()) *changedFilesIterator {
	return &changedFilesIterator{
		fetchFunc: fetchFunc,
		closeFunc: closeFunc,
		closeChan: make(chan struct{}),
	}
}

type changedFilesIterator struct {
	// fetchFunc is the function that will be invoked when the buffer is empty.
	//
	// The function should return the next batch of data, or an error if the fetch
	// failed.
	//
	// fetchFunc should return an io.EOF error when there is no more data to fetch.
	fetchFunc func() ([]gitdomain.PathStatus, error)
	fetchErr  error

	closeOnce sync.Once
	closeFunc func()
	closeChan chan struct{}

	buffer []gitdomain.PathStatus
	index  int
}

func (i *changedFilesIterator) Next() (gitdomain.PathStatus, error) {
	select {
	case <-i.closeChan:
		return gitdomain.PathStatus{}, io.EOF
	default:
	}

	if i.fetchErr != nil {
		return gitdomain.PathStatus{}, i.fetchErr
	}

	for len(i.buffer) == 0 { // If we've exhausted the buffer, fetch more data
		// If we've exhausted the buffer, fetch more data
		//
		// We keep trying until we get a non-empty buffer since it's technically possible for the fetchFunc to return an empty slice
		// even if there is more data to fetch.

		i.buffer, i.fetchErr = i.fetchFunc()
		if i.fetchErr != nil { // Check if there was an error fetching the data
			return gitdomain.PathStatus{}, i.fetchErr
		}
	}

	out := i.buffer[0]
	i.buffer = i.buffer[1:]

	return out, nil
}

func (i *changedFilesIterator) Close() {
	i.closeOnce.Do(func() {
		if i.closeFunc != nil {
			i.closeFunc()
		}
		close(i.closeChan)
	})
}

// ReadDir reads the contents of the named directory at commit.
func (c *clientImplementor) ReadDir(ctx context.Context, repo api.RepoName, commit api.CommitID, path string, recurse bool) (_ []fs.FileInfo, err error) {
	ctx, _, endObservation := c.operations.readDir.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			commit.Attr(),
			attribute.String("path", path),
			attribute.Bool("recurse", recurse),
		},
	})
	defer endObservation(1, observation.Args{})

	if err := checkSpecArgSafety(string(commit)); err != nil {
		return nil, err
	}

	if path != "" {
		// Trailing slash is necessary to ls-tree under the dir (not just
		// to list the dir's tree entry in its parent dir).
		path = filepath.Clean(rel(path)) + "/"
	}
	files, err := c.lsTree(ctx, repo, commit, path, recurse)

	if err != nil || !authz.SubRepoEnabled(c.subRepoPermsChecker) {
		return files, err
	}

	a := actor.FromContext(ctx)
	filtered, filteringErr := authz.FilterActorFileInfos(ctx, c.subRepoPermsChecker, a, repo, files)
	if filteringErr != nil {
		return nil, errors.Wrap(err, "filtering paths")
	} else {
		return filtered, nil
	}
}

// lsTreeRootCache caches the result of running `git ls-tree ...` on a repository's root path
// (because non-root paths are likely to have a lower cache hit rate). It is intended to improve the
// perceived performance of large monorepos, where the tree for a given repo+commit (usually the
// repo's latest commit on default branch) will be requested frequently and would take multiple
// seconds to compute if uncached.
var (
	lsTreeRootCacheMu sync.Mutex
	lsTreeRootCache   = lru.New(5)
)

// lsTree returns ls of tree at path.
func (c *clientImplementor) lsTree(
	ctx context.Context,
	repo api.RepoName,
	commit api.CommitID,
	path string,
	recurse bool,
) (files []fs.FileInfo, err error) {
	if path != "" || !recurse {
		// Only cache the root recursive ls-tree.
		return c.lsTreeUncached(ctx, repo, commit, path, recurse)
	}

	key := string(repo) + ":" + string(commit) + ":" + path
	lsTreeRootCacheMu.Lock()
	v, ok := lsTreeRootCache.Get(key)
	lsTreeRootCacheMu.Unlock()
	var entries []fs.FileInfo
	if ok {
		// Cache hit.
		entries = v.([]fs.FileInfo)
	} else {
		// Cache miss.
		var err error
		start := time.Now()
		entries, err = c.lsTreeUncached(ctx, repo, commit, path, recurse)
		if err != nil {
			return nil, err
		}

		// It's only worthwhile to cache if the operation took a while and returned a lot of
		// data. This is a heuristic.
		if time.Since(start) > 500*time.Millisecond && len(entries) > 5000 {
			lsTreeRootCacheMu.Lock()
			lsTreeRootCache.Add(key, entries)
			lsTreeRootCacheMu.Unlock()
		}
	}
	return entries, nil
}

type objectInfo gitdomain.OID

func (oid objectInfo) OID() gitdomain.OID { return gitdomain.OID(oid) }

// lStat returns a FileInfo describing the named file at commit. If the file is a
// symbolic link, the returned FileInfo describes the symbolic link. lStat makes
// no attempt to follow the link.
func (c *clientImplementor) lStat(ctx context.Context, repo api.RepoName, commit api.CommitID, path string) (_ fs.FileInfo, err error) {
	ctx, _, endObservation := c.operations.lstat.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			commit.Attr(),
			attribute.String("path", path),
		},
	})
	defer endObservation(1, observation.Args{})

	if err := checkSpecArgSafety(string(commit)); err != nil {
		return nil, err
	}

	path = filepath.Clean(rel(path))

	if path == "." {
		// Special case root, which is not returned by `git ls-tree`.
		obj, err := c.GetObject(ctx, repo, string(commit)+"^{tree}")
		if err != nil {
			return nil, err
		}
		return &fileutil.FileInfo{Mode_: os.ModeDir, Sys_: objectInfo(obj.ID)}, nil
	}

	fis, err := c.lsTree(ctx, repo, commit, path, false)
	if err != nil {
		return nil, err
	}
	if len(fis) == 0 {
		return nil, &os.PathError{Op: "ls-tree", Path: path, Err: os.ErrNotExist}
	}

	if !authz.SubRepoEnabled(c.subRepoPermsChecker) {
		return fis[0], nil
	}
	// Applying sub-repo permissions
	a := actor.FromContext(ctx)
	include, filteringErr := authz.FilterActorFileInfo(ctx, c.subRepoPermsChecker, a, repo, fis[0])
	if include && filteringErr == nil {
		return fis[0], nil
	} else {
		if filteringErr != nil {
			err = errors.Wrap(filteringErr, "filtering paths")
		} else {
			err = &os.PathError{Op: "ls-tree", Path: path, Err: os.ErrNotExist}
		}
		return nil, err
	}
}

func errorMessageTruncatedOutput(cmd []string, out []byte) string {
	const maxOutput = 5000

	message := fmt.Sprintf("git command %v failed", cmd)
	if len(out) > maxOutput {
		message += fmt.Sprintf(" (truncated output: %q, %d more)", out[:maxOutput], len(out)-maxOutput)
	} else {
		message += fmt.Sprintf(" (output: %q)", out)
	}

	return message
}

func (c *clientImplementor) lsTreeUncached(ctx context.Context, repo api.RepoName, commit api.CommitID, path string, recurse bool) ([]fs.FileInfo, error) {
	if err := gitdomain.EnsureAbsoluteCommit(commit); err != nil {
		return nil, err
	}

	// Don't call filepath.Clean(path) because ReadDir needs to pass
	// path with a trailing slash.

	if err := checkSpecArgSafety(path); err != nil {
		return nil, err
	}

	args := []string{
		"ls-tree",
		"--long", // show size
		"--full-name",
		"-z",
		string(commit),
	}
	if recurse {
		args = append(args, "-r", "-t")
	}
	if path != "" {
		args = append(args, "--", filepath.ToSlash(path))
	}
	cmd := c.gitCommand(repo, args...)
	out, err := cmd.CombinedOutput(ctx)
	if err != nil {
		if bytes.Contains(out, []byte("exists on disk, but not in")) {
			return nil, &os.PathError{Op: "ls-tree", Path: filepath.ToSlash(path), Err: os.ErrNotExist}
		}

		message := errorMessageTruncatedOutput(cmd.Args(), out)
		return nil, errors.WithMessage(err, message)
	}

	if len(out) == 0 {
		// If we are listing the empty root tree, we will have no output.
		if stdlibpath.Clean(path) == "." {
			return []fs.FileInfo{}, nil
		}
		return nil, &os.PathError{Op: "git ls-tree", Path: path, Err: os.ErrNotExist}
	}

	trimPath := strings.TrimPrefix(path, "./")
	lines := strings.Split(string(out), "\x00")
	fis := make([]fs.FileInfo, len(lines)-1)
	for i, line := range lines {
		if i == len(lines)-1 {
			// last entry is empty
			continue
		}

		tabPos := strings.IndexByte(line, '\t')
		if tabPos == -1 {
			return nil, errors.Errorf("invalid `git ls-tree` output: %q", out)
		}
		info := strings.SplitN(line[:tabPos], " ", 4)
		name := line[tabPos+1:]
		if len(name) < len(trimPath) {
			// This is in a submodule; return the original path to avoid a slice out of bounds panic
			// when setting the FileInfo._Name below.
			name = trimPath
		}

		if len(info) != 4 {
			return nil, errors.Errorf("invalid `git ls-tree` output: %q", out)
		}
		typ := info[1]
		sha := info[2]
		if !gitdomain.IsAbsoluteRevision(sha) {
			return nil, errors.Errorf("invalid `git ls-tree` SHA output: %q", sha)
		}
		oid, err := decodeOID(sha)
		if err != nil {
			return nil, err
		}

		sizeStr := strings.TrimSpace(info[3])
		var size int64
		if sizeStr != "-" {
			// Size of "-" indicates a dir or submodule.
			size, err = strconv.ParseInt(sizeStr, 10, 64)
			if err != nil || size < 0 {
				return nil, errors.Errorf("invalid `git ls-tree` size output: %q (error: %s)", sizeStr, err)
			}
		}

		var sys any
		modeVal, err := strconv.ParseInt(info[0], 8, 32)
		if err != nil {
			return nil, err
		}
		mode := os.FileMode(modeVal)
		switch typ {
		case "blob":
			const gitModeSymlink = 0o20000
			if mode&gitModeSymlink != 0 {
				mode = os.ModeSymlink
			} else {
				// Regular file.
				mode = mode | 0o644
			}
		case "commit":
			mode = mode | gitdomain.ModeSubmodule
			cmd := c.gitCommand(repo, "show", fmt.Sprintf("%s:.gitmodules", commit))
			var submodule gitdomain.Submodule
			if out, err := cmd.Output(ctx); err == nil {

				var cfg config.Config
				err := config.NewDecoder(bytes.NewBuffer(out)).Decode(&cfg)
				if err != nil {
					return nil, errors.Errorf("error parsing .gitmodules: %s", err)
				}

				submodule.Path = cfg.Section("submodule").Subsection(name).Option("path")
				submodule.URL = cfg.Section("submodule").Subsection(name).Option("url")
			}
			submodule.CommitID = api.CommitID(oid.String())
			sys = submodule
		case "tree":
			mode = mode | os.ModeDir
		}

		if sys == nil {
			// Some callers might find it useful to know the object's OID.
			sys = objectInfo(oid)
		}

		fis[i] = &fileutil.FileInfo{
			Name_: name, // full path relative to root (not just basename)
			Mode_: mode,
			Size_: size,
			Sys_:  sys,
		}
	}
	fileutil.SortFileInfosByName(fis)

	return fis, nil
}

func decodeOID(sha string) (gitdomain.OID, error) {
	oidBytes, err := hex.DecodeString(sha)
	if err != nil {
		return gitdomain.OID{}, err
	}
	var oid gitdomain.OID
	copy(oid[:], oidBytes)
	return oid, nil
}

func (c *clientImplementor) LogReverseEach(ctx context.Context, repo string, commit string, n int, onLogEntry func(entry gitdomain.LogEntry) error) (err error) {
	ctx, _, endObservation := c.operations.logReverseEach.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			api.RepoName(repo).Attr(),
			attribute.String("commit", commit),
		},
	})
	defer endObservation(1, observation.Args{})

	ctx, cancel := context.WithCancel(ctx)
	defer cancel()

	command := c.gitCommand(api.RepoName(repo), gitdomain.LogReverseArgs(n, commit)...)

	// We run a single `git log` command and stream the output while the repo is being processed, which
	// can take much longer than 1 minute (the default timeout).
	command.DisableTimeout()
	stdout, err := command.StdoutReader(ctx)
	if err != nil {
		return err
	}
	defer stdout.Close()

	return errors.Wrap(gitdomain.ParseLogReverseEach(stdout, onLogEntry), "ParseLogReverseEach")
}

// StreamBlameFile returns Git blame information about a file.
func (c *clientImplementor) StreamBlameFile(ctx context.Context, repo api.RepoName, path string, opt *BlameOptions) (_ HunkReader, err error) {
	ctx, _, endObservation := c.operations.streamBlameFile.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: append([]attribute.KeyValue{
			repo.Attr(),
			attribute.String("path", path),
		}, opt.Attrs()...),
	})

	// First, verify that the actor has access to the given path.
	hasAccess, err := authz.FilterActorPath(ctx, c.subRepoPermsChecker, actor.FromContext(ctx), repo, path)
	if err != nil {
		return nil, err
	}
	if !hasAccess {
		return nil, fs.ErrNotExist
	}

	client, err := c.clientSource.ClientForRepo(ctx, repo)
	if err != nil {
		endObservation(1, observation.Args{})
		return nil, err
	}

	req := &proto.BlameRequest{
		RepoName:         string(repo),
		Commit:           string(opt.NewestCommit),
		Path:             path,
		IgnoreWhitespace: opt.IgnoreWhitespace,
	}
	if opt.Range != nil {
		req.Range = &proto.BlameRange{
			StartLine: uint32(opt.Range.StartLine),
			EndLine:   uint32(opt.Range.EndLine),
		}
	}

	ctx, cancel := context.WithCancel(ctx)
	cli, err := client.Blame(ctx, req)
	if err != nil {
		cancel()
		endObservation(1, observation.Args{})
		return nil, err
	}

	// We start by reading the first hunk to early-exit on potential errors,
	// ie. permission denied errors or invalid git command.
	firstHunkResp, err := cli.Recv()
	if err != nil {
		s, ok := status.FromError(err)
		if ok {
			if s.Code() == codes.PermissionDenied {
				cancel()
				endObservation(1, observation.Args{})
				return nil, &os.PathError{Op: "open", Path: path, Err: os.ErrNotExist}
			}

			if s.Code() == codes.NotFound {
				for _, d := range s.Details() {
					switch d.(type) {
					case *proto.FileNotFoundPayload:
						cancel()
						endObservation(1, observation.Args{})
						return nil, &os.PathError{Op: "open", Path: path, Err: os.ErrNotExist}
					}
				}
			}
		}

		if err != io.EOF {
			cancel()
			endObservation(1, observation.Args{})
			return nil, err
		}
	}

	var hunk *proto.BlameHunk
	if firstHunkResp != nil {
		hunk = firstHunkResp.GetHunk()
	}
	return &grpcBlameHunkReader{
		firstHunk:      hunk,
		firstHunkErr:   err,
		c:              cli,
		cancel:         cancel,
		endObservation: func() { endObservation(1, observation.Args{}) },
	}, nil
}

type grpcBlameHunkReader struct {
	firstHunk      *proto.BlameHunk
	firstHunkErr   error
	firstHunkRead  bool
	c              proto.GitserverService_BlameClient
	cancel         context.CancelFunc
	endObservation func()
}

func (r *grpcBlameHunkReader) Read() (_ *gitdomain.Hunk, err error) {
	if !r.firstHunkRead {
		r.firstHunkRead = true
		if r.firstHunkErr != nil {
			return nil, r.firstHunkErr
		}
		return gitdomain.HunkFromBlameProto(r.firstHunk), nil
	}
	p, err := r.c.Recv()
	if err != nil {
		return nil, err
	}
	return gitdomain.HunkFromBlameProto(p.GetHunk()), nil
}

func (r *grpcBlameHunkReader) Close() error {
	r.cancel()
	r.endObservation()
	return nil
}

// ResolveRevisionOptions configure how we resolve revisions.
// The zero value should contain appropriate default values.
type ResolveRevisionOptions struct {
	// If set, try to fetch from remote if revision doesn't exist locally.
	EnsureRevision bool
}

// ResolveRevision will return the absolute commit for a commit-ish spec. If spec is empty, HEAD is
// used.
//
// Error cases:
// * Repo does not exist: gitdomain.RepoNotExistError
// * Commit does not exist: gitdomain.RevisionNotFoundError
// * Empty repository: gitdomain.RevisionNotFoundError
// * Other unexpected errors.
func (c *clientImplementor) ResolveRevision(ctx context.Context, repo api.RepoName, spec string, opt ResolveRevisionOptions) (_ api.CommitID, err error) {
	ctx, _, endObservation := c.operations.resolveRevision.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			attribute.String("spec", spec),
			attribute.Bool("ensureRevision", opt.EnsureRevision),
		},
	})
	defer endObservation(1, observation.Args{})

	client, err := c.clientSource.ClientForRepo(ctx, repo)
	if err != nil {
		return "", err
	}

	req := &proto.ResolveRevisionRequest{
		RepoName: string(repo),
		RevSpec:  []byte(spec),
	}
	if opt.EnsureRevision {
		req.EnsureRevision = pointers.Ptr(true)
	}
	res, err := client.ResolveRevision(ctx, req)
	if err != nil {
		return "", err
	}

	return api.CommitID(res.GetCommitSha()), nil
}

func (c *clientImplementor) RevAtTime(ctx context.Context, repo api.RepoName, spec string, date time.Time) (_ api.CommitID, ok bool, err error) {
	ctx, _, endObservation := c.operations.revAtTime.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			attribute.String("spec", spec),
		},
	})
	defer endObservation(1, observation.Args{})

	client, err := c.clientSource.ClientForRepo(ctx, repo)
	if err != nil {
		return "", false, err
	}

	req := &proto.RevAtTimeRequest{
		RepoName: string(repo),
		RevSpec:  []byte(spec),
		Time:     timestamppb.New(date),
	}
	res, err := client.RevAtTime(ctx, req)
	if err != nil {
		return "", false, err
	}

	return api.CommitID(res.GetCommitSha()), res.GetCommitSha() != "", nil
}

// LsFiles returns the output of `git ls-files`.
func (c *clientImplementor) LsFiles(ctx context.Context, repo api.RepoName, commit api.CommitID, pathspecs ...gitdomain.Pathspec) (_ []string, err error) {
	ctx, _, endObservation := c.operations.lsFiles.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			attribute.String("commit", string(commit)),
			attribute.Bool("hasPathSpecs", len(pathspecs) > 0),
		},
	})
	defer endObservation(1, observation.Args{})

	args := []string{
		"ls-files",
		"-z",
		"--with-tree",
		string(commit),
	}

	if len(pathspecs) > 0 {
		args = append(args, "--")
		for _, pathspec := range pathspecs {
			args = append(args, string(pathspec))
		}
	}

	cmd := c.gitCommand(repo, args...)
	out, err := cmd.CombinedOutput(ctx)
	if err != nil {
		return nil, errors.WithMessage(err, fmt.Sprintf("git command %v failed (output: %q)", cmd.Args(), out))
	}

	files := strings.Split(string(out), "\x00")
	// Drop trailing empty string
	if len(files) > 0 && files[len(files)-1] == "" {
		files = files[:len(files)-1]
	}
	return filterPaths(ctx, c.subRepoPermsChecker, repo, files)
}

// 🚨 SECURITY: All git methods that deal with file or path access need to have
// sub-repo permissions applied
func filterPaths(ctx context.Context, checker authz.SubRepoPermissionChecker, repo api.RepoName, paths []string) ([]string, error) {
	if !authz.SubRepoEnabled(checker) {
		return paths, nil
	}
	a := actor.FromContext(ctx)
	filtered, err := authz.FilterActorPaths(ctx, checker, a, repo, paths)
	if err != nil {
		return nil, errors.Wrap(err, "filtering paths")
	}
	return filtered, nil
}

// ListDirectoryChildren fetches the list of children under the given directory
// names. The result is a map keyed by the directory names with the list of files
// under each.
func (c *clientImplementor) ListDirectoryChildren(
	ctx context.Context,
	repo api.RepoName,
	commit api.CommitID,
	dirnames []string,
) (_ map[string][]string, err error) {
	ctx, _, endObservation := c.operations.listDirectoryChildren.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			attribute.String("commit", string(commit)),
			attribute.Int("dirs", len(dirnames)),
		},
	})
	defer endObservation(1, observation.Args{})

	args := []string{"ls-tree", "--name-only", string(commit), "--"}
	args = append(args, cleanDirectoriesForLsTree(dirnames)...)
	cmd := c.gitCommand(repo, args...)

	out, err := cmd.CombinedOutput(ctx)
	if err != nil {
		return nil, err
	}

	paths := strings.Split(string(out), "\n")
	if authz.SubRepoEnabled(c.subRepoPermsChecker) {
		paths, err = authz.FilterActorPaths(ctx, c.subRepoPermsChecker, actor.FromContext(ctx), repo, paths)
		if err != nil {
			return nil, err
		}
	}
	return parseDirectoryChildren(dirnames, paths), nil
}

// cleanDirectoriesForLsTree sanitizes the input dirnames to a git ls-tree command. There are a
// few peculiarities handled here:
//
//  1. The root of the tree must be indicated with `.`, and
//  2. In order for git ls-tree to return a directory's contents, the name must end in a slash.
func cleanDirectoriesForLsTree(dirnames []string) []string {
	var args []string
	for _, dir := range dirnames {
		if dir == "" {
			args = append(args, ".")
		} else {
			if !strings.HasSuffix(dir, "/") {
				dir += "/"
			}
			args = append(args, dir)
		}
	}

	return args
}

// parseDirectoryChildren converts the flat list of files from git ls-tree into a map. The keys of the
// resulting map are the input (unsanitized) dirnames, and the value of that key are the files nested
// under that directory. If dirnames contains a directory that encloses another, then the paths will
// be placed into the key sharing the longest path prefix.
func parseDirectoryChildren(dirnames, paths []string) map[string][]string {
	childrenMap := map[string][]string{}

	// Ensure each directory has an entry, even if it has no children
	// listed in the gitserver output.
	for _, dirname := range dirnames {
		childrenMap[dirname] = nil
	}

	// Order directory names by length (biggest first) so that we assign
	// paths to the most specific enclosing directory in the following loop.
	sort.Slice(dirnames, func(i, j int) bool {
		return len(dirnames[i]) > len(dirnames[j])
	})

	for _, path := range paths {
		if strings.Contains(path, "/") {
			for _, dirname := range dirnames {
				if strings.HasPrefix(path, dirname) {
					childrenMap[dirname] = append(childrenMap[dirname], path)
					break
				}
			}
		} else if len(dirnames) > 0 && dirnames[len(dirnames)-1] == "" {
			// No need to loop here. If we have a root input directory it
			// will necessarily be the last element due to the previous
			// sorting step.
			childrenMap[""] = append(childrenMap[""], path)
		}
	}

	return childrenMap
}

func (c *clientImplementor) GetDefaultBranch(ctx context.Context, repo api.RepoName, short bool) (refName string, commit api.CommitID, err error) {
	ctx, _, endObservation := c.operations.getDefaultBranch.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
		},
	})
	defer endObservation(1, observation.Args{})

	client, err := c.clientSource.ClientForRepo(ctx, repo)
	if err != nil {
		return "", "", err
	}

	res, err := client.DefaultBranch(ctx, &proto.DefaultBranchRequest{
		RepoName: string(repo),
	})
	if err != nil {
		// If we fail to get the default branch due to cloning or being empty, we return nothing.
		if errors.HasType(err, &gitdomain.RepoNotExistError{}) {
			return "", "", nil
		}
		if errors.HasType(err, &gitdomain.RevisionNotFoundError{}) {
			return "", "", nil
		}
		return "", "", err
	}

	return res.GetRefName(), api.CommitID(res.GetCommit()), nil
}

func (c *clientImplementor) MergeBase(ctx context.Context, repo api.RepoName, base, head string) (_ api.CommitID, err error) {
	ctx, _, endObservation := c.operations.mergeBase.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			attribute.String("base", base),
			attribute.String("head", head),
		},
	})
	defer endObservation(1, observation.Args{})

	client, err := c.clientSource.ClientForRepo(ctx, repo)
	if err != nil {
		return "", err
	}

	res, err := client.MergeBase(ctx, &proto.MergeBaseRequest{
		RepoName: string(repo),
		Base:     []byte(base),
		Head:     []byte(head),
	})
	if err != nil {
		return "", err
	}

	return api.CommitID(res.GetMergeBaseCommitSha()), nil
}

// BehindAhead returns the behind/ahead commit counts information for right vs. left (both Git
// revspecs).
func (c *clientImplementor) BehindAhead(ctx context.Context, repo api.RepoName, left, right string) (_ *gitdomain.BehindAhead, err error) {
	ctx, _, endObservation := c.operations.behindAhead.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			attribute.String("left", left),
			attribute.String("right", right),
		},
	})
	defer endObservation(1, observation.Args{})

	client, err := c.clientSource.ClientForRepo(ctx, repo)
	if err != nil {
		return nil, err
	}

	response, err := client.BehindAhead(ctx, &proto.BehindAheadRequest{
		RepoName: string(repo),
		Left:     []byte(left),
		Right:    []byte(right),
	})
	if err != nil {
		return nil, err
	}

	return gitdomain.BehindAheadFromProto(response), nil
}

func (c *clientImplementor) NewFileReader(ctx context.Context, repo api.RepoName, commit api.CommitID, name string) (_ io.ReadCloser, err error) {
	ctx, _, endObservation := c.operations.newFileReader.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			commit.Attr(),
			attribute.String("name", name),
		},
	})

	// First, verify the actor can see the path.
	a := actor.FromContext(ctx)
	if hasAccess, err := authz.FilterActorPath(ctx, c.subRepoPermsChecker, a, repo, name); err != nil {
		return nil, err
	} else if !hasAccess {
		return nil, os.ErrNotExist
	}

	client, err := c.clientSource.ClientForRepo(ctx, repo)
	if err != nil {
		endObservation(1, observation.Args{})
		return nil, err
	}

	req := &proto.ReadFileRequest{
		RepoName: string(repo),
		Commit:   string(commit),
		Path:     rel(name),
	}

	ctx, cancel := context.WithCancel(ctx)
	cli, err := client.ReadFile(ctx, req)
	if err != nil {
		cancel()
		endObservation(1, observation.Args{})
		return nil, err
	}

	// We start by reading the first message to early-exit on potential errors,
	// ie. permission denied errors or invalid git command.
	firstResp, firstRespErr := cli.Recv()
	if firstRespErr != nil {
		if s, ok := status.FromError(firstRespErr); ok {
			// If sub repo permissions deny access to the file, we return os.ErrNotExist.
			if s.Code() == codes.PermissionDenied {
				cancel()
				err = firstRespErr
				endObservation(1, observation.Args{})
				return nil, &os.PathError{Op: "open", Path: req.GetPath(), Err: os.ErrNotExist}
			}
			if s.Code() == codes.NotFound {
				for _, d := range s.Details() {
					switch d.(type) {
					case *proto.FileNotFoundPayload:
						cancel()
						err = firstRespErr
						endObservation(1, observation.Args{})
						return nil, &os.PathError{Op: "open", Path: req.GetPath(), Err: os.ErrNotExist}
					}
				}
			}
		}
		if errors.HasType(firstRespErr, &gitdomain.RevisionNotFoundError{}) {
			cancel()
			err = firstRespErr
			endObservation(1, observation.Args{})
			return nil, err
		}
	}

	firstRespRead := false
	r := streamio.NewReader(func() ([]byte, error) {
		if !firstRespRead {
			firstRespRead = true
			if firstRespErr != nil {
				return nil, firstRespErr
			}
			return firstResp.GetData(), nil
		}

		m, err := cli.Recv()
		if err != nil {
			return nil, err
		}
		return m.GetData(), nil
	})

	return &blobReader{
		Reader: r,
		onClose: func() {
			cancel()
			endObservation(1, observation.Args{})
		},
	}, nil
}

type blobReader struct {
	io.Reader
	onClose func()
}

func (br *blobReader) Close() error {
	br.onClose()
	return nil
}

// Stat returns a FileInfo describing the named file at commit.
func (c *clientImplementor) Stat(ctx context.Context, repo api.RepoName, commit api.CommitID, path string) (_ fs.FileInfo, err error) {
	ctx, _, endObservation := c.operations.stat.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			commit.Attr(),
			attribute.String("path", path),
		},
	})
	defer endObservation(1, observation.Args{})

	if err := checkSpecArgSafety(string(commit)); err != nil {
		return nil, err
	}

	path = rel(path)

	fi, err := c.lStat(ctx, repo, commit, path)
	if err != nil {
		return nil, err
	}

	return fi, nil
}

type CommitsOrder int

const (
	// CommitsOrderDefault uses the default ordering of git log: in reverse chronological order.
	// See https://git-scm.com/docs/git-log#_commit_ordering for more details.
	CommitsOrderDefault CommitsOrder = iota
	// Show no parents before all of its children are shown, but otherwise show commits
	// in the commit timestamp order.
	// See https://git-scm.com/docs/git-log#_commit_ordering for more details.
	CommitsOrderCommitDate
	// Show no parents before all of its children are shown, and avoid showing commits
	// on multiple lines of history intermixed.
	// See https://git-scm.com/docs/git-log#_commit_ordering for more details.
	CommitsOrderTopoDate
)

// CommitsOptions specifies options for Commits.
type CommitsOptions struct {
	AllRefs bool   // if true, all refs are searched for commits, not just a given range. When set, Range should not be specified.
	Range   string // commit range (revspec, "A..B", "A...B", etc.)

	N    uint // limit the number of returned commits to this many (0 means no limit)
	Skip uint // skip this many commits at the beginning

	MessageQuery string // include only commits whose commit message contains this substring

	Author string // include only commits whose author matches this
	After  string // include only commits after this date
	Before string // include only commits before this date

	Order CommitsOrder

	Path string // only commits modifying the given path are selected (optional)

	Follow bool // follow the history of the path beyond renames (works only for a single path)

	// When finding commits to include, follow only the first parent commit upon
	// seeing a merge commit. This option can give a better overview when viewing
	// the evolution of a particular topic branch, because merges into a topic
	// branch tend to be only about adjusting to updated upstream from time to time,
	// and this option allows you to ignore the individual commits brought in to
	// your history by such a merge.
	FirstParent bool

	// When true return the names of the files changed in the commit
	NameOnly bool
}

func (c *clientImplementor) GetCommit(ctx context.Context, repo api.RepoName, id api.CommitID) (_ *gitdomain.Commit, err error) {
	ctx, _, endObservation := c.operations.getCommit.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			id.Attr(),
		},
	})
	defer endObservation(1, observation.Args{})

	client, err := c.clientSource.ClientForRepo(ctx, repo)
	if err != nil {
		return nil, err
	}

	subRepoEnabled := false
	if authz.SubRepoEnabled(c.subRepoPermsChecker) {
		subRepoEnabled, err = authz.SubRepoEnabledForRepo(ctx, c.subRepoPermsChecker, repo)
		if err != nil {
			return nil, errors.Wrap(err, "failed to check sub repo permissions")
		}
	}

	res, err := client.GetCommit(ctx, &proto.GetCommitRequest{
		RepoName:             string(repo),
		Commit:               string(id),
		IncludeModifiedFiles: subRepoEnabled,
	})
	if err != nil {
		return nil, err
	}

	commit := gitdomain.CommitFromProto(res.GetCommit())

	if subRepoEnabled {
		ok, err := hasAccessToCommit(ctx, &wrappedCommit{Commit: commit, files: byteSlicesToStrings(res.GetModifiedFiles())}, repo, c.subRepoPermsChecker)
		if err != nil {
			return nil, errors.Wrap(err, "failed to check sub repo permissions")
		}
		if !ok {
			return nil, &gitdomain.RevisionNotFoundError{Repo: repo, Spec: string(id)}
		}
	}

	return commit, nil
}

// Commits returns all commits matching the options.
func (c *clientImplementor) Commits(ctx context.Context, repo api.RepoName, opt CommitsOptions) (_ []*gitdomain.Commit, err error) {
	opt = addNameOnly(opt, c.subRepoPermsChecker)
	ctx, _, endObservation := c.operations.commits.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			attribute.String("opts", fmt.Sprintf("%#v", opt)),
		},
	})
	defer endObservation(1, observation.Args{})

	if err := checkSpecArgSafety(opt.Range); err != nil {
		return nil, err
	}

	wrappedCommits, err := c.getWrappedCommits(ctx, repo, opt)
	if err != nil {
		return nil, err
	}

	filtered, err := filterCommits(ctx, c.subRepoPermsChecker, wrappedCommits, repo)
	if err != nil {
		return nil, errors.Wrap(err, "filtering commits")
	}

	if needMoreCommits(filtered, wrappedCommits, opt, c.subRepoPermsChecker) {
		return c.getMoreCommits(ctx, repo, opt, filtered)
	}
	return filtered, err
}

func filterCommits(ctx context.Context, checker authz.SubRepoPermissionChecker, commits []*wrappedCommit, repoName api.RepoName) ([]*gitdomain.Commit, error) {
	if !authz.SubRepoEnabled(checker) {
		return unWrapCommits(commits), nil
	}
	filtered := make([]*gitdomain.Commit, 0, len(commits))
	for _, commit := range commits {
		if hasAccess, err := hasAccessToCommit(ctx, commit, repoName, checker); hasAccess {
			filtered = append(filtered, commit.Commit)
		} else if err != nil {
			return nil, err
		}
	}
	return filtered, nil
}

func unWrapCommits(wrappedCommits []*wrappedCommit) []*gitdomain.Commit {
	commits := make([]*gitdomain.Commit, 0, len(wrappedCommits))
	for _, wc := range wrappedCommits {
		commits = append(commits, wc.Commit)
	}
	return commits
}

func hasAccessToCommit(ctx context.Context, commit *wrappedCommit, repoName api.RepoName, checker authz.SubRepoPermissionChecker) (bool, error) {
	a := actor.FromContext(ctx)
	if commit.files == nil || len(commit.files) == 0 {
		return true, nil // If commit has no files, assume user has access to view the commit.
	}
	for _, fileName := range commit.files {
		if hasAccess, err := authz.FilterActorPath(ctx, checker, a, repoName, fileName); err != nil {
			return false, err
		} else if hasAccess {
			// if the user has access to one file modified in the commit, they have access to view the commit
			return true, nil
		}
	}
	return false, nil
}

// CommitsUniqueToBranch returns a map from commits that exist on a particular
// branch in the given repository to their committer date. This set of commits is
// determined by listing `{branchName} ^HEAD`, which is interpreted as: all
// commits on {branchName} not also on the tip of the default branch. If the
// supplied branch name is the default branch, then this method instead returns
// all commits reachable from HEAD.
func (c *clientImplementor) CommitsUniqueToBranch(ctx context.Context, repo api.RepoName, branchName string, isDefaultBranch bool, maxAge *time.Time) (_ map[string]time.Time, err error) {
	ctx, _, endObservation := c.operations.commitsUniqueToBranch.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			attribute.String("branch", branchName),
		},
	})
	defer endObservation(1, observation.Args{})

	args := []string{"log", "--pretty=format:%H:%cI"}
	if maxAge != nil {
		args = append(args, fmt.Sprintf("--after=%s", *maxAge))
	}
	if isDefaultBranch {
		args = append(args, "HEAD")
	} else {
		args = append(args, branchName, "^HEAD")
	}

	cmd := c.gitCommand(repo, args...)
	out, err := cmd.CombinedOutput(ctx)
	if err != nil {
		return nil, err
	}

	commits, err := parseCommitsUniqueToBranch(strings.Split(string(out), "\n"))
	if authz.SubRepoEnabled(c.subRepoPermsChecker) && err == nil {
		return c.filterCommitsUniqueToBranch(ctx, repo, commits), nil
	}
	return commits, err
}

func (c *clientImplementor) filterCommitsUniqueToBranch(ctx context.Context, repo api.RepoName, commitsMap map[string]time.Time) map[string]time.Time {
	filtered := make(map[string]time.Time, len(commitsMap))
	for commitID, timeStamp := range commitsMap {
		_, err := c.GetCommit(ctx, repo, api.CommitID(commitID))
		if !errors.HasType(err, &gitdomain.RevisionNotFoundError{}) {
			filtered[commitID] = timeStamp
		}
	}
	return filtered
}

func parseCommitsUniqueToBranch(lines []string) (_ map[string]time.Time, err error) {
	commitDates := make(map[string]time.Time, len(lines))
	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}

		parts := strings.SplitN(line, ":", 2)
		if len(parts) != 2 {
			return nil, errors.Errorf(`unexpected output from git log "%s"`, line)
		}

		duration, err := time.Parse(time.RFC3339, parts[1])
		if err != nil {
			return nil, errors.Errorf(`unexpected output from git log (bad date format) "%s"`, line)
		}

		commitDates[parts[0]] = duration
	}

	return commitDates, nil
}

// HasCommitAfter indicates the staleness of a repository. It returns a boolean indicating if a repository
// contains a commit past a specified date.
func (c *clientImplementor) HasCommitAfter(ctx context.Context, repo api.RepoName, date string, revspec string) (_ bool, err error) {
	ctx, _, endObservation := c.operations.hasCommitAfter.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			attribute.String("date", date),
			attribute.String("revSpec", revspec),
		},
	})
	defer endObservation(1, observation.Args{})

	if authz.SubRepoEnabled(c.subRepoPermsChecker) {
		return c.hasCommitAfterWithFiltering(ctx, repo, date, revspec)
	}

	if revspec == "" {
		revspec = "HEAD"
	}

	commitid, err := c.ResolveRevision(ctx, repo, revspec, ResolveRevisionOptions{EnsureRevision: false})
	if err != nil {
		return false, err
	}

	args, err := commitLogArgs([]string{"rev-list", "--count"}, CommitsOptions{
		N:     1,
		After: date,
		Range: string(commitid),
	})
	if err != nil {
		return false, err
	}

	cmd := c.gitCommand(repo, args...)
	out, err := cmd.Output(ctx)
	if err != nil {
		return false, errors.WithMessage(err, fmt.Sprintf("git command %v failed (output: %q)", cmd.Args(), out))
	}

	out = bytes.TrimSpace(out)
	n, err := strconv.Atoi(string(out))
	return n > 0, err
}

func (c *clientImplementor) hasCommitAfterWithFiltering(ctx context.Context, repo api.RepoName, date, revspec string) (bool, error) {
	if commits, err := c.Commits(ctx, repo, CommitsOptions{After: date, Range: revspec}); err != nil {
		return false, err
	} else if len(commits) > 0 {
		return true, nil
	}
	return false, nil
}

func isBadObjectErr(output, obj string) bool {
	return output == "fatal: bad object "+obj
}

func (c *clientImplementor) getWrappedCommits(ctx context.Context, repo api.RepoName, opt CommitsOptions) ([]*wrappedCommit, error) {
	args, err := commitLogArgs([]string{"log", logFormatWithoutRefs}, opt)
	if err != nil {
		return nil, err
	}

	cmd := c.gitCommand(repo, args...)
	wrappedCommits, err := runCommitLog(ctx, cmd, opt)
	if err != nil {
		return nil, err
	}
	return wrappedCommits, nil
}

func needMoreCommits(filtered []*gitdomain.Commit, commits []*wrappedCommit, opt CommitsOptions, checker authz.SubRepoPermissionChecker) bool {
	if !authz.SubRepoEnabled(checker) {
		return false
	}
	if opt.N == 0 || isRequestForSingleCommit(opt) {
		return false
	}
	if len(filtered) < len(commits) {
		return true
	}
	return false
}

func isRequestForSingleCommit(opt CommitsOptions) bool {
	return opt.Range != "" && opt.N == 1
}

// getMoreCommits handles the case where a specific number of commits was requested via CommitsOptions, but after sub-repo
// filtering, fewer than that requested number was left. This function requests the next N commits (where N was the number
// originally requested), filters the commits, and determines if this is at least N commits total after filtering. If not,
// the loop continues until N total filtered commits are collected _or_ there are no commits left to request.
func (c *clientImplementor) getMoreCommits(ctx context.Context, repo api.RepoName, opt CommitsOptions, baselineCommits []*gitdomain.Commit) ([]*gitdomain.Commit, error) {
	// We want to place an upper bound on the number of times we loop here so that we
	// don't hit pathological conditions where a lot of filtering has been applied.
	const maxIterations = 5

	totalCommits := make([]*gitdomain.Commit, 0, opt.N)
	for range maxIterations {
		if uint(len(totalCommits)) == opt.N {
			break
		}
		// Increment the Skip number to get the next N commits
		opt.Skip += opt.N
		wrappedCommits, err := c.getWrappedCommits(ctx, repo, opt)
		if err != nil {
			return nil, err
		}
		filtered, err := filterCommits(ctx, c.subRepoPermsChecker, wrappedCommits, repo)
		if err != nil {
			return nil, err
		}
		// join the new (filtered) commits with those already fetched (potentially truncating the list to have length N if necessary)
		totalCommits = joinCommits(baselineCommits, filtered, opt.N)
		baselineCommits = totalCommits
		if uint(len(wrappedCommits)) < opt.N {
			// No more commits available before filtering, so return current total commits (e.g. the last "page" of N commits has been reached)
			break
		}
	}
	return totalCommits, nil
}

func joinCommits(previous, next []*gitdomain.Commit, desiredTotal uint) []*gitdomain.Commit {
	allCommits := append(previous, next...)
	// ensure that we don't return more than what was requested
	if uint(len(allCommits)) > desiredTotal {
		return allCommits[:desiredTotal]
	}
	return allCommits
}

// runCommitLog sends the git command to gitserver. It interprets missing
// revision responses and converts them into RevisionNotFoundError.
// It is declared as a variable so that we can swap it out in tests
var runCommitLog = func(ctx context.Context, cmd GitCommand, opt CommitsOptions) ([]*wrappedCommit, error) {
	data, stderr, err := cmd.DividedOutput(ctx)
	if err != nil {
		data = bytes.TrimSpace(data)
		if isBadObjectErr(string(stderr), opt.Range) {
			return nil, &gitdomain.RevisionNotFoundError{Repo: cmd.Repo(), Spec: opt.Range}
		}
		return nil, errors.WithMessage(err, fmt.Sprintf("git command %v failed (output: %q)", cmd.Args(), data))
	}

	return parseCommitLogOutput(bytes.NewReader(data))
}

func parseCommitLogOutput(r io.Reader) ([]*wrappedCommit, error) {
	commitScanner := bufio.NewScanner(r)
	// We use an increased buffer size since sub-repo permissions
	// can result in very lengthy output.
	commitScanner.Buffer(make([]byte, 0, 65536), 4294967296)
	commitScanner.Split(commitSplitFunc)

	var commits []*wrappedCommit
	for commitScanner.Scan() {
		rawCommit := commitScanner.Bytes()
		parts := bytes.Split(rawCommit, []byte{'\x00'})
		if len(parts) != partsPerCommit {
			return nil, errors.Newf("internal error: expected %d parts, got %d", partsPerCommit, len(parts))
		}

		commit, err := parseCommitFromLog(parts)
		if err != nil {
			return nil, err
		}
		commits = append(commits, commit)
	}
	return commits, nil
}

func commitSplitFunc(data []byte, atEOF bool) (advance int, token []byte, err error) {
	if len(data) == 0 {
		// Request more data
		return 0, nil, nil
	}

	// Safety check: ensure we are always starting with a record separator
	if data[0] != '\x1e' {
		return 0, nil, errors.New("internal error: data should always start with an ASCII record separator")
	}

	loc := bytes.IndexByte(data[1:], '\x1e')
	if loc < 0 {
		// We can't find the start of the next record
		if atEOF {
			// If we're at the end of the stream, just return the rest as the last record
			return len(data), data[1:], bufio.ErrFinalToken
		} else {
			// If we're not at the end of the stream, request more data
			return 0, nil, nil
		}
	}
	nextStart := loc + 1 // correct for searching at an offset

	return nextStart, data[1:nextStart], nil
}

type wrappedCommit struct {
	*gitdomain.Commit
	files []string
}

func commitLogArgs(initialArgs []string, opt CommitsOptions) (args []string, err error) {
	if err := checkSpecArgSafety(opt.Range); err != nil {
		return nil, err
	}

	args = initialArgs
	if opt.N != 0 {
		args = append(args, "-n", strconv.FormatUint(uint64(opt.N), 10))
	}
	if opt.Skip != 0 {
		args = append(args, "--skip="+strconv.FormatUint(uint64(opt.Skip), 10))
	}

	if opt.Author != "" {
		args = append(args, "--fixed-strings", "--author="+opt.Author)
	}

	if opt.After != "" {
		args = append(args, "--after="+opt.After)
	}
	if opt.Before != "" {
		args = append(args, "--before="+opt.Before)
	}
	switch opt.Order {
	case CommitsOrderCommitDate:
		args = append(args, "--date-order")
	case CommitsOrderTopoDate:
		args = append(args, "--topo-order")
	case CommitsOrderDefault:
		// nothing to do
	default:
		return nil, errors.Newf("invalid ordering %d", opt.Order)
	}

	if opt.MessageQuery != "" {
		args = append(args, "--fixed-strings", "--regexp-ignore-case", "--grep="+opt.MessageQuery)
	}

	if opt.FirstParent {
		args = append(args, "--first-parent")
	}

	if opt.Range != "" {
		args = append(args, opt.Range)
	}
	if opt.AllRefs {
		args = append(args, "--all")
	}
	if opt.Range != "" && opt.AllRefs {
		return nil, errors.New("cannot specify both a Range and AllRefs")
	}
	if opt.NameOnly {
		args = append(args, "--name-only")
	}
	if opt.Follow {
		args = append(args, "--follow")
	}
	if opt.Path != "" {
		args = append(args, "--", opt.Path)
	}
	return args, nil
}

// FirstEverCommit returns the first commit ever made to the repository.
func (c *clientImplementor) FirstEverCommit(ctx context.Context, repo api.RepoName) (commit *gitdomain.Commit, err error) {
	ctx, _, endObservation := c.operations.firstEverCommit.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
		},
	})
	defer endObservation(1, observation.Args{})

	client, err := c.clientSource.ClientForRepo(ctx, repo)
	if err != nil {
		return nil, err
	}

	result, err := client.FirstEverCommit(ctx, &proto.FirstEverCommitRequest{
		RepoName: string(repo),
	})
	if err != nil {
		return nil, err
	}

	return gitdomain.CommitFromProto(result.GetCommit()), nil
}

const (
	partsPerCommit = 10 // number of \x00-separated fields per commit

	// This format string has 10 parts:
	//  1) oid
	//  2) author name
	//  3) author email
	//  4) author time
	//  5) committer name
	//  6) committer email
	//  7) committer time
	//  8) message body
	//  9) parent hashes
	// 10) modified files (optional)
	//
	// Each commit starts with an ASCII record separator byte (0x1E), and
	// each field of the commit is separated by a null byte (0x00).
	//
	// Refs are slow, and are intentionally not included because they are usually not needed.
	logFormatWithoutRefs = "--format=format:%x1e%H%x00%aN%x00%aE%x00%at%x00%cN%x00%cE%x00%ct%x00%B%x00%P%x00"
)

// parseCommitFromLog parses the next commit from data and returns the commit and the remaining
// data. The data arg is a byte array that contains NUL-separated log fields as formatted by
// logFormatFlag.
func parseCommitFromLog(parts [][]byte) (*wrappedCommit, error) {
	// log outputs are newline separated, so all but the 1st commit ID part
	// has an erroneous leading newline.
	parts[0] = bytes.TrimPrefix(parts[0], []byte{'\n'})
	commitID := api.CommitID(parts[0])

	authorTime, err := strconv.ParseInt(string(parts[3]), 10, 64)
	if err != nil {
		return nil, errors.Errorf("parsing git commit author time: %s", err)
	}
	committerTime, err := strconv.ParseInt(string(parts[6]), 10, 64)
	if err != nil {
		return nil, errors.Errorf("parsing git commit committer time: %s", err)
	}

	var parents []api.CommitID
	if parentPart := parts[8]; len(parentPart) > 0 {
		parentIDs := bytes.Split(parentPart, []byte{' '})
		parents = make([]api.CommitID, len(parentIDs))
		for i, id := range parentIDs {
			parents[i] = api.CommitID(id)
		}
	}

	fileNames := strings.Split(string(bytes.TrimSpace(parts[9])), "\n")

	return &wrappedCommit{
		Commit: &gitdomain.Commit{
			ID:        commitID,
			Author:    gitdomain.Signature{Name: string(parts[1]), Email: string(parts[2]), Date: time.Unix(authorTime, 0).UTC()},
			Committer: &gitdomain.Signature{Name: string(parts[4]), Email: string(parts[5]), Date: time.Unix(committerTime, 0).UTC()},
			Message:   gitdomain.Message(strings.TrimSuffix(string(parts[7]), "\n")),
			Parents:   parents,
		}, files: fileNames,
	}, nil
}

type ArchiveFormat string

const (
	// ArchiveFormatZip indicates a zip archive is desired.
	ArchiveFormatZip ArchiveFormat = "zip"

	// ArchiveFormatTar indicates a tar archive is desired.
	ArchiveFormatTar ArchiveFormat = "tar"
)

func ArchiveFormatFromProto(pf proto.ArchiveFormat) ArchiveFormat {
	switch pf {
	case proto.ArchiveFormat_ARCHIVE_FORMAT_ZIP:
		return ArchiveFormatZip
	case proto.ArchiveFormat_ARCHIVE_FORMAT_TAR:
		return ArchiveFormatTar
	default:
		return ""
	}
}

func (f ArchiveFormat) ToProto() proto.ArchiveFormat {
	switch f {
	case ArchiveFormatZip:
		return proto.ArchiveFormat_ARCHIVE_FORMAT_ZIP
	case ArchiveFormatTar:
		return proto.ArchiveFormat_ARCHIVE_FORMAT_TAR
	default:
		return proto.ArchiveFormat_ARCHIVE_FORMAT_UNSPECIFIED
	}
}

func (c *clientImplementor) ArchiveReader(ctx context.Context, repo api.RepoName, options ArchiveOptions) (_ io.ReadCloser, err error) {
	ctx, _, endObservation := c.operations.archiveReader.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: append(
			[]attribute.KeyValue{repo.Attr()},
			options.Attrs()...,
		),
	})

	if authz.SubRepoEnabled(c.subRepoPermsChecker) && !actor.FromContext(ctx).IsInternal() {
		if enabled, err := authz.SubRepoEnabledForRepo(ctx, c.subRepoPermsChecker, repo); err != nil {
			return nil, errors.Wrap(err, "sub-repo permissions check")
		} else if enabled {
			return nil, errors.New("archiveReader invoked for a repo with sub-repo permissions")
		}
	}

	client, err := c.clientSource.ClientForRepo(ctx, repo)
	if err != nil {
		endObservation(1, observation.Args{})
		return nil, err
	}

	req := options.ToProto(string(repo))

	ctx, cancel := context.WithCancel(ctx)
	cli, err := client.Archive(ctx, req)
	if err != nil {
		cancel()
		endObservation(1, observation.Args{})
		return nil, err
	}

	// We start by reading the first message to early-exit on potential errors,
	// ie. revision not found errors or invalid git command.
	firstMessage, firstErr := cli.Recv()
	if firstErr != nil {
		if errors.HasType(firstErr, &gitdomain.RevisionNotFoundError{}) {
			cancel()
			err = firstErr
			endObservation(1, observation.Args{})
			return nil, err
		}
	}

	firstRespRead := false
	r := streamio.NewReader(func() ([]byte, error) {
		if !firstRespRead {
			firstRespRead = true
			if firstErr != nil {
				return nil, firstErr
			}
			return firstMessage.GetData(), nil
		}

		m, err := cli.Recv()
		if err != nil {
			return nil, err
		}
		return m.GetData(), nil
	})

	return &archiveReader{
		Reader: r,
		cancel: cancel,
		onClose: func() {
			endObservation(1, observation.Args{})
		},
	}, nil
}

type archiveReader struct {
	io.Reader
	cancel  context.CancelFunc
	onClose func()
}

func (br *archiveReader) Close() error {
	br.cancel()
	br.onClose()
	return nil
}

func addNameOnly(opt CommitsOptions, checker authz.SubRepoPermissionChecker) CommitsOptions {
	if authz.SubRepoEnabled(checker) {
		// If sub-repo permissions enabled, must fetch files modified w/ commits to determine if user has access to view this commit
		opt.NameOnly = true
	}
	return opt
}

// ListRefs returns a list of all refs in the repository.
func (c *clientImplementor) ListRefs(ctx context.Context, repo api.RepoName, opt ListRefsOpts) (_ []gitdomain.Ref, err error) {
	ctx, _, endObservation := c.operations.listRefs.With(ctx, &err, observation.Args{
		MetricLabelValues: []string{c.scope},
		Attrs: []attribute.KeyValue{
			repo.Attr(),
			attribute.Bool("headsOnly", opt.HeadsOnly),
			attribute.Bool("tagsOnly", opt.TagsOnly),
		},
	})
	defer endObservation(1, observation.Args{})

	client, err := c.clientSource.ClientForRepo(ctx, repo)
	if err != nil {
		return nil, err
	}

	req := &proto.ListRefsRequest{
		RepoName:  string(repo),
		HeadsOnly: opt.HeadsOnly,
		TagsOnly:  opt.TagsOnly,
	}

	for _, c := range opt.PointsAtCommit {
		req.PointsAtCommit = append(req.PointsAtCommit, string(c))
	}

	if opt.Contains != "" {
		req.ContainsSha = pointers.Ptr(string(opt.Contains))
	}

	cc, err := client.ListRefs(ctx, req)
	if err != nil {
		return nil, err
	}

	refs := make([]gitdomain.Ref, 0)
	for {
		resp, err := cc.Recv()
		if err != nil {
			if err == io.EOF {
				break
			}
			return nil, err
		}
		for _, ref := range resp.GetRefs() {
			refs = append(refs, gitdomain.RefFromProto(ref))
		}
	}

	return refs, nil
}

// rel strips the leading "/" prefix from the path string, effectively turning
// an absolute path into one relative to the root directory. A path that is just
// "/" is treated specially, returning just ".".
//
// The elements in a file path are separated by slash ('/', U+002F) characters,
// regardless of host operating system convention.
func rel(path string) string {
	if path == "/" {
		return "."
	}
	return strings.TrimPrefix(path, "/")
}
