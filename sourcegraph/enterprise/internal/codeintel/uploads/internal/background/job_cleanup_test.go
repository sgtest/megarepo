package background

import (
	"context"
	"sort"
	"strings"
	"testing"
	"time"

	"github.com/derision-test/glock"
	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/shared"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func TestUnknownCommitsJanitor(t *testing.T) {
	resolveRevisionFunc := func(commit string) error {
		return nil
	}

	testUnknownCommitsJanitor(t, resolveRevisionFunc, []updateInvocation{
		{1, "foo-x", false},
		{1, "foo-y", false},
		{1, "foo-z", false},
		{2, "bar-x", false},
		{2, "bar-y", false},
		{2, "bar-z", false},
		{3, "baz-x", false},
		{3, "baz-y", false},
		{3, "baz-z", false},
	})
}

func TestUnknownCommitsJanitorUnknownCommit(t *testing.T) {
	resolveRevisionFunc := func(commit string) error {
		if commit == "foo-y" || commit == "bar-x" || commit == "baz-z" {
			return &gitdomain.RevisionNotFoundError{}
		}

		return nil
	}

	testUnknownCommitsJanitor(t, resolveRevisionFunc, []updateInvocation{
		{1, "foo-x", false},
		{1, "foo-y", true},
		{1, "foo-z", false},
		{2, "bar-x", true},
		{2, "bar-y", false},
		{2, "bar-z", false},
		{3, "baz-x", false},
		{3, "baz-y", false},
		{3, "baz-z", true},
	})
}

func TestUnknownCommitsJanitorUnknownRepository(t *testing.T) {
	resolveRevisionFunc := func(commit string) error {
		if strings.HasPrefix(commit, "foo-") {
			return &gitdomain.RepoNotExistError{}
		}

		return nil
	}

	testUnknownCommitsJanitor(t, resolveRevisionFunc, []updateInvocation{
		{1, "foo-x", false},
		{1, "foo-y", false},
		{1, "foo-z", false},
		{2, "bar-x", false},
		{2, "bar-y", false},
		{2, "bar-z", false},
		{3, "baz-x", false},
		{3, "baz-y", false},
		{3, "baz-z", false},
	})
}

type updateInvocation struct {
	RepositoryID int
	Commit       string
	Delete       bool
}

var testSourcedCommits = []shared.SourcedCommits{
	{RepositoryID: 1, RepositoryName: "foo", Commits: []string{"foo-x", "foo-y", "foo-z"}},
	{RepositoryID: 2, RepositoryName: "bar", Commits: []string{"bar-x", "bar-y", "bar-z"}},
	{RepositoryID: 3, RepositoryName: "baz", Commits: []string{"baz-x", "baz-y", "baz-z"}},
}

func testUnknownCommitsJanitor(t *testing.T, resolveRevisionFunc func(commit string) error, expectedCalls []updateInvocation) {
	gitserverClient := NewMockGitserverClient()
	gitserverClient.ResolveRevisionFunc.SetDefaultHook(func(ctx context.Context, i int, spec string) (api.CommitID, error) {
		return api.CommitID(spec), resolveRevisionFunc(spec)
	})

	mockUploadSvc := NewMockStore()
	mockUploadSvc.GetStaleSourcedCommitsFunc.SetDefaultReturn(testSourcedCommits, nil)

	janitor := janitorJob{
		store:           mockUploadSvc,
		lsifStore:       NewMockLsifStore(),
		logger:          logtest.Scoped(t),
		metrics:         NewJanitorMetrics(&observation.TestContext),
		clock:           glock.NewRealClock(),
		gitserverClient: gitserverClient,
	}

	if err := janitor.handleCleanup(
		context.Background(), JanitorConfig{
			MinimumTimeSinceLastCheck:      1 * time.Hour,
			CommitResolverBatchSize:        10,
			AuditLogMaxAge:                 1 * time.Hour,
			UnreferencedDocumentMaxAge:     1 * time.Hour,
			CommitResolverMaximumCommitLag: 1 * time.Hour,
			UploadTimeout:                  1 * time.Hour,
		}); err != nil {
		t.Fatalf("unexpected error running janitor: %s", err)
	}

	var sanitizedCalls []updateInvocation
	for _, call := range mockUploadSvc.UpdateSourcedCommitsFunc.History() {
		sanitizedCalls = append(sanitizedCalls, updateInvocation{
			RepositoryID: call.Arg1,
			Commit:       call.Arg2,
			Delete:       false,
		})
	}
	for _, call := range mockUploadSvc.DeleteSourcedCommitsFunc.History() {
		sanitizedCalls = append(sanitizedCalls, updateInvocation{
			RepositoryID: call.Arg1,
			Commit:       call.Arg2,
			Delete:       true,
		})
	}
	sort.Slice(sanitizedCalls, func(i, j int) bool {
		if sanitizedCalls[i].RepositoryID < sanitizedCalls[j].RepositoryID {
			return true
		}

		return sanitizedCalls[i].RepositoryID == sanitizedCalls[j].RepositoryID && sanitizedCalls[i].Commit < sanitizedCalls[j].Commit
	})

	if diff := cmp.Diff(expectedCalls, sanitizedCalls); diff != "" {
		t.Errorf("unexpected calls to UpdateSourcedCommits and DeleteSourcedCommits (-want +got):\n%s", diff)
	}
}
