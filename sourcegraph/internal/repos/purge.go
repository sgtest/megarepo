package repos

import (
	"context"
	"math/rand"
	"os"
	"strconv"
	"time"

	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/protocol"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// RunRepositoryPurgeWorker is a worker which deletes repos which are present
// on gitserver, but not enabled/present in our repos table.
func RunRepositoryPurgeWorker(ctx context.Context, db database.DB) {
	log := log15.Root().New("worker", "repo-purge")

	// Temporary escape hatch if this feature proves to be dangerous
	if disabled, _ := strconv.ParseBool(os.Getenv("DISABLE_REPO_PURGE")); disabled {
		log.Info("repository purger is disabled via env DISABLE_REPO_PURGE")
		return
	}

	for {
		// We only run in a 1-hour period on the weekend. During normal
		// working hours a migration or admin could accidentally remove all
		// repositories. Recloning all of them is slow, so we drastically
		// reduce the chance of this happening by only purging at a weird time
		// to be configuring Sourcegraph.
		if isSaturdayNight(time.Now()) {
			err := purge(ctx, db, log)
			if err != nil {
				log.Error("failed to run repository clone purge", "error", err)
			}
		}
		randSleep(10*time.Minute, time.Minute)
	}
}

func purge(ctx context.Context, db database.DB, log log15.Logger) error {
	start := time.Now()
	gitserverClient := gitserver.NewClient(db)
	var (
		total   int
		success int
		failed  int
	)

	err := database.GitserverRepos(db).IteratePurgeableRepos(ctx, time.Time{}, func(repo api.RepoName) error {
		total++
		repo = protocol.NormalizeRepo(repo)
		if err := gitserverClient.Remove(ctx, repo); err != nil {
			// Do not fail at this point, just log so we can remove other repos.
			log.Warn("failed to remove repository", "repo", repo, "error", err)
			purgeFailed.Inc()
			failed++
			return nil
		}
		success++
		purgeSuccess.Inc()
		return nil
	})
	// If we did something we log with a higher level.
	statusLogger := log.Info
	if failed > 0 {
		statusLogger = log.Warn
	}
	statusLogger("repository cloned purge finished", "total", total, "removed", success, "failed", failed, "duration", time.Since(start))
	if err != nil {
		return errors.Wrap(err, "iterating purgeable repos")
	}

	return nil
}

func isSaturdayNight(t time.Time) bool {
	// According to The Cure, 10:15 Saturday Night you should be sitting in your
	// kitchen sink, not adjusting your external service configuration.
	return t.Format("Mon 15") == "Sat 22"
}

// randSleep will sleep for an expected d duration with a jitter in [-jitter /
// 2, jitter / 2].
func randSleep(d, jitter time.Duration) {
	delta := time.Duration(rand.Int63n(int64(jitter))) - (jitter / 2)
	time.Sleep(d + delta)
}
