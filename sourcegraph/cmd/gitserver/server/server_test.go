package server

import (
	"bytes"
	"container/list"
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"math/rand"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/google/go-cmp/cmp/cmpopts"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"golang.org/x/sync/semaphore"
	"golang.org/x/time/rate"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	"github.com/sourcegraph/log"
	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/cmd/gitserver/server/common"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/server/perforce"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbmocks"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/protocol"
	proto "github.com/sourcegraph/sourcegraph/internal/gitserver/v1"
	"github.com/sourcegraph/sourcegraph/internal/grpc"
	"github.com/sourcegraph/sourcegraph/internal/grpc/defaults"
	"github.com/sourcegraph/sourcegraph/internal/limiter"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/ratelimit"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/vcs"
	"github.com/sourcegraph/sourcegraph/internal/wrexec"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type Test struct {
	Name             string
	Request          *http.Request
	ExpectedCode     int
	ExpectedBody     string
	ExpectedTrailers http.Header
}

func newRequest(method, path string, body io.Reader) *http.Request {
	r := httptest.NewRequest(method, path, body)
	r.Header.Add("X-Requested-With", "Sourcegraph")
	return r
}

func TestExecRequest(t *testing.T) {
	tests := []Test{
		{
			Name:         "HTTP GET",
			Request:      newRequest("GET", "/exec", strings.NewReader("{}")),
			ExpectedCode: http.StatusMethodNotAllowed,
			ExpectedBody: "",
		},
		{
			Name:         "Command",
			Request:      newRequest("POST", "/exec", strings.NewReader(`{"repo": "github.com/gorilla/mux", "args": ["testcommand"]}`)),
			ExpectedCode: http.StatusOK,
			ExpectedBody: "teststdout",
			ExpectedTrailers: http.Header{
				"X-Exec-Error":       {""},
				"X-Exec-Exit-Status": {"42"},
				"X-Exec-Stderr":      {"teststderr"},
			},
		},
		{
			Name:         "CommandWithURL",
			Request:      newRequest("POST", "/exec", strings.NewReader(`{"repo": "my-mux", "url": "https://github.com/gorilla/mux.git", "args": ["testcommand"]}`)),
			ExpectedCode: http.StatusOK,
			ExpectedBody: "teststdout",
			ExpectedTrailers: http.Header{
				"X-Exec-Error":       {""},
				"X-Exec-Exit-Status": {"42"},
				"X-Exec-Stderr":      {"teststderr"},
			},
		},
		{
			Name: "echo",
			Request: newRequest(
				"POST", "/exec", strings.NewReader(
					`{"repo": "github.com/gorilla/mux", "args": ["testecho", "hi"]}`,
				),
			),
			ExpectedCode: http.StatusOK,
			ExpectedBody: "hi",
			ExpectedTrailers: http.Header{
				"X-Exec-Error":       {""},
				"X-Exec-Exit-Status": {"0"},
				"X-Exec-Stderr":      {""},
			},
		},
		{
			Name: "stdin",
			Request: newRequest(
				"POST", "/exec", strings.NewReader(
					`{"repo": "github.com/gorilla/mux", "args": ["testcat"], "stdin": "aGk="}`,
				),
			),
			ExpectedCode: http.StatusOK,
			ExpectedBody: "hi",
			ExpectedTrailers: http.Header{
				"X-Exec-Error":       {""},
				"X-Exec-Exit-Status": {"0"},
				"X-Exec-Stderr":      {""},
			},
		},
		{
			Name:         "NonexistingRepo",
			Request:      newRequest("POST", "/exec", strings.NewReader(`{"repo": "github.com/gorilla/doesnotexist", "args": ["testcommand"]}`)),
			ExpectedCode: http.StatusNotFound,
			ExpectedBody: `{"cloneInProgress":false}`,
		},
		{
			Name: "NonexistingRepoWithURL",
			Request: newRequest(
				"POST", "/exec", strings.NewReader(`{"repo": "my-doesnotexist", "url": "https://github.com/gorilla/doesntexist.git", "args": ["testcommand"]}`)),
			ExpectedCode: http.StatusNotFound,
			ExpectedBody: `{"cloneInProgress":false}`,
		},
		{
			Name:         "UnclonedRepoWithoutURL",
			Request:      newRequest("POST", "/exec", strings.NewReader(`{"repo": "github.com/nicksnyder/go-i18n", "args": ["testcommand"]}`)),
			ExpectedCode: http.StatusNotFound,
			ExpectedBody: `{"cloneInProgress":true}`, // we now fetch the URL from GetRemoteURL so it works.
		},
		{
			Name:         "UnclonedRepoWithURL",
			Request:      newRequest("POST", "/exec", strings.NewReader(`{"repo": "github.com/nicksnyder/go-i18n", "url": "https://github.com/nicksnyder/go-i18n.git", "args": ["testcommand"]}`)),
			ExpectedCode: http.StatusNotFound,
			ExpectedBody: `{"cloneInProgress":true}`,
		},
		{
			Name:         "Error",
			Request:      newRequest("POST", "/exec", strings.NewReader(`{"repo": "github.com/gorilla/mux", "args": ["testerror"]}`)),
			ExpectedCode: http.StatusOK,
			ExpectedTrailers: http.Header{
				"X-Exec-Error":       {"testerror"},
				"X-Exec-Exit-Status": {"0"},
				"X-Exec-Stderr":      {""},
			},
		},
		{
			Name:         "EmptyInput",
			Request:      newRequest("POST", "/exec", strings.NewReader("{}")),
			ExpectedCode: http.StatusBadRequest,
			ExpectedBody: "invalid command",
		},
		{
			Name:         "BadCommand",
			Request:      newRequest("POST", "/exec", strings.NewReader(`{"repo":"github.com/sourcegraph/sourcegraph", "args": ["invalid-command"]}`)),
			ExpectedCode: http.StatusBadRequest,
			ExpectedBody: "invalid command",
		},
	}

	db := dbmocks.NewMockDB()
	gr := dbmocks.NewMockGitserverRepoStore()
	db.GitserverReposFunc.SetDefaultReturn(gr)
	reposDir := "/testroot"
	s := &Server{
		Logger:            logtest.Scoped(t),
		ObservationCtx:    observation.TestContextTB(t),
		ReposDir:          reposDir,
		skipCloneForTests: true,
		GetRemoteURLFunc: func(ctx context.Context, name api.RepoName) (string, error) {
			return "https://" + string(name) + ".git", nil
		},
		GetVCSSyncer: func(ctx context.Context, name api.RepoName) (VCSSyncer, error) {
			return NewGitRepoSyncer(wrexec.NewNoOpRecordingCommandFactory()), nil
		},
		DB:                      db,
		RecordingCommandFactory: wrexec.NewNoOpRecordingCommandFactory(),
		Locker:                  NewRepositoryLocker(),
		RPSLimiter:              ratelimit.NewInstrumentedLimiter("GitserverTest", rate.NewLimiter(rate.Inf, 10)),
	}
	h := s.Handler()

	origRepoCloned := repoCloned
	repoCloned = func(dir common.GitDir) bool {
		return dir == repoDirFromName(reposDir, "github.com/gorilla/mux") || dir == repoDirFromName(reposDir, "my-mux")
	}
	t.Cleanup(func() { repoCloned = origRepoCloned })

	testGitRepoExists = func(ctx context.Context, remoteURL *vcs.URL) error {
		if remoteURL.String() == "https://github.com/nicksnyder/go-i18n.git" {
			return nil
		}
		return errors.New("not cloneable")
	}
	t.Cleanup(func() { testGitRepoExists = nil })

	runCommandMock = func(ctx context.Context, cmd *exec.Cmd) (int, error) {
		switch cmd.Args[1] {
		case "testcommand":
			_, _ = cmd.Stdout.Write([]byte("teststdout"))
			_, _ = cmd.Stderr.Write([]byte("teststderr"))
			return 42, nil
		case "testerror":
			return 0, errors.New("testerror")
		case "testecho", "testcat":
			// We do an actual exec in this case to test that code path.
			exe := strings.TrimPrefix(cmd.Args[1], "test")
			lp, err := exec.LookPath(exe)
			if err != nil {
				return -1, err
			}
			cmd.Path = lp
			cmd.Args = cmd.Args[1:]
			cmd.Args[0] = exe
			cmd.Dir = "" // the test doesn't setup the dir

			// We run the real codepath cause we can in this case.
			m := runCommandMock
			runCommandMock = nil
			defer func() { runCommandMock = m }()
			return runCommand(ctx, wrexec.Wrap(ctx, logtest.Scoped(t), cmd))
		}
		return 0, nil
	}
	t.Cleanup(func() { runCommandMock = nil })

	for _, test := range tests {
		t.Run(test.Name, func(t *testing.T) {
			w := httptest.ResponseRecorder{Body: new(bytes.Buffer)}
			h.ServeHTTP(&w, test.Request)

			res := w.Result()
			if res.StatusCode != test.ExpectedCode {
				t.Errorf("wrong status: expected %d, got %d", test.ExpectedCode, w.Code)
			}

			body, err := io.ReadAll(res.Body)
			if err != nil {
				t.Fatal(err)
			}
			if strings.TrimSpace(string(body)) != test.ExpectedBody {
				t.Errorf("wrong body: expected %q, got %q", test.ExpectedBody, string(body))
			}

			for k, v := range test.ExpectedTrailers {
				if got := res.Trailer.Get(k); got != v[0] {
					t.Errorf("wrong trailer %q: expected %q, got %q", k, v[0], got)
				}
			}
		})
	}
}

func TestServer_handleP4Exec(t *testing.T) {
	defaultMockRunCommand := func(ctx context.Context, cmd *exec.Cmd) (int, error) {
		switch cmd.Args[1] {
		case "users":
			_, _ = cmd.Stdout.Write([]byte("admin <admin@joe-perforce-server> (admin) accessed 2021/01/31"))
			_, _ = cmd.Stderr.Write([]byte("teststderr"))
			return 42, errors.New("the answer to life the universe and everything")
		}
		return 0, nil
	}

	t.Cleanup(func() {
		updateRunCommandMock(nil)
	})

	startServer := func(t *testing.T) (handler http.Handler, client proto.GitserverServiceClient, cleanup func()) {
		t.Helper()

		logger := logtest.Scoped(t)

		s := &Server{
			Logger:                  logger,
			ReposDir:                "/testroot",
			ObservationCtx:          observation.TestContextTB(t),
			skipCloneForTests:       true,
			DB:                      dbmocks.NewMockDB(),
			RecordingCommandFactory: wrexec.NewNoOpRecordingCommandFactory(),
			Locker:                  NewRepositoryLocker(),
		}

		server := defaults.NewServer(logger)
		proto.RegisterGitserverServiceServer(server, &GRPCServer{Server: s})
		handler = grpc.MultiplexHandlers(server, s.Handler())

		srv := httptest.NewServer(handler)

		u, _ := url.Parse(srv.URL)
		conn, err := defaults.Dial(u.Host, logger.Scoped("gRPC client", ""))
		if err != nil {
			t.Fatalf("failed to dial: %v", err)
		}

		client = proto.NewGitserverServiceClient(conn)

		return handler, client, func() {
			srv.Close()
			conn.Close()
			server.Stop()
		}
	}

	t.Run("gRPC", func(t *testing.T) {
		readAll := func(execClient proto.GitserverService_P4ExecClient) ([]byte, error) {
			var buf bytes.Buffer
			for {
				resp, err := execClient.Recv()
				if errors.Is(err, io.EOF) {
					return buf.Bytes(), nil
				}

				if err != nil {
					return buf.Bytes(), err
				}

				_, err = buf.Write(resp.GetData())
				if err != nil {
					t.Fatalf("failed to write data: %v", err)
				}
			}
		}

		t.Run("users", func(t *testing.T) {
			updateRunCommandMock(defaultMockRunCommand)

			_, client, closeFunc := startServer(t)
			t.Cleanup(closeFunc)

			stream, err := client.P4Exec(context.Background(), &proto.P4ExecRequest{
				Args: [][]byte{[]byte("users")},
			})
			if err != nil {
				t.Fatalf("failed to call P4Exec: %v", err)
			}

			data, err := readAll(stream)
			s, ok := status.FromError(err)
			if !ok {
				t.Fatal("received non-status error from p4exec call")
			}

			if diff := cmp.Diff("the answer to life the universe and everything", s.Message()); diff != "" {
				t.Fatalf("unexpected error in stream (-want +got):\n%s", diff)
			}

			expectedData := []byte("admin <admin@joe-perforce-server> (admin) accessed 2021/01/31")

			if diff := cmp.Diff(expectedData, data); diff != "" {
				t.Fatalf("unexpected data (-want +got):\n%s", diff)
			}
		})

		t.Run("empty request", func(t *testing.T) {
			updateRunCommandMock(defaultMockRunCommand)

			_, client, closeFunc := startServer(t)
			t.Cleanup(closeFunc)

			stream, err := client.P4Exec(context.Background(), &proto.P4ExecRequest{})
			if err != nil {
				t.Fatalf("failed to call P4Exec: %v", err)
			}

			_, err = readAll(stream)
			if status.Code(err) != codes.InvalidArgument {
				t.Fatalf("expected InvalidArgument error, got %v", err)
			}
		})

		t.Run("disallowed command", func(t *testing.T) {

			updateRunCommandMock(defaultMockRunCommand)

			_, client, closeFunc := startServer(t)
			t.Cleanup(closeFunc)

			stream, err := client.P4Exec(context.Background(), &proto.P4ExecRequest{
				Args: [][]byte{[]byte("bad_command")},
			})
			if err != nil {
				t.Fatalf("failed to call P4Exec: %v", err)
			}

			_, err = readAll(stream)
			if status.Code(err) != codes.InvalidArgument {
				t.Fatalf("expected InvalidArgument error, got %v", err)
			}
		})

		t.Run("context cancelled", func(t *testing.T) {
			ctx, cancel := context.WithCancel(context.Background())

			updateRunCommandMock(func(ctx context.Context, _ *exec.Cmd) (int, error) {
				// fake a context cancellation that occurs while the process is running

				cancel()
				return 0, ctx.Err()
			})

			_, client, closeFunc := startServer(t)
			t.Cleanup(closeFunc)

			stream, err := client.P4Exec(ctx, &proto.P4ExecRequest{
				Args: [][]byte{[]byte("users")},
			})
			if err != nil {
				t.Fatalf("failed to call P4Exec: %v", err)
			}

			_, err = readAll(stream)
			if !(errors.Is(err, context.Canceled) || status.Code(err) == codes.Canceled) {
				t.Fatalf("expected context cancelation error, got %v", err)
			}
		})

	})

	t.Run("HTTP", func(t *testing.T) {

		tests := []Test{
			{
				Name:         "Command",
				Request:      newRequest("POST", "/p4-exec", strings.NewReader(`{"args": ["users"]}`)),
				ExpectedCode: http.StatusOK,
				ExpectedBody: "admin <admin@joe-perforce-server> (admin) accessed 2021/01/31",
				ExpectedTrailers: http.Header{
					"X-Exec-Error":       {"the answer to life the universe and everything"},
					"X-Exec-Exit-Status": {"42"},
					"X-Exec-Stderr":      {"teststderr"},
				},
			},
			{
				Name:         "Error",
				Request:      newRequest("POST", "/p4-exec", strings.NewReader(`{"args": ["bad_command"]}`)),
				ExpectedCode: http.StatusBadRequest,
				ExpectedBody: "subcommand \"bad_command\" is not allowed",
			},
			{
				Name:         "EmptyBody",
				Request:      newRequest("POST", "/p4-exec", nil),
				ExpectedCode: http.StatusBadRequest,
				ExpectedBody: `EOF`,
			},
			{
				Name:         "EmptyInput",
				Request:      newRequest("POST", "/p4-exec", strings.NewReader("{}")),
				ExpectedCode: http.StatusBadRequest,
				ExpectedBody: `args must be greater than or equal to 1`,
			},
		}

		for _, test := range tests {
			t.Run(test.Name, func(t *testing.T) {
				updateRunCommandMock(defaultMockRunCommand)

				handler, _, closeFunc := startServer(t)
				t.Cleanup(closeFunc)

				w := httptest.ResponseRecorder{Body: new(bytes.Buffer)}
				handler.ServeHTTP(&w, test.Request)

				res := w.Result()
				if res.StatusCode != test.ExpectedCode {
					t.Errorf("wrong status: expected %d, got %d", test.ExpectedCode, w.Code)
				}

				body, err := io.ReadAll(res.Body)
				if err != nil {
					t.Fatal(err)
				}
				if strings.TrimSpace(string(body)) != test.ExpectedBody {
					t.Errorf("wrong body: expected %q, got %q", test.ExpectedBody, string(body))
				}

				for k, v := range test.ExpectedTrailers {
					if got := res.Trailer.Get(k); got != v[0] {
						t.Errorf("wrong trailer %q: expected %q, got %q", k, v[0], got)
					}
				}
			})
		}
	})
}

func BenchmarkQuickRevParseHeadQuickSymbolicRefHead_packed_refs(b *testing.B) {
	tmp := b.TempDir()

	dir := filepath.Join(tmp, ".git")
	gitDir := common.GitDir(dir)
	if err := os.Mkdir(dir, 0o700); err != nil {
		b.Fatal(err)
	}

	masterRef := "refs/heads/master"
	// This simulates the most amount of work quickRevParseHead has to do, and
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
		rev, err := quickRevParseHead(gitDir)
		if err != nil {
			b.Fatal(err)
		}
		if rev != masterRev {
			b.Fatal("unexpected rev: ", rev)
		}
		ref, err := quickSymbolicRefHead(gitDir)
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
		rev, err := quickRevParseHead(gitDir)
		if err != nil {
			b.Fatal(err)
		}
		if rev != masterRev {
			b.Fatal("unexpected rev: ", rev)
		}
		ref, err := quickSymbolicRefHead(gitDir)
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

func staticGetRemoteURL(remote string) func(context.Context, api.RepoName) (string, error) {
	return func(context.Context, api.RepoName) (string, error) {
		return remote, nil
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

func makeTestServer(ctx context.Context, t *testing.T, repoDir, remote string, db database.DB) *Server {
	t.Helper()

	if db == nil {
		mDB := dbmocks.NewMockDB()
		mDB.GitserverReposFunc.SetDefaultReturn(dbmocks.NewMockGitserverRepoStore())
		mDB.FeatureFlagsFunc.SetDefaultReturn(dbmocks.NewMockFeatureFlagStore())

		repoStore := dbmocks.NewMockRepoStore()
		repoStore.GetByNameFunc.SetDefaultReturn(nil, &database.RepoNotFoundErr{})

		mDB.ReposFunc.SetDefaultReturn(repoStore)

		db = mDB
	}

	logger := logtest.Scoped(t)
	obctx := observation.TestContextTB(t)

	cloneQueue := NewCloneQueue(obctx, list.New())
	s := &Server{
		Logger:           logger,
		ObservationCtx:   obctx,
		ReposDir:         repoDir,
		GetRemoteURLFunc: staticGetRemoteURL(remote),
		GetVCSSyncer: func(ctx context.Context, name api.RepoName) (VCSSyncer, error) {
			return NewGitRepoSyncer(wrexec.NewNoOpRecordingCommandFactory()), nil
		},
		DB:                      db,
		CloneQueue:              cloneQueue,
		ctx:                     ctx,
		Locker:                  NewRepositoryLocker(),
		cloneLimiter:            limiter.NewMutable(1),
		cloneableLimiter:        limiter.NewMutable(1),
		RPSLimiter:              ratelimit.NewInstrumentedLimiter("GitserverTest", rate.NewLimiter(rate.Inf, 10)),
		RecordingCommandFactory: wrexec.NewRecordingCommandFactory(nil, 0),
		Perforce:                perforce.NewService(ctx, obctx, logger, db, list.New()),
	}

	p := s.NewClonePipeline(logtest.Scoped(t), cloneQueue)
	p.Start()
	t.Cleanup(p.Stop)
	return s
}

func TestCloneRepo(t *testing.T) {
	logger := logtest.Scoped(t)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	reposDir := t.TempDir()

	repoName := api.RepoName("example.com/foo/bar")
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	if _, err := db.FeatureFlags().CreateBool(ctx, "clone-progress-logging", true); err != nil {
		t.Fatal(err)
	}
	dbRepo := &types.Repo{
		Name:        repoName,
		Description: "Test",
	}
	// Insert the repo into our database
	if err := db.Repos().Create(ctx, dbRepo); err != nil {
		t.Fatal(err)
	}

	assertRepoState := func(status types.CloneStatus, size int64, wantErr error) {
		t.Helper()
		fromDB, err := db.GitserverRepos().GetByID(ctx, dbRepo.ID)
		if err != nil {
			t.Fatal(err)
		}
		assert.Equal(t, status, fromDB.CloneStatus)
		assert.Equal(t, size, fromDB.RepoSizeBytes)
		var errString string
		if wantErr != nil {
			errString = wantErr.Error()
		}
		assert.Equal(t, errString, fromDB.LastError)
	}

	// Verify the gitserver repo entry exists.
	assertRepoState(types.CloneStatusNotCloned, 0, nil)

	repoDir := repoDirFromName(reposDir, repoName)
	remoteDir := filepath.Join(reposDir, "remote")
	if err := os.Mkdir(remoteDir, os.ModePerm); err != nil {
		t.Fatal(err)
	}
	cmdExecDir := remoteDir
	cmd := func(name string, arg ...string) string {
		t.Helper()
		return runCmd(t, cmdExecDir, name, arg...)
	}
	wantCommit := makeSingleCommitRepo(cmd)
	// Add a bad tag
	cmd("git", "tag", "HEAD")

	s := makeTestServer(ctx, t, reposDir, remoteDir, db)

	// Enqueue repo clone.
	_, err := s.CloneRepo(ctx, repoName, CloneOptions{})
	require.NoError(t, err)

	// Wait until the clone is done. Please do not use this code snippet
	// outside of a test. We only know this works since our test only starts
	// one clone and will have nothing else attempt to lock.
	for i := 0; i < 1000; i++ {
		_, cloning := s.Locker.Status(repoDir)
		if !cloning {
			break
		}
		time.Sleep(10 * time.Millisecond)
	}
	wantRepoSize := dirSize(repoDir.Path("."))
	assertRepoState(types.CloneStatusCloned, wantRepoSize, err)

	cmdExecDir = repoDir.Path(".")
	gotCommit := cmd("git", "rev-parse", "HEAD")
	if wantCommit != gotCommit {
		t.Fatal("failed to clone:", gotCommit)
	}

	// Test blocking with a failure (already exists since we didn't specify overwrite)
	_, err = s.CloneRepo(context.Background(), repoName, CloneOptions{Block: true})
	if !errors.Is(err, os.ErrExist) {
		t.Fatalf("expected clone repo to fail with already exists: %s", err)
	}
	assertRepoState(types.CloneStatusCloned, wantRepoSize, err)

	// Test blocking with overwrite. First add random file to GIT_DIR. If the
	// file is missing after cloning we know the directory was replaced
	mkFiles(t, repoDir.Path("."), "HELLO")
	_, err = s.CloneRepo(context.Background(), repoName, CloneOptions{Block: true, Overwrite: true})
	if err != nil {
		t.Fatal(err)
	}
	assertRepoState(types.CloneStatusCloned, wantRepoSize, err)

	if _, err := os.Stat(repoDir.Path("HELLO")); !os.IsNotExist(err) {
		t.Fatalf("expected clone to be overwritten: %s", err)
	}

	gotCommit = cmd("git", "rev-parse", "HEAD")
	if wantCommit != gotCommit {
		t.Fatal("failed to clone:", gotCommit)
	}
	gitserverRepo, err := db.GitserverRepos().GetByName(ctx, repoName)
	if err != nil {
		t.Fatal(err)
	}
	if gitserverRepo.CloningProgress == "" {
		t.Error("want non-empty CloningProgress")
	}
}

func TestCloneRepoRecordsFailures(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	logger := logtest.Scoped(t)
	remote := t.TempDir()
	repoName := api.RepoName("example.com/foo/bar")
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	dbRepo := &types.Repo{
		Name:        repoName,
		Description: "Test",
	}
	// Insert the repo into our database
	if err := db.Repos().Create(ctx, dbRepo); err != nil {
		t.Fatal(err)
	}

	assertRepoState := func(status types.CloneStatus, size int64, wantErr error) {
		t.Helper()
		fromDB, err := db.GitserverRepos().GetByID(ctx, dbRepo.ID)
		if err != nil {
			t.Fatal(err)
		}
		assert.Equal(t, status, fromDB.CloneStatus)
		assert.Equal(t, size, fromDB.RepoSizeBytes)
		var errString string
		if wantErr != nil {
			errString = wantErr.Error()
		}
		assert.Equal(t, errString, fromDB.LastError)
	}

	// Verify the gitserver repo entry exists.
	assertRepoState(types.CloneStatusNotCloned, 0, nil)

	reposDir := t.TempDir()
	s := makeTestServer(ctx, t, reposDir, remote, db)

	for _, tc := range []struct {
		name         string
		getVCSSyncer func(ctx context.Context, name api.RepoName) (VCSSyncer, error)
		wantErr      error
	}{
		{
			name: "Not cloneable",
			getVCSSyncer: func(ctx context.Context, name api.RepoName) (VCSSyncer, error) {
				m := NewMockVCSSyncer()
				m.IsCloneableFunc.SetDefaultHook(func(context.Context, api.RepoName, *vcs.URL) error {
					return errors.New("not_cloneable")
				})
				return m, nil
			},
			wantErr: errors.New("error cloning repo: repo example.com/foo/bar not cloneable: not_cloneable"),
		},
		{
			name: "Failing clone",
			getVCSSyncer: func(ctx context.Context, name api.RepoName) (VCSSyncer, error) {
				m := NewMockVCSSyncer()
				m.CloneCommandFunc.SetDefaultHook(func(ctx context.Context, url *vcs.URL, s string) (*exec.Cmd, error) {
					return exec.Command("git", "clone", "/dev/null"), nil
				})
				return m, nil
			},
			wantErr: errors.New("failed to clone example.com/foo/bar: clone failed. Output: fatal: repository '/dev/null' does not exist: exit status 128"),
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			s.GetVCSSyncer = tc.getVCSSyncer
			_, _ = s.CloneRepo(ctx, repoName, CloneOptions{
				Block: true,
			})
			assertRepoState(types.CloneStatusNotCloned, 0, tc.wantErr)
		})
	}
}

var ignoreVolatileGitserverRepoFields = cmpopts.IgnoreFields(
	types.GitserverRepo{},
	"LastFetched",
	"LastChanged",
	"RepoSizeBytes",
	"UpdatedAt",
	"CorruptionLogs",
	"CloningProgress",
)

func TestHandleRepoDelete(t *testing.T) {
	testHandleRepoDelete(t, false)
}

func TestHandleRepoDeleteWhenDeleteInDB(t *testing.T) {
	// We also want to ensure that we can delete repo data on disk for a repo that
	// has already been deleted in the DB.
	testHandleRepoDelete(t, true)
}

func testHandleRepoDelete(t *testing.T, deletedInDB bool) {
	logger := logtest.Scoped(t)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	remote := t.TempDir()
	repoName := api.RepoName("example.com/foo/bar")
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	dbRepo := &types.Repo{
		Name:        repoName,
		Description: "Test",
	}

	// Insert the repo into our database
	if err := db.Repos().Create(ctx, dbRepo); err != nil {
		t.Fatal(err)
	}

	repo := remote
	cmd := func(name string, arg ...string) string {
		t.Helper()
		return runCmd(t, repo, name, arg...)
	}
	_ = makeSingleCommitRepo(cmd)
	// Add a bad tag
	cmd("git", "tag", "HEAD")

	reposDir := t.TempDir()

	s := makeTestServer(ctx, t, reposDir, remote, db)

	// We need some of the side effects here
	_ = s.Handler()

	rr := httptest.NewRecorder()

	updateReq := protocol.RepoUpdateRequest{
		Repo: repoName,
	}
	body, err := json.Marshal(updateReq)
	if err != nil {
		t.Fatal(err)
	}

	// This will perform an initial clone
	req := newRequest("GET", "/repo-update", bytes.NewReader(body))
	s.handleRepoUpdate(rr, req)

	size := dirSize(repoDirFromName(s.ReposDir, repoName).Path("."))
	want := &types.GitserverRepo{
		RepoID:        dbRepo.ID,
		ShardID:       "",
		CloneStatus:   types.CloneStatusCloned,
		RepoSizeBytes: size,
	}
	fromDB, err := db.GitserverRepos().GetByID(ctx, dbRepo.ID)
	if err != nil {
		t.Fatal(err)
	}

	// We don't expect an error
	if diff := cmp.Diff(want, fromDB, ignoreVolatileGitserverRepoFields); diff != "" {
		t.Fatal(diff)
	}

	if deletedInDB {
		if err := db.Repos().Delete(ctx, dbRepo.ID); err != nil {
			t.Fatal(err)
		}
		repos, err := db.Repos().List(ctx, database.ReposListOptions{IncludeDeleted: true, IDs: []api.RepoID{dbRepo.ID}})
		if err != nil {
			t.Fatal(err)
		}
		if len(repos) != 1 {
			t.Fatalf("Expected 1 repo, got %d", len(repos))
		}
		dbRepo = repos[0]
	}

	// Now we can delete it
	deleteReq := protocol.RepoDeleteRequest{
		Repo: dbRepo.Name,
	}
	body, err = json.Marshal(deleteReq)
	if err != nil {
		t.Fatal(err)
	}
	req = newRequest("GET", "/delete", bytes.NewReader(body))
	s.handleRepoDelete(rr, req)

	size = dirSize(repoDirFromName(s.ReposDir, repoName).Path("."))
	if size != 0 {
		t.Fatalf("Size should be 0, got %d", size)
	}

	// Check status in gitserver_repos
	want = &types.GitserverRepo{
		RepoID:        dbRepo.ID,
		ShardID:       "",
		CloneStatus:   types.CloneStatusNotCloned,
		RepoSizeBytes: size,
	}
	fromDB, err = db.GitserverRepos().GetByID(ctx, dbRepo.ID)
	if err != nil {
		t.Fatal(err)
	}

	// We don't expect an error
	if diff := cmp.Diff(want, fromDB, ignoreVolatileGitserverRepoFields); diff != "" {
		t.Fatal(diff)
	}
}

func TestHandleRepoUpdate(t *testing.T) {
	logger := logtest.Scoped(t)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	remote := t.TempDir()
	repoName := api.RepoName("example.com/foo/bar")
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	dbRepo := &types.Repo{
		Name:        repoName,
		Description: "Test",
	}
	// Insert the repo into our database
	if err := db.Repos().Create(ctx, dbRepo); err != nil {
		t.Fatal(err)
	}

	repo := remote
	cmd := func(name string, arg ...string) string {
		t.Helper()
		return runCmd(t, repo, name, arg...)
	}
	_ = makeSingleCommitRepo(cmd)
	// Add a bad tag
	cmd("git", "tag", "HEAD")

	reposDir := t.TempDir()

	s := makeTestServer(ctx, t, reposDir, remote, db)

	// We need the side effects here
	_ = s.Handler()

	rr := httptest.NewRecorder()

	updateReq := protocol.RepoUpdateRequest{
		Repo: repoName,
	}
	body, err := json.Marshal(updateReq)
	if err != nil {
		t.Fatal(err)
	}

	// Confirm that failing to clone the repo stores the error
	oldRemoveURLFunc := s.GetRemoteURLFunc
	s.GetRemoteURLFunc = func(ctx context.Context, name api.RepoName) (string, error) {
		return "https://invalid.example.com/", nil
	}
	req := newRequest("GET", "/repo-update", bytes.NewReader(body))
	s.handleRepoUpdate(rr, req)

	size := dirSize(repoDirFromName(s.ReposDir, repoName).Path("."))
	want := &types.GitserverRepo{
		RepoID:        dbRepo.ID,
		ShardID:       "",
		CloneStatus:   types.CloneStatusNotCloned,
		RepoSizeBytes: size,
		LastError:     "",
	}
	fromDB, err := db.GitserverRepos().GetByID(ctx, dbRepo.ID)
	if err != nil {
		t.Fatal(err)
	}

	// We don't care exactly what the error is here
	cmpIgnored := cmpopts.IgnoreFields(types.GitserverRepo{}, "LastFetched", "LastChanged", "RepoSizeBytes", "UpdatedAt", "LastError", "CorruptionLogs")
	// But we do care that it exists
	if fromDB.LastError == "" {
		t.Errorf("Expected an error when trying to clone from an invalid URL")
	}

	// We don't expect an error
	if diff := cmp.Diff(want, fromDB, cmpIgnored); diff != "" {
		t.Fatal(diff)
	}

	// This will perform an initial clone
	s.GetRemoteURLFunc = oldRemoveURLFunc
	req = newRequest("GET", "/repo-update", bytes.NewReader(body))
	s.handleRepoUpdate(rr, req)

	size = dirSize(repoDirFromName(s.ReposDir, repoName).Path("."))
	want = &types.GitserverRepo{
		RepoID:        dbRepo.ID,
		ShardID:       "",
		CloneStatus:   types.CloneStatusCloned,
		RepoSizeBytes: size,
		LastError:     "",
	}
	fromDB, err = db.GitserverRepos().GetByID(ctx, dbRepo.ID)
	if err != nil {
		t.Fatal(err)
	}

	// We don't expect an error
	if diff := cmp.Diff(want, fromDB, ignoreVolatileGitserverRepoFields); diff != "" {
		t.Fatal(diff)
	}

	// Now we'll call again and with an update that fails
	doBackgroundRepoUpdateMock = func(name api.RepoName) error {
		return errors.New("fail")
	}
	t.Cleanup(func() { doBackgroundRepoUpdateMock = nil })

	// This will trigger an update since the repo is already cloned
	req = newRequest("GET", "/repo-update", bytes.NewReader(body))
	s.handleRepoUpdate(rr, req)

	want = &types.GitserverRepo{
		RepoID:        dbRepo.ID,
		ShardID:       "",
		CloneStatus:   types.CloneStatusCloned,
		LastError:     "fail",
		RepoSizeBytes: size,
	}
	fromDB, err = db.GitserverRepos().GetByID(ctx, dbRepo.ID)
	if err != nil {
		t.Fatal(err)
	}

	// We expect an error
	if diff := cmp.Diff(want, fromDB, ignoreVolatileGitserverRepoFields); diff != "" {
		t.Fatal(diff)
	}

	// Now we'll call again and with an update that succeeds
	doBackgroundRepoUpdateMock = nil

	// This will trigger an update since the repo is already cloned
	req = newRequest("GET", "/repo-update", bytes.NewReader(body))
	s.handleRepoUpdate(rr, req)

	want = &types.GitserverRepo{
		RepoID:        dbRepo.ID,
		ShardID:       "",
		CloneStatus:   types.CloneStatusCloned,
		RepoSizeBytes: dirSize(repoDirFromName(s.ReposDir, repoName).Path(".")), // we compute the new size
	}
	fromDB, err = db.GitserverRepos().GetByID(ctx, dbRepo.ID)
	if err != nil {
		t.Fatal(err)
	}

	// We expect an update
	if diff := cmp.Diff(want, fromDB, ignoreVolatileGitserverRepoFields); diff != "" {
		t.Fatal(diff)
	}
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

		if err := removeBadRefs(context.Background(), gitDir); err != nil {
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

		if err := removeBadRefs(context.Background(), gitDir); err != nil {
			t.Fatal(err)
		}

		if got := cmd("git", "rev-parse", "HEAD"); got != wantCommit {
			t.Fatalf("git ref %s failed to be removed: %s", name, got)
		}
	}
}

func TestCloneRepo_EnsureValidity(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	t.Run("with no remote HEAD file", func(t *testing.T) {
		var (
			remote   = t.TempDir()
			reposDir = t.TempDir()
			cmd      = func(name string, arg ...string) {
				t.Helper()
				runCmd(t, remote, name, arg...)
			}
		)

		cmd("git", "init", ".")
		cmd("rm", ".git/HEAD")

		s := makeTestServer(ctx, t, reposDir, remote, nil)
		if _, err := s.CloneRepo(ctx, "example.com/foo/bar", CloneOptions{}); err == nil {
			t.Fatal("expected an error, got none")
		}
	})
	t.Run("with an empty remote HEAD file", func(t *testing.T) {
		var (
			remote   = t.TempDir()
			reposDir = t.TempDir()
			cmd      = func(name string, arg ...string) {
				t.Helper()
				runCmd(t, remote, name, arg...)
			}
		)

		cmd("git", "init", ".")
		cmd("sh", "-c", ": > .git/HEAD")

		s := makeTestServer(ctx, t, reposDir, remote, nil)
		if _, err := s.CloneRepo(ctx, "example.com/foo/bar", CloneOptions{}); err == nil {
			t.Fatal("expected an error, got none")
		}
	})
	t.Run("with no local HEAD file", func(t *testing.T) {
		var (
			reposDir = t.TempDir()
			remote   = filepath.Join(reposDir, "remote")
			cmd      = func(name string, arg ...string) string {
				t.Helper()
				return runCmd(t, remote, name, arg...)
			}
			repoName = api.RepoName("example.com/foo/bar")
		)

		if err := os.Mkdir(remote, os.ModePerm); err != nil {
			t.Fatal(err)
		}

		_ = makeSingleCommitRepo(cmd)
		s := makeTestServer(ctx, t, reposDir, remote, nil)

		testRepoCorrupter = func(_ context.Context, tmpDir common.GitDir) {
			if err := os.Remove(tmpDir.Path("HEAD")); err != nil {
				t.Fatal(err)
			}
		}
		t.Cleanup(func() { testRepoCorrupter = nil })
		// Use block so we get clone errors right here and don't have to rely on the
		// clone queue. There's no other reason for blocking here, just convenience/simplicity.
		if _, err := s.CloneRepo(ctx, repoName, CloneOptions{Block: true}); err != nil {
			t.Fatalf("expected no error, got %v", err)
		}

		dst := repoDirFromName(s.ReposDir, repoName)
		for i := 0; i < 1000; i++ {
			_, cloning := s.Locker.Status(dst)
			if !cloning {
				break
			}
			time.Sleep(10 * time.Millisecond)
		}

		head, err := os.ReadFile(fmt.Sprintf("%s/HEAD", dst))
		if os.IsNotExist(err) {
			t.Fatal("expected a reconstituted HEAD, but no file exists")
		}
		if head == nil {
			t.Fatal("expected a reconstituted HEAD, but the file is empty")
		}
	})
	t.Run("with an empty local HEAD file", func(t *testing.T) {
		var (
			remote   = t.TempDir()
			reposDir = t.TempDir()
			cmd      = func(name string, arg ...string) string {
				t.Helper()
				return runCmd(t, remote, name, arg...)
			}
		)

		_ = makeSingleCommitRepo(cmd)
		s := makeTestServer(ctx, t, reposDir, remote, nil)

		testRepoCorrupter = func(_ context.Context, tmpDir common.GitDir) {
			cmd("sh", "-c", fmt.Sprintf(": > %s/HEAD", tmpDir))
		}
		t.Cleanup(func() { testRepoCorrupter = nil })
		if _, err := s.CloneRepo(ctx, "example.com/foo/bar", CloneOptions{}); err != nil {
			t.Fatalf("expected no error, got %v", err)
		}

		// wait for repo to be cloned
		dst := repoDirFromName(s.ReposDir, "example.com/foo/bar")
		for i := 0; i < 1000; i++ {
			_, cloning := s.Locker.Status(dst)
			if !cloning {
				break
			}
			time.Sleep(10 * time.Millisecond)
		}

		head, err := os.ReadFile(fmt.Sprintf("%s/HEAD", dst))
		if os.IsNotExist(err) {
			t.Fatal("expected a reconstituted HEAD, but no file exists")
		}
		if head == nil {
			t.Fatal("expected a reconstituted HEAD, but the file is empty")
		}
	})
}

func TestHostnameMatch(t *testing.T) {
	testCases := []struct {
		hostname    string
		addr        string
		shouldMatch bool
	}{
		{
			hostname:    "gitserver-1",
			addr:        "gitserver-1",
			shouldMatch: true,
		},
		{
			hostname:    "gitserver-1",
			addr:        "gitserver-1.gitserver:3178",
			shouldMatch: true,
		},
		{
			hostname:    "gitserver-1",
			addr:        "gitserver-10.gitserver:3178",
			shouldMatch: false,
		},
		{
			hostname:    "gitserver-1",
			addr:        "gitserver-10",
			shouldMatch: false,
		},
		{
			hostname:    "gitserver-10",
			addr:        "",
			shouldMatch: false,
		},
		{
			hostname:    "gitserver-10",
			addr:        "gitserver-10:3178",
			shouldMatch: true,
		},
		{
			hostname:    "gitserver-10",
			addr:        "gitserver-10:3178",
			shouldMatch: true,
		},
		{
			hostname:    "gitserver-0.prod",
			addr:        "gitserver-0.prod.default.namespace",
			shouldMatch: true,
		},
	}

	for _, tc := range testCases {
		t.Run("", func(t *testing.T) {
			have := hostnameMatch(tc.hostname, tc.addr)
			if have != tc.shouldMatch {
				t.Fatalf("Want %v, got %v", tc.shouldMatch, have)
			}
		})
	}
}

func TestSyncRepoState(t *testing.T) {
	logger := logtest.Scoped(t)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	remoteDir := t.TempDir()

	cmd := func(name string, arg ...string) {
		t.Helper()
		runCmd(t, remoteDir, name, arg...)
	}

	// Setup a repo with a commit so we can see if we can clone it.
	cmd("git", "init", ".")
	cmd("sh", "-c", "echo hello world > hello.txt")
	cmd("git", "add", "hello.txt")
	cmd("git", "commit", "-m", "hello")

	reposDir := t.TempDir()
	repoName := api.RepoName("example.com/foo/bar")
	hostname := "test"

	s := makeTestServer(ctx, t, reposDir, remoteDir, db)
	s.Hostname = hostname

	dbRepo := &types.Repo{
		Name:        repoName,
		URI:         string(repoName),
		Description: "Test",
	}

	// Insert the repo into our database
	err := db.Repos().Create(ctx, dbRepo)
	if err != nil {
		t.Fatal(err)
	}

	_, err = s.CloneRepo(ctx, repoName, CloneOptions{Block: true})
	if err != nil {
		t.Fatal(err)
	}

	_, err = db.GitserverRepos().GetByID(ctx, dbRepo.ID)
	if err != nil {
		// GitserverRepo should exist after updating the lastFetched time
		t.Fatal(err)
	}

	err = syncRepoState(ctx, logger, db, s.Locker, hostname, reposDir, gitserver.GitserverAddresses{Addresses: []string{hostname}}, 10, 10, true)
	if err != nil {
		t.Fatal(err)
	}

	gr, err := db.GitserverRepos().GetByID(ctx, dbRepo.ID)
	if err != nil {
		t.Fatal(err)
	}

	if gr.CloneStatus != types.CloneStatusCloned {
		t.Fatalf("Want %v, got %v", types.CloneStatusCloned, gr.CloneStatus)
	}

	t.Run("sync deleted repo", func(t *testing.T) {
		// Fake setting an incorrect status
		if err := db.GitserverRepos().SetCloneStatus(ctx, dbRepo.Name, types.CloneStatusUnknown, hostname); err != nil {
			t.Fatal(err)
		}

		// We should continue to sync deleted repos
		if err := db.Repos().Delete(ctx, dbRepo.ID); err != nil {
			t.Fatal(err)
		}

		err = syncRepoState(ctx, logger, db, s.Locker, hostname, reposDir, gitserver.GitserverAddresses{Addresses: []string{hostname}}, 10, 10, true)
		if err != nil {
			t.Fatal(err)
		}

		gr, err := db.GitserverRepos().GetByID(ctx, dbRepo.ID)
		if err != nil {
			t.Fatal(err)
		}

		if gr.CloneStatus != types.CloneStatusCloned {
			t.Fatalf("Want %v, got %v", types.CloneStatusCloned, gr.CloneStatus)
		}
	})
}

type BatchLogTest struct {
	Name           string
	Request        *http.Request
	ExpectedCode   int
	ExpectedBody   string
	RunCommandMock func(ctx context.Context, cmd *exec.Cmd) (int, error)
}

func TestHandleBatchLog(t *testing.T) {
	originalRepoCloned := repoCloned
	repoCloned = func(dir common.GitDir) bool {
		return dir == "github.com/foo/bar/.git" || dir == "github.com/foo/baz/.git" || dir == "github.com/foo/bonk/.git"
	}
	t.Cleanup(func() { repoCloned = originalRepoCloned })

	runCommandMock = func(ctx context.Context, cmd *exec.Cmd) (int, error) {
		for _, v := range cmd.Args {
			if strings.HasPrefix(v, "dumbmilk") {
				return 128, errors.New("test error")
			}
		}

		cmd.Stdout.Write([]byte(fmt.Sprintf("stdout<%s:%s>", cmd.Dir, strings.Join(cmd.Args, " "))))
		return 0, nil
	}
	t.Cleanup(func() { runCommandMock = nil })

	tests := []BatchLogTest{
		{
			Name:         "bad request",
			Request:      newRequest("POST", "/batch-log", strings.NewReader(``)),
			ExpectedCode: http.StatusBadRequest,
			ExpectedBody: "EOF", // the particular error when parsing empty payload
		},
		{
			Name:         "empty",
			Request:      newRequest("POST", "/batch-log", strings.NewReader(`{}`)),
			ExpectedCode: http.StatusOK,
			ExpectedBody: mustEncodeJSONResponse(protocol.BatchLogResponse{
				Results: []protocol.BatchLogResult{},
			}),
		},
		{
			Name: "all resolved",
			Request: newRequest("POST", "/batch-log", strings.NewReader(`{
				"repoCommits": [
					{"repo": "github.com/foo/bar", "commitId": "deadbeef1"},
					{"repo": "github.com/foo/baz", "commitId": "deadbeef2"},
					{"repo": "github.com/foo/bonk", "commitId": "deadbeef3"}
				],
				"format": "--format=test"
			}`)),
			ExpectedCode: http.StatusOK,
			ExpectedBody: mustEncodeJSONResponse(protocol.BatchLogResponse{
				Results: []protocol.BatchLogResult{
					{
						RepoCommit:    api.RepoCommit{Repo: "github.com/foo/bar", CommitID: "deadbeef1"},
						CommandOutput: "stdout<github.com/foo/bar/.git:git log -n 1 --name-only --format=test deadbeef1>",
						CommandError:  "",
					},
					{
						RepoCommit:    api.RepoCommit{Repo: "github.com/foo/baz", CommitID: "deadbeef2"},
						CommandOutput: "stdout<github.com/foo/baz/.git:git log -n 1 --name-only --format=test deadbeef2>",
						CommandError:  "",
					},
					{
						RepoCommit:    api.RepoCommit{Repo: "github.com/foo/bonk", CommitID: "deadbeef3"},
						CommandOutput: "stdout<github.com/foo/bonk/.git:git log -n 1 --name-only --format=test deadbeef3>",
						CommandError:  "",
					},
				},
			}),
		},
		{
			Name: "partially resolved",
			Request: newRequest("POST", "/batch-log", strings.NewReader(`{
				"repoCommits": [
					{"repo": "github.com/foo/bar", "commitId": "deadbeef1"},
					{"repo": "github.com/foo/baz", "commitId": "dumbmilk1"},
					{"repo": "github.com/foo/honk", "commitId": "deadbeef3"}
				],
				"format": "--format=test"
			}`)),
			ExpectedCode: http.StatusOK,
			ExpectedBody: mustEncodeJSONResponse(protocol.BatchLogResponse{
				Results: []protocol.BatchLogResult{
					{
						RepoCommit:    api.RepoCommit{Repo: "github.com/foo/bar", CommitID: "deadbeef1"},
						CommandOutput: "stdout<github.com/foo/bar/.git:git log -n 1 --name-only --format=test deadbeef1>",
						CommandError:  "",
					},
					{
						// git directory found, but cmd.Run returned error
						RepoCommit:    api.RepoCommit{Repo: "github.com/foo/baz", CommitID: "dumbmilk1"},
						CommandOutput: "",
						CommandError:  "test error",
					},
					{
						// no .git directory here
						RepoCommit:    api.RepoCommit{Repo: "github.com/foo/honk", CommitID: "deadbeef3"},
						CommandOutput: "",
						CommandError:  "repo not found",
					},
				},
			}),
		},
	}

	for _, test := range tests {
		t.Run(test.Name, func(t *testing.T) {
			server := &Server{
				Logger:                  logtest.Scoped(t),
				ObservationCtx:          observation.TestContextTB(t),
				GlobalBatchLogSemaphore: semaphore.NewWeighted(8),
				DB:                      dbmocks.NewMockDB(),
				RecordingCommandFactory: wrexec.NewNoOpRecordingCommandFactory(),
				Locker:                  NewRepositoryLocker(),
			}
			h := server.Handler()

			w := httptest.ResponseRecorder{Body: new(bytes.Buffer)}
			h.ServeHTTP(&w, test.Request)

			res := w.Result()
			if res.StatusCode != test.ExpectedCode {
				t.Errorf("wrong status: expected %d, got %d", test.ExpectedCode, w.Code)
			}

			body, err := io.ReadAll(res.Body)
			if err != nil {
				t.Fatal(err)
			}
			if strings.TrimSpace(string(body)) != test.ExpectedBody {
				t.Errorf("wrong body: expected %q, got %q", test.ExpectedBody, string(body))
			}
		})
	}
}

func TestHeaderXRequestedWithMiddleware(t *testing.T) {
	test := headerXRequestedWithMiddleware(
		http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Write([]byte("success"))
			w.WriteHeader(http.StatusOK)
		}),
	)

	assertBody := func(result *http.Response, want string) {
		b, err := io.ReadAll(result.Body)
		if err != nil {
			t.Fatalf("failed to read body: %v", err)
		}

		data := string(b)

		if data != want {
			t.Fatalf(`Expected body to contain %q, but found %q`, want, data)
		}
	}

	failureExpectation := "header X-Requested-With is not set or is invalid\n"

	t.Run("x-requested-with not set", func(t *testing.T) {
		r := httptest.NewRequest(http.MethodGet, "/", nil)
		w := httptest.NewRecorder()

		test(w, r)

		result := w.Result()
		defer result.Body.Close()

		if result.StatusCode != http.StatusBadRequest {
			t.Fatalf("expected HTTP status code %d, but got %d", http.StatusBadRequest, result.StatusCode)
		}

		assertBody(result, failureExpectation)
	})

	t.Run("x-requested-with invalid value", func(t *testing.T) {
		r := httptest.NewRequest(http.MethodGet, "/", nil)
		r.Header.Add("X-Requested-With", "foo")
		w := httptest.NewRecorder()

		test(w, r)

		result := w.Result()
		defer result.Body.Close()

		if result.StatusCode != http.StatusBadRequest {
			t.Fatalf("expected HTTP status code %d, but got %d", http.StatusBadRequest, result.StatusCode)
		}

		assertBody(result, failureExpectation)
	})

	t.Run("x-requested-with correct value", func(t *testing.T) {
		r := httptest.NewRequest(http.MethodGet, "/", nil)
		r.Header.Add("X-Requested-With", "Sourcegraph")
		w := httptest.NewRecorder()

		test(w, r)

		result := w.Result()
		defer result.Body.Close()

		if result.StatusCode != http.StatusOK {
			t.Fatalf("expected HTTP status code %d, but got %d", http.StatusOK, result.StatusCode)
		}

		assertBody(result, "success")
	})

	t.Run("check skippped for /ping", func(t *testing.T) {
		r := httptest.NewRequest(http.MethodGet, "/ping", nil)
		w := httptest.NewRecorder()

		test(w, r)

		result := w.Result()
		defer result.Body.Close()

		if result.StatusCode != http.StatusOK {
			t.Fatalf("expected HTTP status code %d, but got %d", http.StatusOK, result.StatusCode)
		}
	})

	t.Run("check skipped for /git", func(t *testing.T) {
		r := httptest.NewRequest(http.MethodGet, "/git/foo/bar", nil)
		w := httptest.NewRecorder()

		test(w, r)

		result := w.Result()
		defer result.Body.Close()

		if result.StatusCode != http.StatusOK {
			t.Fatalf("expected HTTP status code %d, but got %d", http.StatusOK, result.StatusCode)
		}
	})
}

func TestLogIfCorrupt(t *testing.T) {
	logger := logtest.Scoped(t)
	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	remoteDir := t.TempDir()

	reposDir := t.TempDir()
	hostname := "test"

	repoName := api.RepoName("example.com/bar/foo")
	s := makeTestServer(ctx, t, reposDir, remoteDir, db)
	s.Hostname = hostname

	t.Run("git corruption output creates corruption log", func(t *testing.T) {
		dbRepo := &types.Repo{
			Name:        repoName,
			URI:         string(repoName),
			Description: "Test",
		}

		// Insert the repo into our database
		err := db.Repos().Create(ctx, dbRepo)
		if err != nil {
			t.Fatal(err)
		}
		t.Cleanup(func() {
			db.Repos().Delete(ctx, dbRepo.ID)
		})

		stdErr := "error: packfile .git/objects/pack/pack-e26c1fc0add58b7649a95f3e901e30f29395e174.pack does not match index"

		s.logIfCorrupt(ctx, repoName, repoDirFromName(s.ReposDir, repoName), stdErr)

		fromDB, err := s.DB.GitserverRepos().GetByName(ctx, repoName)
		assert.NoError(t, err)
		assert.Len(t, fromDB.CorruptionLogs, 1)
		assert.Contains(t, fromDB.CorruptionLogs[0].Reason, stdErr)
	})

	t.Run("non corruption output does not create corruption log", func(t *testing.T) {
		dbRepo := &types.Repo{
			Name:        repoName,
			URI:         string(repoName),
			Description: "Test",
		}

		// Insert the repo into our database
		err := db.Repos().Create(ctx, dbRepo)
		if err != nil {
			t.Fatal(err)
		}
		t.Cleanup(func() {
			db.Repos().Delete(ctx, dbRepo.ID)
		})

		stdErr := "Brought to you by Horsegraph"

		s.logIfCorrupt(ctx, repoName, repoDirFromName(s.ReposDir, repoName), stdErr)

		fromDB, err := s.DB.GitserverRepos().GetByName(ctx, repoName)
		assert.NoError(t, err)
		assert.Len(t, fromDB.CorruptionLogs, 0)
	})
}

func mustEncodeJSONResponse(value any) string {
	encoded, _ := json.Marshal(value)
	return strings.TrimSpace(string(encoded))
}

func TestIgnorePath(t *testing.T) {
	reposDir := "/data/repos"

	for _, tc := range []struct {
		path         string
		shouldIgnore bool
	}{
		{path: filepath.Join(reposDir, TempDirName), shouldIgnore: true},
		{path: filepath.Join(reposDir, P4HomeName), shouldIgnore: true},
		// Double check handling of trailing space
		{path: filepath.Join(reposDir, P4HomeName+"   "), shouldIgnore: true},
		{path: filepath.Join(reposDir, "sourcegraph/sourcegraph"), shouldIgnore: false},
	} {
		t.Run("", func(t *testing.T) {
			assert.Equal(t, tc.shouldIgnore, ignorePath(reposDir, tc.path))
		})
	}
}

func TestMain(m *testing.M) {
	flag.Parse()
	if !testing.Verbose() {
		logtest.InitWithLevel(m, log.LevelNone)
	} else {
		logtest.Init(m)
	}
	os.Exit(m.Run())
}
