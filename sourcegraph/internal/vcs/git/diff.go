package git

import (
	"bytes"
	"context"
	"io"
	"os"
	"strings"

	"github.com/cockroachdb/errors"
	"github.com/sourcegraph/go-diff/diff"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
)

type DiffOptions struct {
	Repo api.RepoName

	// These fields must be valid <commit> inputs as defined by gitrevisions(7).
	Base string
	Head string
}

// Diff returns an iterator that can be used to access the diff between two
// commits on a per-file basis. The iterator must be closed with Close when no
// longer required.
func Diff(ctx context.Context, opts DiffOptions) (*DiffFileIterator, error) {
	rangeType := "..."
	// Rare case: the base is the empty tree, in which case we must use ..
	// instead of ... as the latter only works for commits.
	if opts.Base == DevNullSHA {
		rangeType = ".."
	}
	rangeSpec := opts.Base + rangeType + opts.Head
	if strings.HasPrefix(rangeSpec, "-") || strings.HasPrefix(rangeSpec, ".") {
		// We don't want to allow user input to add `git diff` command line
		// flags or refer to a file.
		return nil, errors.Errorf("invalid diff range argument: %q", rangeSpec)
	}

	rdr, err := execReader(ctx, opts.Repo, []string{
		"diff",
		"--find-renames",
		// TODO(eseliger): Enable once we have support for copy detection in go-diff
		// and actually expose a `isCopy` field in the api, otherwise this
		// information is thrown away anyways.
		// "--find-copies",
		"--full-index",
		"--inter-hunk-context=3",
		"--no-prefix",
		rangeSpec,
		"--",
	})
	if err != nil {
		return nil, errors.Wrap(err, "executing git diff")
	}

	return &DiffFileIterator{
		rdr:  rdr,
		mfdr: diff.NewMultiFileDiffReader(rdr),
	}, nil
}

// DiffPath returns a position-ordered slice of changes (additions or deletions)
// of the given path between the given source and target commits.
func DiffPath(ctx context.Context, repo api.RepoName, sourceCommit, targetCommit, path string, checker authz.SubRepoPermissionChecker) ([]*diff.Hunk, error) {
	a := actor.FromContext(ctx)
	if hasAccess, err := authz.FilterActorPath(ctx, checker, a, repo, path); err != nil {
		return nil, err
	} else if !hasAccess {
		return nil, os.ErrNotExist
	}
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
}

// DiffSymbols performs a diff command which is expected to be parsed by our symbols package
func DiffSymbols(ctx context.Context, repo api.RepoName, commitA, commitB api.CommitID) ([]byte, error) {
	command := gitserver.DefaultClient.Command("git", "diff", "-z", "--name-status", "--no-renames", string(commitA), string(commitB))
	command.Repo = repo
	return command.Output(ctx)
}

type DiffFileIterator struct {
	rdr  io.ReadCloser
	mfdr *diff.MultiFileDiffReader
}

func (i *DiffFileIterator) Close() error {
	return i.rdr.Close()
}

// Next returns the next file diff. If no more diffs are available, the diff
// will be nil and the error will be io.EOF.
func (i *DiffFileIterator) Next() (*diff.FileDiff, error) {
	return i.mfdr.ReadFile()
}
