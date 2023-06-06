package own

import (
	"context"
	"os"
	"sort"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/hexops/autogold/v2"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/log/logtest"

	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/own/codeowners"
	codeownerspb "github.com/sourcegraph/sourcegraph/enterprise/internal/own/codeowners/v1"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/own/types"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	itypes "github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

const (
	repoOwnerID          = 71
	srcMainOwnerID       = 72
	srcMainSecondOwnerID = 73
	srcMainJavaOwnerID   = 74
	assignerID           = 76
	repoID               = 41
)

type repoPath struct {
	Repo     api.RepoName
	CommitID api.CommitID
	Path     string
}

// repoFiles is a fake git client mapping a file
type repoFiles map[repoPath]string

func (fs repoFiles) ReadFile(_ context.Context, _ authz.SubRepoPermissionChecker, repoName api.RepoName, commitID api.CommitID, file string) ([]byte, error) {
	content, ok := fs[repoPath{Repo: repoName, CommitID: commitID, Path: file}]
	if !ok {
		return nil, os.ErrNotExist
	}
	return []byte(content), nil
}

func TestOwnersServesFilesAtVariousLocations(t *testing.T) {
	codeownersText := codeowners.NewRuleset(
		codeowners.IngestedRulesetSource{},
		&codeownerspb.File{
			Rule: []*codeownerspb.Rule{
				{
					Pattern: "README.md",
					Owner:   []*codeownerspb.Owner{{Email: "owner@example.com"}},
				},
			},
		},
	).Repr()
	for name, repo := range map[string]repoFiles{
		"top-level": {{"repo", "SHA", "CODEOWNERS"}: codeownersText},
		".github":   {{"repo", "SHA", ".github/CODEOWNERS"}: codeownersText},
		".gitlab":   {{"repo", "SHA", ".gitlab/CODEOWNERS"}: codeownersText},
	} {
		t.Run(name, func(t *testing.T) {
			git := gitserver.NewMockClient()
			git.ReadFileFunc.SetDefaultHook(repo.ReadFile)

			codeownersStore := edb.NewMockCodeownersStore()
			codeownersStore.GetCodeownersForRepoFunc.SetDefaultReturn(nil, nil)
			db := edb.NewMockEnterpriseDB()
			db.CodeownersFunc.SetDefaultReturn(codeownersStore)

			got, err := NewService(git, db).RulesetForRepo(context.Background(), "repo", 1, "SHA")
			require.NoError(t, err)
			assert.Equal(t, codeownersText, got.Repr())
		})
	}
}

func TestOwnersCannotFindFile(t *testing.T) {
	codeownersFile := codeowners.NewRuleset(
		codeowners.IngestedRulesetSource{},
		&codeownerspb.File{
			Rule: []*codeownerspb.Rule{
				{
					Pattern: "README.md",
					Owner:   []*codeownerspb.Owner{{Email: "owner@example.com"}},
				},
			},
		},
	)
	repo := repoFiles{
		{"repo", "SHA", "notCODEOWNERS"}: codeownersFile.Repr(),
	}
	git := gitserver.NewMockClient()
	git.ReadFileFunc.SetDefaultHook(repo.ReadFile)

	codeownersStore := edb.NewMockCodeownersStore()
	codeownersStore.GetCodeownersForRepoFunc.SetDefaultReturn(nil, edb.CodeownersFileNotFoundError{})
	db := edb.NewMockEnterpriseDB()
	db.CodeownersFunc.SetDefaultReturn(codeownersStore)

	got, err := NewService(git, db).RulesetForRepo(context.Background(), "repo", 1, "SHA")
	require.NoError(t, err)
	assert.Nil(t, got)
}

func TestOwnersServesIngestedFile(t *testing.T) {
	t.Run("return manually ingested codeowners file", func(t *testing.T) {
		codeownersProto := &codeownerspb.File{
			Rule: []*codeownerspb.Rule{
				{
					Pattern: "README.md",
					Owner:   []*codeownerspb.Owner{{Email: "owner@example.com"}},
				},
			},
		}
		codeownersText := codeowners.NewRuleset(codeowners.IngestedRulesetSource{}, codeownersProto).Repr()

		git := gitserver.NewMockClient()

		codeownersStore := edb.NewMockCodeownersStore()
		codeownersStore.GetCodeownersForRepoFunc.SetDefaultReturn(&types.CodeownersFile{
			Proto: codeownersProto,
		}, nil)
		db := edb.NewMockEnterpriseDB()
		db.CodeownersFunc.SetDefaultReturn(codeownersStore)

		got, err := NewService(git, db).RulesetForRepo(context.Background(), "repo", 1, "SHA")
		require.NoError(t, err)
		assert.Equal(t, codeownersText, got.Repr())
	})
	t.Run("file not found and codeowners file does not exist return nil", func(t *testing.T) {
		git := gitserver.NewMockClient()
		git.ReadFileFunc.SetDefaultReturn(nil, nil)

		codeownersStore := edb.NewMockCodeownersStore()
		codeownersStore.GetCodeownersForRepoFunc.SetDefaultReturn(nil, edb.CodeownersFileNotFoundError{})
		db := edb.NewMockEnterpriseDB()
		db.CodeownersFunc.SetDefaultReturn(codeownersStore)

		got, err := NewService(git, db).RulesetForRepo(context.Background(), "repo", 1, "SHA")
		require.NoError(t, err)
		require.Nil(t, got)
	})
}

func TestResolveOwnersWithType(t *testing.T) {
	t.Run("no owners returns empty", func(t *testing.T) {
		git := gitserver.NewMockClient()
		got, err := NewService(git, database.NewMockDB()).ResolveOwnersWithType(context.Background(), nil)
		require.NoError(t, err)
		assert.Empty(t, got)
	})
	t.Run("no user or team match returns unknown owner", func(t *testing.T) {
		git := gitserver.NewMockClient()
		mockUserStore := database.NewMockUserStore()
		mockTeamStore := database.NewMockTeamStore()
		db := database.NewMockDB()
		db.UsersFunc.SetDefaultReturn(mockUserStore)
		db.UserEmailsFunc.SetDefaultReturn(database.NewMockUserEmailsStore())
		db.TeamsFunc.SetDefaultReturn(mockTeamStore)
		ownService := NewService(git, db)

		mockUserStore.GetByUsernameFunc.SetDefaultReturn(nil, database.MockUserNotFoundErr)
		mockTeamStore.GetTeamByNameFunc.SetDefaultReturn(nil, database.TeamNotFoundError{})
		owners := []*codeownerspb.Owner{
			{Handle: "unknown"},
		}

		got, err := ownService.ResolveOwnersWithType(context.Background(), owners)
		require.NoError(t, err)
		assert.Equal(t, []codeowners.ResolvedOwner{
			newTestUnknownOwner("unknown", ""),
		}, got)
	})
	t.Run("user match from handle returns person owner", func(t *testing.T) {
		git := gitserver.NewMockClient()
		mockUserStore := database.NewMockUserStore()
		mockTeamStore := database.NewMockTeamStore()
		db := database.NewMockDB()
		db.UsersFunc.SetDefaultReturn(mockUserStore)
		db.UserEmailsFunc.SetDefaultReturn(database.NewMockUserEmailsStore())
		db.TeamsFunc.SetDefaultReturn(mockTeamStore)
		ownService := NewService(git, db)

		handle := "person"
		testUser := newTestUser(handle)
		mockUserStore.GetByUsernameFunc.PushReturn(testUser, nil)
		mockTeamStore.GetTeamByNameFunc.SetDefaultReturn(nil, errors.New("I'm panicking because I should not be called"))
		owners := []*codeownerspb.Owner{
			{Handle: handle},
		}

		got, err := ownService.ResolveOwnersWithType(context.Background(), owners)
		require.NoError(t, err)
		assert.Equal(t, []codeowners.ResolvedOwner{
			&codeowners.Person{
				User:   testUser,
				Handle: handle,
			},
		}, got)
	})
	t.Run("user match from email returns person owner", func(t *testing.T) {
		mockUserStore := database.NewMockUserStore()
		mockTeamStore := database.NewMockTeamStore()
		db := database.NewMockDB()
		db.UsersFunc.SetDefaultReturn(mockUserStore)
		db.UserEmailsFunc.SetDefaultReturn(database.NewMockUserEmailsStore())
		db.TeamsFunc.SetDefaultReturn(mockTeamStore)
		ownService := NewService(gitserver.NewMockClient(), db)

		email := "person@sourcegraph.com"
		testUser := newTestUser("person")
		mockUserStore.GetByVerifiedEmailFunc.PushReturn(testUser, nil)
		owners := []*codeownerspb.Owner{
			{Email: email},
		}

		got, err := ownService.ResolveOwnersWithType(context.Background(), owners)
		require.NoError(t, err)
		assert.Equal(t, []codeowners.ResolvedOwner{
			&codeowners.Person{
				User:  testUser,
				Email: email,
			},
		}, got)
	})
	t.Run("team match from handle returns team owner", func(t *testing.T) {
		mockUserStore := database.NewMockUserStore()
		mockTeamStore := database.NewMockTeamStore()
		db := database.NewMockDB()
		db.UsersFunc.SetDefaultReturn(mockUserStore)
		db.UserEmailsFunc.SetDefaultReturn(database.NewMockUserEmailsStore())
		db.TeamsFunc.SetDefaultReturn(mockTeamStore)
		ownService := NewService(gitserver.NewMockClient(), db)

		handle := "team"
		testTeam := newTestTeam(handle)
		mockUserStore.GetByUsernameFunc.PushReturn(nil, database.MockUserNotFoundErr)
		mockTeamStore.GetTeamByNameFunc.PushReturn(testTeam, nil)
		owners := []*codeownerspb.Owner{
			{Handle: handle},
		}

		got, err := ownService.ResolveOwnersWithType(context.Background(), owners)
		require.NoError(t, err)
		assert.Equal(t, []codeowners.ResolvedOwner{
			&codeowners.Team{
				Team:   testTeam,
				Handle: handle,
			},
		}, got)
	})
	t.Run("team match from handle with slash", func(t *testing.T) {
		mockUserStore := database.NewMockUserStore()
		mockTeamStore := database.NewMockTeamStore()
		db := database.NewMockDB()
		db.UsersFunc.SetDefaultReturn(mockUserStore)
		db.TeamsFunc.SetDefaultReturn(mockTeamStore)

		handle := "team/handle"
		testTeam := newTestTeam("handle")
		mockUserStore.GetByUsernameFunc.PushReturn(nil, database.MockUserNotFoundErr)
		mockTeamStore.GetTeamByNameFunc.SetDefaultHook(func(ctx context.Context, handle string) (*itypes.Team, error) {
			if handle == "handle" {
				return testTeam, nil
			}
			return nil, database.TeamNotFoundError{}
		})
		owners := []*codeownerspb.Owner{
			{Handle: handle},
		}
		t.Run("best effort matching", func(t *testing.T) {
			ownService := NewService(gitserver.NewMockClient(), db)
			got, err := ownService.ResolveOwnersWithType(context.Background(), owners)
			require.NoError(t, err)
			assert.Equal(t, []codeowners.ResolvedOwner{
				&codeowners.Team{
					Team:   testTeam,
					Handle: handle,
				},
			}, got)
		})
		t.Run("early stop", func(t *testing.T) {
			ownService := NewService(gitserver.NewMockClient(), db)
			bestEffort := false
			conf.Get().OwnBestEffortTeamMatching = &bestEffort
			t.Cleanup(func() {
				conf.Get().OwnBestEffortTeamMatching = nil
			})
			got, err := ownService.ResolveOwnersWithType(context.Background(), owners)
			require.NoError(t, err)
			assert.Equal(t, []codeowners.ResolvedOwner{
				newTestUnknownOwner(handle, ""),
			}, got)
		})
	})
	t.Run("no user match from email returns unknown owner", func(t *testing.T) {
		mockUserStore := database.NewMockUserStore()
		mockTeamStore := database.NewMockTeamStore()
		db := database.NewMockDB()
		db.UsersFunc.SetDefaultReturn(mockUserStore)
		db.UserEmailsFunc.SetDefaultReturn(database.NewMockUserEmailsStore())
		db.TeamsFunc.SetDefaultReturn(mockTeamStore)
		ownService := NewService(gitserver.NewMockClient(), db)

		email := "superman"
		mockUserStore.GetByVerifiedEmailFunc.PushReturn(nil, database.MockUserNotFoundErr)
		owners := []*codeownerspb.Owner{
			{Email: email},
		}

		got, err := ownService.ResolveOwnersWithType(context.Background(), owners)
		require.NoError(t, err)
		assert.Equal(t, []codeowners.ResolvedOwner{
			newTestUnknownOwner("", email),
		}, got)
	})
	t.Run("mix of person, team, and unknown matches", func(t *testing.T) {
		mockUserStore := database.NewMockUserStore()
		mockTeamStore := database.NewMockTeamStore()
		db := database.NewMockDB()
		db.UsersFunc.SetDefaultReturn(mockUserStore)
		db.UserEmailsFunc.SetDefaultReturn(database.NewMockUserEmailsStore())
		db.TeamsFunc.SetDefaultReturn(mockTeamStore)
		ownService := NewService(gitserver.NewMockClient(), db)

		userHandle := "userWithHandle"
		userEmail := "userWithEmail"
		teamHandle := "teamWithHandle"
		unknownOwnerEmail := "plato@sourcegraph.com"

		testUserWithHandle := newTestUser(userHandle)
		testUserWithEmail := newTestUser(userEmail)
		testTeamWithHandle := newTestTeam(teamHandle)
		testUnknownOwner := newTestUnknownOwner("", unknownOwnerEmail)

		mockUserStore.GetByUsernameFunc.SetDefaultHook(func(ctx context.Context, username string) (*itypes.User, error) {
			if username == userHandle {
				return testUserWithHandle, nil
			}
			return nil, database.MockUserNotFoundErr
		})
		mockUserStore.GetByVerifiedEmailFunc.SetDefaultHook(func(ctx context.Context, email string) (*itypes.User, error) {
			if email == userEmail {
				return testUserWithEmail, nil
			}
			return nil, database.MockUserNotFoundErr
		})
		mockTeamStore.GetTeamByNameFunc.SetDefaultHook(func(ctx context.Context, name string) (*itypes.Team, error) {
			if name == teamHandle {
				return testTeamWithHandle, nil
			}
			return nil, database.TeamNotFoundError{}
		})

		owners := []*codeownerspb.Owner{
			{Email: userEmail},
			{Handle: userHandle},
			{Email: unknownOwnerEmail},
			{Handle: teamHandle},
		}

		got, err := ownService.ResolveOwnersWithType(context.Background(), owners)
		require.NoError(t, err)
		want := []codeowners.ResolvedOwner{
			&codeowners.Person{User: testUserWithHandle, Handle: userHandle},
			&codeowners.Person{User: testUserWithEmail, Email: userEmail},
			&codeowners.Team{Team: testTeamWithHandle, Handle: teamHandle},
			testUnknownOwner,
		}
		sort.Slice(want, func(x, j int) bool {
			return want[x].Identifier() < want[j].Identifier()
		})
		sort.Slice(got, func(x, j int) bool {
			return got[x].Identifier() < got[j].Identifier()
		})
		assert.Equal(t, want, got)
	})
	t.Run("makes use of cache", func(t *testing.T) {
		mockUserStore := database.NewMockUserStore()
		mockTeamStore := database.NewMockTeamStore()
		db := database.NewMockDB()
		db.UsersFunc.SetDefaultReturn(mockUserStore)
		db.UserEmailsFunc.SetDefaultReturn(database.NewMockUserEmailsStore())
		db.TeamsFunc.SetDefaultReturn(mockTeamStore)
		ownService := NewService(gitserver.NewMockClient(), db)

		email := "person@sourcegraph.com"
		testUser := newTestUser("person")
		mockUserStore.GetByVerifiedEmailFunc.PushReturn(testUser, nil)
		mockUserStore.GetByVerifiedEmailFunc.PushReturn(nil, errors.New("should have been cached"))
		owners := []*codeownerspb.Owner{
			{Email: email},
		}

		got, err := ownService.ResolveOwnersWithType(context.Background(), owners)
		require.NoError(t, err)
		assert.Equal(t, []codeowners.ResolvedOwner{
			&codeowners.Person{
				User:  testUser,
				Email: email,
			},
		}, got)
		// do it again
		got, err = ownService.ResolveOwnersWithType(context.Background(), owners)
		require.NoError(t, err)
		assert.Equal(t, []codeowners.ResolvedOwner{
			&codeowners.Person{
				User:  testUser,
				Email: email,
			},
		}, got)
	})
	t.Run("errors", func(t *testing.T) {
		mockUserStore := database.NewMockUserStore()
		mockTeamStore := database.NewMockTeamStore()
		db := database.NewMockDB()
		db.UsersFunc.SetDefaultReturn(mockUserStore)
		db.UserEmailsFunc.SetDefaultReturn(database.NewMockUserEmailsStore())
		db.TeamsFunc.SetDefaultReturn(mockTeamStore)
		ownService := NewService(gitserver.NewMockClient(), db)

		email := "person@sourcegraph.com"
		var myError = errors.New("you shall not pass")
		mockUserStore.GetByVerifiedEmailFunc.PushReturn(nil, myError)
		owners := []*codeownerspb.Owner{
			{Email: email},
		}

		got, err := ownService.ResolveOwnersWithType(context.Background(), owners)
		require.Error(t, err)
		assert.ErrorIs(t, err, myError)
		assert.Empty(t, got)
	})
	t.Run("no errors if no handle or email", func(t *testing.T) {
		mockUserStore := database.NewMockUserStore()
		mockTeamStore := database.NewMockTeamStore()
		db := database.NewMockDB()
		db.UsersFunc.SetDefaultReturn(mockUserStore)
		db.UserEmailsFunc.SetDefaultReturn(database.NewMockUserEmailsStore())
		db.TeamsFunc.SetDefaultReturn(mockTeamStore)
		ownService := NewService(gitserver.NewMockClient(), db)

		owners := []*codeownerspb.Owner{
			{},
		}

		got, err := ownService.ResolveOwnersWithType(context.Background(), owners)
		require.NoError(t, err)
		assert.Empty(t, got)
	})
}

func newTestUser(username string) *itypes.User {
	return &itypes.User{
		ID:          1,
		Username:    username,
		AvatarURL:   "https://sourcegraph.com/avatar/" + username,
		DisplayName: "User " + username,
	}
}

func newTestTeam(teamName string) *itypes.Team {
	return &itypes.Team{
		ID:          1,
		Name:        teamName,
		DisplayName: "Team " + teamName,
	}
}

// an unknown owner is just a person with no user set
func newTestUnknownOwner(handle, email string) codeowners.ResolvedOwner {
	return &codeowners.Person{
		Handle: handle,
		Email:  email,
	}
}

func Test_getLastPartOfTeamHandle(t *testing.T) {
	testCases := []struct {
		name   string
		handle string
		want   autogold.Value
	}{
		{
			name:   "empty string",
			handle: "",
			want:   autogold.Expect(""),
		},
		{
			name:   "single character",
			handle: "x",
			want:   autogold.Expect("x"),
		},
		{
			name:   "single slash",
			handle: "team/name",
			want:   autogold.Expect("name"),
		},
		{
			name:   "two slashes",
			handle: "double/team/name",
			want:   autogold.Expect("name"),
		},
		{
			name:   "ends with a slash",
			handle: "double/team/name/",
			want:   autogold.Expect(""),
		},
		{
			name:   "double slash",
			handle: "//",
			want:   autogold.Expect(""),
		},
	}
	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			tc.want.Equal(t, getLastPartOfTeamHandle(tc.handle))
		})
	}
}

func TestAssignedOwners(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	t.Parallel()
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Creating 2 users.
	user1, err := db.Users().Create(ctx, database.NewUser{Username: "user1"})
	require.NoError(t, err)
	user2, err := db.Users().Create(ctx, database.NewUser{Username: "user2"})
	require.NoError(t, err)

	// Create repo
	var repoID api.RepoID = 1
	require.NoError(t, db.Repos().Create(ctx, &itypes.Repo{
		ID:   repoID,
		Name: "github.com/sourcegraph/sourcegraph",
	}))

	store := db.AssignedOwners()
	require.NoError(t, store.Insert(ctx, user1.ID, repoID, "src/test", user2.ID))
	require.NoError(t, store.Insert(ctx, user2.ID, repoID, "src/test", user1.ID))
	require.NoError(t, store.Insert(ctx, user2.ID, repoID, "src/main", user1.ID))

	s := NewService(nil, db)
	var exampleCommitID api.CommitID = "sha"
	got, err := s.AssignedOwnership(ctx, repoID, exampleCommitID)
	// Erase the time for comparison
	for _, summaries := range got {
		for i := range summaries {
			summaries[i].AssignedAt = time.Time{}
		}
	}
	require.NoError(t, err)
	want := AssignedOwners{
		"src/test": []database.AssignedOwnerSummary{
			{
				OwnerUserID:       user1.ID,
				FilePath:          "src/test",
				RepoID:            repoID,
				WhoAssignedUserID: user2.ID,
			},
			{
				OwnerUserID:       user2.ID,
				FilePath:          "src/test",
				RepoID:            repoID,
				WhoAssignedUserID: user1.ID,
			},
		},
		"src/main": []database.AssignedOwnerSummary{
			{
				OwnerUserID:       user2.ID,
				FilePath:          "src/main",
				RepoID:            repoID,
				WhoAssignedUserID: user1.ID,
			},
		},
	}
	if diff := cmp.Diff(want, got); diff != "" {
		t.Fatalf("AssignedOwnership -want+got: %s", diff)
	}
}

func TestAssignedOwnersMatch(t *testing.T) {
	var (
		repoOwner = database.AssignedOwnerSummary{
			OwnerUserID:       repoOwnerID,
			FilePath:          "",
			RepoID:            repoID,
			WhoAssignedUserID: assignerID,
		}
		srcMainOwner = database.AssignedOwnerSummary{
			OwnerUserID:       srcMainOwnerID,
			FilePath:          "src/main",
			RepoID:            repoID,
			WhoAssignedUserID: assignerID,
		}
		srcMainSecondOwner = database.AssignedOwnerSummary{
			OwnerUserID:       srcMainSecondOwnerID,
			FilePath:          "src/main",
			RepoID:            repoID,
			WhoAssignedUserID: assignerID,
		}
		srcMainJavaOwner = database.AssignedOwnerSummary{
			OwnerUserID:       srcMainJavaOwnerID,
			FilePath:          "src/main/java",
			RepoID:            repoID,
			WhoAssignedUserID: assignerID,
		}
		srcTestOwner = database.AssignedOwnerSummary{
			OwnerUserID:       srcMainJavaOwnerID,
			FilePath:          "src/test",
			RepoID:            repoID,
			WhoAssignedUserID: assignerID,
		}
	)
	owners := AssignedOwners{
		"": []database.AssignedOwnerSummary{
			repoOwner,
		},
		"src/main": []database.AssignedOwnerSummary{
			srcMainOwner,
			srcMainSecondOwner,
		},
		"src/main/java": []database.AssignedOwnerSummary{
			srcMainJavaOwner,
		},
		"src/test": []database.AssignedOwnerSummary{
			srcTestOwner,
		},
	}
	order := func(os []database.AssignedOwnerSummary) {
		sort.Slice(os, func(i, j int) bool {
			if os[i].OwnerUserID < os[j].OwnerUserID {
				return true
			}
			if os[i].FilePath < os[j].FilePath {
				return true
			}
			return false
		})
	}
	for _, testCase := range []struct {
		path string
		want []database.AssignedOwnerSummary
	}{
		{
			path: "",
			want: []database.AssignedOwnerSummary{
				repoOwner,
			},
		},
		{
			path: "resources/pom.xml",
			want: []database.AssignedOwnerSummary{
				repoOwner,
			},
		},
		{
			path: "src/main",
			want: []database.AssignedOwnerSummary{
				repoOwner,
				srcMainOwner,
				srcMainSecondOwner,
			},
		},
		{
			path: "src/main/java/com/sourcegraph/GitServer.java",
			want: []database.AssignedOwnerSummary{
				repoOwner,
				srcMainOwner,
				srcMainSecondOwner,
				srcMainJavaOwner,
			},
		},
		{
			path: "src/test/java/com/sourcegraph/GitServerTest.java",
			want: []database.AssignedOwnerSummary{
				repoOwner,
				srcTestOwner,
			},
		},
	} {
		got := owners.Match(testCase.path)
		order(got)
		order(testCase.want)
		if diff := cmp.Diff(testCase.want, got); diff != "" {
			t.Errorf("path: %q, unexpected owners (-want+got): %s", testCase.path, diff)
		}
	}
}

func TestAssignedTeams(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	t.Parallel()
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Creating a user and 2 teams.
	user1, err := db.Users().Create(ctx, database.NewUser{Username: "user1"})
	require.NoError(t, err)
	team1 := createTeam(t, ctx, db, "team-a")
	team2 := createTeam(t, ctx, db, "team-a2")

	// Create repo
	var repoID api.RepoID = 1
	require.NoError(t, db.Repos().Create(ctx, &itypes.Repo{
		ID:   repoID,
		Name: "github.com/sourcegraph/sourcegraph",
	}))

	store := db.AssignedTeams()
	require.NoError(t, store.Insert(ctx, team1.ID, repoID, "src/test", user1.ID))
	require.NoError(t, store.Insert(ctx, team2.ID, repoID, "src/test", user1.ID))
	require.NoError(t, store.Insert(ctx, team2.ID, repoID, "src/main", user1.ID))

	s := NewService(nil, db)
	var exampleCommitID api.CommitID = "sha"
	got, err := s.AssignedTeams(ctx, repoID, exampleCommitID)
	// Erase the time for comparison
	for _, summaries := range got {
		for i := range summaries {
			summaries[i].AssignedAt = time.Time{}
		}
	}
	require.NoError(t, err)
	want := AssignedTeams{
		"src/test": []database.AssignedTeamSummary{
			{
				OwnerTeamID:       team1.ID,
				FilePath:          "src/test",
				RepoID:            repoID,
				WhoAssignedUserID: user1.ID,
			},
			{
				OwnerTeamID:       team2.ID,
				FilePath:          "src/test",
				RepoID:            repoID,
				WhoAssignedUserID: user1.ID,
			},
		},
		"src/main": []database.AssignedTeamSummary{
			{
				OwnerTeamID:       team2.ID,
				FilePath:          "src/main",
				RepoID:            repoID,
				WhoAssignedUserID: user1.ID,
			},
		},
	}
	if diff := cmp.Diff(want, got); diff != "" {
		t.Fatalf("AssignedTeams -want+got: %s", diff)
	}
}

func TestAssignedTeamsMatch(t *testing.T) {
	var (
		repoOwner = database.AssignedTeamSummary{
			OwnerTeamID:       repoOwnerID,
			FilePath:          "",
			RepoID:            repoID,
			WhoAssignedUserID: assignerID,
		}
		srcMainOwner = database.AssignedTeamSummary{
			OwnerTeamID:       srcMainOwnerID,
			FilePath:          "src/main",
			RepoID:            repoID,
			WhoAssignedUserID: assignerID,
		}
		srcMainSecondOwner = database.AssignedTeamSummary{
			OwnerTeamID:       srcMainSecondOwnerID,
			FilePath:          "src/main",
			RepoID:            repoID,
			WhoAssignedUserID: assignerID,
		}
		srcMainJavaOwner = database.AssignedTeamSummary{
			OwnerTeamID:       srcMainJavaOwnerID,
			FilePath:          "src/main/java",
			RepoID:            repoID,
			WhoAssignedUserID: assignerID,
		}
		srcTestOwner = database.AssignedTeamSummary{
			OwnerTeamID:       srcMainJavaOwnerID,
			FilePath:          "src/test",
			RepoID:            repoID,
			WhoAssignedUserID: assignerID,
		}
	)
	owners := AssignedTeams{
		"": []database.AssignedTeamSummary{
			repoOwner,
		},
		"src/main": []database.AssignedTeamSummary{
			srcMainOwner,
			srcMainSecondOwner,
		},
		"src/main/java": []database.AssignedTeamSummary{
			srcMainJavaOwner,
		},
		"src/test": []database.AssignedTeamSummary{
			srcTestOwner,
		},
	}
	order := func(os []database.AssignedTeamSummary) {
		sort.Slice(os, func(i, j int) bool {
			if os[i].OwnerTeamID < os[j].OwnerTeamID {
				return true
			}
			if os[i].FilePath < os[j].FilePath {
				return true
			}
			return false
		})
	}
	for _, testCase := range []struct {
		path string
		want []database.AssignedTeamSummary
	}{
		{
			path: "",
			want: []database.AssignedTeamSummary{
				repoOwner,
			},
		},
		{
			path: "resources/pom.xml",
			want: []database.AssignedTeamSummary{
				repoOwner,
			},
		},
		{
			path: "src/main",
			want: []database.AssignedTeamSummary{
				repoOwner,
				srcMainOwner,
				srcMainSecondOwner,
			},
		},
		{
			path: "src/main/java/com/sourcegraph/GitServer.java",
			want: []database.AssignedTeamSummary{
				repoOwner,
				srcMainOwner,
				srcMainSecondOwner,
				srcMainJavaOwner,
			},
		},
		{
			path: "src/test/java/com/sourcegraph/GitServerTest.java",
			want: []database.AssignedTeamSummary{
				repoOwner,
				srcTestOwner,
			},
		},
	} {
		got := owners.Match(testCase.path)
		order(got)
		order(testCase.want)
		if diff := cmp.Diff(testCase.want, got); diff != "" {
			t.Errorf("path: %q, unexpected owners (-want+got): %s", testCase.path, diff)
		}
	}
}

func createTeam(t *testing.T, ctx context.Context, db database.DB, teamName string) *itypes.Team {
	t.Helper()
	err := db.Teams().CreateTeam(ctx, &itypes.Team{Name: teamName})
	require.NoError(t, err)
	team, err := db.Teams().GetTeamByName(ctx, teamName)
	require.NoError(t, err)
	return team
}
