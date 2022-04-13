package git

import (
	"context"
	"strings"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/trace/ot"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// execSafe executes a Git subcommand iff it is allowed according to a allowlist.
//
// An error is only returned when there is a failure unrelated to the actual
// command being executed. If the executed command exits with a nonzero exit
// code, err == nil. This is similar to how http.Get returns a nil error for HTTP
// non-2xx responses.
//
// execSafe should NOT be exported. We want to limit direct git calls to this
// package.
func execSafe(ctx context.Context, db database.DB, repo api.RepoName, params []string) (stdout, stderr []byte, exitCode int, err error) {
	if Mocks.ExecSafe != nil {
		return Mocks.ExecSafe(params)
	}

	span, ctx := ot.StartSpanFromContext(ctx, "Git: execSafe")
	defer span.Finish()

	if len(params) == 0 {
		return nil, nil, 0, errors.New("at least one argument required")
	}

	if !gitdomain.IsAllowedGitCmd(params) {
		return nil, nil, 0, errors.Errorf("command failed: %q is not a allowed git command", params)
	}

	cmd := gitserver.NewClient(db).Command(repo, "git", params...)
	stdout, stderr, err = cmd.DividedOutput(ctx)
	exitCode = cmd.ExitStatus()
	if exitCode != 0 && err != nil {
		err = nil // the error must just indicate that the exit code was nonzero
	}
	return stdout, stderr, exitCode, err
}

// checkSpecArgSafety returns a non-nil err if spec begins with a "-", which
// could cause it to be interpreted as a git command line argument.
func checkSpecArgSafety(spec string) error {
	if strings.HasPrefix(spec, "-") {
		return errors.Errorf("invalid git revision spec %q (begins with '-')", spec)
	}
	return nil
}

func gitserverCmdFunc(repo api.RepoName, db database.DB) cmdFunc {
	return func(args []string) cmd {
		return gitserver.NewClient(db).Command(repo, "git", args...)
	}
}

// cmdFunc is a func that creates a new executable Git command.
type cmdFunc func(args []string) cmd

// cmd is an executable Git command.
type cmd interface {
	Output(context.Context) ([]byte, error)
	String() string
}
