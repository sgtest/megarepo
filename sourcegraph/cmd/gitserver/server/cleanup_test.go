package server

import (
	"context"
	"io"
	"io/ioutil"
	"log"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"sort"
	"strings"
	"testing"
	"testing/quick"
	"time"

	"github.com/pkg/errors"
)

const (
	testRepoA = "testrepo-A"
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
	repoC := path.Join(root, testRepoC, ".git")
	if err := os.MkdirAll(repoC, os.ModePerm); err != nil {
		t.Fatal(err)
	}

	s := &Server{ReposDir: root, DeleteStaleRepositories: true}
	s.Handler() // Handler as a side-effect sets up Server
	s.cleanupRepos()

	if _, err := os.Stat(repoA); os.IsNotExist(err) {
		t.Error("expected repoA not to be removed")
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

	repoNew := path.Join(root, "repo-new", ".git")
	repoOld := path.Join(root, "repo-old", ".git")
	repoGCNew := path.Join(root, "repo-gc-new", ".git")
	repoGCOld := path.Join(root, "repo-gc-old", ".git")
	repoBoom := path.Join(root, "repo-boom", ".git")
	repoCorrupt := path.Join(root, "repo-corrupt", ".git")
	remote := path.Join(root, "remote", ".git")
	for _, path := range []string{repoNew, repoOld, repoGCNew, repoGCOld, repoBoom, repoCorrupt, remote} {
		cmd := exec.Command("git", "--bare", "init", path)
		if err := cmd.Run(); err != nil {
			t.Fatal(err)
		}
	}

	origRepoRemoteURL := repoRemoteURL
	repoRemoteURL = func(ctx context.Context, dir GitDir) (string, error) {
		if string(dir) == repoBoom {
			return "", errors.Errorf("boom")
		}
		return remote, nil
	}
	defer func() { repoRemoteURL = origRepoRemoteURL }()

	modTime := func(path string) time.Time {
		t.Helper()
		fi, err := os.Stat(filepath.Join(path, "HEAD"))
		if err != nil {
			t.Fatal(err)
		}
		return fi.ModTime()
	}
	recloneTime := func(path string) time.Time {
		t.Helper()
		ts, err := getRecloneTime(GitDir(path))
		if err != nil {
			t.Fatal(err)
		}
		return ts
	}

	writeFile(t, filepath.Join(repoGCNew, "gc.log"), []byte("warning: There are too many unreachable loose objects; run 'git prune' to remove them."))
	writeFile(t, filepath.Join(repoGCOld, "gc.log"), []byte("warning: There are too many unreachable loose objects; run 'git prune' to remove them."))

	for path, delta := range map[string]time.Duration{
		repoOld:     2 * repoTTL,
		repoGCOld:   2 * repoTTLGC,
		repoBoom:    2 * repoTTL,
		repoCorrupt: repoTTLGC / 2, // should only trigger corrupt, not old
	} {
		ts := time.Now().Add(-delta)
		if err := setRecloneTime(GitDir(path), ts); err != nil {
			t.Fatal(err)
		}
		if err := os.Chtimes(filepath.Join(path, "HEAD"), ts, ts); err != nil {
			t.Fatal(err)
		}
	}
	if err := gitConfigSet(GitDir(repoCorrupt), "sourcegraph.maybeCorruptRepo", "1"); err != nil {
		t.Fatal(err)
	}

	now := time.Now()
	repoNewTime := modTime(repoNew)
	repoOldTime := modTime(repoOld)
	repoGCNewTime := modTime(repoGCNew)
	repoGCOldTime := modTime(repoGCOld)
	repoCorruptTime := modTime(repoBoom)
	repoBoomTime := modTime(repoBoom)
	repoBoomRecloneTime := recloneTime(repoBoom)

	s := &Server{ReposDir: root}
	s.Handler() // Handler as a side-effect sets up Server
	s.cleanupRepos()

	// repos that shouldn't be recloned
	if repoNewTime.Before(modTime(repoNew)) {
		t.Error("expected repoNew to not be modified")
	}
	if repoGCNewTime.Before(modTime(repoGCNew)) {
		t.Error("expected repoGCNew to not be modified")
	}

	// repos that should be recloned
	if !repoOldTime.Before(modTime(repoOld)) {
		t.Error("expected repoOld to be recloned during clean up")
	}
	if !repoGCOldTime.Before(modTime(repoGCOld)) {
		t.Error("expected repoGCOld to be recloned during clean up")
	}
	if !repoCorruptTime.Before(modTime(repoCorrupt)) {
		t.Error("expected repoCorrupt to be recloned during clean up")
	}

	// repos that fail to clone need to have recloneTime updated
	if repoBoomTime.Before(modTime(repoBoom)) {
		t.Fatal("expected repoBoom to fail to reclone due to hardcoding getRemoteURL failure")
	}
	if !repoBoomRecloneTime.Before(recloneTime(repoBoom)) {
		t.Error("expected repoBoom reclone time to be updated")
	}
	if !now.After(recloneTime(repoBoom)) {
		t.Error("expected repoBoom reclone time to be updated to not now")
	}
}

func TestCleanupOldLocks(t *testing.T) {
	root := tmpDir(t)

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
	root := tmpDir(t)

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
	root := tmpDir(t)

	s := &Server{ReposDir: root}

	_, err := s.SetupAndClearTmp()
	if err != nil {
		t.Fatal(err)
	}

	// No files, just the empty .tmp dir should exist
	assertPaths(t, root, ".tmp")
}

func TestRemoveRepoDirectory(t *testing.T) {
	root := tmpDir(t)

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
		if err := s.removeRepoDirectory(GitDir(filepath.Join(root, d))); err != nil {
			t.Fatalf("failed to remove %s: %s", d, err)
		}
	}

	assertPaths(t, root,
		"github.com/foo/survior/.git/HEAD",
		".tmp",
	)
}

func TestRemoveRepoDirectory_Empty(t *testing.T) {
	root := tmpDir(t)

	mkFiles(t, root,
		"github.com/foo/baz/.git/HEAD",
	)
	s := &Server{
		ReposDir: root,
	}

	if err := s.removeRepoDirectory(GitDir(filepath.Join(root, "github.com/foo/baz/.git"))); err != nil {
		t.Fatal(err)
	}

	assertPaths(t, root,
		".tmp",
	)
}

func Test_howManyBytesToFree(t *testing.T) {
	const G = 1024 * 1024 * 1024
	s := &Server{
		DesiredPercentFree: 10,
	}

	tcs := []struct {
		name      string
		diskSize  uint64
		bytesFree uint64
		want      int64
	}{
		{
			name:      "if there is already enough space, no space is freed",
			diskSize:  10 * G,
			bytesFree: 1.5 * G,
			want:      0,
		},
		{
			name:      "if there is exactly enough space, no space is freed",
			diskSize:  10 * G,
			bytesFree: 1 * G,
			want:      0,
		},
		{
			name:      "if there not enough space, some space is freed",
			diskSize:  10 * G,
			bytesFree: 0.5 * G,
			want:      int64(0.5 * G),
		},
	}

	for _, tc := range tcs {
		t.Run(tc.name, func(t *testing.T) {
			s.DiskSizer = &fakeDiskSizer{
				diskSize:  tc.diskSize,
				bytesFree: tc.bytesFree,
			}
			b, err := s.howManyBytesToFree()
			if err != nil {
				t.Fatal(err)
			}
			if b != tc.want {
				t.Errorf("s.howManyBytesToFree(...) is %v, want 0", b)
			}
		})
	}
}

type fakeDiskSizer struct {
	bytesFree uint64
	diskSize  uint64
}

func (f *fakeDiskSizer) BytesFreeOnDisk(mountPoint string) (uint64, error) {
	return f.bytesFree, nil
}

func (f *fakeDiskSizer) DiskSizeBytes(mountPoint string) (uint64, error) {
	return f.diskSize, nil
}

func tmpDir(t *testing.T) string {
	t.Helper()
	dir, err := ioutil.TempDir("", filepath.Base(t.Name()))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { os.RemoveAll(dir) })
	return dir
}

func mkFiles(t *testing.T, root string, paths ...string) {
	t.Helper()
	for _, p := range paths {
		if err := os.MkdirAll(filepath.Join(root, filepath.Dir(p)), os.ModePerm); err != nil {
			t.Fatal(err)
		}
		writeFile(t, filepath.Join(root, p), nil)
	}
}

func writeFile(t *testing.T, path string, content []byte) {
	t.Helper()
	err := ioutil.WriteFile(path, content, 0666)
	if err != nil {
		t.Fatal(err)
	}
}

// assertPaths checks that all paths under want exist. It excludes non-empty directories
func assertPaths(t *testing.T, root string, want ...string) {
	t.Helper()
	notfound := make(map[string]struct{})
	for _, p := range want {
		notfound[p] = struct{}{}
	}
	var unwanted []string
	err := filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		if info.IsDir() {
			if empty, err := isEmptyDir(path); err != nil {
				t.Fatal(err)
			} else if !empty {
				return nil
			}
		}
		rel, err := filepath.Rel(root, path)
		if err != nil {
			return err
		}
		if _, ok := notfound[rel]; ok {
			delete(notfound, rel)
		} else {
			unwanted = append(unwanted, rel)
		}
		return nil
	})
	if err != nil {
		log.Fatal(err)
	}

	if len(notfound) > 0 {
		var paths []string
		for p := range notfound {
			paths = append(paths, p)
		}
		sort.Strings(paths)
		t.Errorf("did not find expected paths: %s", strings.Join(paths, " "))
	}
	if len(unwanted) > 0 {
		sort.Strings(unwanted)
		t.Errorf("found unexpected paths: %s", strings.Join(unwanted, " "))
	}
}

func isEmptyDir(path string) (bool, error) {
	f, err := os.Open(path)
	if err != nil {
		return false, err
	}
	defer f.Close()

	_, err = f.Readdirnames(1)
	if err == io.EOF {
		return true, nil
	}
	return false, err
}

func TestFreeUpSpace(t *testing.T) {
	t.Run("no error if no space requested and no repos", func(t *testing.T) {
		s := &Server{DiskSizer: &fakeDiskSizer{}}
		if err := s.freeUpSpace(0); err != nil {
			t.Fatal(err)
		}
	})
	t.Run("error if space requested and no repos", func(t *testing.T) {
		s := &Server{DiskSizer: &fakeDiskSizer{}}
		if err := s.freeUpSpace(1); err == nil {
			t.Fatal("want error")
		}
	})
	t.Run("oldest repo gets removed to free up space", func(t *testing.T) {
		// Set up.
		rd, err := ioutil.TempDir("", "freeUpSpace")
		if err != nil {
			t.Fatal(err)
		}
		r1 := filepath.Join(rd, "repo1")
		r2 := filepath.Join(rd, "repo2")
		if err := makeFakeRepo(r1, 1000); err != nil {
			t.Fatal(err)
		}
		if err := makeFakeRepo(r2, 1000); err != nil {
			t.Fatal(err)
		}
		// Force the modification time of r2 to be after that of r1.
		fi1, err := os.Stat(r1)
		if err != nil {
			t.Fatal(err)
		}
		mtime2 := fi1.ModTime().Add(time.Second)
		if err := os.Chtimes(r2, time.Now(), mtime2); err != nil {
			t.Fatal(err)
		}

		// Run.
		s := Server{
			ReposDir:  rd,
			DiskSizer: &fakeDiskSizer{},
		}
		if err := s.freeUpSpace(1000); err != nil {
			t.Fatal(err)
		}

		// Check.
		assertPaths(t, rd,
			".tmp",
			"repo2/.git/HEAD",
			"repo2/.git/space_eater")
		rds, err := dirSize(rd)
		if err != nil {
			t.Fatal(err)
		}
		wantSize := int64(1000)
		if rds > wantSize {
			t.Errorf("repo dir size is %d, want no more than %d", rds, wantSize)
		}
	})
}

func makeFakeRepo(d string, sizeBytes int) error {
	gd := filepath.Join(d, ".git")
	if err := os.MkdirAll(gd, 0700); err != nil {
		return errors.Wrap(err, "creating .git dir and any parents")
	}
	if err := ioutil.WriteFile(filepath.Join(gd, "HEAD"), nil, 0666); err != nil {
		return errors.Wrap(err, "creating HEAD file")
	}
	if err := ioutil.WriteFile(filepath.Join(gd, "space_eater"), make([]byte, sizeBytes), 0666); err != nil {
		return errors.Wrapf(err, "writing to space_eater file")
	}
	return nil
}

func TestMaybeCorruptStderrRe(t *testing.T) {
	bad := []string{
		"error: packfile .git/objects/pack/pack-a.pack does not match index",
		"error: Could not read d24d09b8bc5d1ea2c3aa24455f4578db6aa3afda\n",
		`error: short SHA1 1325 is ambiguous
error: Could not read d24d09b8bc5d1ea2c3aa24455f4578db6aa3afda`,
		`unrelated
error: Could not read d24d09b8bc5d1ea2c3aa24455f4578db6aa3afda`,
		"\n\nerror: Could not read d24d09b8bc5d1ea2c3aa24455f4578db6aa3afda",
	}
	good := []string{
		"",
		"error: short SHA1 1325 is ambiguous",
		"error: object 156639577dd2ea91cdd53b25352648387d985743 is a blob, not a commit",
		"error: object 45043b3ff0440f4d7937f8c68f8fb2881759edef is a tree, not a commit",
	}
	for _, stderr := range bad {
		if !maybeCorruptStderrRe.MatchString(stderr) {
			t.Errorf("should contain corrupt line:\n%s", stderr)
		}
	}
	for _, stderr := range good {
		if maybeCorruptStderrRe.MatchString(stderr) {
			t.Errorf("should not contain corrupt line:\n%s", stderr)
		}
	}
}

func TestJitterDuration(t *testing.T) {
	f := func(key string) bool {
		d := jitterDuration(key, repoTTLGC/4)
		return 0 <= d && d < repoTTLGC/4
	}
	if err := quick.Check(f, nil); err != nil {
		t.Error(err)
	}
}
