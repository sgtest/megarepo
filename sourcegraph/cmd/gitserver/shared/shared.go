// gitserver is the gitserver server.
package shared

import (
	"container/list"
	"context"
	"database/sql"
	"fmt"
	"net/http"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"

	"github.com/sourcegraph/log"
	"golang.org/x/sync/semaphore"
	"google.golang.org/grpc"

	server "github.com/sourcegraph/sourcegraph/cmd/gitserver/internal"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/accesslog"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/cloneurl"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/gitserverfs"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/perforce"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/vcssyncer"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/authz/subrepoperms"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/dependencies"
	"github.com/sourcegraph/sourcegraph/internal/collections"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/database"
	connections "github.com/sourcegraph/sourcegraph/internal/database/connections/live"
	"github.com/sourcegraph/sourcegraph/internal/encryption/keyring"
	"github.com/sourcegraph/sourcegraph/internal/env"
	proto "github.com/sourcegraph/sourcegraph/internal/gitserver/v1"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/goroutine/recorder"
	internalgrpc "github.com/sourcegraph/sourcegraph/internal/grpc"
	"github.com/sourcegraph/sourcegraph/internal/grpc/defaults"
	"github.com/sourcegraph/sourcegraph/internal/httpserver"
	"github.com/sourcegraph/sourcegraph/internal/instrumentation"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/ratelimit"
	"github.com/sourcegraph/sourcegraph/internal/requestclient"
	"github.com/sourcegraph/sourcegraph/internal/service"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/wrexec"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func Main(ctx context.Context, observationCtx *observation.Context, ready service.ReadyFunc, config *Config) error {
	logger := observationCtx.Logger

	// Load and validate configuration.
	if err := config.Validate(); err != nil {
		return errors.Wrap(err, "failed to validate configuration")
	}

	// Prepare the file system.
	if err := gitserverfs.InitGitserverFileSystem(logger, config.ReposDir); err != nil {
		return err
	}

	// Create a database connection.
	sqlDB, err := getDB(observationCtx)
	if err != nil {
		return errors.Wrap(err, "initializing database stores")
	}
	db := database.NewDB(observationCtx.Logger, sqlDB)

	// Initialize the keyring.
	err = keyring.Init(ctx)
	if err != nil {
		return errors.Wrap(err, "initializing keyring")
	}

	authz.DefaultSubRepoPermsChecker = subrepoperms.NewSubRepoPermsClient(db.SubRepoPerms())

	// Setup our server megastruct.
	recordingCommandFactory := wrexec.NewRecordingCommandFactory(nil, 0)
	cloneQueue := server.NewCloneQueue(observationCtx, list.New())
	locker := server.NewRepositoryLocker()
	gitserver := server.Server{
		Logger:         logger,
		ObservationCtx: observationCtx,
		ReposDir:       config.ReposDir,
		GetRemoteURLFunc: func(ctx context.Context, repo api.RepoName) (string, error) {
			return getRemoteURLFunc(ctx, logger, db, repo)
		},
		GetVCSSyncer: func(ctx context.Context, repo api.RepoName) (vcssyncer.VCSSyncer, error) {
			return vcssyncer.NewVCSSyncer(ctx, &vcssyncer.NewVCSSyncerOpts{
				ExternalServiceStore:    db.ExternalServices(),
				RepoStore:               db.Repos(),
				DepsSvc:                 dependencies.NewService(observationCtx, db),
				Repo:                    repo,
				ReposDir:                config.ReposDir,
				CoursierCacheDir:        config.CoursierCacheDir,
				RecordingCommandFactory: recordingCommandFactory,
				Logger:                  logger,
			})
		},
		Hostname:                config.ExternalAddress,
		DB:                      db,
		CloneQueue:              cloneQueue,
		GlobalBatchLogSemaphore: semaphore.NewWeighted(int64(config.BatchLogGlobalConcurrencyLimit)),
		Perforce:                perforce.NewService(ctx, observationCtx, logger, db, list.New()),
		RecordingCommandFactory: recordingCommandFactory,
		Locker:                  locker,
		RPSLimiter: ratelimit.NewInstrumentedLimiter(
			ratelimit.GitRPSLimiterBucketName,
			ratelimit.NewGlobalRateLimiter(logger, ratelimit.GitRPSLimiterBucketName),
		),
	}

	// Make sure we watch for config updates that affect the recordingCommandFactory.
	go conf.Watch(func() {
		// We update the factory with a predicate func. Each subsequent recordable command will use this predicate
		// to determine whether a command should be recorded or not.
		recordingConf := conf.Get().SiteConfig().GitRecorder
		if recordingConf == nil {
			recordingCommandFactory.Disable()
			return
		}
		recordingCommandFactory.Update(recordCommandsOnRepos(recordingConf.Repos, recordingConf.IgnoredGitCommands), recordingConf.Size)
	})

	gitserver.RegisterMetrics(observationCtx, db)

	// Create Handler now since it also initializes state
	// TODO: Why do we set server state as a side effect of creating our handler?
	handler := gitserver.Handler()
	handler = actor.HTTPMiddleware(logger, handler)
	handler = requestclient.InternalHTTPMiddleware(handler)
	handler = trace.HTTPMiddleware(logger, handler, conf.DefaultClient())
	handler = instrumentation.HTTPMiddleware("", handler)
	handler = internalgrpc.MultiplexHandlers(makeGRPCServer(logger, &gitserver), handler)

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	routines := []goroutine.BackgroundRoutine{
		httpserver.NewFromAddr(config.ListenAddress, &http.Server{
			Handler: handler,
		}),
		gitserver.NewClonePipeline(logger, cloneQueue),
		server.NewRepoStateSyncer(
			ctx,
			logger,
			db,
			locker,
			gitserver.Hostname,
			config.ReposDir,
			config.SyncRepoStateInterval,
			config.SyncRepoStateBatchSize,
			config.SyncRepoStateUpdatePerSecond,
		),
	}

	if runtime.GOOS == "windows" {
		// See https://github.com/sourcegraph/sourcegraph/issues/54317 for details.
		logger.Warn("Janitor is disabled on windows")
	} else {
		routines = append(
			routines,
			server.NewJanitor(
				ctx,
				server.JanitorConfig{
					ShardID:            gitserver.Hostname,
					JanitorInterval:    config.JanitorInterval,
					ReposDir:           config.ReposDir,
					DesiredPercentFree: config.JanitorReposDesiredPercentFree,
				},
				db,
				recordingCommandFactory,
				gitserver.CloneRepo,
				logger,
			),
		)
	}

	// Register recorder in all routines that support it.
	recorderCache := recorder.GetCache()
	rec := recorder.New(observationCtx.Logger, env.MyName, recorderCache)
	for _, r := range routines {
		if recordable, ok := r.(recorder.Recordable); ok {
			// Set the hostname to the shardID so we record the routines per
			// gitserver instance.
			recordable.SetJobName(fmt.Sprintf("gitserver %s", gitserver.Hostname))
			recordable.RegisterRecorder(rec)
			rec.Register(recordable)
		}
	}
	rec.RegistrationDone()

	logger.Info("git-server: listening", log.String("addr", config.ListenAddress))

	// We're ready!
	ready()

	// Launch all routines!
	goroutine.MonitorBackgroundRoutines(ctx, routines...)

	// The most important thing this does is kill all our clones. If we just
	// shutdown they will be orphaned and continue running.
	gitserver.Stop()

	return nil
}

// makeGRPCServer creates a new *grpc.Server for the gitserver endpoints and registers
// it with methods on the given server.
func makeGRPCServer(logger log.Logger, s *server.Server) *grpc.Server {
	configurationWatcher := conf.DefaultClient()

	var additionalServerOptions []grpc.ServerOption

	for method, scopedLogger := range map[string]log.Logger{
		proto.GitserverService_Exec_FullMethodName:      logger.Scoped("exec.accesslog"),
		proto.GitserverService_Archive_FullMethodName:   logger.Scoped("archive.accesslog"),
		proto.GitserverService_P4Exec_FullMethodName:    logger.Scoped("p4exec.accesslog"),
		proto.GitserverService_GetObject_FullMethodName: logger.Scoped("get-object.accesslog"),
	} {
		streamInterceptor := accesslog.StreamServerInterceptor(scopedLogger, configurationWatcher)
		unaryInterceptor := accesslog.UnaryServerInterceptor(scopedLogger, configurationWatcher)

		additionalServerOptions = append(additionalServerOptions,
			grpc.ChainStreamInterceptor(methodSpecificStreamInterceptor(method, streamInterceptor)),
			grpc.ChainUnaryInterceptor(methodSpecificUnaryInterceptor(method, unaryInterceptor)),
		)
	}

	grpcServer := defaults.NewServer(logger, additionalServerOptions...)
	proto.RegisterGitserverServiceServer(grpcServer, &server.GRPCServer{
		Server: s,
	})

	return grpcServer
}

// getDB initializes a connection to the database and returns a dbutil.DB
func getDB(observationCtx *observation.Context) (*sql.DB, error) {
	// Gitserver is an internal actor. We rely on the frontend to do authz checks for
	// user requests.
	//
	// This call to SetProviders is here so that calls to GetProviders don't block.
	authz.SetProviders(true, []authz.Provider{})

	dsn := conf.GetServiceConnectionValueAndRestartOnChange(func(serviceConnections conftypes.ServiceConnections) string {
		return serviceConnections.PostgresDSN
	})
	return connections.EnsureNewFrontendDB(observationCtx, dsn, "gitserver")
}

// getRemoteURLFunc returns a remote URL for the given repo, if any external service
// connections reference it. The first external service mentioned in repo.Sources
// will be used to construct the URL and get credentials.
// Since r.Sources is a map, a random referencing service will be used, so this
// function is not idempotent.
// This allows us to try different tokens, to maximize the chances of a repo eventually
// cloning successfully.
func getRemoteURLFunc(
	ctx context.Context,
	logger log.Logger,
	db database.DB,
	repo api.RepoName,
) (string, error) {
	r, err := db.Repos().GetByName(ctx, repo)
	if err != nil {
		return "", err
	}

	for _, info := range r.Sources {
		// build the clone url using the external service config instead of using
		// the source CloneURL field
		svc, err := db.ExternalServices().GetByID(ctx, info.ExternalServiceID())
		if err != nil {
			return "", err
		}

		return cloneurl.ForEncryptableConfig(ctx, logger.Scoped("repos.CloneURL"), db, svc.Kind, svc.Config, r)
	}
	return "", errors.Errorf("no sources for %q", repo)
}

// methodSpecificStreamInterceptor returns a gRPC stream server interceptor that only calls the next interceptor if the method matches.
//
// The returned interceptor will call next if the invoked gRPC method matches the method parameter. Otherwise, it will call handler directly.
func methodSpecificStreamInterceptor(method string, next grpc.StreamServerInterceptor) grpc.StreamServerInterceptor {
	return func(srv interface{}, ss grpc.ServerStream, info *grpc.StreamServerInfo, handler grpc.StreamHandler) error {
		if method != info.FullMethod {
			return handler(srv, ss)
		}

		return next(srv, ss, info, handler)
	}
}

// methodSpecificUnaryInterceptor returns a gRPC unary server interceptor that only calls the next interceptor if the method matches.
//
// The returned interceptor will call next if the invoked gRPC method matches the method parameter. Otherwise, it will call handler directly.
func methodSpecificUnaryInterceptor(method string, next grpc.UnaryServerInterceptor) grpc.UnaryServerInterceptor {
	return func(ctx context.Context, req interface{}, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (resp interface{}, err error) {
		if method != info.FullMethod {
			return handler(ctx, req)
		}

		return next(ctx, req, info, handler)
	}
}

var defaultIgnoredGitCommands = []string{
	"show",
	"rev-parse",
	"log",
	"diff",
	"ls-tree",
}

// recordCommandsOnRepos returns a ShouldRecordFunc which determines whether the given command should be recorded
// for a particular repository.
func recordCommandsOnRepos(repos []string, ignoredGitCommands []string) wrexec.ShouldRecordFunc {
	// empty repos, means we should never record since there is nothing to match on
	if len(repos) == 0 {
		return func(ctx context.Context, c *exec.Cmd) bool {
			return false
		}
	}

	if len(ignoredGitCommands) == 0 {
		ignoredGitCommands = append(ignoredGitCommands, defaultIgnoredGitCommands...)
	}

	// we won't record any git commands with these commands since they are considered to be not destructive
	ignoredGitCommandsMap := collections.NewSet(ignoredGitCommands...)

	return func(ctx context.Context, cmd *exec.Cmd) bool {
		base := filepath.Base(cmd.Path)
		if base != "git" {
			return false
		}

		repoMatch := false
		// If repos contains a single "*" element, it means to record commands
		// for all repositories.
		if len(repos) == 1 && repos[0] == "*" {
			repoMatch = true
		} else {
			for _, repo := range repos {
				// We need to check the suffix, because we can have some common parts in
				// different repo names. E.g. "sourcegraph/sourcegraph" and
				// "sourcegraph/sourcegraph-code-ownership" will both be allowed even if only the
				// first name is included in the config.
				if strings.HasSuffix(cmd.Dir, repo+"/.git") {
					repoMatch = true
					break
				}
			}
		}

		// If the repo doesn't match, no use in checking if it is a command we should record.
		if !repoMatch {
			return false
		}
		// we have to scan the Args, since it isn't guaranteed that the Arg at index 1 is the git command:
		// git -c "protocol.version=2" remote show
		for _, arg := range cmd.Args {
			if ok := ignoredGitCommandsMap.Has(arg); ok {
				return false
			}
		}
		return true
	}
}
