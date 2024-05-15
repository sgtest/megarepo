package server

import (
	"context"
	"io/ioutil"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"
)

const (
	testRepoA = "testrepo-A"
	testRepoB = "testrepo-B"
	testRepoC = "testrepo-C"
)

func TestCleanupInactive(t *testing.T) {
	root, err := ioutil.TempDir("", "gitserver-test-")
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(root)

	repoA := path.Join(root, testRepoA, ".git")
	cmd := exec.Command("git", "--bare", "init", repoA)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}
	repoB := path.Join(root, testRepoB, ".git")
	cmd = exec.Command("git", "--bare", "init", repoB)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}
	repoC := path.Join(root, testRepoC, ".git")
	if err := os.MkdirAll(repoC, os.ModePerm); err != nil {
		t.Fatal(err)
	}
	filepath.Walk(repoB, func(p string, _ os.FileInfo, _ error) error {
		// Rollback the mtime for these files to simulate an old repo.
		return os.Chtimes(p, time.Now().Add(-inactiveRepoTTL-time.Hour), time.Now().Add(-inactiveRepoTTL-time.Hour))
	})

	s := &Server{ReposDir: root, DeleteStaleRepositories: true}
	s.Handler() // Handler as a side-effect sets up Server
	s.cleanupRepos()

	if _, err := os.Stat(repoA); os.IsNotExist(err) {
		t.Error("expected repoA not to be removed")
	}
	if _, err := os.Stat(repoB); err == nil {
		t.Error("expected repoB to be removed during clean up")
	}
	if _, err := os.Stat(repoC); err == nil {
		t.Error("expected corrupt repoC to be removed during clean up")
	}
}

func TestCleanupExpired(t *testing.T) {
	root, err := ioutil.TempDir("", "gitserver-test-")
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(root)

	repoA := path.Join(root, testRepoA, ".git")
	cmd := exec.Command("git", "--bare", "init", repoA)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}
	repoB := path.Join(root, testRepoB, ".git")
	cmd = exec.Command("git", "--bare", "init", repoB)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}
	remote := path.Join(root, testRepoC, ".git")
	cmd = exec.Command("git", "--bare", "init", remote)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}

	origRepoRemoteURL := repoRemoteURL
	repoRemoteURL = func(ctx context.Context, dir string) (string, error) {
		return remote, nil
	}
	defer func() { repoRemoteURL = origRepoRemoteURL }()

	atime, err := os.Stat(filepath.Join(repoA, "HEAD"))
	if err != nil {
		t.Fatal(err)
	}
	cmd = exec.Command("git", "config", "--add", "sourcegraph.recloneTimestamp", strconv.FormatInt(time.Now().Add(-(2*repoTTL)).Unix(), 10))
	cmd.Dir = repoB
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}

	s := &Server{ReposDir: root}
	s.Handler() // Handler as a side-effect sets up Server
	s.cleanupRepos()

	fi, err := os.Stat(filepath.Join(repoA, "HEAD"))
	if err != nil {
		// repoA should still exist.
		t.Fatal(err)
	}
	if atime.ModTime().Before(fi.ModTime()) {
		// repoA should not have been recloned.
		t.Error("expected repoA to not be modified")
	}
	fi, err = os.Stat(repoB)
	if err != nil {
		// repoB should still exist after being recloned.
		t.Fatal(err)
	}
	// Expect the repo to be recloned hand have a recent mod time.
	ti := time.Now().Add(-repoTTL)
	if fi.ModTime().Before(ti) {
		t.Error("expected repoB to be recloned during clean up")
	}
}

func TestCleanupOldLocks(t *testing.T) {
	root, cleanup := tmpDir(t)
	defer cleanup()

	// Only recent lock files should remain.
	mkFiles(t, root,
		"github.com/foo/empty/.git/HEAD",

		"github.com/foo/freshconfiglock/.git/HEAD",
		"github.com/foo/freshconfiglock/.git/config.lock",

		"github.com/foo/freshpacked/.git/HEAD",
		"github.com/foo/freshpacked/.git/packed-refs.lock",

		"github.com/foo/staleconfiglock/.git/HEAD",
		"github.com/foo/staleconfiglock/.git/config.lock",

		"github.com/foo/stalepacked/.git/HEAD",
		"github.com/foo/stalepacked/.git/packed-refs.lock",

		"github.com/foo/refslock/.git/HEAD",
		"github.com/foo/refslock/.git/refs/heads/fresh",
		"github.com/foo/refslock/.git/refs/heads/fresh.lock",
		"github.com/foo/refslock/.git/refs/heads/stale",
		"github.com/foo/refslock/.git/refs/heads/stale.lock",
	)

	chtime := func(p string, age time.Duration) {
		err := os.Chtimes(filepath.Join(root, p), time.Now().Add(-age), time.Now().Add(-age))
		if err != nil {
			t.Fatal(err)
		}
	}
	chtime("github.com/foo/staleconfiglock/.git/config.lock", time.Hour)
	chtime("github.com/foo/stalepacked/.git/packed-refs.lock", 2*time.Hour)
	chtime("github.com/foo/refslock/.git/refs/heads/stale.lock", 2*time.Hour)

	s := &Server{ReposDir: root}
	s.Handler() // Handler as a side-effect sets up Server
	s.cleanupRepos()

	assertPaths(t, root,
		"github.com/foo/empty/.git/HEAD",
		"github.com/foo/empty/.git/info/attributes",

		"github.com/foo/freshconfiglock/.git/HEAD",
		"github.com/foo/freshconfiglock/.git/config.lock",
		"github.com/foo/freshconfiglock/.git/info/attributes",

		"github.com/foo/freshpacked/.git/HEAD",
		"github.com/foo/freshpacked/.git/packed-refs.lock",
		"github.com/foo/freshpacked/.git/info/attributes",

		"github.com/foo/staleconfiglock/.git/HEAD",
		"github.com/foo/staleconfiglock/.git/info/attributes",

		"github.com/foo/stalepacked/.git/HEAD",
		"github.com/foo/stalepacked/.git/info/attributes",

		"github.com/foo/refslock/.git/HEAD",
		"github.com/foo/refslock/.git/refs/heads/fresh",
		"github.com/foo/refslock/.git/refs/heads/fresh.lock",
		"github.com/foo/refslock/.git/refs/heads/stale",
		"github.com/foo/refslock/.git/info/attributes",
	)
}

func TestSetupAndClearTmp(t *testing.T) {
	root, cleanup := tmpDir(t)
	defer cleanup()

	s := &Server{ReposDir: root}

	// All non .git paths should become .git
	mkFiles(t, root,
		"github.com/foo/baz/.git/HEAD",
		"example.org/repo/.git/HEAD",

		// Needs to be deleted
		".tmp/foo",
		".tmp/baz/bam",

		// Older tmp cleanups that failed
		".tmp-old123/foo",
	)

	tmp, err := s.SetupAndClearTmp()
	if err != nil {
		t.Fatal(err)
	}

	// Straight after cleaning .tmp should be empty
	assertPaths(t, filepath.Join(root, ".tmp"), ".")

	// tmp should exist
	if info, err := os.Stat(tmp); err != nil {
		t.Fatal(err)
	} else if !info.IsDir() {
		t.Fatal("tmpdir is not a dir")
	}

	// tmp should be on the same mount as root, ie root is parent.
	if filepath.Dir(tmp) != root {
		t.Fatalf("tmp is not under root: tmp=%s root=%s", tmp, root)
	}

	// Wait until async cleaning is done
	for i := 0; i < 1000; i++ {
		found := false
		files, err := ioutil.ReadDir(s.ReposDir)
		if err != nil {
			t.Fatal(err)
		}
		for _, f := range files {
			found = found || strings.HasPrefix(f.Name(), ".tmp-old")
		}
		if !found {
			break
		}
		time.Sleep(10 * time.Millisecond)
	}

	// Only files should be the repo files
	assertPaths(t, root,
		"github.com/foo/baz/.git/HEAD",
		"example.org/repo/.git/HEAD",
		".tmp",
	)
}

func TestSetupAndClearTmp_Empty(t *testing.T) {
	root, cleanup := tmpDir(t)
	defer cleanup()

	s := &Server{ReposDir: root}

	_, err := s.SetupAndClearTmp()
	if err != nil {
		t.Fatal(err)
	}

	// No files, just the empty .tmp dir should exist
	assertPaths(t, root, ".tmp")
}

func TestRemoveRepoDirectory(t *testing.T) {
	root, cleanup := tmpDir(t)
	defer cleanup()

	mkFiles(t, root,
		"github.com/foo/baz/.git/HEAD",
		"github.com/foo/survior/.git/HEAD",
		"github.com/bam/bam/.git/HEAD",
		"example.com/repo/.git/HEAD",
	)
	s := &Server{
		ReposDir: root,
	}

	// Remove everything but github.com/foo/survior
	for _, d := range []string{
		"github.com/foo/baz/.git",
		"github.com/bam/bam/.git",
		"example.com/repo/.git",
	} {
		if err := s.removeRepoDirectory(filepath.Join(root, d)); err != nil {
			t.Fatalf("failed to remove %s: %s", d, err)
		}
	}

	assertPaths(t, root,
		"github.com/foo/survior/.git/HEAD",
		".tmp",
	)
}

func TestRemoveRepoDirectory_Empty(t *testing.T) {
	root, cleanup := tmpDir(t)
	defer cleanup()

	mkFiles(t, root,
		"github.com/foo/baz/.git/HEAD",
	)
	s := &Server{
		ReposDir: root,
	}

	if err := s.removeRepoDirectory(filepath.Join(root, "github.com/foo/baz/.git")); err != nil {
		t.Fatal(err)
	}

	assertPaths(t, root,
		".tmp",
	)
}
