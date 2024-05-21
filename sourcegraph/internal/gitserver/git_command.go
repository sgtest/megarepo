package gitserver

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"syscall"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/protocol"
	proto "github.com/sourcegraph/sourcegraph/internal/gitserver/v1"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// GitCommand is an interface describing a git commands to be executed.
type GitCommand interface {
	// DividedOutput runs the command and returns its standard output and standard error.
	DividedOutput(ctx context.Context) ([]byte, []byte, error)

	// Output runs the command and returns its standard output.
	Output(ctx context.Context) ([]byte, error)

	// CombinedOutput runs the command and returns its combined standard output and standard error.
	CombinedOutput(ctx context.Context) ([]byte, error)

	// Repo returns repo against which the command is run
	Repo() api.RepoName

	// Args returns arguments of the command
	Args() []string

	// ExitStatus returns exit status of the command
	ExitStatus() int

	// String returns string representation of the command (in fact prints args parameter of the command)
	String() string

	// StdoutReader returns an io.ReadCloser of stdout of c. If the command has a
	// non-zero return value, Read returns a non io.EOF error. Do not pass in a
	// started command.
	StdoutReader(ctx context.Context) (io.ReadCloser, error)
}

// LocalGitCommand is a GitCommand interface implementation which runs git commands against local file system.
//
// This struct uses composition with exec.RemoteGitCommand which already provides all necessary means to run commands against
// local system.
type LocalGitCommand struct {
	Logger log.Logger

	// ReposDir is needed in order to LocalGitCommand be used like RemoteGitCommand (providing only repo name without its full path)
	// Unlike RemoteGitCommand, which is run against server who knows the directory where repos are located, LocalGitCommand is
	// run locally, therefore the knowledge about repos location should be provided explicitly by setting this field
	ReposDir   string
	repo       api.RepoName
	args       []string
	exitStatus int
}

func NewLocalGitCommand(repo api.RepoName, arg ...string) *LocalGitCommand {
	args := append([]string{git}, arg...)
	return &LocalGitCommand{
		repo:   repo,
		args:   args,
		Logger: log.Scoped("local"),
	}
}

const NoReposDirErrorMsg = "No ReposDir provided, command cannot be run without it"

func (l *LocalGitCommand) DividedOutput(ctx context.Context) ([]byte, []byte, error) {
	if l.ReposDir == "" {
		l.Logger.Error(NoReposDirErrorMsg)
		return nil, nil, errors.New(NoReposDirErrorMsg)
	}
	cmd := exec.CommandContext(ctx, git, l.Args()[1:]...) // stripping "git" itself
	var stderrBuf bytes.Buffer
	var stdoutBuf bytes.Buffer
	cmd.Stdout = &stdoutBuf
	cmd.Stderr = &stderrBuf

	dir := protocol.NormalizeRepo(l.Repo())
	repoPath := filepath.Join(l.ReposDir, filepath.FromSlash(string(dir)))
	gitPath := filepath.Join(repoPath, ".git")
	cmd.Dir = repoPath
	if cmd.Env == nil {
		// Do not strip out existing env when setting.
		cmd.Env = os.Environ()
	}
	cmd.Env = append(cmd.Env, "GIT_DIR="+gitPath)

	err := cmd.Run()
	exitStatus := -10810         // sentinel value to indicate not set
	if cmd.ProcessState != nil { // is nil if process failed to start
		exitStatus = cmd.ProcessState.Sys().(syscall.WaitStatus).ExitStatus()
	}
	l.exitStatus = exitStatus

	return stdoutBuf.Bytes(), bytes.TrimSpace(stderrBuf.Bytes()), err
}

func (l *LocalGitCommand) Output(ctx context.Context) ([]byte, error) {
	stdout, _, err := l.DividedOutput(ctx)
	return stdout, err
}

func (l *LocalGitCommand) CombinedOutput(ctx context.Context) ([]byte, error) {
	stdout, stderr, err := l.DividedOutput(ctx)
	return append(stdout, stderr...), err
}

func (l *LocalGitCommand) Repo() api.RepoName { return l.repo }

func (l *LocalGitCommand) Args() []string { return l.args }

func (l *LocalGitCommand) ExitStatus() int { return l.exitStatus }

func (l *LocalGitCommand) StdoutReader(ctx context.Context) (io.ReadCloser, error) {
	output, err := l.Output(ctx)
	return io.NopCloser(bytes.NewReader(output)), err
}

func (l *LocalGitCommand) String() string { return fmt.Sprintf("%q", l.Args()) }

// RemoteGitCommand represents a command to be executed remotely.
type RemoteGitCommand struct {
	repo       api.RepoName // the repository to execute the command in
	args       []string
	exitStatus int
	execer     execer
	execOp     *observation.Operation
	scope      string
}

type execer interface {
	ClientForRepo(ctx context.Context, repo api.RepoName) (proto.GitserverServiceClient, error)
}

// DividedOutput runs the command and returns its standard output and standard error.
func (c *RemoteGitCommand) DividedOutput(ctx context.Context) ([]byte, []byte, error) {
	rc, err := c.sendExec(ctx)
	if err != nil {
		return nil, nil, err
	}
	defer rc.Close()

	stdout, err := io.ReadAll(rc)
	if err != nil {
		if v := (&CommandStatusError{}); errors.As(err, &v) {
			c.exitStatus = int(v.StatusCode)
			if v.Message != "" {
				return stdout, []byte(v.Stderr), errors.New(v.Message)
			} else {
				return stdout, []byte(v.Stderr), v
			}
		}
		return nil, nil, errors.Wrap(err, "reading exec output")
	}

	return stdout, nil, nil
}

// Output runs the command and returns its standard output. If the command
// fails it usually returns CommandStatusError.
func (c *RemoteGitCommand) Output(ctx context.Context) ([]byte, error) {
	// Note: we do not use DividedOutput because we don't want its behaviour
	// where it throws away stderr in the error message. Stderr in error is
	// useful to us because the client is not asking for it.

	rc, err := c.sendExec(ctx)
	if err != nil {
		return nil, err
	}
	defer rc.Close()

	return io.ReadAll(rc)
}

// CombinedOutput runs the command and returns its combined standard output and standard error.
func (c *RemoteGitCommand) CombinedOutput(ctx context.Context) ([]byte, error) {
	stdout, stderr, err := c.DividedOutput(ctx)
	return append(stdout, stderr...), err
}

func (c *RemoteGitCommand) Repo() api.RepoName { return c.repo }

func (c *RemoteGitCommand) Args() []string { return c.args }

func (c *RemoteGitCommand) ExitStatus() int { return c.exitStatus }

func (c *RemoteGitCommand) String() string { return fmt.Sprintf("%q", c.args) }

// StdoutReader returns an io.ReadCloser of stdout of c. If the command has a
// non-zero return value, Read returns a non io.EOF error. Do not pass in a
// started command.
func (c *RemoteGitCommand) StdoutReader(ctx context.Context) (io.ReadCloser, error) {
	return c.sendExec(ctx)
}
