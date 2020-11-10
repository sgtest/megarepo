package server

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"sync/atomic"
	"time"

	"github.com/inconshreveable/log15"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/protocol"
)

var patchID uint64

func (s *Server) handleCreateCommitFromPatch(w http.ResponseWriter, r *http.Request) {
	var req protocol.CreateCommitFromPatchRequest
	var resp protocol.CreateCommitFromPatchResponse
	var status int

	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		resp := new(protocol.CreateCommitFromPatchResponse)
		resp.SetError("", "", "", errors.Wrap(err, "decoding CreateCommitFromPatchRequest"))
		status = http.StatusBadRequest
	} else {
		status, resp = s.createCommitFromPatch(r.Context(), req)
	}

	w.WriteHeader(status)
	if err := json.NewEncoder(w).Encode(resp); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
}

func (s *Server) createCommitFromPatch(ctx context.Context, req protocol.CreateCommitFromPatchRequest) (int, protocol.CreateCommitFromPatchResponse) {
	var resp protocol.CreateCommitFromPatchResponse

	repo := string(protocol.NormalizeRepo(req.Repo))
	repoGitDir := filepath.Join(s.ReposDir, repo, ".git")
	if _, err := os.Stat(repoGitDir); os.IsNotExist(err) {
		repoGitDir = filepath.Join(s.ReposDir, repo)
		if _, err := os.Stat(repoGitDir); os.IsNotExist(err) {
			resp.SetError(repo, "", "", errors.Wrap(err, "gitserver: repo does not exist"))
			return http.StatusInternalServerError, resp
		}
	}

	ref := req.TargetRef

	var (
		remoteURL string
		err       error
	)
	remoteURL, err = repoRemoteURL(ctx, GitDir(repoGitDir))
	if err != nil {
		log15.Error("Failed to get remote URL", "ref", ref, "err", err)
		resp.SetError(repo, "", "", errors.Wrap(err, "repoRemoteURL"))
		return http.StatusInternalServerError, resp
	}

	// Patch in the new token if needed.
	remoteURL, err = updateRemoteURLForPush(req.Push, remoteURL)
	if err != nil {
		log15.Error("Failed to apply push options to the remote URL", "err", err)
		resp.SetError(repo, "", "", errors.Wrap(err, "applying push options"))
		return http.StatusInternalServerError, resp
	}

	redactor := newURLRedactor(remoteURL)
	defer func() {
		if resp.Error != nil {
			resp.Error.Command = redactor.redact(resp.Error.Command)
			resp.Error.CombinedOutput = redactor.redact(resp.Error.CombinedOutput)
			if resp.Error.InternalError != "" {
				resp.Error.InternalError = redactor.redact(resp.Error.InternalError)
			}
		}
	}()

	// Ensure tmp directory exists
	tmpRepoDir, err := s.tempDir("patch-repo-")
	if err != nil {
		resp.SetError(repo, "", "", errors.Wrap(err, "gitserver: make tmp repo"))
		return http.StatusInternalServerError, resp
	}
	defer cleanUpTmpRepo(tmpRepoDir)

	argsToString := func(args []string) string {
		return strings.Join(args, " ")
	}

	// Temporary logging command wrapper
	prefix := fmt.Sprintf("%d %s ", atomic.AddUint64(&patchID, 1), repo)
	run := func(cmd *exec.Cmd, reason string) ([]byte, error) {
		t := time.Now()
		out, err := cmd.CombinedOutput()
		if err != nil {
			resp.SetError(repo, argsToString(cmd.Args), string(out), errors.Wrap(err, "gitserver: "+reason))
			log15.Info("command failed", "prefix", prefix, "command", argsToString(cmd.Args), "duration", time.Since(t), "error", err, "output", string(out))
		} else {
			log15.Info("command ran successfully", "prefix", prefix, "command", argsToString(cmd.Args), "duration", time.Since(t), "output", string(out))
		}
		return out, err
	}

	if req.UniqueRef {
		refs, err := repoRemoteRefs(ctx, remoteURL, ref)
		if err != nil {
			log15.Error("Failed to get remote refs", "ref", ref, "err", err)
			resp.SetError(repo, "", "", errors.Wrap(err, "repoRemoteRefs"))
			return http.StatusInternalServerError, resp
		}

		retry := 1
		tmp := ref
		for {
			if _, ok := refs[tmp]; !ok {
				break
			}
			tmp = ref + "-" + strconv.Itoa(retry)
			retry++
		}
		ref = tmp
	}

	if req.Push != nil {
		ref = ensureRefPrefix(ref)
	}

	tmpGitPathEnv := "GIT_DIR=" + filepath.Join(tmpRepoDir, ".git")

	tmpObjectsDir := filepath.Join(tmpRepoDir, ".git", "objects")
	repoObjectsDir := filepath.Join(repoGitDir, "objects")

	altObjectsEnv := "GIT_ALTERNATE_OBJECT_DIRECTORIES=" + repoObjectsDir

	cmd := exec.CommandContext(ctx, "git", "init")
	cmd.Dir = tmpRepoDir
	cmd.Env = append(os.Environ(), tmpGitPathEnv)

	if _, err := run(cmd, "init tmp repo"); err != nil {
		return http.StatusInternalServerError, resp
	}

	cmd = exec.CommandContext(ctx, "git", "reset", "-q", string(req.BaseCommit))
	cmd.Dir = tmpRepoDir
	cmd.Env = append(os.Environ(), tmpGitPathEnv, altObjectsEnv)

	if out, err := run(cmd, "basing staging on base rev"); err != nil {
		log15.Error("Failed to base the temporary repo on the base revision.", "ref", ref, "base", req.BaseCommit, "output", string(out))
		return http.StatusInternalServerError, resp
	}

	applyArgs := append([]string{"apply", "--cached"}, req.GitApplyArgs...)
	cmd = exec.CommandContext(ctx, "git", applyArgs...)
	cmd.Dir = tmpRepoDir
	cmd.Env = append(os.Environ(), tmpGitPathEnv, altObjectsEnv)
	cmd.Stdin = strings.NewReader(req.Patch)

	if out, err := run(cmd, "applying patch"); err != nil {
		log15.Error("Failed to apply patch.", "ref", ref, "output", string(out))
		return http.StatusInternalServerError, resp
	}

	message := req.CommitInfo.Message
	if message == "" {
		message = "<Sourcegraph> Creating commit from patch"
	}
	authorName := req.CommitInfo.AuthorName
	if authorName == "" {
		authorName = "Sourcegraph"
	}
	authorEmail := req.CommitInfo.AuthorEmail
	if authorEmail == "" {
		authorEmail = "support@sourcegraph.com"
	}
	committerName := req.CommitInfo.CommitterName
	if committerName == "" {
		committerName = authorName
	}
	committerEmail := req.CommitInfo.CommitterEmail
	if committerEmail == "" {
		committerEmail = authorEmail
	}

	cmd = exec.CommandContext(ctx, "git", "commit", "-m", message)
	cmd.Dir = tmpRepoDir
	cmd.Env = append(os.Environ(), []string{
		tmpGitPathEnv,
		altObjectsEnv,
		fmt.Sprintf("GIT_COMMITTER_NAME=%s", committerName),
		fmt.Sprintf("GIT_COMMITTER_EMAIL=%s", committerEmail),
		fmt.Sprintf("GIT_AUTHOR_NAME=%s", authorName),
		fmt.Sprintf("GIT_AUTHOR_EMAIL=%s", authorEmail),
		fmt.Sprintf("GIT_COMMITTER_DATE=%v", req.CommitInfo.Date),
		fmt.Sprintf("GIT_AUTHOR_DATE=%v", req.CommitInfo.Date),
	}...)

	if out, err := run(cmd, "committing patch"); err != nil {
		log15.Error("Failed to commit patch.", "ref", ref, "output", out)
		return http.StatusInternalServerError, resp
	}

	cmd = exec.CommandContext(ctx, "git", "rev-parse", "HEAD")
	cmd.Dir = tmpRepoDir
	cmd.Env = append(os.Environ(), tmpGitPathEnv, altObjectsEnv)

	// We don't use 'run' here as we only want stdout
	out, err := cmd.Output()
	if err != nil {
		resp.SetError(repo, argsToString(cmd.Args), string(out), errors.Wrap(err, "gitserver: retrieving new commit id"))
		return http.StatusInternalServerError, resp
	}
	cmtHash := strings.TrimSpace(string(out))

	// Move objects from tmpObjectsDir to repoObjectsDir.
	err = filepath.Walk(tmpObjectsDir, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		if info.IsDir() {
			return nil
		}
		rel, err := filepath.Rel(tmpObjectsDir, path)
		if err != nil {
			return err
		}
		dst := filepath.Join(repoObjectsDir, rel)
		if err := os.MkdirAll(filepath.Dir(dst), os.ModePerm); err != nil {
			return err
		}
		// do the actual move. If dst exists we can ignore the error since it
		// will contain the same content (content addressable FTW).
		if err := os.Rename(path, dst); err != nil && !os.IsExist(err) {
			return err
		}
		return nil
	})
	if err != nil {
		resp.SetError(repo, "", "", errors.Wrap(err, "copying git objects"))
		return http.StatusInternalServerError, resp
	}

	if req.Push != nil {
		cmd = exec.CommandContext(ctx, "git", "push", "--force", remoteURL, fmt.Sprintf("%s:%s", cmtHash, ref))
		cmd.Dir = repoGitDir

		if out, err = run(cmd, "pushing ref"); err != nil {
			log15.Error("Failed to push", "ref", ref, "commit", cmtHash, "output", string(out))
			return http.StatusInternalServerError, resp
		}
	}

	resp.Rev = "refs/" + strings.TrimPrefix(ref, "refs/")

	cmd = exec.CommandContext(ctx, "git", "update-ref", "--", ref, cmtHash)
	cmd.Dir = repoGitDir

	if out, err = run(cmd, "creating ref"); err != nil {
		log15.Error("Failed to create ref for commit.", "ref", ref, "commit", cmtHash, "output", string(out))
		return http.StatusInternalServerError, resp
	}

	return http.StatusOK, resp
}

func cleanUpTmpRepo(path string) {
	err := os.RemoveAll(path)
	if err != nil {
		log15.Info("unable to clean up tmp repo", "path", path, "err", err)
	}
}

// ensureRefPrefix checks whether the ref is a full ref and contains the
// "refs/heads" prefix (i.e. "refs/heads/master") or just an abbreviated ref
// (i.e. "master") and adds the "refs/heads/" prefix if the latter is the case.
//
// Copied from git package to avoid cycle import when testing git package.
func ensureRefPrefix(ref string) string {
	return "refs/heads/" + strings.TrimPrefix(ref, "refs/heads/")
}

// updateRemoteURLForPush applies the given PushConfig to the remote URL,
// returning the new remote URL.
func updateRemoteURLForPush(push *protocol.PushConfig, remoteURL string) (string, error) {
	if push == nil {
		return remoteURL, nil

	}

	u, err := url.Parse(remoteURL)
	if err != nil {
		return "", errors.Wrap(err, "parsing remote URL")
	}

	if push.Username != "" && push.Password == "" {
		u.User = url.User(push.Username)
	} else if push.Username != "" && push.Password != "" {
		u.User = url.UserPassword(push.Username, push.Password)
	}

	return u.String(), nil
}
