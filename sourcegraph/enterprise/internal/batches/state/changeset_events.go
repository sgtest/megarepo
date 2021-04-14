package state

import (
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
)

// ChangesetEvents is a collection of changeset events
type ChangesetEvents []*btypes.ChangesetEvent

func (ce ChangesetEvents) Len() int      { return len(ce) }
func (ce ChangesetEvents) Swap(i, j int) { ce[i], ce[j] = ce[j], ce[i] }

// Less sorts changeset events by their Timestamps
func (ce ChangesetEvents) Less(i, j int) bool {
	return ce[i].Timestamp().Before(ce[j].Timestamp())
}
