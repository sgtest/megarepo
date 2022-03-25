package main

import (
	"context"
	"os"
	"sort"
	"strconv"
	"strings"
	"text/template"
	"time"

	"github.com/google/go-github/v41/github"
	"github.com/grafana/regexp"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var (
	repoOwner           = "sourcegraph"
	repoName            = "sourcegraph"
	commitsPerPage      = 30
	maxCommitsPageCount = 5
)

var (
	ErrNoRelevantChanges = errors.New("no services changed, nothing to notify")
)

type DeploymentNotifier struct {
	dd          DeploymentDiffer
	ghc         *github.Client
	environment string
}

func NewDeploymentNotifier(ghc *github.Client, dd DeploymentDiffer, environment string) *DeploymentNotifier {
	return &DeploymentNotifier{
		dd:          dd,
		ghc:         ghc,
		environment: environment,
	}
}

func (dn *DeploymentNotifier) Report(ctx context.Context) (*report, error) {
	services, err := dn.dd.Services()
	if err != nil {
		return nil, errors.Wrap(err, "failed to infer changes")
	}

	// Use a map so we avoid duplicate PRs.
	prSet := map[int64]*github.PullRequest{}

	groups := groupByDiff(services)
	for diff := range groups {
		if diff.Old == diff.New {
			// If nothing changed, just skip.
			continue
		}
		groupPrs, err := dn.getNewPullRequests(ctx, diff.Old, diff.New)
		if err != nil {
			return nil, errors.Wrap(err, "failed to get pull requests")
		}
		for _, pr := range groupPrs {
			prSet[pr.GetID()] = pr
		}
	}

	var prs []*github.PullRequest
	for _, pr := range prSet {
		prs = append(prs, pr)
	}

	// Sort the PRs so the tests are stable.
	sort.Slice(prs, func(i, j int) bool {
		return prs[i].GetMergedAt().After(prs[j].GetMergedAt())
	})

	var deployedServices []string
	for app := range services {
		deployedServices = append(deployedServices, app)
	}

	// Sort the Services so the tests are stable.
	sort.Strings(deployedServices)

	if len(prs) == 0 {
		return nil, ErrNoRelevantChanges
	}

	report := report{
		Environment:       dn.environment,
		PullRequests:      prs,
		DeployedAt:        time.Now().In(time.UTC).Format(time.RFC822Z),
		Services:          deployedServices,
		BuildkiteBuildURL: os.Getenv("BUILDKITE_BUILD_URL"),
	}

	return &report, nil
}

// getNewCommits returns a slice of commits starting from the target commit up to the currently deployed commit.
func (dn *DeploymentNotifier) getNewCommits(ctx context.Context, oldCommit string, newCommit string) ([]*github.RepositoryCommit, error) {
	var page = 1
	var commits []*github.RepositoryCommit
	for page != 0 && page != maxCommitsPageCount {
		cs, resp, err := dn.ghc.Repositories.ListCommits(ctx, repoOwner, repoName, &github.CommitsListOptions{
			SHA: "main",
			ListOptions: github.ListOptions{
				Page:    page,
				PerPage: commitsPerPage,
			},
		})
		if err != nil {
			return nil, err
		}
		commits = append(commits, cs...)
		var currentCommitIdx int
		for i, commit := range commits {
			if strings.HasPrefix(commit.GetSHA(), newCommit) {
				currentCommitIdx = i
			}
			if strings.HasPrefix(commit.GetSHA(), oldCommit) {
				return commits[currentCommitIdx:i], nil
			}
		}
		page = resp.NextPage
	}
	return nil, errors.Newf("commit %s not found in the last %d commits", oldCommit, maxCommitsPageCount*commitsPerPage)
}

func parsePRNumberInMergeCommit(message string) int {
	mergeCommitMessageRegexp := regexp.MustCompile(`\(#(\d+)\)$`)
	matches := mergeCommitMessageRegexp.FindStringSubmatch(message)
	if len(matches) > 1 {
		num, err := strconv.Atoi(matches[1])
		if err != nil {
			return 0
		}
		return num
	}
	return 0
}

func (dn *DeploymentNotifier) getNewPullRequests(ctx context.Context, oldCommit string, newCommit string) ([]*github.PullRequest, error) {
	repoCommits, err := dn.getNewCommits(ctx, oldCommit, newCommit)
	if err != nil {
		return nil, err
	}
	prNums := map[int]struct{}{}
	for _, rc := range repoCommits {
		message := rc.GetCommit().GetMessage()
		if prNum := parsePRNumberInMergeCommit(message); prNum > 0 {
			prNums[prNum] = struct{}{}
		}
	}
	var pullsSinceLastCommit []*github.PullRequest
	for prNum := range prNums {
		pull, _, err := dn.ghc.PullRequests.Get(
			ctx,
			repoOwner,
			repoName,
			prNum,
		)
		if err != nil {
			return nil, err
		}
		pullsSinceLastCommit = append(pullsSinceLastCommit, pull)
	}
	return pullsSinceLastCommit, nil
}

type deploymentGroups map[ServiceVersionDiff][]string

func groupByDiff(diffs map[string]*ServiceVersionDiff) deploymentGroups {
	groups := deploymentGroups{}
	for appName, diff := range diffs {
		groups[*diff] = append(groups[*diff], appName)
	}
	return groups
}

func renderComment(report *report) (string, error) {
	tmpl, err := template.New("deployment-status-comment").Parse(commentTemplate)
	if err != nil {
		return "", err
	}
	var sb strings.Builder
	err = tmpl.Execute(&sb, report)
	if err != nil {
		return "", err
	}
	return sb.String(), nil
}

type report struct {
	Environment       string
	PullRequests      []*github.PullRequest
	DeployedAt        string
	Services          []string
	BuildkiteBuildURL string
}

var commentTemplate = `### Deployment status

[Deployed at {{ .DeployedAt }}]({{ .BuildkiteBuildURL }}):

{{- range .Services }}
- ` + "`" + `{{ . }}` + "`" + `
{{- end }}
`
