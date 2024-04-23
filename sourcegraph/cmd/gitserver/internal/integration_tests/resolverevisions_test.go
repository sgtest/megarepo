package inttests

import (
	"container/list"
	"context"
	"net/http"
	"net/http/httptest"
	"net/url"
	"path/filepath"
	"testing"

	"github.com/sourcegraph/sourcegraph/internal/vcs"
	"github.com/sourcegraph/sourcegraph/lib/errors"

	"github.com/sourcegraph/log/logtest"
	"github.com/stretchr/testify/require"
	"golang.org/x/time/rate"

	server "github.com/sourcegraph/sourcegraph/cmd/gitserver/internal"
	common "github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/common"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/git"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/git/gitcli"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/gitserverfs"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/perforce"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/vcssyncer"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database/dbmocks"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	proto "github.com/sourcegraph/sourcegraph/internal/gitserver/v1"
	internalgrpc "github.com/sourcegraph/sourcegraph/internal/grpc"
	"github.com/sourcegraph/sourcegraph/internal/grpc/defaults"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/ratelimit"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/wrexec"
)

func TestClient_ResolveRevision(t *testing.T) {
	root := t.TempDir()
	remote := createSimpleGitRepo(t, root)
	// These hashes should be stable since we set the timestamps
	// when creating the commits.
	hash1 := "b6602ca96bdc0ab647278577a3c6edcb8fe18fb0"
	hash2 := "c5151eceb40d5e625716589b745248e1a6c6228d"

	tests := []struct {
		input string
		want  api.CommitID
		err   error
	}{{
		input: "",
		want:  api.CommitID(hash2),
	}, {
		input: "HEAD",
		want:  api.CommitID(hash2),
	}, {
		input: "HEAD~1",
		want:  api.CommitID(hash1),
	}, {
		input: "test-ref",
		want:  api.CommitID(hash1),
	}, {
		input: "test-nested-ref",
		want:  api.CommitID(hash1),
	}, {
		input: "test-fake-ref",
		err:   &gitdomain.RevisionNotFoundError{Repo: api.RepoName(remote), Spec: "test-fake-ref^0"},
	}}

	logger := logtest.Scoped(t)
	db := newMockDB()
	ctx := context.Background()

	fs := gitserverfs.New(observation.TestContextTB(t), filepath.Join(root, "repos"))
	require.NoError(t, fs.Initialize())
	getRemoteURLFunc := func(_ context.Context, name api.RepoName) (string, error) { //nolint:unparam
		return remote, nil
	}

	s := server.NewServer(&server.ServerOpts{
		Logger: logger,
		FS:     fs,
		GetBackendFunc: func(dir common.GitDir, repoName api.RepoName) git.GitBackend {
			return gitcli.NewBackend(logtest.Scoped(t), wrexec.NewNoOpRecordingCommandFactory(), dir, repoName)
		},
		GetRemoteURLFunc: getRemoteURLFunc,
		GetVCSSyncer: func(ctx context.Context, name api.RepoName) (vcssyncer.VCSSyncer, error) {
			getRemoteURLSource := func(ctx context.Context, name api.RepoName) (vcssyncer.RemoteURLSource, error) {
				return vcssyncer.RemoteURLSourceFunc(func(ctx context.Context) (*vcs.URL, error) {
					raw, err := getRemoteURLFunc(ctx, name)
					if err != nil {
						return nil, errors.Wrapf(err, "failed to get remote URL for %s", name)
					}

					u, err := vcs.ParseURL(raw)
					if err != nil {
						return nil, errors.Wrapf(err, "failed to parse remote URL %q", raw)
					}
					return u, nil
				}), nil
			}
			return vcssyncer.NewGitRepoSyncer(logtest.Scoped(t), wrexec.NewNoOpRecordingCommandFactory(), getRemoteURLSource), nil
		},
		DB:                      db,
		Perforce:                perforce.NewService(ctx, observation.TestContextTB(t), logger, db, list.New()),
		RecordingCommandFactory: wrexec.NewNoOpRecordingCommandFactory(),
		Locker:                  server.NewRepositoryLocker(),
		RPSLimiter:              ratelimit.NewInstrumentedLimiter("GitserverTest", rate.NewLimiter(100, 10)),
	})

	grpcServer := defaults.NewServer(logtest.Scoped(t))
	proto.RegisterGitserverServiceServer(grpcServer, server.NewGRPCServer(s, &server.GRPCServerConfig{
		ExhaustiveRequestLoggingEnabled: true,
	}))

	handler := internalgrpc.MultiplexHandlers(grpcServer, http.NotFoundHandler())
	srv := httptest.NewServer(handler)

	defer srv.Close()

	u, _ := url.Parse(srv.URL)
	addrs := []string{u.Host}
	source := gitserver.NewTestClientSource(t, addrs)

	cli := gitserver.NewTestClient(t).WithClientSource(source)

	for _, test := range tests {
		t.Run("", func(t *testing.T) {
			_, _, err := s.FetchRepository(ctx, api.RepoName(remote))
			require.NoError(t, err)

			got, err := cli.ResolveRevision(ctx, api.RepoName(remote), test.input, gitserver.ResolveRevisionOptions{EnsureRevision: false})
			if test.err != nil {
				require.Equal(t, test.err, err)
				return
			}
			require.NoError(t, err)
			require.Equal(t, test.want, got)
		})
	}

}

func newMockDB() *dbmocks.MockDB {
	db := dbmocks.NewMockDB()
	db.GitserverReposFunc.SetDefaultReturn(dbmocks.NewMockGitserverRepoStore())
	db.FeatureFlagsFunc.SetDefaultReturn(dbmocks.NewMockFeatureFlagStore())

	r := dbmocks.NewMockRepoStore()
	r.GetByNameFunc.SetDefaultHook(func(ctx context.Context, repoName api.RepoName) (*types.Repo, error) {
		return &types.Repo{
			Name: repoName,
			ExternalRepo: api.ExternalRepoSpec{
				ServiceType: extsvc.TypeGitHub,
			},
		}, nil
	})
	db.ReposFunc.SetDefaultReturn(r)

	return db
}
