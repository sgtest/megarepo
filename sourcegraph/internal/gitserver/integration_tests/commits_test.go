package inttests

import (
	"context"
	"fmt"
	"net/http"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
)

func TestGetCommits(t *testing.T) {
	t.Parallel()
	ctx := actor.WithActor(context.Background(), &actor.Actor{
		UID: 1,
	})
	db := database.NewMockDB()
	gr := database.NewMockGitserverRepoStore()
	db.GitserverReposFunc.SetDefaultReturn(gr)

	repo1 := MakeGitRepository(t, getGitCommandsWithFiles("file1", "file2")...)
	repo2 := MakeGitRepository(t, getGitCommandsWithFiles("file3", "file4")...)
	repo3 := MakeGitRepository(t, getGitCommandsWithFiles("file5", "file6")...)

	repoCommits := []api.RepoCommit{
		{Repo: repo1, CommitID: api.CommitID("HEAD")},                                     // HEAD (file2)
		{Repo: repo1, CommitID: api.CommitID("HEAD~1")},                                   // HEAD~1 (file1)
		{Repo: repo2, CommitID: api.CommitID("67762ad757dd26cac4145f2b744fd93ad10a48e0")}, // HEAD (file4)
		{Repo: repo2, CommitID: api.CommitID("2b988222e844b570959a493f5b07ec020b89e122")}, // HEAD~1 (file3)
		{Repo: repo3, CommitID: api.CommitID("01bed0a")},                                  // abbrev HEAD (file6)
		{Repo: repo3, CommitID: api.CommitID("unresolvable")},                             // unresolvable
		{Repo: api.RepoName("unresolvable"), CommitID: api.CommitID("deadbeef")},          // unresolvable
	}

	t.Run("basic", func(t *testing.T) {
		expectedCommits := []*gitdomain.Commit{
			{
				ID:        "2ba4dd2b9a27ec125fea7d72e12b9824ead18631",
				Author:    gitdomain.Signature{Name: "a", Email: "a@a.com", Date: *mustParseDate("2006-01-02T15:04:05Z", t)},
				Committer: &gitdomain.Signature{Name: "a", Email: "a@a.com", Date: *mustParseDate("2006-01-02T15:04:05Z", t)},
				Message:   "commit2",
				Parents:   []api.CommitID{"d38233a79e037d2ab8170b0d0bc0aa438473e6da"},
			},
			{
				ID:        "d38233a79e037d2ab8170b0d0bc0aa438473e6da",
				Author:    gitdomain.Signature{Name: "a", Email: "a@a.com", Date: *mustParseDate("2006-01-02T15:04:05Z", t)},
				Committer: &gitdomain.Signature{Name: "a", Email: "a@a.com", Date: *mustParseDate("2006-01-02T15:04:05Z", t)},
				Message:   "commit1",
			},
			{
				ID:        "67762ad757dd26cac4145f2b744fd93ad10a48e0",
				Author:    gitdomain.Signature{Name: "a", Email: "a@a.com", Date: *mustParseDate("2006-01-02T15:04:05Z", t)},
				Committer: &gitdomain.Signature{Name: "a", Email: "a@a.com", Date: *mustParseDate("2006-01-02T15:04:05Z", t)},
				Message:   "commit2",
				Parents:   []api.CommitID{"2b988222e844b570959a493f5b07ec020b89e122"},
			},
			{
				ID:        "2b988222e844b570959a493f5b07ec020b89e122",
				Author:    gitdomain.Signature{Name: "a", Email: "a@a.com", Date: *mustParseDate("2006-01-02T15:04:05Z", t)},
				Committer: &gitdomain.Signature{Name: "a", Email: "a@a.com", Date: *mustParseDate("2006-01-02T15:04:05Z", t)},
				Message:   "commit1",
			},
			{
				ID:        "01bed0ae660668c57539cecaacb4c33d77609f43",
				Author:    gitdomain.Signature{Name: "a", Email: "a@a.com", Date: *mustParseDate("2006-01-02T15:04:05Z", t)},
				Committer: &gitdomain.Signature{Name: "a", Email: "a@a.com", Date: *mustParseDate("2006-01-02T15:04:05Z", t)},
				Message:   "commit2",
				Parents:   []api.CommitID{"d6ce2e76d171569d81c0afdc4573f461cec17d45"},
			},
			nil,
			nil,
		}

		source := gitserver.NewTestClientSource(t, GitserverAddresses)
		commits, err := gitserver.NewTestClient(http.DefaultClient, source).GetCommits(ctx, nil, repoCommits, true)
		if err != nil {
			t.Fatalf("unexpected error calling getCommits: %s", err)
		}
		if diff := cmp.Diff(expectedCommits, commits); diff != "" {
			t.Errorf("unexpected commits (-want +got):\n%s", diff)
		}
	})
}

func getGitCommandsWithFiles(fileName1, fileName2 string) []string {
	return []string{
		fmt.Sprintf("touch %s", fileName1),
		fmt.Sprintf("git add %s", fileName1),
		"GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@a.com GIT_COMMITTER_DATE=2006-01-02T15:04:05Z git commit -m commit1 --author='a <a@a.com>' --date 2006-01-02T15:04:05Z",
		fmt.Sprintf("touch %s", fileName2),
		fmt.Sprintf("git add %s", fileName2),
		"GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@a.com GIT_COMMITTER_DATE=2006-01-02T15:04:05Z git commit -m commit2 --author='a <a@a.com>' --date 2006-01-02T15:04:05Z",
	}
}

func mustParseDate(s string, t *testing.T) *time.Time {
	t.Helper()
	date, err := time.Parse(time.RFC3339, s)
	if err != nil {
		t.Fatalf("unexpected error parsing date string: %s", err)
	}
	return &date
}

func TestHead(t *testing.T) {
	source := gitserver.NewTestClientSource(t, GitserverAddresses)
	client := gitserver.NewTestClient(http.DefaultClient, source)
	t.Run("basic", func(t *testing.T) {
		gitCommands := []string{
			"GIT_COMMITTER_NAME=a GIT_COMMITTER_EMAIL=a@a.com GIT_COMMITTER_DATE=2006-01-02T15:04:05Z git commit --allow-empty -m foo --author='a <a@a.com>' --date 2006-01-02T15:04:05Z",
		}
		repo := MakeGitRepository(t, gitCommands...)
		ctx := context.Background()

		head, exists, err := client.Head(ctx, nil, repo)
		if err != nil {
			t.Fatal(err)
		}
		wantHead := "ea167fe3d76b1e5fd3ed8ca44cbd2fe3897684f8"
		if head != wantHead {
			t.Fatalf("Want %q, got %q", wantHead, head)
		}
		if !exists {
			t.Fatal("Should exist")
		}
	})
}
