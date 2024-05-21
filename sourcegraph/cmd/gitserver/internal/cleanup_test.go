package internal

import (
	"context"
	"io"
	"io/fs"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"testing"
	"testing/quick"
	"time"

	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/common"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/git"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/git/gitcli"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/gitserverfs"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbmocks"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/connection"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/wrexec"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const (
	testRepoA = "testrepo-A"
	testRepoC = "testrepo-C"
)

func newMockedGitserverDB() database.DB {
	db := dbmocks.NewMockDB()
	gs := dbmocks.NewMockGitserverRepoStore()
	db.GitserverReposFunc.SetDefaultReturn(gs)
	return db
}

// TODO: Only test the repo size part of the cleanup routine, not all of it.
func TestCleanup_computeStats(t *testing.T) {
	root := t.TempDir()

	for _, name := range []string{"a", "b/d", "c"} {
		p := path.Join(root, name, ".git")
		if err := os.MkdirAll(p, 0o755); err != nil {
			t.Fatal(err)
		}
		cmd := exec.Command("git", "--bare", "init", p)
		if err := cmd.Run(); err != nil {
			t.Fatal(err)
		}
	}

	logger, capturedLogs := logtest.Captured(t)
	db := database.NewDB(logger, dbtest.NewDB(t))

	if _, err := db.ExecContext(context.Background(), `
INSERT INTO repo(id, name, private) VALUES (1, 'a', false), (2, 'b/d', false), (3, 'c', true);
UPDATE gitserver_repos SET shard_id = 1, clone_status = 'cloned';
UPDATE gitserver_repos SET repo_size_bytes = 5 where repo_id = 3;
`); err != nil {
		t.Fatalf("unexpected error while inserting test data: %s", err)
	}

	fs := gitserverfs.New(observation.TestContextTB(t), root)
	require.NoError(t, fs.Initialize())
	cleanupRepos(
		actor.WithInternalActor(context.Background()),
		logger,
		db,
		fs,
		func(dir common.GitDir, repoName api.RepoName) git.GitBackend {
			b := git.NewMockGitBackend()
			b.ConfigFunc.SetDefaultReturn(git.NewMockGitConfigBackend())
			return b
		},
		wrexec.NewNoOpRecordingCommandFactory(),
		"test-gitserver",
		connection.GitserverAddresses{Addresses: []string{"test-gitserver"}},
		false,
	)

	// This may be different in practice, but the way we setup the tests
	// we only have .git dirs to measure so this is correct.
	wantGitDirBytes, err := fs.DirSize(root)
	require.NoError(t, err)

	for i := 1; i <= 3; i++ {
		repo, err := db.GitserverRepos().GetByID(context.Background(), api.RepoID(i))
		if err != nil {
			t.Fatal(err)
		}
		if repo.RepoSizeBytes == 0 {
			t.Fatalf("repo %d - repo_size_bytes is not updated: %d", i, repo.RepoSizeBytes)
		}
	}

	// Check that the size in the DB is properly set.
	haveGitDirBytes, err := db.GitserverRepos().GetGitserverGitDirSize(context.Background())
	if err != nil {
		t.Fatal(err)
	}

	if wantGitDirBytes != haveGitDirBytes {
		t.Fatalf("git dir size in db does not match actual size. want=%d have=%d", wantGitDirBytes, haveGitDirBytes)
	}

	logs := capturedLogs()
	for _, cl := range logs {
		if cl.Level == "error" {
			t.Errorf("test run has collected an errorneous log: %s", cl.Message)
		}
	}
}

func TestCleanupInactive(t *testing.T) {
	root := t.TempDir()

	repoA := path.Join(root, testRepoA, ".git")
	cmd := exec.Command("git", "--bare", "init", repoA)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}

	repoC := path.Join(root, testRepoC, ".git")
	if err := os.MkdirAll(repoC, os.ModePerm); err != nil {
		t.Fatal(err)
	}

	fs := gitserverfs.New(observation.TestContextTB(t), root)
	require.NoError(t, fs.Initialize())

	cleanupRepos(
		context.Background(),
		logtest.Scoped(t),
		newMockedGitserverDB(),
		fs,
		func(dir common.GitDir, repoName api.RepoName) git.GitBackend {
			b := git.NewMockGitBackend()
			b.ConfigFunc.SetDefaultReturn(git.NewMockGitConfigBackend())
			return b
		},
		wrexec.NewNoOpRecordingCommandFactory(),
		"test-gitserver",
		connection.GitserverAddresses{Addresses: []string{"test-gitserver"}},
		false,
	)

	if _, err := os.Stat(repoA); os.IsNotExist(err) {
		t.Error("expected repoA not to be removed")
	}
	if _, err := os.Stat(repoC); err == nil {
		t.Error("expected corrupt repoC to be removed during clean up")
	}
}

func TestCleanupWrongShard(t *testing.T) {
	t.Run("wrongShardName", func(t *testing.T) {
		root := t.TempDir()
		// should be allocated to shard gitserver-1
		testRepoD := "testrepo-D"

		repoA := path.Join(root, testRepoA, ".git")
		cmd := exec.Command("git", "--bare", "init", repoA)
		if err := cmd.Run(); err != nil {
			t.Fatal(err)
		}
		repoD := path.Join(root, testRepoD, ".git")
		cmdD := exec.Command("git", "--bare", "init", repoD)
		if err := cmdD.Run(); err != nil {
			t.Fatal(err)
		}

		fs := gitserverfs.New(observation.TestContextTB(t), root)
		require.NoError(t, fs.Initialize())

		cleanupRepos(
			context.Background(),
			logtest.Scoped(t),
			newMockedGitserverDB(),
			fs,
			func(dir common.GitDir, repoName api.RepoName) git.GitBackend {
				b := git.NewMockGitBackend()
				b.ConfigFunc.SetDefaultReturn(git.NewMockGitConfigBackend())
				return b
			},
			wrexec.NewNoOpRecordingCommandFactory(),
			"does-not-exist",
			connection.GitserverAddresses{Addresses: []string{"gitserver-0", "gitserver-1"}},
			false,
		)

		if _, err := os.Stat(repoA); err != nil {
			t.Error("expected repoA not to be removed")
		}
		if _, err := os.Stat(repoD); err != nil {
			t.Error("expected repoD assigned to different shard not to be removed")
		}
	})
	t.Run("substringShardName", func(t *testing.T) {
		root := t.TempDir()
		// should be allocated to shard gitserver-1
		testRepoD := "testrepo-D"

		repoA := path.Join(root, testRepoA, ".git")
		cmd := exec.Command("git", "--bare", "init", repoA)
		if err := cmd.Run(); err != nil {
			t.Fatal(err)
		}
		repoD := path.Join(root, testRepoD, ".git")
		cmdD := exec.Command("git", "--bare", "init", repoD)
		if err := cmdD.Run(); err != nil {
			t.Fatal(err)
		}

		fs := gitserverfs.New(observation.TestContextTB(t), root)
		require.NoError(t, fs.Initialize())

		cleanupRepos(
			context.Background(),
			logtest.Scoped(t),
			newMockedGitserverDB(),
			fs,
			func(dir common.GitDir, repoName api.RepoName) git.GitBackend {
				b := git.NewMockGitBackend()
				b.ConfigFunc.SetDefaultReturn(git.NewMockGitConfigBackend())
				return b
			},
			wrexec.NewNoOpRecordingCommandFactory(),
			"gitserver-0",
			connection.GitserverAddresses{Addresses: []string{"gitserver-0.cluster.local:3178", "gitserver-1.cluster.local:3178"}},
			false,
		)

		if _, err := os.Stat(repoA); err != nil {
			t.Error("expected repoA not to be removed")
		}
		if _, err := os.Stat(repoD); !os.IsNotExist(err) {
			t.Error("expected repoD assigned to different shard to be removed")
		}
	})
	t.Run("cleanupDisabled", func(t *testing.T) {
		root := t.TempDir()
		// should be allocated to shard gitserver-1
		testRepoD := "testrepo-D"

		repoA := path.Join(root, testRepoA, ".git")
		cmd := exec.Command("git", "--bare", "init", repoA)
		if err := cmd.Run(); err != nil {
			t.Fatal(err)
		}
		repoD := path.Join(root, testRepoD, ".git")
		cmdD := exec.Command("git", "--bare", "init", repoD)
		if err := cmdD.Run(); err != nil {
			t.Fatal(err)
		}

		fs := gitserverfs.New(observation.TestContextTB(t), root)
		require.NoError(t, fs.Initialize())

		cleanupRepos(
			context.Background(),
			logtest.Scoped(t),
			newMockedGitserverDB(),
			fs,
			func(dir common.GitDir, repoName api.RepoName) git.GitBackend {
				b := git.NewMockGitBackend()
				b.ConfigFunc.SetDefaultReturn(git.NewMockGitConfigBackend())
				return b
			},
			wrexec.NewNoOpRecordingCommandFactory(),
			"gitserver-0",
			connection.GitserverAddresses{Addresses: []string{"gitserver-0", "gitserver-1"}},
			true,
		)

		if _, err := os.Stat(repoA); os.IsNotExist(err) {
			t.Error("expected repoA not to be removed")
		}
		if _, err := os.Stat(repoD); err != nil {
			t.Error("expected repoD assigned to different shard not to be removed", err)
		}
	})
}

// Note that the exact values (e.g. 50 commits) below are related to git's
// internal heuristics regarding whether or not to invoke `git gc --auto`.
//
// They are stable today, but may become flaky in the future if/when the
// relevant internal magic numbers and transformations change.
func TestGitGCAuto(t *testing.T) {
	// Create a test repository with detectable garbage that GC can prune.
	wd := t.TempDir()
	repo := filepath.Join(wd, "garbage-repo")
	runCmd(t, wd, "git", "init", "--initial-branch", "main", repo)

	// First we need to generate a moderate number of commits.
	for range 50 {
		runCmd(t, repo, "sh", "-c", "echo 1 >> file1")
		runCmd(t, repo, "git", "add", "file1")
		runCmd(t, repo, "git", "commit", "-m", "file1")
	}

	// Now on a second branch, we do the same thing.
	runCmd(t, repo, "git", "checkout", "-b", "secondary")
	for range 50 {
		runCmd(t, repo, "sh", "-c", "echo 2 >> file2")
		runCmd(t, repo, "git", "add", "file2")
		runCmd(t, repo, "git", "commit", "-m", "file2")
	}

	// Bring everything back together in one branch.
	runCmd(t, repo, "git", "checkout", "main")
	runCmd(t, repo, "git", "merge", "secondary")

	// Now create a bare repo like gitserver expects
	root := t.TempDir()
	wdRepo := repo
	repo = filepath.Join(root, "garbage-repo")
	runCmd(t, root, "git", "clone", "--bare", wdRepo, filepath.Join(repo, ".git"))

	// `git count-objects -v` can indicate objects, packs, etc.
	// We'll run this before and after to verify that an action
	// was taken by `git gc --auto`.
	countObjects := func() string {
		t.Helper()
		return runCmd(t, repo, "git", "count-objects", "-v")
	}

	// Verify that we have GC-able objects in the repository.
	if strings.Contains(countObjects(), "count: 0") {
		t.Fatalf("expected git to report objects but none found")
	}

	fs := gitserverfs.New(observation.TestContextTB(t), root)
	require.NoError(t, fs.Initialize())

	cleanupRepos(
		context.Background(),
		logtest.Scoped(t),
		newMockedGitserverDB(),
		fs,
		func(dir common.GitDir, repoName api.RepoName) git.GitBackend {
			b := git.NewMockGitBackend()
			b.ConfigFunc.SetDefaultReturn(git.NewMockGitConfigBackend())
			return b
		},
		wrexec.NewNoOpRecordingCommandFactory(),
		"test-gitserver",
		connection.GitserverAddresses{Addresses: []string{"test-gitserver"}},
		false,
	)

	// Verify that there are no more GC-able objects in the repository.
	if !strings.Contains(countObjects(), "count: 0") {
		t.Fatalf("expected git to report no objects, but found some")
	}
}

func TestCleanupBroken(t *testing.T) {
	conf.Mock(&conf.Unified{})
	t.Cleanup(func() {
		conf.Mock(nil)
	})

	// Don't attempt to run GC.
	gitGCMode = gitGCModeGitAutoGC

	ctx := context.Background()
	root := t.TempDir()

	repoOld := path.Join(root, "repo-old", ".git")
	repoGCNew := path.Join(root, "repo-gc-new", ".git")
	repoGCOld := path.Join(root, "repo-gc-old", ".git")
	repoCorrupt := path.Join(root, "repo-corrupt", ".git")
	repoNonBare := path.Join(root, "repo-non-bare", ".git")
	repoPerforceGCOld := path.Join(root, "repo-perforce-gc-old", ".git")
	remote := path.Join(root, "remote", ".git")
	for _, gitDirPath := range []string{
		repoOld,
		repoGCNew, repoGCOld,
		repoCorrupt,
		repoPerforceGCOld,
		remote,
	} {
		cmd := exec.Command("git", "--bare", "init", gitDirPath)
		if err := cmd.Run(); err != nil {
			t.Fatal(err)
		}
	}

	if err := exec.Command("git", "init", filepath.Dir(repoNonBare)).Run(); err != nil {
		t.Fatal(err)
	}

	writeFile(t, filepath.Join(repoGCNew, "gc.log"), []byte("warning: There are too many unreachable loose objects; run 'git prune' to remove them."))
	writeFile(t, filepath.Join(repoGCOld, "gc.log"), []byte("warning: There are too many unreachable loose objects; run 'git prune' to remove them."))

	for gitDirPath, delta := range map[string]time.Duration{
		repoGCOld:         2 * repoTTLGC,
		repoCorrupt:       repoTTLGC / 2, // should only trigger corrupt, not old
		repoPerforceGCOld: 2 * repoTTLGC,
	} {
		ts := time.Now().Add(-delta)
		cli := gitcli.NewBackend(logtest.Scoped(t), wrexec.NewNoOpRecordingCommandFactory(), common.GitDir(gitDirPath), "git")
		require.NoError(t, cli.Config().Set(ctx, gitConfigGCFailed, strconv.Itoa(int(ts.Unix()))))
		if err := os.Chtimes(filepath.Join(gitDirPath, "HEAD"), ts, ts); err != nil {
			t.Fatal(err)
		}
	}
	{
		f, err := os.Create(filepath.Join(repoCorrupt, gitcli.RepoMaybeCorruptFlagFilepath))
		require.NoError(t, err)
		require.NoError(t, f.Close())
	}
	{
		cli := gitcli.NewBackend(logtest.Scoped(t), wrexec.NewNoOpRecordingCommandFactory(), common.GitDir(repoPerforceGCOld), "perforce")
		if err := git.SetRepositoryType(ctx, cli.Config(), "perforce"); err != nil {
			t.Fatal(err)
		}
	}

	if _, err := os.Stat(repoNonBare); err != nil {
		t.Fatal(err)
	}

	fs := gitserverfs.NewMockFSFrom(gitserverfs.New(&observation.TestContext, root))
	require.NoError(t, fs.Initialize())
	calledRemove := []api.RepoName{}
	fs.RemoveRepoFunc.SetDefaultHook(func(repo api.RepoName) error {
		calledRemove = append(calledRemove, repo)
		return nil
	})

	cleanupRepos(
		context.Background(),
		logtest.Scoped(t),
		newMockedGitserverDB(),
		fs,
		func(dir common.GitDir, repoName api.RepoName) git.GitBackend {
			return gitcli.NewBackend(logtest.Scoped(t), wrexec.NewNoOpRecordingCommandFactory(), dir, repoName)
		},
		wrexec.NewNoOpRecordingCommandFactory(),
		"test-gitserver",
		connection.GitserverAddresses{Addresses: []string{"test-gitserver"}},
		false,
	)

	require.Equal(t, []api.RepoName{"repo-corrupt", "repo-gc-old", "repo-non-bare"}, calledRemove)
}

func TestCleanup_RemoveNonExistentRepos(t *testing.T) {
	initRepos := func(root string) (repoExists string, repoNotExists string) {
		repoExists = path.Join(root, "repo-exists", ".git")
		repoNotExists = path.Join(root, "repo-not-exists", ".git")
		for _, gitDirPath := range []string{
			repoExists, repoNotExists,
		} {
			cmd := exec.Command("git", "--bare", "init", gitDirPath)
			if err := cmd.Run(); err != nil {
				t.Fatal(err)
			}
		}
		return repoExists, repoNotExists
	}

	mockGitServerRepos := dbmocks.NewMockGitserverRepoStore()
	mockGitServerRepos.GetByNameFunc.SetDefaultHook(func(_ context.Context, name api.RepoName) (*types.GitserverRepo, error) {
		if strings.Contains(string(name), "repo-exists") {
			return &types.GitserverRepo{}, nil
		} else {
			return nil, &database.ErrGitserverRepoNotFound{}
		}
	})
	mockRepos := dbmocks.NewMockRepoStore()
	mockRepos.ListMinimalReposFunc.SetDefaultReturn([]types.MinimalRepo{}, nil)

	mockDB := dbmocks.NewMockDB()
	mockDB.GitserverReposFunc.SetDefaultReturn(mockGitServerRepos)
	mockDB.ReposFunc.SetDefaultReturn(mockRepos)

	t.Run("Nothing happens if env var is not set", func(t *testing.T) {
		root := t.TempDir()
		repoExists, repoNotExists := initRepos(root)

		fs := gitserverfs.New(observation.TestContextTB(t), root)
		require.NoError(t, fs.Initialize())

		cleanupRepos(
			context.Background(),
			logtest.Scoped(t),
			mockDB,
			fs,
			func(dir common.GitDir, repoName api.RepoName) git.GitBackend {
				b := git.NewMockGitBackend()
				b.ConfigFunc.SetDefaultReturn(git.NewMockGitConfigBackend())
				return b
			},
			wrexec.NewNoOpRecordingCommandFactory(),
			"test-gitserver",
			connection.GitserverAddresses{Addresses: []string{"test-gitserver"}},
			false,
		)

		// nothing should happen if test env not declared to true
		if _, err := os.Stat(repoExists); err != nil {
			t.Fatalf("repo dir does not exist anymore %s", repoExists)
		}
		if _, err := os.Stat(repoNotExists); err != nil {
			t.Fatalf("repo dir does not exist anymore %s", repoNotExists)
		}
	})

	t.Run("Should delete the repo dir that is not defined in DB", func(t *testing.T) {
		mockRemoveNonExistingReposConfig(true)
		defer mockRemoveNonExistingReposConfig(false)
		root := t.TempDir()
		repoExists, repoNotExists := initRepos(root)

		fs := gitserverfs.New(observation.TestContextTB(t), root)
		require.NoError(t, fs.Initialize())

		cleanupRepos(
			context.Background(),
			logtest.Scoped(t),
			mockDB,
			fs,
			func(dir common.GitDir, repoName api.RepoName) git.GitBackend {
				b := git.NewMockGitBackend()
				b.ConfigFunc.SetDefaultReturn(git.NewMockGitConfigBackend())
				return b
			},
			wrexec.NewNoOpRecordingCommandFactory(),
			"test-gitserver",
			connection.GitserverAddresses{Addresses: []string{"test-gitserver"}},
			false,
		)

		if _, err := os.Stat(repoNotExists); err == nil {
			t.Fatal("repo not existing in DB was not removed")
		}
		if _, err := os.Stat(repoExists); err != nil {
			t.Fatal("repo existing in DB does not exist on disk anymore")
		}
	})
}

// TestCleanupOldLocks checks whether cleanupRepos removes stale lock files. It
// does not check whether each job in cleanupRepos finishes successfully, nor
// does it check if other files or directories have been created.
func TestCleanupOldLocks(t *testing.T) {
	type file struct {
		name        string
		age         time.Duration
		wantRemoved bool
	}

	cases := []struct {
		name  string
		files []file
	}{
		{
			name: "fresh_config_lock",
			files: []file{
				{
					name: "config.lock",
				},
			},
		},
		{
			name: "stale_config_lock",
			files: []file{
				{
					name:        "config.lock",
					age:         time.Hour,
					wantRemoved: true,
				},
			},
		},
		{
			name: "fresh_packed",
			files: []file{
				{
					name: "packed-refs.lock",
				},
			},
		},
		{
			name: "stale_packed",
			files: []file{
				{
					name:        "packed-refs.lock",
					age:         2 * time.Hour,
					wantRemoved: true,
				},
			},
		},
		{
			name: "fresh_commit-graph_lock",
			files: []file{
				{
					name: "objects/info/commit-graph.lock",
				},
			},
		},
		{
			name: "stale_commit-graph_lock",
			files: []file{
				{
					name:        "objects/info/commit-graph.lock",
					age:         2 * time.Hour,
					wantRemoved: true,
				},
			},
		},
		{
			name: "refs_lock",
			files: []file{
				{
					name: "refs/heads/fresh",
				},
				{
					name: "refs/heads/fresh.lock",
				},
				{
					name: "refs/heads/stale",
				},
				{
					name:        "refs/heads/stale.lock",
					age:         2 * time.Hour,
					wantRemoved: true,
				},
			},
		},
		{
			name: "fresh_gc.pid",
			files: []file{
				{
					name: "gc.pid",
				},
			},
		},
		{
			name: "stale_gc.pid",
			files: []file{
				{
					name:        "gc.pid",
					age:         48 * time.Hour,
					wantRemoved: true,
				},
			},
		},
	}

	root := t.TempDir()

	// initialize git directories and place files
	for _, c := range cases {
		cmd := exec.Command("git", "--bare", "init", c.name+"/.git")
		cmd.Dir = root
		if err := cmd.Run(); err != nil {
			t.Fatal(err)
		}
		dir := common.GitDir(filepath.Join(root, c.name, ".git"))
		for _, f := range c.files {
			writeFile(t, dir.Path(f.name), nil)
			if f.age == 0 {
				continue
			}
			err := os.Chtimes(dir.Path(f.name), time.Now().Add(-f.age), time.Now().Add(-f.age))
			if err != nil {
				t.Fatal(err)
			}
		}
	}

	fs := gitserverfs.New(observation.TestContextTB(t), root)
	require.NoError(t, fs.Initialize())

	cleanupRepos(
		context.Background(),
		logtest.Scoped(t),
		newMockedGitserverDB(),
		fs,
		func(dir common.GitDir, repoName api.RepoName) git.GitBackend {
			b := git.NewMockGitBackend()
			b.ConfigFunc.SetDefaultReturn(git.NewMockGitConfigBackend())
			return b
		},
		wrexec.NewNoOpRecordingCommandFactory(),
		"test-gitserver",
		connection.GitserverAddresses{Addresses: []string{"gitserver-0"}},
		false,
	)

	isRemoved := func(path string) bool {
		_, err := os.Stat(path)
		return errors.Is(err, os.ErrNotExist)
	}

	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			dir := common.GitDir(filepath.Join(root, c.name, ".git"))
			for _, f := range c.files {
				if f.wantRemoved != isRemoved(dir.Path(f.name)) {
					t.Fatalf("%s should have been removed", f.name)
				}
			}
		})
	}
}

func TestHowManyBytesToFree(t *testing.T) {
	const G = 1024 * 1024 * 1024
	logger := logtest.Scoped(t)

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
			b := howManyBytesToFree(
				logger,
				&fakeDiskUsage{
					diskSize:  tc.diskSize,
					bytesFree: tc.bytesFree,
				},
				10,
			)
			if b != tc.want {
				t.Errorf("s.howManyBytesToFree(...) is %v, want 0", b)
			}
		})
	}
}

type fakeDiskUsage struct {
	bytesFree uint64
	diskSize  uint64
}

func (f *fakeDiskUsage) Free() uint64 {
	return f.bytesFree
}

func (f *fakeDiskUsage) Size() uint64 {
	return f.diskSize
}

func (f *fakeDiskUsage) PercentUsed() float32 {
	return 1
}

func (f *fakeDiskUsage) Available() uint64 {
	return 1
}

// assertPaths checks that all paths under want exist. It excludes non-empty directories
func assertPaths(t *testing.T, root string, want ...string) {
	t.Helper()
	notfound := make(map[string]struct{})
	for _, p := range want {
		notfound[p] = struct{}{}
	}
	var unwanted []string
	err := filepath.Walk(root, func(path string, info fs.FileInfo, err error) error {
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
		t.Fatal(err)
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
	logger := logtest.Scoped(t)
	root := t.TempDir()
	fs := gitserverfs.New(observation.TestContextTB(t), root)
	require.NoError(t, fs.Initialize())
	t.Run("no error if no space requested and no repos", func(t *testing.T) {
		if err := freeUpSpace(context.Background(), logger, newMockedGitserverDB(), fs, "test-gitserver", &fakeDiskUsage{}, 10, 0); err != nil {
			t.Fatal(err)
		}
	})
	t.Run("error if space requested and no repos", func(t *testing.T) {
		if err := freeUpSpace(context.Background(), logger, newMockedGitserverDB(), fs, "test-gitserver", &fakeDiskUsage{}, 10, 1); err == nil {
			t.Fatal("want error")
		}
	})
	t.Run("oldest repo gets removed to free up space", func(t *testing.T) {
		r1 := filepath.Join(root, "repo1")
		r2 := filepath.Join(root, "repo2")
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

		db := dbmocks.NewMockDB()
		gr := dbmocks.NewMockGitserverRepoStore()
		db.GitserverReposFunc.SetDefaultReturn(gr)
		// Run.
		if err := freeUpSpace(context.Background(), logger, db, fs, "test-gitserver", &fakeDiskUsage{}, 10, 1000); err != nil {
			t.Fatal(err)
		}

		// Check.
		assertPaths(t, root,
			".tmp",
			".p4home",
			"repo2/.git/HEAD",
			"repo2/.git/space_eater")
		rds, err := fs.DirSize(root)
		require.NoError(t, err)
		wantSize := int64(1000)
		if rds > wantSize {
			t.Errorf("repo dir size is %d, want no more than %d", rds, wantSize)
		}

		if len(gr.SetCloneStatusFunc.History()) == 0 {
			t.Fatal("expected gitserverRepos.SetCloneStatus to be called, but wasn't")
		}
		require.Equal(t, gr.SetCloneStatusFunc.History()[0].Arg2, types.CloneStatusNotCloned)
	})
}

func makeFakeRepo(d string, sizeBytes int) error {
	gd := filepath.Join(d, ".git")
	if err := os.MkdirAll(gd, 0o700); err != nil {
		return errors.Wrap(err, "creating .git dir and any parents")
	}
	if err := os.WriteFile(filepath.Join(gd, "HEAD"), nil, 0o666); err != nil {
		return errors.Wrap(err, "creating HEAD file")
	}
	if err := os.WriteFile(filepath.Join(gd, "space_eater"), make([]byte, sizeBytes), 0o666); err != nil {
		return errors.Wrapf(err, "writing to space_eater file")
	}
	return nil
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

func prepareEmptyGitRepo(t *testing.T, dir string) common.GitDir {
	t.Helper()
	cmd := exec.Command("git", "init", ".")
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("execution error: %v, output %s", err, out)
	}
	cmd = exec.Command("git", "config", "user.email", "test@sourcegraph.com")
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("execution error: %v, output %s", err, out)
	}
	return common.GitDir(filepath.Join(dir, ".git"))
}

func TestTooManyLooseObjects(t *testing.T) {
	dir := t.TempDir()
	gitDir := prepareEmptyGitRepo(t, dir)

	// create sentinel object folder
	if err := os.MkdirAll(gitDir.Path("objects", "17"), fs.ModePerm); err != nil {
		t.Fatal(err)
	}

	touch := func(name string) error {
		file, err := os.Create(gitDir.Path("objects", "17", name))
		if err != nil {
			return err
		}
		return file.Close()
	}

	limit := 2 * 256 // 2 objects per folder

	cases := []struct {
		name string
		file string
		want bool
	}{
		{
			name: "empty",
			file: "",
			want: false,
		},
		{
			name: "1 file",
			file: "abc1",
			want: false,
		},
		{
			name: "ignore files with non-hexadecimal names",
			file: "abcxyz123",
			want: false,
		},
		{
			name: "2 files",
			file: "abc2",
			want: false,
		},
		{
			name: "3 files (too many)",
			file: "abc3",
			want: true,
		},
	}

	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if c.file != "" {
				err := touch(c.file)
				if err != nil {
					t.Fatal(err)
				}
			}
			tooManyLO, err := tooManyLooseObjects(gitDir, limit)
			if err != nil {
				t.Fatal(err)
			}
			if tooManyLO != c.want {
				t.Fatalf("want %t, got %t\n", c.want, tooManyLO)
			}
		})
	}
}

func TestTooManyLooseObjectsMissingSentinelDir(t *testing.T) {
	dir := t.TempDir()
	gitDir := prepareEmptyGitRepo(t, dir)

	_, err := tooManyLooseObjects(gitDir, 1)
	if err != nil {
		t.Fatal(err)
	}
}

func TestHasBitmap(t *testing.T) {
	dir := t.TempDir()
	gitDir := prepareEmptyGitRepo(t, dir)

	t.Run("empty git repo", func(t *testing.T) {
		hasBm, err := hasBitmap(gitDir)
		if err != nil {
			t.Fatal(err)
		}
		if hasBm {
			t.Fatalf("expected no bitmap file for an empty git repository")
		}
	})

	t.Run("repo with bitmap", func(t *testing.T) {
		script := `echo acont > afile
git add afile
git commit -am amsg
git repack -d -l -A --write-bitmap
`
		cmd := exec.Command("/bin/sh", "-euxc", script)
		cmd.Dir = dir
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("out=%s, err=%s", out, err)
		}
		hasBm, err := hasBitmap(gitDir)
		if err != nil {
			t.Fatal(err)
		}
		if !hasBm {
			t.Fatalf("expected bitmap file after running git repack -d -l -A --write-bitmap")
		}
	})
}

func TestTooManyPackFiles(t *testing.T) {
	dir := t.TempDir()
	gitDir := prepareEmptyGitRepo(t, dir)

	newPackFile := func(name string) error {
		file, err := os.Create(gitDir.Path("objects", "pack", name))
		if err != nil {
			return err
		}
		return file.Close()
	}

	packLimit := 1

	cases := []struct {
		name string
		file string
		want bool
	}{
		{
			name: "empty",
			file: "",
			want: false,
		},
		{
			name: "1 pack",
			file: "a.pack",
			want: false,
		},
		{
			name: "2 packs",
			file: "b.pack",
			want: true,
		},
		{
			name: "2 packs, with 1 keep file",
			file: "b.keep",
			want: false,
		},
	}

	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			if c.file != "" {
				err := newPackFile(c.file)
				if err != nil {
					t.Fatal(err)
				}
			}
			tooManyPf, err := tooManyPackfiles(gitDir, packLimit)
			if err != nil {
				t.Fatal(err)
			}
			if tooManyPf != c.want {
				t.Fatalf("want %t, got %t\n", c.want, tooManyPf)
			}
		})
	}
}

func TestHasCommitGraph(t *testing.T) {
	dir := t.TempDir()
	gitDir := prepareEmptyGitRepo(t, dir)

	t.Run("empty git repo", func(t *testing.T) {
		hasBm, err := hasCommitGraph(gitDir)
		if err != nil {
			t.Fatal(err)
		}
		if hasBm {
			t.Fatalf("expected no commit-graph file for an empty git repository")
		}
	})

	t.Run("commit-graph", func(t *testing.T) {
		script := `echo acont > afile
git add afile
git commit -am amsg
git commit-graph write --reachable --changed-paths
`
		cmd := exec.Command("/bin/sh", "-euxc", script)
		cmd.Dir = dir
		if out, err := cmd.CombinedOutput(); err != nil {
			t.Fatalf("out=%s, err=%s", out, err)
		}
		hasCg, err := hasCommitGraph(gitDir)
		if err != nil {
			t.Fatal(err)
		}
		if !hasCg {
			t.Fatalf("expected commit-graph file after running git commit-graph write --reachable --changed-paths")
		}
	})
}

func TestNeedsMaintenance(t *testing.T) {
	dir := t.TempDir()
	gitDir := prepareEmptyGitRepo(t, dir)

	needed, reason, err := needsMaintenance(gitDir)
	if err != nil {
		t.Fatal(err)
	}
	if reason != "bitmap" {
		t.Fatalf("want %s, got %s", "bitmap", reason)
	}
	if !needed {
		t.Fatal("repos without a bitmap should require a repack")
	}

	// create bitmap file and commit-graph
	script := `echo acont > afile
git add afile
git commit -am amsg
git repack -d -l -A --write-bitmap
git commit-graph write --reachable --changed-paths
`
	cmd := exec.Command("/bin/sh", "-euxc", script)
	cmd.Dir = dir
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("out=%s, err=%s", out, err)
	}

	needed, reason, err = needsMaintenance(gitDir)
	if err != nil {
		t.Fatal(err)
	}
	if reason != "skipped" {
		t.Fatalf("want %s, got %s", "skipped", reason)
	}
	if needed {
		t.Fatal("this repo doesn't need maintenance")
	}
}

func TestPruneIfNeeded(t *testing.T) {
	reposDir := t.TempDir()
	gitDir := prepareEmptyGitRepo(t, reposDir)

	// create sentinel object folder
	if err := os.MkdirAll(gitDir.Path("objects", "17"), fs.ModePerm); err != nil {
		t.Fatal(err)
	}

	limit := -1 // always run prune
	if err := pruneIfNeeded(wrexec.NewNoOpRecordingCommandFactory(), "reponame", gitDir, limit); err != nil {
		t.Fatal(err)
	}
}

func TestSGMLogFile(t *testing.T) {
	logger := logtest.Scoped(t)
	dir := common.GitDir(t.TempDir())
	cmd := exec.Command("git", "--bare", "init")
	dir.Set(cmd)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}

	mustHaveLogFile := func(t *testing.T) {
		t.Helper()
		content, err := os.ReadFile(dir.Path(sgmLog))
		if err != nil {
			t.Fatalf("%s should have been set: %s", sgmLog, err)
		}
		if len(content) == 0 {
			t.Fatal("log file should have contained command output")
		}
	}

	// break the repo
	fakeRef := dir.Path("refs", "heads", "apple")
	if _, err := os.Create(fakeRef); err != nil {
		t.Fatal("test setup failed. Could not create fake ref")
	}

	// failed run => log file
	if err := sgMaintenance(logger, dir); err == nil {
		t.Fatal("sgMaintenance should have returned an error")
	}
	mustHaveLogFile(t)

	if got := bestEffortReadFailed(dir); got != 1 {
		t.Fatalf("want 1, got %d", got)
	}

	// fix the repo
	_ = os.Remove(fakeRef)

	// fresh sgmLog file => skip execution
	if err := sgMaintenance(logger, dir); err != nil {
		t.Fatalf("unexpected error %s", err)
	}
	mustHaveLogFile(t)

	// backdate sgmLog file => sgMaintenance ignores log file
	old := time.Now().Add(-2 * sgmLogExpire)
	if err := os.Chtimes(dir.Path(sgmLog), old, old); err != nil {
		t.Fatal(err)
	}
	if err := sgMaintenance(logger, dir); err != nil {
		t.Fatalf("unexpected error %s", err)
	}
	if _, err := os.Stat(dir.Path(sgmLog)); err == nil {
		t.Fatalf("%s should have been removed", sgmLog)
	}
}

func TestBestEffortReadFailed(t *testing.T) {
	tc := []struct {
		content     []byte
		wantRetries int
	}{
		{
			content:     nil,
			wantRetries: 0,
		},
		{
			content:     []byte("any content"),
			wantRetries: 0,
		},
		{
			content: []byte(`failed=1

error message`),
			wantRetries: 1,
		},
		{
			content: []byte(`header text
failed=2
error message`),
			wantRetries: 2,
		},
		{
			content: []byte(`failed=

error message`),
			wantRetries: 0,
		},
		{
			content: []byte(`failed=deadbeaf

error message`),
			wantRetries: 0,
		},
		{
			content: []byte(`failed
failed=deadbeaf
failed=1`),
			wantRetries: 0,
		},
		{
			content: []byte(`failed
failed=1
failed=deadbead`),
			wantRetries: 1,
		},
		{
			content: []byte(`failed=
failed=
error message`),
			wantRetries: 0,
		},
		{
			content: []byte(`header failed text

failed=3
failed=4

error message
`),
			wantRetries: 3,
		},
	}

	for _, tt := range tc {
		t.Run(string(tt.content), func(t *testing.T) {
			if got := bestEffortParseFailed(tt.content); got != tt.wantRetries {
				t.Fatalf("want %d, got %d", tt.wantRetries, got)
			}
		})
	}
}

// We test whether the lock set by sg maintenance is respected by git gc.
func TestGitGCRespectsLock(t *testing.T) {
	dir := common.GitDir(t.TempDir())
	cmd := exec.Command("git", "--bare", "init")
	dir.Set(cmd)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}

	err, unlock := lockRepoForGC(dir)
	if err != nil {
		t.Fatal(err)
	}

	cmd = exec.Command("git", "gc")
	dir.Set(cmd)
	b, err := cmd.CombinedOutput()
	if err == nil {
		t.Fatal("expected command to return with non-zero exit value")
	}

	// We check that git complains about the lockfile as expected. By comparing the
	// output string we make sure we catch changes to Git. If the test fails here,
	// this means that a new version of Git might have changed the logic around
	// locking.
	if !strings.Contains(string(b), "gc is already running on machine") {
		t.Fatal("git gc should have complained about an existing lockfile")
	}

	err = unlock()
	if err != nil {
		t.Fatal(err)
	}

	cmd = exec.Command("git", "gc")
	dir.Set(cmd)
	_, err = cmd.CombinedOutput()
	if err != nil {
		t.Fatal(err)
	}
}

func TestSGMaintenanceRespectsLock(t *testing.T) {
	logger, getLogs := logtest.Captured(t)

	dir := common.GitDir(t.TempDir())
	cmd := exec.Command("git", "--bare", "init")
	dir.Set(cmd)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}

	err, _ := lockRepoForGC(dir)
	if err != nil {
		t.Fatal(err)
	}

	err = sgMaintenance(logger, dir)
	if err != nil {
		t.Fatal(err)
	}

	cl := getLogs()
	if len(cl) == 0 {
		t.Fatal("expected at least 1 log message")
	}

	if !strings.Contains(cl[len(cl)-1].Message, "could not lock repository for sg maintenance") {
		t.Fatal("expected sg maintenance to complain about the lockfile")
	}
}

func TestSGMaintenanceRemovesLock(t *testing.T) {
	logger := logtest.Scoped(t)

	dir := common.GitDir(t.TempDir())
	cmd := exec.Command("git", "--bare", "init")
	dir.Set(cmd)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}

	err := sgMaintenance(logger, dir)
	if err != nil {
		t.Fatal(err)
	}

	_, err = os.Stat(dir.Path(gcLockFile))
	if !errors.Is(err, fs.ErrNotExist) {
		t.Fatal("sg maintenance should have removed the lockfile it created")
	}
}

func TestGetSetLastSizeCalculation(t *testing.T) {
	dir := common.GitDir(t.TempDir())
	cmd := exec.Command("git", "--bare", "init")
	dir.Set(cmd)
	if err := cmd.Run(); err != nil {
		t.Fatal(err)
	}
	at, err := getLastSizeCalculation(dir)
	require.NoError(t, err)
	// Never computed, should be zero.
	require.True(t, at.IsZero())
	now := time.Now().Truncate(time.Millisecond)
	// Setting the value should work.
	err = setLastSizeCalculation(dir, now)
	require.NoError(t, err)
	at, err = getLastSizeCalculation(dir)
	require.NoError(t, err)
	require.Equal(t, now, at)
	// Setting again should work.
	now = time.Now().Truncate(time.Millisecond)
	err = setLastSizeCalculation(dir, now)
	require.NoError(t, err)
	at, err = getLastSizeCalculation(dir)
	require.NoError(t, err)
	require.Equal(t, now, at)
}
