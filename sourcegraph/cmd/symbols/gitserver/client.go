package gitserver

import (
	"bytes"
	"context"
	"io"

	"go.opentelemetry.io/otel/attribute"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type GitserverClient interface {
	// FetchTar returns an io.ReadCloser to a tar archive of a repository at the specified Git
	// remote URL and commit ID. If the error implements "BadRequest() bool", it will be used to
	// determine if the error is a bad request (eg invalid repo).
	FetchTar(context.Context, api.RepoName, api.CommitID, []string) (io.ReadCloser, error)

	// ChangedFiles returns an iterator that yields the paths that have changed between two commits.
	ChangedFiles(context.Context, api.RepoName, api.CommitID, api.CommitID) (gitserver.ChangedFilesIterator, error)

	// NewFileReader returns an io.ReadCloser reading from the named file at commit.
	// The caller should always close the reader after use.
	// (If you just need to check a file's existence, use Stat, not a file reader.)
	NewFileReader(ctx context.Context, repoCommitPath types.RepoCommitPath) (io.ReadCloser, error)

	// LogReverseEach runs git log in reverse order and calls the given callback for each entry.
	LogReverseEach(ctx context.Context, repo string, commit string, n int, onLogEntry func(entry gitdomain.LogEntry) error) error

	// RevList makes a git rev-list call and iterates through the resulting commits, calling the provided
	// onCommit function for each.
	RevList(ctx context.Context, repo string, commit string, onCommit func(commit string) (shouldContinue bool, err error)) error
}

// Changes are added, deleted, and modified paths.
type Changes struct {
	Added    []string
	Modified []string
	Deleted  []string
}

type gitserverClient struct {
	innerClient gitserver.Client
	operations  *operations
}

func NewClient(observationCtx *observation.Context, db database.DB) GitserverClient {
	return &gitserverClient{
		innerClient: gitserver.NewClient("symbols"),
		operations:  newOperations(observationCtx),
	}
}

func (c *gitserverClient) FetchTar(ctx context.Context, repo api.RepoName, commit api.CommitID, paths []string) (_ io.ReadCloser, err error) {
	ctx, _, endObservation := c.operations.fetchTar.With(ctx, &err, observation.Args{Attrs: []attribute.KeyValue{
		repo.Attr(),
		commit.Attr(),
		attribute.Int("paths", len(paths)),
	}})
	defer endObservation(1, observation.Args{})

	opts := gitserver.ArchiveOptions{
		Treeish: string(commit),
		Format:  gitserver.ArchiveFormatTar,
		Paths:   paths,
	}

	// Note: the sub-repo perms checker is nil here because we do the sub-repo filtering at a higher level
	return c.innerClient.ArchiveReader(ctx, repo, opts)
}

func (c *gitserverClient) ChangedFiles(ctx context.Context, repo api.RepoName, commitA, commitB api.CommitID) (iterator gitserver.ChangedFilesIterator, err error) {
	ctx, _, endObservation := c.operations.gitDiff.With(ctx, &err, observation.Args{Attrs: []attribute.KeyValue{
		repo.Attr(),
		attribute.String("commitA", string(commitA)),
		attribute.String("commitB", string(commitB)),
	}})
	defer endObservation(1, observation.Args{})

	return c.innerClient.ChangedFiles(ctx, repo, string(commitA), string(commitB))
}

func (c *gitserverClient) NewFileReader(ctx context.Context, repoCommitPath types.RepoCommitPath) (io.ReadCloser, error) {
	return c.innerClient.NewFileReader(ctx, api.RepoName(repoCommitPath.Repo), api.CommitID(repoCommitPath.Commit), repoCommitPath.Path)
}

func (c *gitserverClient) LogReverseEach(ctx context.Context, repo string, commit string, n int, onLogEntry func(entry gitdomain.LogEntry) error) error {
	return c.innerClient.LogReverseEach(ctx, repo, commit, n, onLogEntry)
}

const revListPageSize = 100

func (c *gitserverClient) RevList(ctx context.Context, repo string, commit string, onCommit func(commit string) (shouldContinue bool, err error)) error {
	nextCursor := commit
	for {
		var commits []api.CommitID
		var err error
		commits, nextCursor, err = c.paginatedRevList(ctx, api.RepoName(repo), nextCursor, revListPageSize)
		if err != nil {
			return err
		}
		for _, c := range commits {
			shouldContinue, err := onCommit(string(c))
			if err != nil {
				return err
			}
			if !shouldContinue {
				return nil
			}
		}
		if nextCursor == "" {
			return nil
		}
	}
}

func (c *gitserverClient) paginatedRevList(ctx context.Context, repo api.RepoName, commit string, count int) ([]api.CommitID, string, error) {
	commits, err := c.innerClient.Commits(ctx, repo, gitserver.CommitsOptions{
		N:           uint(count + 1),
		Range:       commit,
		FirstParent: true,
	})
	if err != nil {
		return nil, "", err
	}

	commitIDs := make([]api.CommitID, 0, count+1)

	for _, commit := range commits {
		commitIDs = append(commitIDs, commit.ID)
	}

	var nextCursor string
	if len(commitIDs) > count {
		nextCursor = string(commitIDs[len(commitIDs)-1])
		commitIDs = commitIDs[:count]
	}

	return commitIDs, nextCursor, nil
}

var NUL = []byte{0}

// parseGitDiffOutput parses the output of a git diff command, which consists
// of a repeated sequence of `<status> NUL <path> NUL` where NUL is the 0 byte.
func parseGitDiffOutput(output []byte) (changes Changes, _ error) {
	if len(output) == 0 {
		return Changes{}, nil
	}

	slices := bytes.Split(bytes.TrimRight(output, string(NUL)), NUL)
	if len(slices)%2 != 0 {
		return changes, errors.Newf("uneven pairs")
	}

	for i := 0; i < len(slices); i += 2 {
		switch slices[i][0] {
		case 'A':
			changes.Added = append(changes.Added, string(slices[i+1]))
		case 'M':
			changes.Modified = append(changes.Modified, string(slices[i+1]))
		case 'D':
			changes.Deleted = append(changes.Deleted, string(slices[i+1]))
		}
	}

	return changes, nil
}
