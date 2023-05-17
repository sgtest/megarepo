package workspace_test

import (
	"context"
	"io"
	"os"
	"path"
	"strings"
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/enterprise/cmd/executor/internal/worker/command"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/executor/internal/worker/workspace"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/executor/types"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func TestNewFirecrackerWorkspace(t *testing.T) {
	operations := command.NewOperations(&observation.TestContext)

	tests := []struct {
		name                   string
		job                    types.Job
		cloneOptions           workspace.CloneOptions
		mockFunc               func(logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string)
		assertMockFunc         func(t *testing.T, logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string)
		expectedWorkspaceFiles map[string]string
		expectedDockerScripts  map[string][]string
		expectedErr            error
	}{
		{
			name: "No repository configured",
			job: types.Job{
				ID:     42,
				Token:  "token",
				Commit: "commit",
			},
			mockFunc: func(logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				logger.LogEntryFunc.SetDefaultReturn(workspace.NewMockLogEntry())
				// losetup --find
				cmdRunner.CombinedOutputFunc.PushReturn([]byte(tempDir), nil)
				// mkfs.ext4
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// mount
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// losetup --detach
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
			},
			assertMockFunc: func(t *testing.T, logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				require.Len(t, filesStore.GetFunc.History(), 0)
				require.Len(t, cmd.RunFunc.History(), 0)
			},
		},
		{
			name: "Clone repository",
			job: types.Job{
				ID:             42,
				Token:          "token",
				Commit:         "commit",
				RepositoryName: "my-repo",
			},
			mockFunc: func(logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				logger.LogEntryFunc.SetDefaultReturn(workspace.NewMockLogEntry())
				// losetup --find
				cmdRunner.CombinedOutputFunc.PushReturn([]byte(tempDir), nil)
				// mkfs.ext4
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// mount
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// losetup --detach
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				cmd.RunFunc.SetDefaultReturn(nil)
			},
			assertMockFunc: func(t *testing.T, logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				require.Len(t, filesStore.GetFunc.History(), 0)
				require.Len(t, cmd.RunFunc.History(), 6)
				// Init
				assert.Equal(t, "setup.git.init", cmd.RunFunc.History()[0].Arg2.Key)
				assert.Equal(t, expectedGitEnv, cmd.RunFunc.History()[0].Arg2.Env)
				assert.Equal(t, []string{
					"git",
					"-C",
					tempDir,
					"init",
				}, cmd.RunFunc.History()[0].Arg2.Command)
				assert.Equal(t, operations.SetupGitInit, cmd.RunFunc.History()[0].Arg2.Operation)
				// Add remote
				assert.Equal(t, "setup.git.add-remote", cmd.RunFunc.History()[1].Arg2.Key)
				assert.Equal(t, expectedGitEnv, cmd.RunFunc.History()[1].Arg2.Env)
				// The origin has the proxy address. The port changes. So we need custom assertions
				assert.Equal(t, "git", cmd.RunFunc.History()[1].Arg2.Command[0])
				assert.Equal(t, "-C", cmd.RunFunc.History()[1].Arg2.Command[1])
				assert.Equal(t, tempDir, cmd.RunFunc.History()[1].Arg2.Command[2])
				assert.Equal(t, "remote", cmd.RunFunc.History()[1].Arg2.Command[3])
				assert.Equal(t, "add", cmd.RunFunc.History()[1].Arg2.Command[4])
				assert.Equal(t, "origin", cmd.RunFunc.History()[1].Arg2.Command[5])
				assert.Regexp(t, "^http://127.0.0.1:[0-9]+/my-repo$", cmd.RunFunc.History()[1].Arg2.Command[6])
				assert.Equal(t, operations.SetupAddRemote, cmd.RunFunc.History()[1].Arg2.Operation)
				// Disable GC
				assert.Equal(t, "setup.git.disable-gc", cmd.RunFunc.History()[2].Arg2.Key)
				assert.Equal(t, expectedGitEnv, cmd.RunFunc.History()[2].Arg2.Env)
				assert.Equal(t, []string{
					"git",
					"-C",
					tempDir,
					"config",
					"--local",
					"gc.auto",
					"0",
				}, cmd.RunFunc.History()[2].Arg2.Command)
				assert.Equal(t, operations.SetupGitDisableGC, cmd.RunFunc.History()[2].Arg2.Operation)
				// Fetch
				assert.Equal(t, "setup.git.fetch", cmd.RunFunc.History()[3].Arg2.Key)
				assert.Equal(t, expectedGitEnv, cmd.RunFunc.History()[3].Arg2.Env)
				assert.Equal(t, []string{
					"git",
					"-C",
					tempDir,
					"-c",
					"protocol.version=2",
					"fetch",
					"--progress",
					"--no-recurse-submodules",
					"origin",
					"commit",
				}, cmd.RunFunc.History()[3].Arg2.Command)
				assert.Equal(t, operations.SetupGitFetch, cmd.RunFunc.History()[3].Arg2.Operation)
				// Checkout
				assert.Equal(t, "setup.git.checkout", cmd.RunFunc.History()[4].Arg2.Key)
				assert.Equal(t, expectedGitEnv, cmd.RunFunc.History()[4].Arg2.Env)
				assert.Equal(t, []string{
					"git",
					"-C",
					tempDir,
					"checkout",
					"--progress",
					"--force",
					"commit",
				}, cmd.RunFunc.History()[4].Arg2.Command)
				assert.Equal(t, operations.SetupGitCheckout, cmd.RunFunc.History()[4].Arg2.Operation)
				// Set Remote
				assert.Equal(t, "setup.git.set-remote", cmd.RunFunc.History()[5].Arg2.Key)
				assert.Equal(t, expectedGitEnv, cmd.RunFunc.History()[5].Arg2.Env)
				assert.Equal(t, []string{
					"git",
					"-C",
					tempDir,
					"remote",
					"set-url",
					"origin",
					"my-repo",
				}, cmd.RunFunc.History()[5].Arg2.Command)
				assert.Equal(t, operations.SetupGitSetRemoteUrl, cmd.RunFunc.History()[5].Arg2.Operation)
			},
		},
		{
			name: "Failed to clone repository",
			job: types.Job{
				ID:             42,
				Token:          "token",
				Commit:         "commit",
				RepositoryName: "my-repo",
			},
			mockFunc: func(logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				logger.LogEntryFunc.SetDefaultReturn(workspace.NewMockLogEntry())
				// losetup --find
				cmdRunner.CombinedOutputFunc.PushReturn([]byte(tempDir), nil)
				// mkfs.ext4
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// mount
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// losetup --detach
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				cmd.RunFunc.SetDefaultReturn(errors.New("failed"))
			},
			assertMockFunc: func(t *testing.T, logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				require.Len(t, filesStore.GetFunc.History(), 0)
				require.Len(t, cmd.RunFunc.History(), 1)
			},
			expectedErr: errors.New("failed setup.git.init: failed"),
		},
		{
			name: "Clone repository with directory",
			job: types.Job{
				ID:                  42,
				Token:               "token",
				Commit:              "commit",
				RepositoryName:      "my-repo",
				RepositoryDirectory: "/my/dir",
			},
			mockFunc: func(logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				logger.LogEntryFunc.SetDefaultReturn(workspace.NewMockLogEntry())
				// losetup --find
				cmdRunner.CombinedOutputFunc.PushReturn([]byte(tempDir), nil)
				// mkfs.ext4
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// mount
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// losetup --detach
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				cmd.RunFunc.SetDefaultReturn(nil)
			},
			assertMockFunc: func(t *testing.T, logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				require.Len(t, filesStore.GetFunc.History(), 0)
				require.Len(t, cmd.RunFunc.History(), 6)
				repoDir := path.Join(tempDir, "/my/dir")
				// Init
				assert.Equal(t, []string{"git", "-C", repoDir, "init"}, cmd.RunFunc.History()[0].Arg2.Command)
				// Add remote
				// The origin has the proxy address. The port changes. So we need custom assertions
				assert.Equal(t, "git", cmd.RunFunc.History()[1].Arg2.Command[0])
				assert.Equal(t, "-C", cmd.RunFunc.History()[1].Arg2.Command[1])
				assert.Equal(t, repoDir, cmd.RunFunc.History()[1].Arg2.Command[2])
				assert.Equal(t, "remote", cmd.RunFunc.History()[1].Arg2.Command[3])
				assert.Equal(t, "add", cmd.RunFunc.History()[1].Arg2.Command[4])
				assert.Equal(t, "origin", cmd.RunFunc.History()[1].Arg2.Command[5])
				assert.Regexp(t, "^http://127.0.0.1:[0-9]+/my-repo$", cmd.RunFunc.History()[1].Arg2.Command[6])
				// Disable GC
				assert.Equal(t, []string{
					"git",
					"-C",
					repoDir,
					"config",
					"--local",
					"gc.auto",
					"0",
				}, cmd.RunFunc.History()[2].Arg2.Command)
				// Fetch
				assert.Equal(t, []string{
					"git",
					"-C",
					repoDir,
					"-c",
					"protocol.version=2",
					"fetch",
					"--progress",
					"--no-recurse-submodules",
					"origin",
					"commit",
				}, cmd.RunFunc.History()[3].Arg2.Command)
				// Checkout
				assert.Equal(t, []string{
					"git",
					"-C",
					repoDir,
					"checkout",
					"--progress",
					"--force",
					"commit",
				}, cmd.RunFunc.History()[4].Arg2.Command)
				// Set Remote
				assert.Equal(t, []string{
					"git",
					"-C",
					repoDir,
					"remote",
					"set-url",
					"origin",
					"my-repo",
				}, cmd.RunFunc.History()[5].Arg2.Command)
			},
		},
		{
			name: "Fetch tags",
			job: types.Job{
				ID:             42,
				Token:          "token",
				Commit:         "commit",
				RepositoryName: "my-repo",
				FetchTags:      true,
			},
			mockFunc: func(logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				logger.LogEntryFunc.SetDefaultReturn(workspace.NewMockLogEntry())
				// losetup --find
				cmdRunner.CombinedOutputFunc.PushReturn([]byte(tempDir), nil)
				// mkfs.ext4
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// mount
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// losetup --detach
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				cmd.RunFunc.SetDefaultReturn(nil)
			},
			assertMockFunc: func(t *testing.T, logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				require.Len(t, filesStore.GetFunc.History(), 0)
				require.Len(t, cmd.RunFunc.History(), 6)
				assert.Equal(t, []string{
					"git",
					"-C",
					tempDir,
					"-c",
					"protocol.version=2",
					"fetch",
					"--progress",
					"--no-recurse-submodules",
					"--tags",
					"origin",
					"commit",
				}, cmd.RunFunc.History()[3].Arg2.Command)
			},
		},
		{
			name: "Shallow clone",
			job: types.Job{
				ID:             42,
				Token:          "token",
				Commit:         "commit",
				RepositoryName: "my-repo",
				ShallowClone:   true,
			},
			mockFunc: func(logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				logger.LogEntryFunc.SetDefaultReturn(workspace.NewMockLogEntry())
				// losetup --find
				cmdRunner.CombinedOutputFunc.PushReturn([]byte(tempDir), nil)
				// mkfs.ext4
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// mount
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// losetup --detach
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				cmd.RunFunc.SetDefaultReturn(nil)
			},
			assertMockFunc: func(t *testing.T, logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				require.Len(t, filesStore.GetFunc.History(), 0)
				require.Len(t, cmd.RunFunc.History(), 6)
				assert.Equal(t, []string{
					"git",
					"-C",
					tempDir,
					"-c",
					"protocol.version=2",
					"fetch",
					"--progress",
					"--no-recurse-submodules",
					"--no-tags",
					"--depth=1",
					"origin",
					"commit",
				}, cmd.RunFunc.History()[3].Arg2.Command)
			},
		},
		{
			name: "Sparse checkout",
			job: types.Job{
				ID:             42,
				Token:          "token",
				Commit:         "commit",
				RepositoryName: "my-repo",
				SparseCheckout: []string{"foo/bar/**"},
			},
			mockFunc: func(logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				logger.LogEntryFunc.SetDefaultReturn(workspace.NewMockLogEntry())
				// losetup --find
				cmdRunner.CombinedOutputFunc.PushReturn([]byte(tempDir), nil)
				// mkfs.ext4
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// mount
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// losetup --detach
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				cmd.RunFunc.SetDefaultReturn(nil)
			},
			assertMockFunc: func(t *testing.T, logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				require.Len(t, filesStore.GetFunc.History(), 0)
				require.Len(t, cmd.RunFunc.History(), 8)
				// Fetch
				assert.Equal(t, []string{
					"git",
					"-C",
					tempDir,
					"-c",
					"protocol.version=2",
					"fetch",
					"--progress",
					"--no-recurse-submodules",
					"--filter=blob:none",
					"origin",
					"commit",
				}, cmd.RunFunc.History()[3].Arg2.Command)
				// Sparse checkout config
				assert.Equal(t, "setup.git.sparse-checkout-config", cmd.RunFunc.History()[4].Arg2.Key)
				assert.Equal(t, expectedGitEnv, cmd.RunFunc.History()[4].Arg2.Env)
				assert.Equal(t, []string{
					"git",
					"-C",
					tempDir,
					"config",
					"--local",
					"core.sparseCheckout",
					"1",
				}, cmd.RunFunc.History()[4].Arg2.Command)
				assert.Equal(t, operations.SetupGitSparseCheckoutConfig, cmd.RunFunc.History()[4].Arg2.Operation)
				// Sparse Checkout Set
				assert.Equal(t, "setup.git.sparse-checkout-set", cmd.RunFunc.History()[5].Arg2.Key)
				assert.Equal(t, expectedGitEnv, cmd.RunFunc.History()[5].Arg2.Env)
				assert.Equal(t, []string{
					"git",
					"-C",
					tempDir,
					"sparse-checkout",
					"set",
					"--no-cone",
					"--",
					"foo/bar/**",
				}, cmd.RunFunc.History()[5].Arg2.Command)
				assert.Equal(t, operations.SetupGitSparseCheckoutSet, cmd.RunFunc.History()[5].Arg2.Operation)
				// Checkout
				assert.Equal(t, []string{
					"git",
					"-C",
					tempDir,
					"-c",
					"protocol.version=2",
					"checkout",
					"--progress",
					"--force",
					"commit",
				}, cmd.RunFunc.History()[6].Arg2.Command)
			},
		},
		{
			name: "Virtual machine files",
			job: types.Job{
				ID:     42,
				Token:  "token",
				Commit: "commit",
				VirtualMachineFiles: map[string]types.VirtualMachineFile{
					"file1.txt": {
						Content:    []byte("content1"),
						ModifiedAt: time.Now(),
					},
					"file2.txt": {
						Bucket:     "foo",
						Key:        "bar",
						ModifiedAt: time.Now(),
					},
				},
			},
			mockFunc: func(logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				logger.LogEntryFunc.SetDefaultReturn(workspace.NewMockLogEntry())
				// losetup --find
				cmdRunner.CombinedOutputFunc.PushReturn([]byte(tempDir), nil)
				// mkfs.ext4
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// mount
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// losetup --detach
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				filesStore.GetFunc.SetDefaultReturn(io.NopCloser(strings.NewReader("content2")), nil)
			},
			assertMockFunc: func(t *testing.T, logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				require.Len(t, logger.LogEntryFunc.History(), 2)
				require.Len(t, cmd.RunFunc.History(), 0)
				require.Len(t, filesStore.GetFunc.History(), 1)
				assert.NotZero(t, filesStore.GetFunc.History()[0].Arg1)
				assert.Equal(t, "foo", filesStore.GetFunc.History()[0].Arg2)
				assert.Equal(t, "bar", filesStore.GetFunc.History()[0].Arg3)
			},
			expectedWorkspaceFiles: map[string]string{
				"file1.txt": "content1",
				"file2.txt": "content2",
			},
		},
		{
			name: "Docker steps",
			job: types.Job{
				ID:     42,
				Token:  "token",
				Commit: "commit",
				DockerSteps: []types.DockerStep{
					{
						Key:      "step1",
						Image:    "my-image-1",
						Commands: []string{"command1", "arg"},
						Dir:      "/my/dir1",
						Env:      []string{"FOO=bar"},
					},
					{
						Key:      "step2",
						Image:    "my-image-2",
						Commands: []string{"command2", "arg"},
						Dir:      "/my/dir2",
						Env:      []string{"FAZ=baz"},
					},
				},
			},
			mockFunc: func(logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				logger.LogEntryFunc.SetDefaultReturn(workspace.NewMockLogEntry())
				// losetup --find
				cmdRunner.CombinedOutputFunc.PushReturn([]byte(tempDir), nil)
				// mkfs.ext4
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// mount
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				// losetup --detach
				cmdRunner.CombinedOutputFunc.PushReturn([]byte{}, nil)
				filesStore.GetFunc.SetDefaultReturn(io.NopCloser(strings.NewReader("content2")), nil)
			},
			assertMockFunc: func(t *testing.T, logger *workspace.MockLogger, filesStore *workspace.MockFilesStore, cmdRunner *workspace.MockCmdRunner, cmd *workspace.MockCommand, tempDir string) {
				require.Len(t, logger.LogEntryFunc.History(), 2)
				require.Len(t, filesStore.GetFunc.History(), 0)
				require.Len(t, cmd.RunFunc.History(), 0)
			},
			expectedDockerScripts: map[string][]string{
				"42.0_@commit.sh": {"command1", "arg"},
				"42.1_@commit.sh": {"command2", "arg"},
			},
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			cmdRunner := workspace.NewMockCmdRunner()
			filesStore := workspace.NewMockFilesStore()
			cmd := workspace.NewMockCommand()
			logger := workspace.NewMockLogger()

			// By default, firecracker will try to write to a directory that it probably should not when testing.
			// Override the default behavior to write to a temporary directory instead.
			firecrackerDir := t.TempDir()
			workspace.MakeLoopFile = func(prefix string) (*os.File, error) {
				return os.CreateTemp(firecrackerDir, prefix)
			}
			workspace.MakeMountDirectory = func(prefix string) (string, error) {
				return os.MkdirTemp(firecrackerDir, prefix)
			}

			if test.mockFunc != nil {
				test.mockFunc(logger, filesStore, cmdRunner, cmd, firecrackerDir)
			}

			ws, err := workspace.NewFirecrackerWorkspace(
				context.Background(),
				filesStore,
				test.job,
				"10G",
				false,
				cmdRunner,
				cmd,
				logger,
				test.cloneOptions,
				operations,
			)
			t.Cleanup(func() {
				if ws != nil {
					ws.Remove(context.Background(), false)
				}
			})

			tempDir := ""
			if ws != nil {
				tempDir = ws.Path()
			}

			var mountpointDir string
			if test.expectedErr != nil {
				require.Error(t, err)
				assert.EqualError(t, err, test.expectedErr.Error())
			} else {
				require.NoError(t, err)
				// Workspace files
				tempEntries, err := os.ReadDir(tempDir)
				require.NoError(t, err)
				// includes workspace-loop and workspace-mountpoints (dir)
				assert.Len(t, tempEntries, 2)
				// ensure that workspace-loop exists
				// We use temp dirs, for all this, so the directory name has a random set of numbers as the suffix.
				for _, entry := range tempEntries {
					if strings.HasPrefix(entry.Name(), "workspace-loop") {
						// ensure this is a file
						assert.False(t, entry.IsDir())
					} else if strings.HasPrefix(entry.Name(), "workspace-mountpoints") {
						mountpointDir = entry.Name()
					} else {
						t.Fatalf("unexpected file in workspace: %s", entry.Name())
					}
				}
				mountEntries, err := os.ReadDir(path.Join(tempDir, mountpointDir))
				require.NoError(t, err)
				// .sourcegraph-executor dir lives in the mountpoint dir
				additionalEntries := 0
				if len(test.job.RepositoryDirectory) > 0 {
					additionalEntries++
				}
				require.Len(t, mountEntries, 1+additionalEntries+len(test.expectedWorkspaceFiles))
				// workspace files
				for f, content := range test.expectedWorkspaceFiles {
					b, err := os.ReadFile(path.Join(tempDir, mountpointDir, f))
					require.NoError(t, err)
					assert.Equal(t, content, string(b))
				}
				// Docker scripts
				scriptEntries, err := os.ReadDir(path.Join(tempDir, mountpointDir, ".sourcegraph-executor"))
				require.NoError(t, err)
				assert.Len(t, scriptEntries, len(test.expectedDockerScripts))
				for f, commands := range test.expectedDockerScripts {
					require.Contains(t, ws.ScriptFilenames(), f)
					b, err := os.ReadFile(path.Join(tempDir, mountpointDir, ".sourcegraph-executor", f))
					require.NoError(t, err)
					assert.Equal(t, toDockerStepScript(commands...), string(b))
				}
			}

			test.assertMockFunc(t, logger, filesStore, cmdRunner, cmd, path.Join(tempDir, mountpointDir))
		})
	}
}
