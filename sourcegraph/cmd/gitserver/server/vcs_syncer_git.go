package server

import (
	"context"
	"os"
	"os/exec"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/api"

	"github.com/sourcegraph/sourcegraph/cmd/gitserver/server/common"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/server/urlredactor"
	"github.com/sourcegraph/sourcegraph/internal/vcs"
	"github.com/sourcegraph/sourcegraph/internal/wrexec"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// gitRepoSyncer is a syncer for Git repositories.
type gitRepoSyncer struct {
	recordingCommandFactory *wrexec.RecordingCommandFactory
}

func NewGitRepoSyncer(r *wrexec.RecordingCommandFactory) *gitRepoSyncer {
	return &gitRepoSyncer{recordingCommandFactory: r}
}

func (s *gitRepoSyncer) Type() string {
	return "git"
}

// testGitRepoExists is a test fixture that overrides the return value for
// GitRepoSyncer.IsCloneable when it is set.
var testGitRepoExists func(ctx context.Context, remoteURL *vcs.URL) error

// IsCloneable checks to see if the Git remote URL is cloneable.
func (s *gitRepoSyncer) IsCloneable(ctx context.Context, repoName api.RepoName, remoteURL *vcs.URL) error {
	if isAlwaysCloningTestRemoteURL(remoteURL) {
		return nil
	}
	if testGitRepoExists != nil {
		return testGitRepoExists(ctx, remoteURL)
	}

	args := []string{"ls-remote", remoteURL.String(), "HEAD"}
	ctx, cancel := context.WithTimeout(ctx, shortGitCommandTimeout(args))
	defer cancel()

	cmd := exec.CommandContext(ctx, "git", args...)
	out, err := runRemoteGitCommand(ctx, s.recordingCommandFactory.WrapWithRepoName(ctx, log.NoOp(), repoName, cmd), true, nil)
	if err != nil {
		if ctxerr := ctx.Err(); ctxerr != nil {
			err = ctxerr
		}
		if len(out) > 0 {
			err = &common.GitCommandError{Err: err, Output: string(out)}
		}
		return err
	}
	return nil
}

// CloneCommand returns the command to be executed for cloning a Git repository.
func (s *gitRepoSyncer) CloneCommand(ctx context.Context, remoteURL *vcs.URL, tmpPath string) (cmd *exec.Cmd, err error) {
	if err := os.MkdirAll(tmpPath, os.ModePerm); err != nil {
		return nil, errors.Wrapf(err, "clone failed to create tmp dir")
	}

	cmd = exec.CommandContext(ctx, "git", "init", "--bare", ".")
	cmd.Dir = tmpPath
	if err := cmd.Run(); err != nil {
		return nil, errors.Wrapf(&common.GitCommandError{Err: err}, "clone setup failed")
	}

	cmd, _ = s.fetchCommand(ctx, remoteURL)
	cmd.Dir = tmpPath
	return cmd, nil
}

// Fetch tries to fetch updates of a Git repository.
func (s *gitRepoSyncer) Fetch(ctx context.Context, remoteURL *vcs.URL, repoName api.RepoName, dir common.GitDir, _ string) ([]byte, error) {
	cmd, configRemoteOpts := s.fetchCommand(ctx, remoteURL)
	dir.Set(cmd)
	output, err := runRemoteGitCommand(ctx, s.recordingCommandFactory.WrapWithRepoName(ctx, log.NoOp(), repoName, cmd), configRemoteOpts, nil)
	if err != nil {
		return nil, &common.GitCommandError{Err: err, Output: urlredactor.New(remoteURL).Redact(string(output))}
	}
	return output, nil
}

// RemoteShowCommand returns the command to be executed for showing remote of a Git repository.
func (s *gitRepoSyncer) RemoteShowCommand(ctx context.Context, remoteURL *vcs.URL) (cmd *exec.Cmd, err error) {
	return exec.CommandContext(ctx, "git", "remote", "show", remoteURL.String()), nil
}

func (s *gitRepoSyncer) fetchCommand(ctx context.Context, remoteURL *vcs.URL) (cmd *exec.Cmd, configRemoteOpts bool) {
	configRemoteOpts = true
	if customCmd := customFetchCmd(ctx, remoteURL); customCmd != nil {
		cmd = customCmd
		configRemoteOpts = false
	} else if useRefspecOverrides() {
		cmd = refspecOverridesFetchCmd(ctx, remoteURL)
	} else {
		cmd = exec.CommandContext(ctx, "git", "fetch",
			"--progress", "--prune", remoteURL.String(),
			// Normal git refs
			"+refs/heads/*:refs/heads/*", "+refs/tags/*:refs/tags/*",
			// GitHub pull requests
			"+refs/pull/*:refs/pull/*",
			// GitLab merge requests
			"+refs/merge-requests/*:refs/merge-requests/*",
			// Bitbucket pull requests
			"+refs/pull-requests/*:refs/pull-requests/*",
			// Gerrit changesets
			"+refs/changes/*:refs/changes/*",
			// Possibly deprecated refs for sourcegraph zap experiment?
			"+refs/sourcegraph/*:refs/sourcegraph/*")
	}
	return cmd, configRemoteOpts
}
