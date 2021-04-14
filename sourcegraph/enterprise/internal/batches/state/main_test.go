package state

import (
	"time"

	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitlab"
)

func setDraft(c *btypes.Changeset) *btypes.Changeset {
	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		m.IsDraft = true
	case *gitlab.MergeRequest:
		m.WorkInProgress = true
	}
	return c
}

func timeToUnixMilli(t time.Time) int {
	return int(t.UnixNano()) / int(time.Millisecond)
}
