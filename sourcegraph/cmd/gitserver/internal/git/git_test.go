package git

import (
	"context"
	"fmt"
	"math/rand"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/common"
)

func TestMakeBareRepo(t *testing.T) {
	dir := t.TempDir()
	ctx := context.Background()

	require.NoError(t, MakeBareRepo(ctx, dir))

	// Now verify we created a valid repo.
	c := exec.CommandContext(ctx, "git", "rev-parse", "HEAD")
	c.Dir = dir
	out, err := c.CombinedOutput()
	require.NoError(t, err)
	require.Equal(t, "HEAD\n", string(out))
}

func BenchmarkQuickRevParseHeadQuickSymbolicRefHead_packed_refs(b *testing.B) {
	tmp := b.TempDir()

	dir := filepath.Join(tmp, ".git")
	gitDir := common.GitDir(dir)
	if err := os.Mkdir(dir, 0o700); err != nil {
		b.Fatal(err)
	}

	masterRef := "refs/heads/master"
	// This simulates the most amount of work QuickRevParseHead has to do, and
	// is also the most common in prod. That is where the final rev is in
	// packed-refs.
	err := os.WriteFile(filepath.Join(dir, "HEAD"), []byte(fmt.Sprintf("ref: %s\n", masterRef)), 0o600)
	if err != nil {
		b.Fatal(err)
	}
	// in prod the kubernetes repo has a packed-refs file that is 62446 lines
	// long. Simulate something like that with everything except master
	masterRev := "4d5092a09bca95e0153c423d76ef62d4fcd168ec"
	{
		f, err := os.Create(filepath.Join(dir, "packed-refs"))
		if err != nil {
			b.Fatal(err)
		}
		writeRef := func(refBase string, num int) {
			_, err := fmt.Fprintf(f, "%016x%016x%08x %s-%d\n", rand.Uint64(), rand.Uint64(), rand.Uint32(), refBase, num)
			if err != nil {
				b.Fatal(err)
			}
		}
		for i := 0; i < 32; i++ {
			writeRef("refs/heads/feature-branch", i)
		}
		_, err = fmt.Fprintf(f, "%s refs/heads/master\n", masterRev)
		if err != nil {
			b.Fatal(err)
		}
		for i := 0; i < 10000; i++ {
			// actual format is refs/pull/${i}/head, but doesn't actually
			// matter for testing
			writeRef("refs/pull/head", i)
			writeRef("refs/pull/merge", i)
		}
		err = f.Close()
		if err != nil {
			b.Fatal(err)
		}
	}

	// Exclude setup
	b.ResetTimer()

	for n := 0; n < b.N; n++ {
		rev, err := QuickRevParseHead(gitDir)
		if err != nil {
			b.Fatal(err)
		}
		if rev != masterRev {
			b.Fatal("unexpected rev: ", rev)
		}
		ref, err := QuickSymbolicRefHead(gitDir)
		if err != nil {
			b.Fatal(err)
		}
		if ref != masterRef {
			b.Fatal("unexpected ref: ", ref)
		}
	}

	// Exclude cleanup (defers)
	b.StopTimer()
}

func BenchmarkQuickRevParseHeadQuickSymbolicRefHead_unpacked_refs(b *testing.B) {
	tmp := b.TempDir()

	dir := filepath.Join(tmp, ".git")
	gitDir := common.GitDir(dir)
	if err := os.Mkdir(dir, 0o700); err != nil {
		b.Fatal(err)
	}

	// This simulates the usual case for a repo that HEAD is often
	// updated. The master ref will be unpacked.
	masterRef := "refs/heads/master"
	masterRev := "4d5092a09bca95e0153c423d76ef62d4fcd168ec"
	files := map[string]string{
		"HEAD":              fmt.Sprintf("ref: %s\n", masterRef),
		"refs/heads/master": masterRev + "\n",
	}
	for path, content := range files {
		path = filepath.Join(dir, path)
		err := os.MkdirAll(filepath.Dir(path), 0o700)
		if err != nil {
			b.Fatal(err)
		}
		err = os.WriteFile(path, []byte(content), 0o600)
		if err != nil {
			b.Fatal(err)
		}
	}

	// Exclude setup
	b.ResetTimer()

	for n := 0; n < b.N; n++ {
		rev, err := QuickRevParseHead(gitDir)
		if err != nil {
			b.Fatal(err)
		}
		if rev != masterRev {
			b.Fatal("unexpected rev: ", rev)
		}
		ref, err := QuickSymbolicRefHead(gitDir)
		if err != nil {
			b.Fatal(err)
		}
		if ref != masterRef {
			b.Fatal("unexpected ref: ", ref)
		}
	}

	// Exclude cleanup (defers)
	b.StopTimer()
}

func TestRemoveBadRefs(t *testing.T) {
	dir := t.TempDir()
	gitDir := common.GitDir(filepath.Join(dir, ".git"))

	cmd := func(name string, arg ...string) string {
		t.Helper()
		return runCmd(t, dir, name, arg...)
	}
	wantCommit := makeSingleCommitRepo(cmd)

	for _, name := range []string{"HEAD", "head", "Head", "HeAd"} {
		// Tag
		cmd("git", "tag", name)

		if dontWant := cmd("git", "rev-parse", "HEAD"); dontWant == wantCommit {
			t.Logf("WARNING: git tag %s failed to produce ambiguous output: %s", name, dontWant)
		}

		if err := RemoveBadRefs(context.Background(), gitDir); err != nil {
			t.Fatal(err)
		}

		if got := cmd("git", "rev-parse", "HEAD"); got != wantCommit {
			t.Fatalf("git tag %s failed to be removed: %s", name, got)
		}

		// Ref
		if err := os.WriteFile(filepath.Join(dir, ".git", "refs", "heads", name), []byte(wantCommit), 0o600); err != nil {
			t.Fatal(err)
		}

		if dontWant := cmd("git", "rev-parse", "HEAD"); dontWant == wantCommit {
			t.Logf("WARNING: git ref %s failed to produce ambiguous output: %s", name, dontWant)
		}

		if err := RemoveBadRefs(context.Background(), gitDir); err != nil {
			t.Fatal(err)
		}

		if got := cmd("git", "rev-parse", "HEAD"); got != wantCommit {
			t.Fatalf("git ref %s failed to be removed: %s", name, got)
		}
	}
}

// makeSingleCommitRepo make create a new repo with a single commit and returns
// the HEAD SHA
func makeSingleCommitRepo(cmd func(string, ...string) string) string {
	// Setup a repo with a commit so we can see if we can clone it.
	cmd("git", "init", ".")
	cmd("sh", "-c", "echo hello world > hello.txt")
	return addCommitToRepo(cmd)
}

// addCommitToRepo adds a commit to the repo at the current path.
func addCommitToRepo(cmd func(string, ...string) string) string {
	// Setup a repo with a commit so we can see if we can clone it.
	cmd("git", "add", "hello.txt")
	cmd("git", "commit", "-m", "hello")
	return cmd("git", "rev-parse", "HEAD")
}

func runCmd(t *testing.T, dir string, cmd string, arg ...string) string {
	t.Helper()
	c := exec.Command(cmd, arg...)
	c.Dir = dir
	c.Env = []string{
		"GIT_COMMITTER_NAME=a",
		"GIT_COMMITTER_EMAIL=a@a.com",
		"GIT_AUTHOR_NAME=a",
		"GIT_AUTHOR_EMAIL=a@a.com",
	}
	b, err := c.CombinedOutput()
	if err != nil {
		t.Fatalf("%s %s failed: %s\nOutput: %s", cmd, strings.Join(arg, " "), err, b)
	}
	return string(b)
}
