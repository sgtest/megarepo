package campaigns

import (
	"context"
	"io"
	"sort"
	"strings"
	"time"

	"github.com/inconshreveable/log15"
	"github.com/pkg/errors"
	"github.com/sourcegraph/go-diff/diff"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/internal/campaigns"
	cmpgn "github.com/sourcegraph/sourcegraph/internal/campaigns"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/bitbucketserver"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
)

// SetDerivedState will update the external state fields on the Changeset based
// on the current state of the changeset and associated events.
func SetDerivedState(ctx context.Context, c *cmpgn.Changeset, es []*cmpgn.ChangesetEvent) {
	// Copy so that we can sort without mutating the argument
	events := make(ChangesetEvents, len(es))
	copy(events, es)
	sort.Sort(events)

	c.ExternalCheckState = ComputeCheckState(c, events)

	history, err := computeHistory(c, events)
	if err != nil {
		log15.Warn("Computing changeset history", "err", err)
		return
	}

	if state, err := ComputeChangesetState(c, history); err != nil {
		log15.Warn("Computing changeset state", "err", err)
	} else {
		c.ExternalState = state
	}
	if state, err := ComputeReviewState(c, history); err != nil {
		log15.Warn("Computing changeset review state", "err", err)
	} else {
		c.ExternalReviewState = state
	}

	// If the changeset was "complete" (that is, not open) the last time we
	// synced, and it's still complete, then we don't need to do any further
	// work: the diffstat should still be correct, and this way we don't need to
	// rely on gitserver having the head OID still available.
	if c.SyncState.IsComplete && c.ExternalState != cmpgn.ChangesetStateOpen {
		return
	}

	// Some of the fields on changesets are dependent on the SyncState: this
	// encapsulates fields that we want to cache based on our current
	// understanding of the changeset's state on the external provider that are
	// not part of the metadata that we get from the provider's API.
	//
	// To update this, first we need gitserver's view of the repo.
	repo, err := changesetGitserverRepo(ctx, c)
	if err != nil {
		log15.Warn("Retrieving gitserver repo for changeset", "err", err)
		return
	}

	// Now we can update the state. Since we'll want to only perform some
	// actions based on how the state changes, we'll keep references to the old
	// and new states for the duration of this function, although we'll update
	// c.SyncState as soon as we can.
	oldState := c.SyncState
	newState, err := computeSyncState(ctx, c, *repo)
	if err != nil {
		log15.Warn("Computing sync state", "err", err)
		return
	}
	c.SyncState = *newState

	// Now we can update fields that are invalidated when the sync state
	// changes.
	if !oldState.Equals(newState) {
		if stat, err := computeDiffStat(ctx, c, *repo); err != nil {
			log15.Warn("Computing diffstat", "err", err)
		} else {
			c.SetDiffStat(stat)
		}
	}
}

// ComputeCheckState computes the overall check state based on the current synced check state
// and any webhook events that have arrived after the most recent sync
func ComputeCheckState(c *cmpgn.Changeset, events ChangesetEvents) cmpgn.ChangesetCheckState {
	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		return computeGitHubCheckState(c.UpdatedAt, m, events)

	case *bitbucketserver.PullRequest:
		return computeBitbucketBuildStatus(c.UpdatedAt, m, events)
	}

	return cmpgn.ChangesetCheckStateUnknown
}

// ComputeChangesetState computes the overall state for the changeset and its
// associated events. The events should be presorted.
func ComputeChangesetState(c *cmpgn.Changeset, history []changesetStatesAtTime) (cmpgn.ChangesetState, error) {
	if len(history) == 0 {
		return computeSingleChangesetState(c)
	}
	newestDataPoint := history[len(history)-1]
	if c.UpdatedAt.After(newestDataPoint.t) {
		return computeSingleChangesetState(c)
	}
	return newestDataPoint.state, nil
}

// ComputeReviewState computes the review state for the changeset and its
// associated events. The events should be presorted.
func ComputeReviewState(c *cmpgn.Changeset, history []changesetStatesAtTime) (cmpgn.ChangesetReviewState, error) {
	if len(history) == 0 {
		return computeSingleChangesetReviewState(c)
	}

	newestDataPoint := history[len(history)-1]

	// GitHub only stores the ReviewState in events, we can't look at the
	// Changeset.
	if c.ExternalServiceType == extsvc.TypeGitHub {
		return newestDataPoint.reviewState, nil
	}

	// For other codehosts we check whether the Changeset is newer or the
	// events and use the newest entity to get the reviewstate.
	if c.UpdatedAt.After(newestDataPoint.t) {
		return computeSingleChangesetReviewState(c)
	}
	return newestDataPoint.reviewState, nil
}

func computeBitbucketBuildStatus(lastSynced time.Time, pr *bitbucketserver.PullRequest, events []*cmpgn.ChangesetEvent) cmpgn.ChangesetCheckState {
	var latestCommit bitbucketserver.Commit
	for _, c := range pr.Commits {
		if latestCommit.CommitterTimestamp <= c.CommitterTimestamp {
			latestCommit = *c
		}
	}

	stateMap := make(map[string]cmpgn.ChangesetCheckState)

	// States from last sync
	for _, status := range pr.CommitStatus {
		stateMap[status.Key()] = parseBitbucketBuildState(status.Status.State)
	}

	// Add any events we've received since our last sync
	for _, e := range events {
		switch m := e.Metadata.(type) {
		case *bitbucketserver.CommitStatus:
			if m.Commit != latestCommit.ID {
				continue
			}
			dateAdded := unixMilliToTime(m.Status.DateAdded)
			if dateAdded.Before(lastSynced) {
				continue
			}
			stateMap[m.Key()] = parseBitbucketBuildState(m.Status.State)
		}
	}

	states := make([]cmpgn.ChangesetCheckState, 0, len(stateMap))
	for _, v := range stateMap {
		states = append(states, v)
	}

	return combineCheckStates(states)
}

func parseBitbucketBuildState(s string) cmpgn.ChangesetCheckState {
	switch s {
	case "FAILED":
		return cmpgn.ChangesetCheckStateFailed
	case "INPROGRESS":
		return cmpgn.ChangesetCheckStatePending
	case "SUCCESSFUL":
		return cmpgn.ChangesetCheckStatePassed
	default:
		return cmpgn.ChangesetCheckStateUnknown
	}
}

func computeGitHubCheckState(lastSynced time.Time, pr *github.PullRequest, events []*cmpgn.ChangesetEvent) cmpgn.ChangesetCheckState {
	// We should only consider the latest commit. This could be from a sync or a webhook that
	// has occurred later
	var latestCommitTime time.Time
	var latestOID string
	statusPerContext := make(map[string]cmpgn.ChangesetCheckState)
	statusPerCheckSuite := make(map[string]cmpgn.ChangesetCheckState)
	statusPerCheckRun := make(map[string]cmpgn.ChangesetCheckState)

	if len(pr.Commits.Nodes) > 0 {
		// We only request the most recent commit
		commit := pr.Commits.Nodes[0]
		latestCommitTime = commit.Commit.CommittedDate
		latestOID = commit.Commit.OID
		// Calc status per context for the most recent synced commit
		for _, c := range commit.Commit.Status.Contexts {
			statusPerContext[c.Context] = parseGithubCheckState(c.State)
		}
		for _, c := range commit.Commit.CheckSuites.Nodes {
			if c.Status == "QUEUED" && len(c.CheckRuns.Nodes) == 0 {
				// Ignore queued suites with no runs.
				// It is common for suites to be created and then stay in the QUEUED state
				// forever with zero runs.
				continue
			}
			statusPerCheckSuite[c.ID] = parseGithubCheckSuiteState(c.Status, c.Conclusion)
			for _, r := range c.CheckRuns.Nodes {
				statusPerCheckRun[r.ID] = parseGithubCheckSuiteState(r.Status, r.Conclusion)
			}
		}
	}

	var statuses []*github.CommitStatus
	// Get all status updates that have happened since our last sync
	for _, e := range events {
		switch m := e.Metadata.(type) {
		case *github.CommitStatus:
			if m.ReceivedAt.After(lastSynced) {
				statuses = append(statuses, m)
			}
		case *github.PullRequestCommit:
			if m.Commit.CommittedDate.After(latestCommitTime) {
				latestCommitTime = m.Commit.CommittedDate
				latestOID = m.Commit.OID
				// statusPerContext is now out of date, reset it
				for k := range statusPerContext {
					delete(statusPerContext, k)
				}
			}
		case *github.CheckSuite:
			if m.Status == "QUEUED" && len(m.CheckRuns.Nodes) == 0 {
				// Ignore suites with no runs.
				// See previous comment.
				continue
			}
			if m.ReceivedAt.After(lastSynced) {
				statusPerCheckSuite[m.ID] = parseGithubCheckSuiteState(m.Status, m.Conclusion)
			}
		case *github.CheckRun:
			if m.ReceivedAt.After(lastSynced) {
				statusPerCheckRun[m.ID] = parseGithubCheckSuiteState(m.Status, m.Conclusion)
			}
		}
	}

	if len(statuses) > 0 {
		// Update the statuses using any new webhook events for the latest commit
		sort.Slice(statuses, func(i, j int) bool {
			return statuses[i].ReceivedAt.Before(statuses[j].ReceivedAt)
		})
		for _, s := range statuses {
			if s.SHA != latestOID {
				continue
			}
			statusPerContext[s.Context] = parseGithubCheckState(s.State)
		}
	}
	finalStates := make([]cmpgn.ChangesetCheckState, 0, len(statusPerContext))
	for k := range statusPerContext {
		finalStates = append(finalStates, statusPerContext[k])
	}
	for k := range statusPerCheckSuite {
		finalStates = append(finalStates, statusPerCheckSuite[k])
	}
	for k := range statusPerCheckRun {
		finalStates = append(finalStates, statusPerCheckRun[k])
	}
	return combineCheckStates(finalStates)
}

// combineCheckStates combines multiple check states into an overall state
// pending takes highest priority
// followed by error
// success return only if all successful
func combineCheckStates(states []cmpgn.ChangesetCheckState) cmpgn.ChangesetCheckState {
	if len(states) == 0 {
		return cmpgn.ChangesetCheckStateUnknown
	}
	stateMap := make(map[cmpgn.ChangesetCheckState]bool)
	for _, s := range states {
		stateMap[s] = true
	}

	switch {
	case stateMap[cmpgn.ChangesetCheckStateUnknown]:
		// If are pending, overall is Pending
		return cmpgn.ChangesetCheckStateUnknown
	case stateMap[cmpgn.ChangesetCheckStatePending]:
		// If are pending, overall is Pending
		return cmpgn.ChangesetCheckStatePending
	case stateMap[cmpgn.ChangesetCheckStateFailed]:
		// If no pending, but have errors then overall is Failed
		return cmpgn.ChangesetCheckStateFailed
	case stateMap[cmpgn.ChangesetCheckStatePassed]:
		// No pending or errors then overall is Passed
		return cmpgn.ChangesetCheckStatePassed
	}

	return cmpgn.ChangesetCheckStateUnknown
}

func parseGithubCheckState(s string) cmpgn.ChangesetCheckState {
	s = strings.ToUpper(s)
	switch s {
	case "ERROR", "FAILURE":
		return cmpgn.ChangesetCheckStateFailed
	case "EXPECTED", "PENDING":
		return cmpgn.ChangesetCheckStatePending
	case "SUCCESS":
		return cmpgn.ChangesetCheckStatePassed
	default:
		return cmpgn.ChangesetCheckStateUnknown
	}
}

func parseGithubCheckSuiteState(status, conclusion string) cmpgn.ChangesetCheckState {
	status = strings.ToUpper(status)
	conclusion = strings.ToUpper(conclusion)
	switch status {
	case "IN_PROGRESS", "QUEUED", "REQUESTED":
		return cmpgn.ChangesetCheckStatePending
	}
	if status != "COMPLETED" {
		return cmpgn.ChangesetCheckStateUnknown
	}
	switch conclusion {
	case "SUCCESS", "NEUTRAL":
		return cmpgn.ChangesetCheckStatePassed
	case "ACTION_REQUIRED":
		return cmpgn.ChangesetCheckStatePending
	case "CANCELLED", "FAILURE", "TIMED_OUT":
		return cmpgn.ChangesetCheckStateFailed
	}
	return cmpgn.ChangesetCheckStateUnknown
}

// computeSingleChangesetState of a Changeset based on the metadata.
// It does NOT reflect the final calculated state, use `ExternalState` instead.
func computeSingleChangesetState(c *cmpgn.Changeset) (s cmpgn.ChangesetState, err error) {
	if !c.ExternalDeletedAt.IsZero() {
		return cmpgn.ChangesetStateDeleted, nil
	}

	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		s = cmpgn.ChangesetState(m.State)
	case *bitbucketserver.PullRequest:
		if m.State == "DECLINED" {
			s = cmpgn.ChangesetStateClosed
		} else {
			s = cmpgn.ChangesetState(m.State)
		}
	default:
		return "", errors.New("unknown changeset type")
	}

	if !s.Valid() {
		return "", errors.Errorf("changeset state %q invalid", s)
	}

	return s, nil
}

// computeSingleChangesetReviewState computes the review state of a Changeset.
// GitHub doesn't keep the review state on a changeset, so a GitHub Changeset
// will always return ChangesetReviewStatePending.
//
// This method should NOT be called directly. Use ComputeReviewState instead.
func computeSingleChangesetReviewState(c *cmpgn.Changeset) (s cmpgn.ChangesetReviewState, err error) {
	states := map[cmpgn.ChangesetReviewState]bool{}

	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		// For GitHub we need to use `ChangesetEvents.ReviewState`
		log15.Warn("Changeset.ReviewState() called, but GitHub review state is calculated through ChangesetEvents.ReviewState", "changeset", c)
		return cmpgn.ChangesetReviewStatePending, nil

	case *bitbucketserver.PullRequest:
		for _, r := range m.Reviewers {
			switch r.Status {
			case "UNAPPROVED":
				states[cmpgn.ChangesetReviewStatePending] = true
			case "NEEDS_WORK":
				states[cmpgn.ChangesetReviewStateChangesRequested] = true
			case "APPROVED":
				states[cmpgn.ChangesetReviewStateApproved] = true
			}
		}
	default:
		return "", errors.New("unknown changeset type")
	}

	return selectReviewState(states), nil
}

// selectReviewState computes the single review state for a given set of
// ChangesetReviewStates. Since a pull request, for example, can have multiple
// reviews with different states, we need a function to determine what the
// state for the pull request is.
func selectReviewState(states map[cmpgn.ChangesetReviewState]bool) cmpgn.ChangesetReviewState {
	// If any review requested changes, that state takes precedence over all
	// other review states, followed by explicit approval. Everything else is
	// considered pending.
	for _, state := range [...]cmpgn.ChangesetReviewState{
		cmpgn.ChangesetReviewStateChangesRequested,
		cmpgn.ChangesetReviewStateApproved,
	} {
		if states[state] {
			return state
		}
	}

	return cmpgn.ChangesetReviewStatePending
}

// computeOverallReviewState returns the overall review state given a map of
// reviews per author.
func computeReviewState(statesByAuthor map[string]campaigns.ChangesetReviewState) campaigns.ChangesetReviewState {
	states := make(map[campaigns.ChangesetReviewState]bool)
	for _, s := range statesByAuthor {
		states[s] = true
	}
	return selectReviewState(states)
}

// computeDiffStat computes the up to date diffstat for the changeset, based on
// the values in c.SyncState.
func computeDiffStat(ctx context.Context, c *cmpgn.Changeset, repo gitserver.Repo) (*diff.Stat, error) {
	iter, err := git.Diff(ctx, git.DiffOptions{
		Repo: repo,
		Base: c.SyncState.BaseRefOid,
		Head: c.SyncState.HeadRefOid,
	})
	if err != nil {
		return nil, err
	}

	stat := &diff.Stat{}
	for {
		file, err := iter.Next()
		if err == io.EOF {
			break
		} else if err != nil {
			return nil, err
		}

		fs := file.Stat()
		log15.Info("file diff", "file", file.NewName, "stat", fs)
		stat.Added += fs.Added
		stat.Changed += fs.Changed
		stat.Deleted += fs.Deleted
	}

	log15.Info("total diff stat", "stat", stat)

	return stat, nil
}

// computeSyncState computes the up to date sync state based on the changeset as
// it currently exists on the external provider.
func computeSyncState(ctx context.Context, c *cmpgn.Changeset, repo gitserver.Repo) (*cmpgn.ChangesetSyncState, error) {
	// If the changeset type can return the OIDs directly, then we can use that
	// for the new state. Otherwise, we need to try to resolve the ref to a
	// revision.
	base, err := computeRev(ctx, c, repo, func(c *cmpgn.Changeset) (string, error) {
		return c.BaseRefOid()
	}, func(c *cmpgn.Changeset) (string, error) {
		return c.BaseRef()
	})
	if err != nil {
		return nil, err
	}

	head, err := computeRev(ctx, c, repo, func(c *cmpgn.Changeset) (string, error) {
		return c.HeadRefOid()
	}, func(c *cmpgn.Changeset) (string, error) {
		return c.HeadRef()
	})
	if err != nil {
		return nil, err
	}

	return &cmpgn.ChangesetSyncState{
		BaseRefOid: base,
		HeadRefOid: head,
		IsComplete: c.ExternalState != cmpgn.ChangesetStateOpen,
	}, nil
}

func computeRev(ctx context.Context, c *cmpgn.Changeset, repo gitserver.Repo, getOid, getRef func(*cmpgn.Changeset) (string, error)) (string, error) {
	if rev, err := getOid(c); err != nil {
		return "", err
	} else if rev != "" {
		return rev, nil
	}

	ref, err := getRef(c)
	if err != nil {
		return "", err
	}

	rev, err := git.ResolveRevision(ctx, repo, nil, ref, nil)
	return string(rev), err
}

// changesetGitserverRepo looks up a gitserver.Repo based on the RepoID within a
// changeset.
func changesetGitserverRepo(ctx context.Context, c *cmpgn.Changeset) (*gitserver.Repo, error) {
	repo, err := db.Repos.Get(ctx, c.RepoID)
	if err != nil {
		return nil, err
	}
	return &gitserver.Repo{Name: repo.Name, URL: repo.URI}, nil
}

func unixMilliToTime(ms int64) time.Time {
	return time.Unix(0, ms*int64(time.Millisecond))
}
