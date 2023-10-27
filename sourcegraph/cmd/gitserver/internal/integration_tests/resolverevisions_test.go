package inttests

import (
	"container/list"
	"context"
	"net/http/httptest"
	"net/url"
	"path/filepath"
	"testing"

	"github.com/sourcegraph/log/logtest"
	"github.com/stretchr/testify/require"
	"golang.org/x/time/rate"

	server "github.com/sourcegraph/sourcegraph/cmd/gitserver/internal"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/perforce"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/vcssyncer"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database/dbmocks"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/protocol"
	proto "github.com/sourcegraph/sourcegraph/internal/gitserver/v1"
	internalgrpc "github.com/sourcegraph/sourcegraph/internal/grpc"
	"github.com/sourcegraph/sourcegraph/internal/grpc/defaults"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/ratelimit"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/wrexec"
)

func TestClient_ResolveRevisions(t *testing.T) {
	root := t.TempDir()
	remote := createSimpleGitRepo(t, root)
	// These hashes should be stable since we set the timestamps
	// when creating the commits.
	hash1 := "b6602ca96bdc0ab647278577a3c6edcb8fe18fb0"
	hash2 := "c5151eceb40d5e625716589b745248e1a6c6228d"

	tests := []struct {
		input []protocol.RevisionSpecifier
		want  []string
		err   error
	}{{
		input: []protocol.RevisionSpecifier{{}},
		want:  []string{hash2},
	}, {
		input: []protocol.RevisionSpecifier{{RevSpec: "HEAD"}},
		want:  []string{hash2},
	}, {
		input: []protocol.RevisionSpecifier{{RevSpec: "HEAD~1"}},
		want:  []string{hash1},
	}, {
		input: []protocol.RevisionSpecifier{{RevSpec: "test-ref"}},
		want:  []string{hash1},
	}, {
		input: []protocol.RevisionSpecifier{{RevSpec: "test-nested-ref"}},
		want:  []string{hash1},
	}, {
		input: []protocol.RevisionSpecifier{{RefGlob: "refs/heads/test-*"}},
		want:  []string{hash1, hash1}, // two hashes because to refs point to that hash
	}, {
		input: []protocol.RevisionSpecifier{{RevSpec: "test-fake-ref"}},
		err:   &gitdomain.RevisionNotFoundError{Repo: api.RepoName(remote), Spec: "test-fake-ref"},
	}}

	logger := logtest.Scoped(t)
	db := newMockDB()
	ctx := context.Background()

	s := server.Server{
		Logger:   logger,
		ReposDir: filepath.Join(root, "repos"),
		GetRemoteURLFunc: func(_ context.Context, name api.RepoName) (string, error) {
			return remote, nil
		},
		GetVCSSyncer: func(ctx context.Context, name api.RepoName) (vcssyncer.VCSSyncer, error) {
			return vcssyncer.NewGitRepoSyncer(logtest.Scoped(t), wrexec.NewNoOpRecordingCommandFactory()), nil
		},
		DB:                      db,
		Perforce:                perforce.NewService(ctx, observation.TestContextTB(t), logger, db, list.New()),
		RecordingCommandFactory: wrexec.NewNoOpRecordingCommandFactory(),
		Locker:                  server.NewRepositoryLocker(),
		RPSLimiter:              ratelimit.NewInstrumentedLimiter("GitserverTest", rate.NewLimiter(100, 10)),
	}

	grpcServer := defaults.NewServer(logtest.Scoped(t))
	proto.RegisterGitserverServiceServer(grpcServer, &server.GRPCServer{Server: &s})

	handler := internalgrpc.MultiplexHandlers(grpcServer, s.Handler())
	srv := httptest.NewServer(handler)

	defer srv.Close()

	u, _ := url.Parse(srv.URL)
	addrs := []string{u.Host}
	source := gitserver.NewTestClientSource(t, addrs)

	cli := gitserver.NewTestClient(t).WithClientSource(source)

	for _, test := range tests {
		t.Run("", func(t *testing.T) {
			_, err := cli.RequestRepoUpdate(ctx, api.RepoName(remote), 0)
			require.NoError(t, err)

			got, err := cli.ResolveRevisions(ctx, api.RepoName(remote), test.input)
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
