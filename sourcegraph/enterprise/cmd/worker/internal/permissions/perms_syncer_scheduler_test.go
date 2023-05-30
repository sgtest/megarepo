package permissions

import (
	"context"
	"fmt"
	"sync/atomic"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/log"
	"github.com/sourcegraph/log/logtest"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/worker/shared/permissions"
	"github.com/stretchr/testify/require"

	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/schema"
)

func addPerms(t *testing.T, s edb.PermsStore, userID, repoID int32) {
	t.Helper()

	ctx := context.Background()

	_, err := s.SetUserExternalAccountPerms(ctx, authz.UserIDWithExternalAccountID{UserID: userID, ExternalAccountID: userID - 1}, []int32{repoID}, authz.SourceUserSync)
	require.NoError(t, err)
}

func TestPermsSyncerScheduler_scheduleJobs(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	permissions.ZeroBackoffDuringTest = true
	t.Cleanup(func() {
		conf.Mock(nil)
		permissions.ZeroBackoffDuringTest = false
	})

	ctx := context.Background()
	logger := logtest.Scoped(t)

	t.Run("schedule jobs", func(t *testing.T) {
		t.Helper()

		db := database.NewDB(logger, dbtest.NewDB(logger, t))

		store := database.PermissionSyncJobsWith(logger, db)
		usersStore := database.UsersWith(logger, db)
		externalAccountStore := database.ExternalAccountsWith(logger, db)
		reposStore := database.ReposWith(logger, db)
		permsStore := edb.Perms(logger, db, clock)

		// Creating site-admin.
		_, err := usersStore.Create(ctx, database.NewUser{Username: "admin"})
		require.NoError(t, err)

		// Creating non-private repo.
		nonPrivateRepo := types.Repo{Name: "test-public-repo"}
		err = reposStore.Create(ctx, &nonPrivateRepo)
		require.NoError(t, err)

		// We should have no jobs scheduled
		runJobsTest(t, ctx, logger, db, store, []testJob{})

		// Creating a user.
		user1, err := usersStore.Create(ctx, database.NewUser{Username: "test-user-1"})
		require.NoError(t, err)

		// Creating an external account
		err = externalAccountStore.Insert(ctx, user1.ID, extsvc.AccountSpec{ServiceType: "test", ServiceID: "test", AccountID: user1.Username}, extsvc.AccountData{})
		require.NoError(t, err)

		// Creating a repo.
		repo1 := types.Repo{Name: "test-repo-1", Private: true}
		err = reposStore.Create(ctx, &repo1)
		require.NoError(t, err)

		// We should have 2 jobs scheduled.
		wantJobs := []testJob{
			{
				UserID:       int(user1.ID),
				RepositoryID: 0,
				Reason:       database.ReasonUserNoPermissions,
				Priority:     database.MediumPriorityPermissionsSync,
				NoPerms:      true,
			},
			{
				UserID:       0,
				RepositoryID: int(repo1.ID),
				Reason:       database.ReasonRepoNoPermissions,
				Priority:     database.MediumPriorityPermissionsSync,
				NoPerms:      true,
			},
		}
		runJobsTest(t, ctx, logger, db, store, wantJobs)

		// Add permissions for user and repo
		addPerms(t, permsStore, user1.ID, int32(repo1.ID))

		// We should have same 2 jobs because jobs with higher priority already exists.
		runJobsTest(t, ctx, logger, db, store, wantJobs)

		// Creating a user.
		user2, err := usersStore.Create(ctx, database.NewUser{Username: "test-user-2"})
		require.NoError(t, err)

		// Creating an external account
		err = externalAccountStore.Insert(ctx, user2.ID, extsvc.AccountSpec{ServiceType: "test", ServiceID: "test", AccountID: user2.Username}, extsvc.AccountData{})
		require.NoError(t, err)

		// Creating a repo.
		repo2 := types.Repo{Name: "test-repo-2", Private: true}
		err = reposStore.Create(ctx, &repo2)
		require.NoError(t, err)

		// Add permissions and sync jobs for the user and repo.
		addPerms(t, permsStore, user2.ID, int32(repo2.ID))
		store.CreateUserSyncJob(ctx, user2.ID, database.PermissionSyncJobOpts{
			Priority: database.LowPriorityPermissionsSync,
			Reason:   database.ReasonUserOutdatedPermissions,
		})
		store.CreateRepoSyncJob(ctx, repo2.ID, database.PermissionSyncJobOpts{
			Priority: database.LowPriorityPermissionsSync,
			Reason:   database.ReasonRepoOutdatedPermissions,
		})

		// We should have 4 jobs scheduled including new jobs for user2 and repo2.
		wantJobs = []testJob{
			{
				UserID:       int(user1.ID),
				RepositoryID: 0,
				Reason:       database.ReasonUserNoPermissions,
				Priority:     database.MediumPriorityPermissionsSync,
				NoPerms:      true,
			},
			{
				UserID:       0,
				RepositoryID: int(repo1.ID),
				Reason:       database.ReasonRepoNoPermissions,
				Priority:     database.MediumPriorityPermissionsSync,
				NoPerms:      true,
			},
			{
				UserID:       int(user2.ID),
				RepositoryID: 0,
				Reason:       database.ReasonUserOutdatedPermissions,
				Priority:     database.LowPriorityPermissionsSync,
			},
			{
				UserID:       0,
				RepositoryID: int(repo2.ID),
				Reason:       database.ReasonRepoOutdatedPermissions,
				Priority:     database.LowPriorityPermissionsSync,
			},
		}
		runJobsTest(t, ctx, logger, db, store, wantJobs)

		// Set user1 and repo1 schedule jobs to completed.
		_, err = db.ExecContext(ctx, fmt.Sprintf(`UPDATE permission_sync_jobs SET state = 'completed' WHERE user_id=%d OR repository_id=%d`, user1.ID, repo1.ID))
		require.NoError(t, err)

		// We should have 4 jobs including new jobs for user1 and repo1.
		wantJobs = []testJob{
			{
				UserID:       int(user2.ID),
				RepositoryID: 0,
				Reason:       database.ReasonUserOutdatedPermissions,
				Priority:     database.LowPriorityPermissionsSync,
			},
			{
				UserID:       0,
				RepositoryID: int(repo2.ID),
				Reason:       database.ReasonRepoOutdatedPermissions,
				Priority:     database.LowPriorityPermissionsSync,
			},
			{
				UserID:       int(user1.ID),
				RepositoryID: 0,
				Reason:       database.ReasonUserOutdatedPermissions,
				Priority:     database.LowPriorityPermissionsSync,
			},
			{
				UserID:       0,
				RepositoryID: int(repo1.ID),
				Reason:       database.ReasonRepoOutdatedPermissions,
				Priority:     database.LowPriorityPermissionsSync,
			},
		}
		runJobsTest(t, ctx, logger, db, store, wantJobs)
	})
}

type testJob struct {
	Reason       database.PermissionsSyncJobReason
	ProcessAfter time.Time
	RepositoryID int
	UserID       int
	Priority     database.PermissionsSyncJobPriority
	NoPerms      bool
}

func runJobsTest(t *testing.T, ctx context.Context, logger log.Logger, db database.DB, store database.PermissionSyncJobStore, wantJobs []testJob) {
	count, err := scheduleJobs(ctx, db, logger)
	require.NoError(t, err)
	require.Equal(t, len(wantJobs), count)

	jobs, err := store.List(ctx, database.ListPermissionSyncJobOpts{State: database.PermissionsSyncJobStateQueued})
	require.NoError(t, err)
	require.Len(t, jobs, len(wantJobs))

	actualJobs := []testJob{}

	for _, job := range jobs {
		actualJob := testJob{
			UserID:       job.UserID,
			RepositoryID: job.RepositoryID,
			Reason:       job.Reason,
			Priority:     job.Priority,
			NoPerms:      job.NoPerms,
		}
		actualJobs = append(actualJobs, actualJob)
	}

	if diff := cmp.Diff(wantJobs, actualJobs); diff != "" {
		t.Fatal(diff)
	}
}

var now = timeutil.Now().UnixNano()

func clock() time.Time {
	return time.Unix(0, atomic.LoadInt64(&now))
}

func toIntPtr(n int) *int { return &n }

func TestOldestUserPermissionsBatchSize(t *testing.T) {
	t.Cleanup(func() { conf.Mock(nil) })

	tests := []struct {
		name      string
		configure *int
		want      int
	}{
		{
			name: "default",
			want: 10,
		},
		{
			name:      "uses number from config",
			configure: toIntPtr(5),
			want:      5,
		},
		{
			name:      "can be set to 0",
			configure: toIntPtr(0),
			want:      0,
		},
		{
			name:      "negative numbers result in default",
			configure: toIntPtr(-248),
			want:      10,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			conf.Mock(&conf.Unified{SiteConfiguration: schema.SiteConfiguration{
				PermissionsSyncOldestUsers: test.configure,
			}})
			require.Equal(t, oldestUserPermissionsBatchSize(), test.want)
		})
	}
}

func TestOldestRepoPermissionsBatchSize(t *testing.T) {
	t.Cleanup(func() { conf.Mock(nil) })

	tests := []struct {
		name      string
		configure *int
		want      int
	}{
		{
			name: "default",
			want: 10,
		},
		{
			name:      "uses number from config",
			configure: toIntPtr(5),
			want:      5,
		},
		{
			name:      "can be set to 0",
			configure: toIntPtr(0),
			want:      0,
		},
		{
			name:      "negative numbers result in default",
			configure: toIntPtr(-248),
			want:      10,
		},
	}

	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			conf.Mock(&conf.Unified{SiteConfiguration: schema.SiteConfiguration{
				PermissionsSyncOldestRepos: test.configure,
			}})
			require.Equal(t, oldestRepoPermissionsBatchSize(), test.want)
		})
	}
}
