package internal

import (
	"bytes"
	"context"
	_ "embed"
	"fmt"
	"hash/fnv"
	"io"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/common"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/executil"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/git"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/git/gitcli"
	"github.com/sourcegraph/sourcegraph/cmd/gitserver/internal/gitserverfs"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/diskusage"
	"github.com/sourcegraph/sourcegraph/internal/dotcom"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/connection"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/hostname"
	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/wrexec"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type JanitorConfig struct {
	JanitorInterval time.Duration
	ShardID         string

	DesiredPercentFree             int
	DisableDeleteReposOnWrongShard bool
}

func NewJanitor(ctx context.Context, cfg JanitorConfig, db database.DB, fs gitserverfs.FS, gitBackendSource git.GitBackendSource, rcf *wrexec.RecordingCommandFactory, logger log.Logger) goroutine.BackgroundRoutine {
	return goroutine.NewPeriodicGoroutine(
		actor.WithInternalActor(ctx),
		goroutine.HandlerFunc(func(ctx context.Context) error {
			logger.Info("Starting janitor run")
			// On Sourcegraph.com, we clone repos lazily, meaning whatever github.com
			// repo is visited will be cloned eventually. So over time, we would always
			// accumulate terabytes of repos, of which many are probably not visited
			// often. Thus, we have this special cleanup worker for Sourcegraph.com that
			// will remove repos that have not been changed in a long time (thats the
			// best metric we have here today) once our disks are running full.
			// On customer instances, this worker is useless, because repos are always
			// managed by an external service connection and they will be recloned
			// ASAP.
			if dotcom.SourcegraphDotComMode() {
				func() {
					logger := logger.Scoped("dotcom-repo-cleaner")
					start := time.Now()
					logger.Info("Starting dotcom repo cleaner")

					usage, err := fs.DiskUsage()
					if err != nil {
						logger.Error("getting free disk space", log.Error(err))
						return
					}

					toFree := howManyBytesToFree(logger, usage, cfg.DesiredPercentFree)

					if err := freeUpSpace(ctx, logger, db, fs, cfg.ShardID, usage, cfg.DesiredPercentFree, toFree); err != nil {
						logger.Error("error freeing up space", log.Error(err))
					}

					logger.Info("dotcom repo cleaner finished", log.Int64("toFree", toFree), log.Bool("failed", err != nil), log.String("duration", time.Since(start).String()))
				}()
			}

			gitserverAddrs := connection.NewGitserverAddresses(conf.Get())
			// TODO: Should this return an error?
			cleanupRepos(ctx, logger, db, fs, gitBackendSource, rcf, cfg.ShardID, gitserverAddrs, cfg.DisableDeleteReposOnWrongShard)

			return nil
		}),
		goroutine.WithName("gitserver.janitor"),
		goroutine.WithDescription("cleans up and maintains repositories regularly"),
		goroutine.WithInterval(cfg.JanitorInterval),
	)
}

var (
	wrongShardReposTotal = promauto.NewGauge(prometheus.GaugeOpts{
		Name: "src_gitserver_repo_wrong_shard",
		Help: "The number of repos that are on disk on the wrong shard",
	})
	wrongShardReposDeletedCounter = promauto.NewCounter(prometheus.CounterOpts{
		Name: "src_gitserver_repo_wrong_shard_deleted",
		Help: "The number of repos on the wrong shard that we deleted",
	})
)

//go:embed sg_maintenance.sh
var sgMaintenanceScript string

const (
	day = 24 * time.Hour
	// repoTTLGC is how often we should re-clone a repository once it is
	// reporting git gc issues.
	repoTTLGC = 2 * day
	// The name of the log file placed by sg maintenance in case it encountered an
	// error.
	sgmLog = "sgm.log"
)

const (
	// gitGCModeGitAutoGC is when we rely on git running auto gc.
	gitGCModeGitAutoGC int = 1
	// gitGCModeJanitorAutoGC is when during janitor jobs we run git gc --auto.
	gitGCModeJanitorAutoGC = 2
	// gitGCModeMaintenance is when during janitor jobs we run sg maintenance.
	gitGCModeMaintenance = 3
)

// gitGCMode describes which mode we should be running git gc.
// See for a detailed description of the modes: https://docs-legacy.sourcegraph.com/dev/background-information/git_gc
var gitGCMode = func() int {
	// EnableGCAuto is a temporary flag that allows us to control whether or not
	// `git gc --auto` is invoked during janitorial activities. This flag will
	// likely evolve into some form of site config value in the future.
	enableGCAuto, _ := strconv.ParseBool(env.Get("SRC_ENABLE_GC_AUTO", "true", "Use git-gc during janitorial cleanup phases"))

	// sg maintenance and git gc must not be enabled at the same time. However, both
	// might be disabled at the same time, hence we need both SRC_ENABLE_GC_AUTO and
	// SRC_ENABLE_SG_MAINTENANCE.
	enableSGMaintenance, _ := strconv.ParseBool(env.Get("SRC_ENABLE_SG_MAINTENANCE", "false", "Use sg maintenance during janitorial cleanup phases"))

	if enableGCAuto && !enableSGMaintenance {
		return gitGCModeJanitorAutoGC
	}

	if enableSGMaintenance && !enableGCAuto {
		return gitGCModeMaintenance
	}

	return gitGCModeGitAutoGC
}()

// The limit of 50 mirrors Git's gc_auto_pack_limit
var autoPackLimit, _ = strconv.Atoi(env.Get("SRC_GIT_AUTO_PACK_LIMIT", "50", "the maximum number of pack files we tolerate before we trigger a repack"))

// Our original Git gc job used 1 as limit, while git's default is 6700. We
// don't want to be too aggressive to avoid unnecessary IO, hence we choose a
// value somewhere in the middle. https://gitlab.com/gitlab-org/gitaly uses a
// limit of 1024, which corresponds to an average of 4 loose objects per folder.
// We can tune this parameter once we gain more experience.
var looseObjectsLimit, _ = strconv.Atoi(env.Get("SRC_GIT_LOOSE_OBJECTS_LIMIT", "1024", "the maximum number of loose objects we tolerate before we trigger a repack"))

// A failed sg maintenance run will place a log file in the git directory.
// Subsequent sg maintenance runs are skipped unless the log file is old.
//
// Based on how https://github.com/git/git handles the gc.log file.
var sgmLogExpire = env.MustGetDuration("SRC_GIT_LOG_FILE_EXPIRY", 24*time.Hour, "the number of hours after which sg maintenance runs even if a log file is present")

// Each failed sg maintenance run increments a counter in the sgmLog file.
// We reclone the repository if the number of retries exceeds sgmRetries.
// Setting SRC_SGM_RETRIES to -1 disables recloning due to sgm failures.
// Default value is 3 (reclone after 3 failed sgm runs).
//
// We mention this ENV variable in the header message of the sgmLog files. Make
// sure that changes here are reflected in sgmLogHeader, too.
var sgmRetries, _ = strconv.Atoi(env.Get("SRC_SGM_RETRIES", "3", "the maximum number of times we retry sg maintenance before triggering a reclone."))

// Controls if gitserver cleanup tries to remove repos from disk which are not defined in the DB. Defaults to false.
var removeNonExistingRepos, _ = strconv.ParseBool(env.Get("SRC_REMOVE_NON_EXISTING_REPOS", "false", "controls if gitserver cleanup tries to remove repos from disk which are not defined in the DB"))

var (
	reposRemoved = promauto.NewCounterVec(prometheus.CounterOpts{
		Name: "src_gitserver_repos_removed",
		Help: "number of repos removed during cleanup",
	}, []string{"reason"})
	reposRecloned = promauto.NewCounter(prometheus.CounterOpts{
		Name: "src_gitserver_repos_recloned",
		Help: "number of repos removed and re-cloned due to age",
	})
	reposRemovedDiskPressure = promauto.NewCounter(prometheus.CounterOpts{
		Name: "src_gitserver_repos_removed_disk_pressure",
		Help: "number of repos removed due to not enough disk space",
	})
	janitorRunning = promauto.NewGauge(prometheus.GaugeOpts{
		Name: "src_gitserver_janitor_running",
		Help: "set to 1 when the gitserver janitor background job is running",
	})
	jobTimer = promauto.NewHistogramVec(prometheus.HistogramOpts{
		Name:    "src_gitserver_janitor_job_duration_seconds",
		Help:    "Duration of the individual jobs within the gitserver janitor background job",
		Buckets: prometheus.ExponentialBucketsRange(0.001, 240, 16),
	}, []string{"success", "job_name"})
	maintenanceStatus = promauto.NewCounterVec(prometheus.CounterOpts{
		Name: "src_gitserver_maintenance_status",
		Help: "whether the maintenance run was a success (true/false) and the reason why a cleanup was needed",
	}, []string{"success", "reason"})
	pruneStatus = promauto.NewCounterVec(prometheus.CounterOpts{
		Name: "src_gitserver_prune_status",
		Help: "whether git prune was a success (true/false) and whether it was skipped (true/false)",
	}, []string{"success", "skipped"})
	janitorTimer = promauto.NewHistogram(prometheus.HistogramOpts{
		Name:    "src_gitserver_janitor_duration_seconds",
		Help:    "Duration of gitserver janitor background job",
		Buckets: []float64{0.1, 1, 10, 60, 300, 3600, 7200},
	})
	nonExistingReposRemoved = promauto.NewCounter(prometheus.CounterOpts{
		Name: "src_gitserver_non_existing_repos_removed",
		Help: "number of non existing repos removed during cleanup",
	})
)

// cleanupRepos walks the repos directory and performs maintenance tasks:
//
// 1. Compute the amount of space used by the repo
// 2. Remove corrupt repos.
// 3. Remove stale lock files.
// 4. Ensure correct git attributes
// 5. Ensure gc.auto=0 or unset depending on gitGCMode
// 6. Perform garbage collection
// 7. Re-clone repos after a while. (simulate git gc)
// 8. Remove repos based on disk pressure.
// 9. Perform sg-maintenance
// 10. Git prune
// 11. Set sizes of repos
func cleanupRepos(
	ctx context.Context,
	logger log.Logger,
	db database.DB,
	fs gitserverfs.FS,
	gitBackendSource git.GitBackendSource,
	rcf *wrexec.RecordingCommandFactory,
	shardID string,
	gitServerAddrs connection.GitserverAddresses,
	disableDeleteReposOnWrongShard bool,
) {
	logger = logger.Scoped("cleanup")

	start := time.Now()

	janitorRunning.Set(1)
	defer janitorRunning.Set(0)

	janitorStart := time.Now()
	defer func() {
		janitorTimer.Observe(time.Since(janitorStart).Seconds())
	}()

	knownGitServerShard := false
	for _, addr := range gitServerAddrs.Addresses {
		if hostnameMatch(shardID, addr) {
			knownGitServerShard = true
			break
		}
	}
	if !knownGitServerShard {
		logger.Warn("current shard is not included in the list of known gitserver shards, will not delete repos", log.String("current-hostname", shardID), log.Strings("all-shards", gitServerAddrs.Addresses))
	}

	repoToSize := make(map[api.RepoName]int64)
	var wrongShardRepoCount int64
	defer func() {
		// We want to set the gauge only at the end when we know the total
		wrongShardReposTotal.Set(float64(wrongShardRepoCount))
	}()

	var wrongShardReposDeleted int64
	defer func() {
		// We want to set the gauge only when wrong shard clean-up is enabled
		if disableDeleteReposOnWrongShard {
			wrongShardReposDeletedCounter.Add(float64(wrongShardReposDeleted))
		}
	}()

	maybeDeleteWrongShardRepos := func(backend git.GitBackend, repoName api.RepoName, dir common.GitDir) (done bool, err error) {
		// Record the number of repos that should not belong on this instance and
		// remove up to SRC_WRONG_SHARD_DELETE_LIMIT in a single Janitor run.
		addr := gitServerAddrs.AddrForRepo(ctx, repoName)

		if hostnameMatch(shardID, addr) {
			return false, nil
		}

		wrongShardRepoCount++

		// If we're on a shard not currently known, basically every repo would
		// be considered on the wrong shard. This is probably a configuration
		// error and we don't want to completely empty our disk in that case,
		// so skip.
		if !knownGitServerShard {
			return false, nil
		}

		// Check that wrong shard deletion has not been disabled.
		if disableDeleteReposOnWrongShard {
			return false, nil
		}

		logger.Info(
			"removing repo cloned on the wrong shard",
			log.String("dir", string(dir)),
			log.String("target-shard", addr),
			log.String("current-shard", shardID),
		)
		if err := fs.RemoveRepo(repoName); err != nil {
			return true, err
		}

		wrongShardReposDeleted++

		// Note: We just deleted the repo. So we're done with any further janitor tasks!
		return true, nil
	}

	collectSize := func(backend git.GitBackend, repoName api.RepoName, dir common.GitDir) (done bool, err error) {
		last, err := getLastSizeCalculation(dir)
		if err != nil {
			return false, err
		}

		if time.Since(last) < repoSizeRecalcInterval {
			// Don't recalculate.
			return false, nil
		}

		size, err := fs.DirSize(dir.Path())
		if err != nil {
			return false, errors.Wrap(err, "calculating repo size")
		}
		repoToSize[repoName] = size

		return false, setLastSizeCalculation(dir, time.Now())
	}

	maybeRemoveCorrupt := func(backend git.GitBackend, repoName api.RepoName, dir common.GitDir) (done bool, _ error) {
		corrupt, shouldLog, reason, err := checkRepoDirCorrupt(rcf, repoName, dir)
		if !corrupt || err != nil {
			return false, err
		}

		if shouldLog {
			err = db.GitserverRepos().LogCorruption(ctx, repoName, fmt.Sprintf("sourcegraph detected corrupt repo: %s", reason), shardID)
			if err != nil {
				logger.Error("failed to log repo corruption", log.String("repo", string(repoName)), log.Error(err))
			}
		}

		logger.Warn("removing corrupt repo", log.String("repo", string(dir)), log.String("reason", reason))

		if err := fs.RemoveRepo(repoName); err != nil {
			return true, err
		}

		reposRemoved.WithLabelValues(reason).Inc()

		// Set as not_cloned in the database.
		if err := db.GitserverRepos().SetCloneStatus(ctx, repoName, types.CloneStatusNotCloned, shardID); err != nil {
			return true, errors.Wrap(err, "failed to update clone status")
		}

		return true, nil
	}

	maybeRemoveNonExisting := func(backend git.GitBackend, repoName api.RepoName, dir common.GitDir) (bool, error) {
		if !removeNonExistingRepos {
			return false, nil
		}

		_, err := db.GitserverRepos().GetByName(ctx, repoName)
		// Repo still exists, nothing to do.
		if err == nil {
			return false, nil
		}

		// Failed to talk to DB, skip this repo.
		if !errcode.IsNotFound(err) {
			logger.Warn("failed to look up repo", log.Error(err), log.String("repo", string(repoName)))
			return false, nil
		}

		// The repo does not exist in the DB (or is soft-deleted), continue deleting it.
		// TODO: For soft-deleted, it might be nice to attempt to update the clone status,
		// but that can only work when we can map a repo on disk back to a repo in DB
		// when the name has been modified to have the DELETED- prefix.
		err = fs.RemoveRepo(repoName)
		if err == nil {
			nonExistingReposRemoved.Inc()
		}
		return true, err
	}

	ensureGitAttributes := func(backend git.GitBackend, repoName api.RepoName, dir common.GitDir) (done bool, err error) {
		return false, git.SetGitAttributes(dir)
	}

	ensureAutoGC := func(backend git.GitBackend, repoName api.RepoName, dir common.GitDir) (done bool, err error) {
		return false, gitSetAutoGC(ctx, backend.Config())
	}

	maybeReclone := func(backend git.GitBackend, repoName api.RepoName, dir common.GitDir) (done bool, err error) {
		var reason string

		// Check if we marked GC as failed and if so, if it's been too long.
		gcFailedAt, err := backend.Config().Get(ctx, gitConfigGCFailed)
		if err != nil {
			return false, errors.Wrap(err, "failed to read git gc fail time")
		}
		if gcFailedAt != "" {
			gcFailedAtInt, err := strconv.Atoi(gcFailedAt)
			if err != nil {
				return false, errors.Wrap(err, "failed to parse git gc fail time")
			}
			firstGCFailure := time.Unix(int64(gcFailedAtInt), 0)
			if time.Since(firstGCFailure) > repoTTLGC+jitterDuration(string(dir), repoTTLGC/4) {
				if gclog, err := os.ReadFile(dir.Path("gc.log")); err == nil && len(gclog) > 0 {
					reason = fmt.Sprintf("git gc %s", string(bytes.TrimSpace(gclog)))
				}
			}
		}

		if (sgmRetries >= 0) && (bestEffortReadFailed(dir) > sgmRetries) {
			if sgmLog, err := os.ReadFile(dir.Path(sgmLog)); err == nil && len(sgmLog) > 0 {
				reason = fmt.Sprintf("sg maintenance, too many retries: %s", string(bytes.TrimSpace(sgmLog)))
			}
		}

		if reason == "" {
			return false, nil
		}

		// We believe converting a Perforce depot to a Git repository is generally a
		// very expensive operation, therefore we do not try to re-clone/redo the
		// conversion only because it is old or slow to do "git gc".
		repoType, err := git.GetRepositoryType(ctx, backend.Config())
		if err != nil {
			return false, err
		}
		if repoType == "perforce" {
			return false, nil
		}

		recloneLogger := logger.With(
			log.String("repo", string(repoName)),
			log.String("reason", reason),
		)

		recloneLogger.Info("re-cloning repo after GC failures")

		// We trigger a reclone by removing the repo from disk and marking it as
		// uncloned in the DB. The reclone will then be performed as if this repo
		// was newly added to Sourcegraph.
		// This will make the repo inaccessible for a bit, but we consider the
		// repo completely broken at this stage anways.
		if err := fs.RemoveRepo(repoName); err != nil {
			return true, errors.Wrap(err, "failed to remove repo")
		}
		// Set as not_cloned in the database.
		if err := db.GitserverRepos().SetCloneStatus(ctx, repoName, types.CloneStatusNotCloned, shardID); err != nil {
			return true, errors.Wrap(err, "failed to update clone status")
		}

		reposRecloned.Inc()

		return true, nil
	}

	removeStaleLocks := func(backend git.GitBackend, repoName api.RepoName, gitDir common.GitDir) (done bool, err error) {
		// if removing a lock fails, we still want to try the other locks.
		var multi error

		// config.lock should be held for a very short amount of time.
		if _, err := removeFileOlderThan(logger, gitDir.Path("config.lock"), time.Minute); err != nil {
			multi = errors.Append(multi, err)
		}
		// packed-refs can be held for quite a while, so we are conservative
		// with the age.
		if _, err := removeFileOlderThan(logger, gitDir.Path("packed-refs.lock"), time.Hour); err != nil {
			multi = errors.Append(multi, err)
		}
		// we use the same conservative age for locks inside of refs
		if err := gitserverfs.BestEffortWalk(gitDir.Path("refs"), func(path string, fi os.DirEntry) error {
			if fi.IsDir() {
				return nil
			}

			if !strings.HasSuffix(path, ".lock") {
				return nil
			}

			_, err := removeFileOlderThan(logger, path, time.Hour)
			return err
		}); err != nil {
			multi = errors.Append(multi, err)
		}
		// We have seen that, occasionally, commit-graph.locks prevent a git repack from
		// succeeding. Benchmarks on our dogfood cluster have shown that a commit-graph
		// call for a 5GB bare repository takes less than 1 min. The lock is only held
		// during a short period during this time. A 1-hour grace period is very
		// conservative.
		if _, err := removeFileOlderThan(logger, gitDir.Path("objects", "info", "commit-graph.lock"), time.Hour); err != nil {
			multi = errors.Append(multi, err)
		}

		// gc.pid is set by git gc and our sg maintenance script. 24 hours is twice the
		// time git gc uses internally.
		gcPIDMaxAge := 24 * time.Hour
		if foundStale, err := removeFileOlderThan(logger, gitDir.Path(gcLockFile), gcPIDMaxAge); err != nil {
			multi = errors.Append(multi, err)
		} else if foundStale {
			logger.Warn(
				"removeStaleLocks found a stale gc.pid lockfile and removed it. This should not happen and points to a problem with garbage collection. Monitor the repo for possible corruption and verify if this error reoccurs",
				log.String("path", string(gitDir)),
				log.Duration("age", gcPIDMaxAge))
		}

		// drop temporary pack files that can be left behind by a fetch.
		packdir := gitDir.Path("objects", "pack")
		err = gitserverfs.BestEffortWalk(packdir, func(path string, d os.DirEntry) error {
			if path != packdir && d.IsDir() {
				return filepath.SkipDir
			}

			file := filepath.Base(path)
			if !strings.HasPrefix(file, "tmp_pack_") {
				return nil
			}

			_, err := removeFileOlderThan(logger, path, 2*conf.GitLongCommandTimeout())
			return err
		})
		if err != nil {
			multi = errors.Append(multi, err)
		}

		return false, multi
	}

	performGC := func(backend git.GitBackend, repoName api.RepoName, dir common.GitDir) (done bool, err error) {
		return false, gitGC(ctx, logger, backend, rcf, repoName, dir)
	}

	performSGMaintenance := func(backend git.GitBackend, repoName api.RepoName, dir common.GitDir) (done bool, err error) {
		return false, sgMaintenance(logger, dir)
	}

	performGitPrune := func(backend git.GitBackend, repoName api.RepoName, dir common.GitDir) (done bool, err error) {
		return false, pruneIfNeeded(rcf, repoName, dir, looseObjectsLimit)
	}

	type cleanupFn struct {
		Name string
		Do   func(git.GitBackend, api.RepoName, common.GitDir) (bool, error)
	}
	cleanups := []cleanupFn{
		// First, check if we should even be having this repo on disk anymore,
		// maybe there's been a resharding event and we can actually remove it
		// and not spend further CPU cycles fixing it.
		{"delete wrong shard repos", maybeDeleteWrongShardRepos},
		// Do some sanity checks on the repository.
		{"maybe remove corrupt", maybeRemoveCorrupt},
		// Remove repo if DB does not contain it anymore
		{"maybe remove non existing", maybeRemoveNonExisting},
		// If git is interrupted it can leave lock files lying around. It does not clean
		// these up, and instead fails commands.
		{"remove stale locks", removeStaleLocks},
		// We always want to have the same git attributes file at info/attributes.
		{"ensure git attributes", ensureGitAttributes},
		// Enable or disable background garbage collection depending on
		// gitGCMode. The purpose is to avoid repository corruption which can
		// happen if several git-gc operations are running at the same time.
		// We only disable if sg is managing gc.
		{"auto gc config", ensureAutoGC},
	}

	if gitGCMode == gitGCModeJanitorAutoGC {
		// Runs a number of housekeeping tasks within the current repository, such as
		// compressing file revisions (to reduce disk space and increase performance),
		// removing unreachable objects which may have been created from prior
		// invocations of git add, packing refs, pruning reflog, rerere metadata or stale
		// working trees. May also update ancillary indexes such as the commit-graph.
		cleanups = append(cleanups, cleanupFn{"garbage collect", performGC})
	}

	if gitGCMode == gitGCModeMaintenance {
		// Run tasks to optimize Git repository data, speeding up other Git commands and
		// reducing storage requirements for the repository. Note: "garbage collect" and
		// "sg maintenance" must not be enabled at the same time.
		cleanups = append(cleanups, cleanupFn{"sg maintenance", performSGMaintenance})
		cleanups = append(cleanups, cleanupFn{"git prune", performGitPrune})
	}

	if !conf.Get().DisableAutoGitUpdates {
		// Old git clones accumulate loose git objects that waste space and slow down git
		// operations. Periodically do a fresh clone to avoid these problems. git gc is
		// slow and resource intensive. It is cheaper and faster to just re-clone the
		// repository. We don't do this if DisableAutoGitUpdates is set as it could
		// potentially kick off a clone operation.
		cleanups = append(cleanups, cleanupFn{
			Name: "maybe re-clone",
			Do:   maybeReclone,
		})
	}

	// Compute the amount of space used by the repo. We do this last, because
	// we want it to reflect the improvements that previous GC methods had.
	cleanups = append(cleanups, cleanupFn{"compute stats", collectSize})

	reposCleaned := 0

	err := fs.ForEachRepo(func(repo api.RepoName, gitDir common.GitDir) (done bool) {
		backend := gitBackendSource(gitDir, repo)
		for _, cfn := range cleanups {
			// Check if context has been canceled, if so skip the rest of the repos.
			select {
			case <-ctx.Done():
				logger.Warn("aborting janitor run", log.Error(ctx.Err()))
				return true
			default:
			}

			start := time.Now()
			done, err := cfn.Do(backend, repo, gitDir)
			if err != nil {
				logger.Error("error running cleanup command",
					log.String("name", cfn.Name),
					log.String("repo", string(gitDir)),
					log.Error(err))
			}
			jobTimer.WithLabelValues(strconv.FormatBool(err == nil), cfn.Name).Observe(time.Since(start).Seconds())
			if done {
				break
			}
		}

		reposCleaned++

		// Every 1000 repos, log a progress message.
		if reposCleaned%1000 == 0 {
			logger.Info("Janitor progress", log.Int("repos_cleaned", reposCleaned))
		}

		return false
	})
	if err != nil {
		logger.Error("error iterating over repositories", log.Error(err))
	}

	if len(repoToSize) > 0 {
		_, err := db.GitserverRepos().UpdateRepoSizes(ctx, logger, shardID, repoToSize)
		if err != nil {
			logger.Error("setting repo sizes", log.Error(err))
		}
	}

	logger.Info("Janitor run finished", log.String("duration", time.Since(start).String()))
}

func checkRepoDirCorrupt(rcf *wrexec.RecordingCommandFactory, repoName api.RepoName, dir common.GitDir) (corrupt, shouldLog bool, description string, err error) {
	// We treat repositories missing HEAD to be corrupt. Both our cloning
	// and fetching ensure there is a HEAD file.
	if _, err := os.Stat(dir.Path("HEAD")); os.IsNotExist(err) {
		return true, true, "missing-head", nil
	} else if err != nil {
		return false, false, "", err
	}

	// We have seen repository corruption fail in such a way that the git
	// config is missing the bare repo option but everything else looks
	// like it works. This leads to failing fetches, so treat non-bare
	// repos as corrupt. Since we often fetch with ensureRevision, this
	// leads to most commands failing against the repository. It is safer
	// to remove now than try a safe reclone.
	if gitIsNonBareBestEffort(rcf, repoName, dir) {
		return true, true, "non-bare", nil
	}

	if maybeCorrupt, err := checkRepoFlaggedForCorruption(dir); err != nil {
		return false, false, "", err
	} else if maybeCorrupt {
		// Repo corruption has already been logged on the git CLI side, we don't
		// need to log it again.
		return true, false, "failed-odb-read", nil
	}

	return false, false, "", nil
}

func checkRepoFlaggedForCorruption(gitDir common.GitDir) (bool, error) {
	p := gitDir.Path(gitcli.RepoMaybeCorruptFlagFilepath)

	_, err := os.Stat(p)
	if err != nil && !os.IsNotExist(err) {
		return false, err
	}

	// Nothing logged, the repo isn't corrupted.
	if os.IsNotExist(err) {
		return false, nil
	}
	// Best effort remove the flag, since we will now attempt to recover from it.
	// If that doesn't work, the next run will flag it again, and we can try to
	// repair it again.
	_ = os.Remove(p)

	return true, nil
}

// howManyBytesToFree returns the number of bytes that should be freed to make sure
// there is sufficient disk space free to satisfy s.DesiredPercentFree.
func howManyBytesToFree(logger log.Logger, usage diskusage.DiskUsage, desiredPercentFree int) int64 {
	actualFreeBytes := usage.Free()

	// Free up space if necessary.
	diskSizeBytes := usage.Size()
	desiredFreeBytes := uint64(float64(desiredPercentFree) / 100.0 * float64(diskSizeBytes))
	howManyBytesToFree := int64(desiredFreeBytes - actualFreeBytes)
	if howManyBytesToFree < 0 {
		howManyBytesToFree = 0
	}
	const G = float64(1024 * 1024 * 1024)

	logger.Debug(
		"howManyBytesToFree",
		log.Int("desired percent free", desiredPercentFree),
		log.Float64("actual percent free", float64(actualFreeBytes)/float64(diskSizeBytes)*100.0),
		log.Float64("amount to free in GiB", float64(howManyBytesToFree)/G),
	)

	return howManyBytesToFree
}

// freeUpSpace removes git directories under the fs, in order from least
// recently to most recently used, until it has freed howManyBytesToFree.
func freeUpSpace(ctx context.Context, logger log.Logger, db database.DB, fs gitserverfs.FS, shardID string, usage diskusage.DiskUsage, desiredPercentFree int, howManyBytesToFree int64) error {
	if howManyBytesToFree <= 0 {
		return nil
	}

	logger = logger.Scoped("freeUpSpace")

	// Get the git directories and their mod times.
	gitDirs, err := findGitDirs(fs)
	if err != nil {
		return errors.Wrap(err, "finding git dirs")
	}
	dirModTimes := make(map[common.GitDir]time.Time, len(gitDirs))
	for _, d := range gitDirs {
		mt, err := gitDirModTime(d)
		if err != nil {
			// If we get an error here, we move it to the end of the queue,
			// since it's the janitor's job to clean/fix this.
			logger.Warn("computing mod time of git dir failed", log.String("dir", string(d)), log.Error(err))
			dirModTimes[d] = time.Now()
			continue
		}
		dirModTimes[d] = mt
	}

	// Sort the repos from least to most recently used.
	sort.Slice(gitDirs, func(i, j int) bool {
		return dirModTimes[gitDirs[i]].Before(dirModTimes[gitDirs[j]])
	})

	// Remove repos until howManyBytesToFree is met or exceeded.
	var spaceFreed int64
	diskSizeBytes := usage.Size()
	for _, d := range gitDirs {
		if spaceFreed >= howManyBytesToFree {
			return nil
		}

		// Fast-exit if the context has been canceled.
		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
		}

		repoName := fs.ResolveRepoName(d)

		delta, err := fs.DirSize(d.Path())
		if err != nil {
			logger.Warn("failed to get dir size", log.String("dir", string(d)), log.Error(err))
			continue
		}
		if err := fs.RemoveRepo(repoName); err != nil {
			logger.Warn("failed to remove least recently used repo", log.String("dir", string(d)), log.Error(err))
			continue
		}
		// Set as not_cloned in the database.
		if err := db.GitserverRepos().SetCloneStatus(ctx, repoName, types.CloneStatusNotCloned, shardID); err != nil {
			logger.Warn("failed to update clone status", log.Error(err))
		}
		spaceFreed += delta
		reposRemovedDiskPressure.Inc()

		// Report the new disk usage situation after removing this repo.
		usage, err := fs.DiskUsage()
		if err != nil {
			return errors.Wrap(err, "finding the amount of space free on disk")
		}
		actualFreeBytes := usage.Free()
		G := float64(1024 * 1024 * 1024)

		logger.Warn("removed least recently used repo",
			log.String("repo", string(d)),
			log.Duration("how old", time.Since(dirModTimes[d])),
			log.Float64("free space in GiB", float64(actualFreeBytes)/G),
			log.Float64("actual percent of disk space free", float64(actualFreeBytes)/float64(diskSizeBytes)*100.0),
			log.Float64("desired percent of disk space free", float64(desiredPercentFree)),
			log.Float64("space freed in GiB", float64(spaceFreed)/G),
			log.Float64("how much space to free in GiB", float64(howManyBytesToFree)/G))
	}

	// Check.
	if spaceFreed < howManyBytesToFree {
		return errors.Errorf("only freed %d bytes, wanted to free %d", spaceFreed, howManyBytesToFree)
	}
	return nil
}

func gitDirModTime(d common.GitDir) (time.Time, error) {
	head, err := os.Stat(d.Path("HEAD"))
	if err != nil {
		return time.Time{}, errors.Wrap(err, "getting repository modification time")
	}
	return head.ModTime(), nil
}

// findGitDirs collects the GitDirs of all repos in the FS.
func findGitDirs(fs gitserverfs.FS) ([]common.GitDir, error) {
	var dirs []common.GitDir
	return dirs, fs.ForEachRepo(func(_ api.RepoName, dir common.GitDir) (done bool) {
		dirs = append(dirs, dir)
		return false
	})
}

// gitIsNonBareBestEffort returns true if the repository is not a bare
// repo. If we fail to check or the repository is bare we return false.
//
// Note: it is not always possible to check if a repository is bare since a
// lock file may prevent the check from succeeding. We only want bare
// repositories and want to avoid transient false positives.
func gitIsNonBareBestEffort(rcf *wrexec.RecordingCommandFactory, repoName api.RepoName, dir common.GitDir) bool {
	cmd := exec.Command("git", "-C", dir.Path(), "rev-parse", "--is-bare-repository")
	dir.Set(cmd)
	wrappedCmd := rcf.WrapWithRepoName(context.Background(), log.NoOp(), repoName, cmd)
	b, _ := wrappedCmd.Output()
	b = bytes.TrimSpace(b)
	return bytes.Equal(b, []byte("false"))
}

const gitConfigGCFailed = "sourcegraph.gcFailedAt"

// gitGC will invoke `git-gc` to clean up any garbage in the repo. It will
// operate synchronously and be aggressive with its internal heuristics when
// deciding to act (meaning it will act now at lower thresholds).
func gitGC(ctx context.Context, logger log.Logger, backend git.GitBackend, rcf *wrexec.RecordingCommandFactory, repoName api.RepoName, dir common.GitDir) error {
	cmd := exec.Command("git", "-c", "gc.auto=1", "-c", "gc.autoDetach=false", "gc", "--auto")
	dir.Set(cmd)
	wrappedCmd := rcf.WrapWithRepoName(context.Background(), log.NoOp(), repoName, cmd)
	err := wrappedCmd.Run()
	if err != nil {
		if gclog, readErr := os.ReadFile(dir.Path("gc.log")); readErr == nil && len(gclog) > 0 {
			// gc failed most likely.
			logger.Error("git gc failed", log.String("repo", string(dir)), log.Error(err), log.String("gc.log", string(gclog)))
			existing, err := backend.Config().Get(ctx, gitConfigGCFailed)
			if err != nil {
				logger.Error("failed to read gitConfigGCFailed config", log.Error(err))
			} else if existing == "" { // Not set yet.
				if err := backend.Config().Set(ctx, gitConfigGCFailed, strconv.Itoa(int(time.Now().Unix()))); err != nil {
					logger.Error("failed to set gitConfigGCFailed config", log.Error(err))
				}
			}
		}

		return errors.Wrapf(executil.WrapCmdError(cmd, err), "failed to git-gc")
	}

	// This run of git gc was a success, reset the failedAt config.
	return backend.Config().Unset(ctx, gitConfigGCFailed)
}

const (
	sgmLogPrefix = "failed="

	sgmLogHeader = `DO NOT EDIT: generated by gitserver.
This file records the number of failed runs of sg maintenance and the
last error message. The number of failed attempts is compared to the
number of allowed retries (see SRC_SGM_RETRIES) to decide whether a
repository should be recloned.`
)

// writeSGMLog writes a log file with the format
//
//	<header>
//
//	<sgmLogPrefix>=<int>
//
//	<error message>
func writeSGMLog(dir common.GitDir, m []byte) error {
	return os.WriteFile(
		dir.Path(sgmLog),
		[]byte(fmt.Sprintf("%s\n\n%s%d\n\n%s\n", sgmLogHeader, sgmLogPrefix, bestEffortReadFailed(dir)+1, m)),
		0600,
	)
}

func bestEffortReadFailed(dir common.GitDir) int {
	b, err := os.ReadFile(dir.Path(sgmLog))
	if err != nil {
		return 0
	}

	return bestEffortParseFailed(b)
}

func bestEffortParseFailed(b []byte) int {
	prefix := []byte(sgmLogPrefix)
	from := bytes.Index(b, prefix)
	if from < 0 {
		return 0
	}

	b = b[from+len(prefix):]
	if to := bytes.IndexByte(b, '\n'); to > 0 {
		b = b[:to]
	}

	n, _ := strconv.Atoi(string(b))
	return n
}

// sgMaintenance runs a set of git cleanup tasks in dir. This must not be run
// concurrently with git gc. sgMaintenance will check the state of the repository
// to avoid running the cleanup tasks if possible. If a sgmLog file is present in
// dir, sgMaintenance will not run unless the file is old.
func sgMaintenance(logger log.Logger, dir common.GitDir) (err error) {
	// Don't run if sgmLog file is younger than sgmLogExpire hours. There is no need
	// to report an error, because the error has already been logged in a previous
	// run.
	if fi, err := os.Stat(dir.Path(sgmLog)); err == nil {
		if fi.ModTime().After(time.Now().Add(-sgmLogExpire)) {
			return nil
		}
	}
	needed, reason, err := needsMaintenance(dir)
	defer func() {
		maintenanceStatus.WithLabelValues(strconv.FormatBool(err == nil), reason).Inc()
	}()
	if err != nil {
		return err
	}
	if !needed {
		return nil
	}

	cmd := exec.Command("sh")
	dir.Set(cmd)

	cmd.Stdin = strings.NewReader(sgMaintenanceScript)

	err, unlock := lockRepoForGC(dir)
	if err != nil {
		logger.Debug(
			"could not lock repository for sg maintenance",
			log.String("dir", string(dir)),
			log.Error(err),
		)
		return nil
	}
	defer func() { _ = unlock() }()

	b, err := cmd.CombinedOutput()
	if err != nil {
		if err := writeSGMLog(dir, b); err != nil {
			logger.Debug("sg maintenance failed to write log file", log.String("file", dir.Path(sgmLog)), log.Error(err))
		}
		logger.Debug("sg maintenance", log.String("dir", string(dir)), log.String("out", string(b)))
		return errors.Wrapf(executil.WrapCmdError(cmd, err), "failed to run sg maintenance")
	}
	// Remove the log file after a successful run.
	_ = os.Remove(dir.Path(sgmLog))
	return nil
}

const gcLockFile = "gc.pid"

func lockRepoForGC(dir common.GitDir) (error, func() error) {
	// Setting permissions to 644 to mirror the permissions that git gc sets for gc.pid.
	f, err := os.OpenFile(dir.Path(gcLockFile), os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0644)
	if err != nil {
		content, err1 := os.ReadFile(dir.Path(gcLockFile))
		if err1 != nil {
			return err, nil
		}
		pidMachine := strings.Split(string(content), " ")
		if len(pidMachine) < 2 {
			return err, nil
		}
		return errors.Wrapf(err, "process %s on machine %s is already running a gc operation", pidMachine[0], pidMachine[1]), nil
	}

	// We cut the hostname to 256 bytes, just like git gc does. See HOST_NAME_MAX in
	// github.com/git/git.
	name := hostname.Get()
	hostNameMax := 256
	if len(name) > hostNameMax {
		name = name[0:hostNameMax]
	}

	_, err = fmt.Fprintf(f, "%d %s", os.Getpid(), name)
	if err1 := f.Close(); err1 != nil && err == nil {
		err = err1
	}

	return err, func() error {
		return os.Remove(dir.Path(gcLockFile))
	}
}

// We run git-prune only if there are enough loose objects. This approach is
// adapted from https://gitlab.com/gitlab-org/gitaly.
func pruneIfNeeded(rcf *wrexec.RecordingCommandFactory, repo api.RepoName, dir common.GitDir, limit int) (err error) {
	needed, err := tooManyLooseObjects(dir, limit)
	defer func() {
		pruneStatus.WithLabelValues(strconv.FormatBool(err == nil), strconv.FormatBool(!needed)).Inc()
	}()
	if err != nil {
		return err
	}
	if !needed {
		return nil
	}

	// "--expire now" will remove all unreachable, loose objects from the store. The
	// default setting is 2 weeks. We choose a more aggressive setting because
	// unreachable, loose objects count towards the threshold that triggers a
	// repack. In the worst case, IE all loose objects are unreachable, we would
	// continuously trigger repacks until the loose objects expire.
	cmd := exec.Command("git", "prune", "--expire", "now")
	dir.Set(cmd)
	wrappedCmd := rcf.WrapWithRepoName(context.Background(), log.NoOp(), repo, cmd)
	err = wrappedCmd.Run()
	if err != nil {
		return errors.Wrapf(executil.WrapCmdError(cmd, err), "failed to git-prune")
	}
	return nil
}

func needsMaintenance(dir common.GitDir) (bool, string, error) {
	// Bitmaps store reachability information about the set of objects in a
	// packfile which speeds up clone and fetch operations.
	hasBm, err := hasBitmap(dir)
	if err != nil {
		return false, "", err
	}
	if !hasBm {
		return true, "bitmap", nil
	}

	// The commit-graph file is a supplemental data structure that accelerates
	// commit graph walks triggered EG by git-log.
	hasCg, err := hasCommitGraph(dir)
	if err != nil {
		return false, "", err
	}
	if !hasCg {
		return true, "commit_graph", nil
	}

	tooManyPf, err := tooManyPackfiles(dir, autoPackLimit)
	if err != nil {
		return false, "", err
	}
	if tooManyPf {
		return true, "packfiles", nil
	}

	tooManyLO, err := tooManyLooseObjects(dir, looseObjectsLimit)
	if err != nil {
		return false, "", err
	}
	if tooManyLO {
		return tooManyLO, "loose_objects", nil
	}
	return false, "skipped", nil
}

var reHexadecimal = lazyregexp.New("^[0-9a-f]+$")

// tooManyLooseObjects follows Git's approach of estimating the number of
// loose objects by counting the objects in a sentinel folder and extrapolating
// based on the assumption that loose objects are randomly distributed in the
// 256 possible folders.
func tooManyLooseObjects(dir common.GitDir, limit int) (bool, error) {
	// We use the same folder git uses to estimate the number of loose objects.
	objs, err := os.ReadDir(filepath.Join(dir.Path(), "objects", "17"))
	if err != nil {
		if errors.Is(err, fs.ErrNotExist) {
			return false, nil
		}
		return false, errors.Wrap(err, "tooManyLooseObjects")
	}

	count := 0
	for _, obj := range objs {
		// Git checks if the file names are hexadecimal and that they have the right
		// length depending on the chosen hash algorithm. Since the hash algorithm might
		// change over time, checking the length seems too brittle. Instead, we just
		// count all files with hexadecimal names.
		if obj.IsDir() {
			continue
		}
		if matches := reHexadecimal.MatchString(obj.Name()); !matches {
			continue
		}
		count++
	}
	return count*256 > limit, nil
}

func hasBitmap(dir common.GitDir) (bool, error) {
	bitmaps, err := filepath.Glob(dir.Path("objects", "pack", "*.bitmap"))
	if err != nil {
		return false, err
	}
	return len(bitmaps) > 0, nil
}

func hasCommitGraph(dir common.GitDir) (bool, error) {
	if _, err := os.Stat(dir.Path("objects", "info", "commit-graph")); err == nil {
		return true, nil
	} else if errors.Is(err, fs.ErrNotExist) {
		return false, nil
	} else {
		return false, err
	}
}

// tooManyPackfiles counts the packfiles in objects/pack. Packfiles with an
// accompanying .keep file are ignored.
func tooManyPackfiles(dir common.GitDir, limit int) (bool, error) {
	packs, err := filepath.Glob(dir.Path("objects", "pack", "*.pack"))
	if err != nil {
		return false, err
	}
	count := 0
	for _, p := range packs {
		// Because we know p has the extension .pack, we can slice it off directly
		// instead of using strings.TrimSuffix and filepath.Ext. Benchmarks showed that
		// this option is 20x faster than strings.TrimSuffix(file, filepath.Ext(file))
		// and 17x faster than file[:strings.LastIndex(file, ".")]. However, the runtime
		// of all options is dominated by adding the extension ".keep".
		keepFile := p[:len(p)-5] + ".keep"
		if _, err := os.Stat(keepFile); err == nil {
			continue
		}
		count++
	}
	return count > limit, nil
}

// jitterDuration returns a duration between [0, d) based on key. This is like
// a random duration, but instead of a random source it is computed via a hash
// on key.
func jitterDuration(key string, d time.Duration) time.Duration {
	h := fnv.New64()
	_, _ = io.WriteString(h, key)
	r := time.Duration(h.Sum64())
	if r < 0 {
		// +1 because we have one more negative value than positive. ie
		// math.MinInt64 == -math.MinInt64.
		r = -(r + 1)
	}
	return r % d
}

// removeFileOlderThan removes path if its mtime is older than maxAge. If the
// file is missing, no error is returned. The first argument indicates whether a
// stale file was present.
func removeFileOlderThan(logger log.Logger, path string, maxAge time.Duration) (bool, error) {
	fi, err := os.Stat(filepath.Clean(path))
	if err != nil {
		if os.IsNotExist(err) {
			return false, nil
		}
		return false, err
	}

	age := time.Since(fi.ModTime())
	if age < maxAge {
		return false, nil
	}

	logger.Debug("removing stale lock file", log.String("path", path), log.Duration("age", age))
	err = os.Remove(path)
	if err != nil && !os.IsNotExist(err) {
		return true, err
	}
	return true, nil
}

func mockRemoveNonExistingReposConfig(value bool) {
	removeNonExistingRepos = value
}

const (
	// We recalculate the repository size every day at most in the janitor.
	// There's no need to recalculate it more often than that, since fetches
	// update it anyways. So this will only be useful for "housekeeping" purposes
	// in case we don't fetch for a while and the size on disk changes.
	repoSizeRecalcInterval = 24 * time.Hour
)

const lastSizeCalculationFilepath = ".sourcegraph-last-size-calculation"

// getLastSizeCalculation returns the time the repository size was last calculated
// by the janitor. We don't set this timestamp in fetches, so janitor still runs
// this task every now and then.
func getLastSizeCalculation(dir common.GitDir) (time.Time, error) {
	path := dir.Path(lastSizeCalculationFilepath)
	fd, err := os.Stat(path)
	if err != nil {
		// If the file doesn't exist yet, we return a zero time.
		if os.IsNotExist(err) {
			return time.Time{}, nil
		}
		// Stat failed, error.
		return time.Time{}, err
	}
	// We use modtime to track the last time fetched.
	return fd.ModTime(), nil
}

func setLastSizeCalculation(dir common.GitDir, when time.Time) error {
	path := dir.Path(lastSizeCalculationFilepath)
	f, err := os.Create(path)
	if err != nil && !os.IsExist(err) {
		// If creating the file failed, and not because it already exists, error.
		return err
	}
	_ = f.Close()
	// We use modtime to track the last time fetched.
	return os.Chtimes(path, time.Time{}, when)
}
