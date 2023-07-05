package policies

import (
	"context"
	"fmt"
	"time"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"
)

func testUploadExpirerMockGitserverClient(defaultBranchName string, now time.Time) *gitserver.MockClient {
	// Test repository:
	//
	//                                              v2.2.2                              02 -- feat/blank
	//                                             /                                   /
	//  09               08 ---- 07              06              05 ------ 04 ------ 03 ------ 01
	//   \                        \               \               \         \                   \
	//    xy/feature-y            xy/feature-x    zw/feature-z     v1.2.2    v1.2.3              develop

	branchHeads := map[string]string{
		"develop":      "deadbeef01",
		"feat/blank":   "deadbeef02",
		"xy/feature-x": "deadbeef07",
		"zw/feature-z": "deadbeef06",
		"xy/feature-y": "deadbeef09",
	}

	tagHeads := map[string]string{
		"v1.2.3": "deadbeef04",
		"v1.2.2": "deadbeef05",
		"v2.2.2": "deadbeef06",
	}

	branchMembers := map[string][]string{
		"develop":      {"deadbeef01", "deadbeef03", "deadbeef04", "deadbeef05"},
		"feat/blank":   {"deadbeef02"},
		"xy/feature-x": {"deadbeef07", "deadbeef08"},
		"xy/feature-y": {"deadbeef09"},
		"zw/feature-z": {"deadbeef06"},
		"deadbeef01":   {"deadbeef01", "deadbeef03", "deadbeef04", "deadbeef05"},
		"deadbeef02":   {"deadbeef02"},
		"deadbeef06":   {"deadbeef06"},
		"deadbeef07":   {"deadbeef07", "deadbeef08"},
		"deadbeef09":   {"deadbeef09"},
	}

	createdAt := map[string]time.Time{
		"deadbeef01": testCommitDateFor("deadbeef01", now),
		"deadbeef02": testCommitDateFor("deadbeef02", now),
		"deadbeef03": testCommitDateFor("deadbeef03", now),
		"deadbeef04": testCommitDateFor("deadbeef04", now),
		"deadbeef07": testCommitDateFor("deadbeef07", now),
		"deadbeef08": testCommitDateFor("deadbeef08", now),
		"deadbeef05": testCommitDateFor("deadbeef05", now),
		"deadbeef06": testCommitDateFor("deadbeef06", now),
		"deadbeef09": testCommitDateFor("deadbeef09", now),
	}

	commitDate := func(ctx context.Context, _ authz.SubRepoPermissionChecker, repo api.RepoName, commitID api.CommitID) (string, time.Time, bool, error) {
		commitDate, ok := createdAt[string(commitID)]
		return string(commitID), commitDate, ok, nil
	}

	refDescriptions := func(ctx context.Context, _ authz.SubRepoPermissionChecker, repo api.RepoName, _ ...string) (map[string][]gitdomain.RefDescription, error) {
		refDescriptions := map[string][]gitdomain.RefDescription{}
		for branch, commit := range branchHeads {
			branchHeadCreateDate := createdAt[commit]
			refDescriptions[commit] = append(refDescriptions[commit], gitdomain.RefDescription{
				Name:            branch,
				Type:            gitdomain.RefTypeBranch,
				IsDefaultBranch: branch == defaultBranchName,
				CreatedDate:     &branchHeadCreateDate,
			})
		}

		for tag, commit := range tagHeads {
			tagCreateDate := createdAt[commit]
			refDescriptions[commit] = append(refDescriptions[commit], gitdomain.RefDescription{
				Name:        tag,
				Type:        gitdomain.RefTypeTag,
				CreatedDate: &tagCreateDate,
			})
		}

		return refDescriptions, nil
	}

	commitsUniqueToBranch := func(ctx context.Context, _ authz.SubRepoPermissionChecker, repo api.RepoName, branchName string, isDefaultBranch bool, maxAge *time.Time) (map[string]time.Time, error) {
		branches := map[string]time.Time{}
		for _, commit := range branchMembers[branchName] {
			if maxAge == nil || !createdAt[commit].Before(*maxAge) {
				branches[commit] = createdAt[commit]
			}
		}

		return branches, nil
	}

	gitserverClient := gitserver.NewMockClient()
	gitserverClient.CommitDateFunc.SetDefaultHook(commitDate)
	gitserverClient.RefDescriptionsFunc.SetDefaultHook(refDescriptions)
	gitserverClient.CommitsUniqueToBranchFunc.SetDefaultHook(commitsUniqueToBranch)

	return gitserverClient
}

func hydrateCommittedAt(expectedPolicyMatches map[string][]PolicyMatch, now time.Time) {
	for commit, matches := range expectedPolicyMatches {
		for i, match := range matches {
			committedAt := testCommitDateFor(commit, now)
			match.CommittedAt = &committedAt
			matches[i] = match
		}
	}
}

func testCommitDateFor(commit string, now time.Time) time.Time {
	switch commit {
	case "deadbeef01":
		return now.Add(-time.Hour * 5)
	case "deadbeef02":
		return now.Add(-time.Hour * 5)
	case "deadbeef03":
		return now.Add(-time.Hour * 5)
	case "deadbeef04":
		return now.Add(-time.Hour * 5)
	case "deadbeef07":
		return now.Add(-time.Hour * 5)
	case "deadbeef08":
		return now.Add(-time.Hour * 5)
	case "deadbeef05":
		return now.Add(-time.Hour * 12)
	case "deadbeef06":
		return now.Add(-time.Hour * 15)
	case "deadbeef09":
		return now.Add(-time.Hour * 15)
	default:
	}

	panic(fmt.Sprintf("unexpected commit date request for %q", commit))
}
