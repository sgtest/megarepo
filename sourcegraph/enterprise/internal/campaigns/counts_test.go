package campaigns

import (
	"reflect"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/sourcegraph/internal/campaigns"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/bitbucketserver"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
)

func TestCalcCounts(t *testing.T) {
	now := time.Now().UTC().Truncate(time.Microsecond)
	daysAgo := func(days int) time.Time { return now.AddDate(0, 0, -days) }

	tests := []struct {
		codehosts  string
		name       string
		changesets []*campaigns.Changeset
		start      time.Time
		end        time.Time
		events     []Event
		want       []*ChangesetCounts
	}{
		{
			codehosts: "github",
			name:      "single changeset open merged",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(2)),
			},
			start: daysAgo(2),
			events: []Event{
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(1), Total: 1, Merged: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset open merged",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(2)),
			},
			start: daysAgo(2),
			events: []Event{
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(1), Total: 1, Merged: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			name: "start end time on subset of events",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(3)),
			},
			start: daysAgo(4),
			end:   daysAgo(2),
			events: []Event{
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 0, Open: 0},
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset created and closed before start time",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(8)),
			},
			start: daysAgo(4),
			end:   daysAgo(2),
			events: []Event{
				fakeEvent{t: daysAgo(7), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 1, Merged: 1},
				{Time: daysAgo(3), Total: 1, Merged: 1},
				{Time: daysAgo(2), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset created and closed before start time",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(8)),
			},
			start: daysAgo(4),
			end:   daysAgo(2),
			events: []Event{
				fakeEvent{t: daysAgo(7), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 1, Merged: 1},
				{Time: daysAgo(3), Total: 1, Merged: 1},
				{Time: daysAgo(2), Total: 1, Merged: 1},
			},
		},
		{
			name: "start time not even x*24hours before end time",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(2)),
			},
			start: daysAgo(3),
			end:   now.Add(-18 * time.Hour),
			events: []Event{
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(2).Add(-18 * time.Hour), Total: 0, Merged: 0},
				{Time: daysAgo(1).Add(-18 * time.Hour), Total: 1, Open: 1, OpenPending: 1},
				{Time: now.Add(-18 * time.Hour), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "github",
			name:      "multiple changesets open merged",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(2)),
				ghChangeset(2, daysAgo(2)),
			},
			start: daysAgo(2),
			events: []Event{
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 2},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(2), Total: 2, Open: 2, OpenPending: 2},
				{Time: daysAgo(1), Total: 2, Merged: 2},
				{Time: daysAgo(0), Total: 2, Merged: 2},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "multiple changesets open merged",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(2)),
				bbsChangeset(2, daysAgo(2)),
			},
			start: daysAgo(2),
			events: []Event{
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 1},
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 2},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(2), Total: 2, Open: 2, OpenPending: 2},
				{Time: daysAgo(1), Total: 2, Merged: 2},
				{Time: daysAgo(0), Total: 2, Merged: 2},
			},
		},
		{
			codehosts: "github",
			name:      "multiple changesets open merged different times",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(3)),
				ghChangeset(2, daysAgo(2)),
			},
			start: daysAgo(4),
			events: []Event{
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 2},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 0, Open: 0},
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 2, Open: 1, OpenPending: 1, Merged: 1},
				{Time: daysAgo(1), Total: 2, Merged: 2},
				{Time: daysAgo(0), Total: 2, Merged: 2},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "multiple changesets open merged different times",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(3)),
				bbsChangeset(2, daysAgo(2)),
			},
			start: daysAgo(4),
			events: []Event{
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 1},
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 2},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 0, Open: 0},
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 2, Open: 1, OpenPending: 1, Merged: 1},
				{Time: daysAgo(1), Total: 2, Merged: 2},
				{Time: daysAgo(0), Total: 2, Merged: 2},
			},
		},
		{
			codehosts: "github",
			name:      "changeset merged and closed at same time",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(2)),
			},
			start: daysAgo(2),
			events: []Event{
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubClosed, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(1), Total: 1, Merged: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "github",
			name:      "changeset merged and closed at same time, reversed order in slice",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(2)),
			},
			start: daysAgo(2),
			events: []Event{
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubClosed, id: 1},
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(1), Total: 1, Merged: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset open closed reopened merged",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(4)),
			},
			start: daysAgo(5),
			events: []Event{
				fakeEvent{t: daysAgo(3), kind: campaigns.ChangesetEventKindGitHubClosed, id: 1},
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindGitHubReopened, id: 1},
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(5), Total: 0, Open: 0},
				{Time: daysAgo(4), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(3), Total: 1, Open: 0, Closed: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(1), Total: 1, Merged: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset open declined reopened merged",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(4)),
			},
			start: daysAgo(5),
			events: []Event{
				fakeEvent{t: daysAgo(3), kind: campaigns.ChangesetEventKindBitbucketServerDeclined, id: 1},
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindBitbucketServerReopened, id: 1},
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(5), Total: 0, Open: 0},
				{Time: daysAgo(4), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(3), Total: 1, Open: 0, Closed: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(1), Total: 1, Merged: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "github",
			name:      "multiple changesets open closed reopened merged different times",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(5)),
				ghChangeset(2, daysAgo(4)),
			},
			start: daysAgo(6),
			events: []Event{
				fakeEvent{t: daysAgo(4), kind: campaigns.ChangesetEventKindGitHubClosed, id: 1},
				fakeEvent{t: daysAgo(3), kind: campaigns.ChangesetEventKindGitHubClosed, id: 2},
				fakeEvent{t: daysAgo(3), kind: campaigns.ChangesetEventKindGitHubReopened, id: 1},
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindGitHubReopened, id: 2},
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
				fakeEvent{t: daysAgo(0), kind: campaigns.ChangesetEventKindGitHubMerged, id: 2},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(6), Total: 0, Open: 0},
				{Time: daysAgo(5), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(4), Total: 2, Open: 1, OpenPending: 1, Closed: 1},
				{Time: daysAgo(3), Total: 2, Open: 1, OpenPending: 1, Closed: 1},
				{Time: daysAgo(2), Total: 2, Open: 2, OpenPending: 2},
				{Time: daysAgo(1), Total: 2, Open: 1, OpenPending: 1, Merged: 1},
				{Time: daysAgo(0), Total: 2, Merged: 2},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "multiple changesets open declined reopened merged different times",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(5)),
				bbsChangeset(2, daysAgo(4)),
			},
			start: daysAgo(6),
			events: []Event{
				fakeEvent{t: daysAgo(4), kind: campaigns.ChangesetEventKindBitbucketServerDeclined, id: 1},
				fakeEvent{t: daysAgo(3), kind: campaigns.ChangesetEventKindBitbucketServerDeclined, id: 2},
				fakeEvent{t: daysAgo(3), kind: campaigns.ChangesetEventKindBitbucketServerReopened, id: 1},
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindBitbucketServerReopened, id: 2},
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 1},
				fakeEvent{t: daysAgo(0), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 2},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(6), Total: 0, Open: 0},
				{Time: daysAgo(5), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(4), Total: 2, Open: 1, OpenPending: 1, Closed: 1},
				{Time: daysAgo(3), Total: 2, Open: 1, OpenPending: 1, Closed: 1},
				{Time: daysAgo(2), Total: 2, Open: 2, OpenPending: 2},
				{Time: daysAgo(1), Total: 2, Open: 1, OpenPending: 1, Merged: 1},
				{Time: daysAgo(0), Total: 2, Merged: 2},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset open closed reopened merged, unsorted events",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(4)),
			},
			start: daysAgo(5),
			events: []Event{
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
				fakeEvent{t: daysAgo(3), kind: campaigns.ChangesetEventKindGitHubClosed, id: 1},
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindGitHubReopened, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(5), Total: 0, Open: 0},
				{Time: daysAgo(4), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(3), Total: 1, Open: 0, Closed: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(1), Total: 1, Merged: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset open closed reopened merged, unsorted events",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(4)),
			},
			start: daysAgo(5),
			events: []Event{
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 1},
				fakeEvent{t: daysAgo(3), kind: campaigns.ChangesetEventKindBitbucketServerDeclined, id: 1},
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindBitbucketServerReopened, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(5), Total: 0, Open: 0},
				{Time: daysAgo(4), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(3), Total: 1, Open: 0, Closed: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(1), Total: 1, Merged: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset open, approved, merged",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(3)),
			},
			start: daysAgo(4),
			events: []Event{
				ghReview(1, daysAgo(2), "user1", "APPROVED"),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 0, Open: 0},
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
				{Time: daysAgo(1), Total: 1, Merged: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset open, approved, merged",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(3)),
			},
			start: daysAgo(4),
			events: []Event{
				bbsActivity(1, daysAgo(2), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 0, Open: 0},
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
				{Time: daysAgo(1), Total: 1, Merged: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset open, approved, closed, reopened",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(3)),
			},
			start: daysAgo(3),
			events: []Event{
				ghReview(1, daysAgo(2), "user1", "APPROVED"),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubClosed, id: 1},
				fakeEvent{t: daysAgo(0), kind: campaigns.ChangesetEventKindGitHubReopened, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenApproved: 1},
				{Time: daysAgo(1), Total: 1, Closed: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenApproved: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset open, approved, declined, reopened",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(3)),
			},
			start: daysAgo(3),
			events: []Event{
				bbsActivity(1, daysAgo(2), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerDeclined, id: 1},
				fakeEvent{t: daysAgo(0), kind: campaigns.ChangesetEventKindBitbucketServerReopened, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenApproved: 1},
				{Time: daysAgo(1), Total: 1, Closed: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenApproved: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset open, approved, closed, merged",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(3)),
			},
			start: daysAgo(3),
			events: []Event{
				ghReview(1, daysAgo(2), "user1", "APPROVED"),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubClosed, id: 1},
				fakeEvent{t: daysAgo(0), kind: campaigns.ChangesetEventKindGitHubReopened, id: 1},
				fakeEvent{t: daysAgo(0), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenApproved: 1},
				{Time: daysAgo(1), Total: 1, Closed: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset open, approved, closed, merged",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(3)),
			},
			start: daysAgo(3),
			events: []Event{
				bbsActivity(1, daysAgo(2), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerDeclined, id: 1},
				fakeEvent{t: daysAgo(0), kind: campaigns.ChangesetEventKindBitbucketServerReopened, id: 1},
				fakeEvent{t: daysAgo(0), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenApproved: 1},
				{Time: daysAgo(1), Total: 1, Closed: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset open, changes-requested, closed, reopened",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(3)),
			},
			start: daysAgo(3),
			events: []Event{
				ghReview(1, daysAgo(2), "user1", "CHANGES_REQUESTED"),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubClosed, id: 1},
				fakeEvent{t: daysAgo(0), kind: campaigns.ChangesetEventKindGitHubReopened, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenChangesRequested: 1},
				{Time: daysAgo(1), Total: 1, Closed: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenChangesRequested: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset open, changes-requested, closed, reopened",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(3)),
			},
			start: daysAgo(3),
			events: []Event{
				bbsActivity(1, daysAgo(2), "user1", campaigns.ChangesetEventKindBitbucketServerReviewed),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerDeclined, id: 1},
				fakeEvent{t: daysAgo(0), kind: campaigns.ChangesetEventKindBitbucketServerReopened, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenChangesRequested: 1},
				{Time: daysAgo(1), Total: 1, Closed: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenChangesRequested: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset open, changes-requested, closed, merged",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(3)),
			},
			start: daysAgo(3),
			events: []Event{
				ghReview(1, daysAgo(2), "user1", "CHANGES_REQUESTED"),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubClosed, id: 1},
				fakeEvent{t: daysAgo(0), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenChangesRequested: 1},
				{Time: daysAgo(1), Total: 1, Closed: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset open, comment review, approved, merged",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(3)),
			},
			start: daysAgo(4),
			events: []Event{
				ghReview(1, daysAgo(3), "user1", "COMMENTED"),
				ghReview(1, daysAgo(2), "user2", "APPROVED"),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 0, Open: 0},
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
				{Time: daysAgo(1), Total: 1, Merged: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset open, comment review, approved, merged",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(3)),
			},
			start: daysAgo(4),
			events: []Event{
				bbsActivity(1, daysAgo(3), "user1", campaigns.ChangesetEventKindBitbucketServerCommented),
				bbsActivity(1, daysAgo(2), "user2", campaigns.ChangesetEventKindBitbucketServerApproved),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 0, Open: 0},
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
				{Time: daysAgo(1), Total: 1, Merged: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset multiple approvals counting once",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(1)),
			},
			start: daysAgo(1),
			events: []Event{
				ghReview(1, daysAgo(1), "user1", "APPROVED"),
				ghReview(1, daysAgo(0), "user2", "APPROVED"),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(1), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset multiple approvals counting once",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(1)),
			},
			start: daysAgo(1),
			events: []Event{
				bbsActivity(1, daysAgo(1), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				bbsActivity(1, daysAgo(0), "user2", campaigns.ChangesetEventKindBitbucketServerApproved),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(1), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset multiple changes-requested reviews counting once",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(1)),
			},
			start: daysAgo(1),
			events: []Event{
				ghReview(1, daysAgo(1), "user1", "CHANGES_REQUESTED"),
				ghReview(1, daysAgo(0), "user2", "CHANGES_REQUESTED"),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(1), Total: 1, Open: 1, OpenPending: 0, OpenChangesRequested: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenPending: 0, OpenChangesRequested: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset multiple changes-requested reviews counting once",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(1)),
			},
			start: daysAgo(1),
			events: []Event{
				bbsActivity(1, daysAgo(1), "user1", campaigns.ChangesetEventKindBitbucketServerReviewed),
				bbsActivity(1, daysAgo(0), "user2", campaigns.ChangesetEventKindBitbucketServerReviewed),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(1), Total: 1, Open: 1, OpenPending: 0, OpenChangesRequested: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenPending: 0, OpenChangesRequested: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset open, changes-requested, merged",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(3)),
			},
			start: daysAgo(4),
			events: []Event{
				ghReview(1, daysAgo(2), "user1", "CHANGES_REQUESTED"),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 0, Open: 0},
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 0, OpenChangesRequested: 1},
				{Time: daysAgo(1), Total: 1, Merged: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset open, changes-requested, merged",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(3)),
			},
			start: daysAgo(4),
			events: []Event{
				bbsActivity(1, daysAgo(2), "user1", campaigns.ChangesetEventKindBitbucketServerReviewed),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 1},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 0, Open: 0},
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 0, OpenChangesRequested: 1},
				{Time: daysAgo(1), Total: 1, Merged: 1},
				{Time: daysAgo(0), Total: 1, Merged: 1},
			},
		},
		{
			codehosts: "github",
			name:      "multiple changesets open different review stages before merge",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(6)),
				ghChangeset(2, daysAgo(6)),
				ghChangeset(3, daysAgo(6)),
			},
			start: daysAgo(7),
			events: []Event{
				ghReview(1, daysAgo(5), "user1", "APPROVED"),
				fakeEvent{t: daysAgo(3), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
				ghReview(2, daysAgo(4), "user1", "APPROVED"),
				ghReview(2, daysAgo(3), "user2", "APPROVED"),
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindGitHubMerged, id: 2},
				ghReview(3, daysAgo(2), "user1", "CHANGES_REQUESTED"),
				ghReview(3, daysAgo(1), "user2", "CHANGES_REQUESTED"),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 3},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(7), Total: 0, Open: 0},
				{Time: daysAgo(6), Total: 3, Open: 3, OpenPending: 3},
				{Time: daysAgo(5), Total: 3, Open: 3, OpenPending: 2, OpenApproved: 1},
				{Time: daysAgo(4), Total: 3, Open: 3, OpenPending: 1, OpenApproved: 2},
				{Time: daysAgo(3), Total: 3, Open: 2, OpenPending: 1, OpenApproved: 1, Merged: 1},
				{Time: daysAgo(2), Total: 3, Open: 1, OpenPending: 0, OpenChangesRequested: 1, Merged: 2},
				{Time: daysAgo(1), Total: 3, Merged: 3},
				{Time: daysAgo(0), Total: 3, Merged: 3},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "multiple changesets open different review stages before merge",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(6)),
				bbsChangeset(2, daysAgo(6)),
				bbsChangeset(3, daysAgo(6)),
			},
			start: daysAgo(7),
			events: []Event{
				bbsActivity(1, daysAgo(5), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				fakeEvent{t: daysAgo(3), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 1},
				bbsActivity(2, daysAgo(4), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				bbsActivity(2, daysAgo(3), "user2", campaigns.ChangesetEventKindBitbucketServerApproved),
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 2},
				bbsActivity(3, daysAgo(2), "user1", campaigns.ChangesetEventKindBitbucketServerReviewed),
				bbsActivity(3, daysAgo(1), "user2", campaigns.ChangesetEventKindBitbucketServerReviewed),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 3},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(7), Total: 0, Open: 0},
				{Time: daysAgo(6), Total: 3, Open: 3, OpenPending: 3},
				{Time: daysAgo(5), Total: 3, Open: 3, OpenPending: 2, OpenApproved: 1},
				{Time: daysAgo(4), Total: 3, Open: 3, OpenPending: 1, OpenApproved: 2},
				{Time: daysAgo(3), Total: 3, Open: 2, OpenPending: 1, OpenApproved: 1, Merged: 1},
				{Time: daysAgo(2), Total: 3, Open: 1, OpenPending: 0, OpenChangesRequested: 1, Merged: 2},
				{Time: daysAgo(1), Total: 3, Merged: 3},
				{Time: daysAgo(0), Total: 3, Merged: 3},
			},
		},
		{
			codehosts: "github",
			name:      "time slice of multiple changesets in different stages before merge",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(6)),
				ghChangeset(2, daysAgo(6)),
				ghChangeset(3, daysAgo(6)),
			},
			// Same test as above, except we only look at 3 days in the middle
			start: daysAgo(4),
			end:   daysAgo(2),
			events: []Event{
				ghReview(1, daysAgo(5), "user1", "APPROVED"),
				fakeEvent{t: daysAgo(3), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
				ghReview(2, daysAgo(4), "user1", "APPROVED"),
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindGitHubMerged, id: 2},
				ghReview(3, daysAgo(2), "user1", "CHANGES_REQUESTED"),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 3},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 3, Open: 3, OpenPending: 1, OpenApproved: 2},
				{Time: daysAgo(3), Total: 3, Open: 2, OpenPending: 1, OpenApproved: 1, Merged: 1},
				{Time: daysAgo(2), Total: 3, Open: 1, OpenPending: 0, OpenChangesRequested: 1, Merged: 2},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset with changes-requested then approved by same person",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(1)),
			},
			start: daysAgo(1),
			events: []Event{
				ghReview(1, daysAgo(1), "user1", "CHANGES_REQUESTED"),
				ghReview(1, daysAgo(0), "user1", "APPROVED"),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(1), Total: 1, Open: 1, OpenChangesRequested: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenApproved: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset with changes-requested then approved by same person",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(1)),
			},
			start: daysAgo(1),
			events: []Event{
				bbsActivity(1, daysAgo(1), "user1", campaigns.ChangesetEventKindBitbucketServerReviewed),
				bbsActivity(1, daysAgo(0), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(1), Total: 1, Open: 1, OpenChangesRequested: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenApproved: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset with approved then changes-requested by same person",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(1)),
			},
			start: daysAgo(1),
			events: []Event{
				ghReview(1, daysAgo(1), "user1", "APPROVED"),
				ghReview(1, daysAgo(0), "user1", "CHANGES_REQUESTED"),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(1), Total: 1, Open: 1, OpenApproved: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenChangesRequested: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset with approved then changes-requested by same person",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(1)),
			},
			start: daysAgo(1),
			events: []Event{
				bbsActivity(1, daysAgo(1), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				bbsActivity(1, daysAgo(0), "user1", campaigns.ChangesetEventKindBitbucketServerReviewed),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(1), Total: 1, Open: 1, OpenApproved: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenChangesRequested: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset with approval by one person then changes-requested by another",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(1)),
			},
			start: daysAgo(1),
			events: []Event{
				ghReview(1, daysAgo(1), "user1", "APPROVED"),
				ghReview(1, daysAgo(0), "user2", "CHANGES_REQUESTED"), // This has higher precedence
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(1), Total: 1, Open: 1, OpenApproved: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenChangesRequested: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset with approval by one person then changes-requested by another",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(1)),
			},
			start: daysAgo(1),
			events: []Event{
				bbsActivity(1, daysAgo(1), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				bbsActivity(1, daysAgo(0), "user2", campaigns.ChangesetEventKindBitbucketServerReviewed),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(1), Total: 1, Open: 1, OpenApproved: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenChangesRequested: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset with changes-requested by one person then approval by another",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(1)),
			},
			start: daysAgo(1),
			events: []Event{
				ghReview(1, daysAgo(1), "user1", "CHANGES_REQUESTED"),
				ghReview(1, daysAgo(0), "user2", "APPROVED"),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(1), Total: 1, Open: 1, OpenChangesRequested: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenChangesRequested: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset with changes-requested by one person then approval by another",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(1)),
			},
			start: daysAgo(1),
			events: []Event{
				bbsActivity(1, daysAgo(1), "user1", campaigns.ChangesetEventKindBitbucketServerReviewed),
				bbsActivity(1, daysAgo(0), "user2", campaigns.ChangesetEventKindBitbucketServerApproved),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(1), Total: 1, Open: 1, OpenChangesRequested: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenChangesRequested: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset with changes-requested by one person, approval by another, then approval by first person",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(2)),
			},
			start: daysAgo(2),
			events: []Event{
				ghReview(1, daysAgo(2), "user1", "CHANGES_REQUESTED"),
				ghReview(1, daysAgo(1), "user2", "APPROVED"),
				ghReview(1, daysAgo(0), "user1", "APPROVED"),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(2), Total: 1, Open: 1, OpenChangesRequested: 1},
				{Time: daysAgo(1), Total: 1, Open: 1, OpenChangesRequested: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenApproved: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset with changes-requested by one person, approval by another, then approval by first person",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(2)),
			},
			start: daysAgo(2),
			events: []Event{
				bbsActivity(1, daysAgo(2), "user1", campaigns.ChangesetEventKindBitbucketServerReviewed),
				bbsActivity(1, daysAgo(1), "user2", campaigns.ChangesetEventKindBitbucketServerApproved),
				bbsActivity(1, daysAgo(0), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(2), Total: 1, Open: 1, OpenChangesRequested: 1},
				{Time: daysAgo(1), Total: 1, Open: 1, OpenChangesRequested: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenApproved: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset with approval by one person, changes-requested by another, then changes-requested by first person",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(2)),
			},
			start: daysAgo(2),
			events: []Event{
				ghReview(1, daysAgo(2), "user1", "APPROVED"),
				ghReview(1, daysAgo(1), "user2", "CHANGES_REQUESTED"),
				ghReview(1, daysAgo(0), "user1", "CHANGES_REQUESTED"),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(2), Total: 1, Open: 1, OpenApproved: 1},
				{Time: daysAgo(1), Total: 1, Open: 1, OpenChangesRequested: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenChangesRequested: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset with approval by one person, changes-requested by another, then changes-requested by first person",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(2)),
			},
			start: daysAgo(2),
			events: []Event{
				bbsActivity(1, daysAgo(2), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				bbsActivity(1, daysAgo(1), "user2", campaigns.ChangesetEventKindBitbucketServerReviewed),
				bbsActivity(1, daysAgo(0), "user1", campaigns.ChangesetEventKindBitbucketServerReviewed),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(2), Total: 1, Open: 1, OpenApproved: 1},
				{Time: daysAgo(1), Total: 1, Open: 1, OpenChangesRequested: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenChangesRequested: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset open, approved, unapproved",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(3)),
			},
			start: daysAgo(4),
			events: []Event{
				bbsActivity(1, daysAgo(2), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				bbsActivity(1, daysAgo(1), "user1", campaigns.ChangesetEventKindBitbucketServerUnapproved),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 0, Open: 0},
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
				{Time: daysAgo(1), Total: 1, Open: 1, OpenPending: 1, OpenApproved: 0},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenPending: 1, OpenApproved: 0},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset open, changes requested, approved, unapproved",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(3)),
			},
			start: daysAgo(4),
			events: []Event{
				bbsActivity(1, daysAgo(2), "user1", campaigns.ChangesetEventKindBitbucketServerReviewed),
				bbsActivity(1, daysAgo(1), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				bbsActivity(1, daysAgo(0), "user1", campaigns.ChangesetEventKindBitbucketServerUnapproved),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 0, Open: 0},
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 0, OpenChangesRequested: 1},
				{Time: daysAgo(1), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenPending: 1, OpenApproved: 0},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset open, approved, unapproved, approved by another person",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(3)),
			},
			start: daysAgo(4),
			events: []Event{
				bbsActivity(1, daysAgo(2), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				bbsActivity(1, daysAgo(1), "user1", campaigns.ChangesetEventKindBitbucketServerUnapproved),
				bbsActivity(1, daysAgo(0), "user2", campaigns.ChangesetEventKindBitbucketServerApproved),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 0, Open: 0},
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
				{Time: daysAgo(1), Total: 1, Open: 1, OpenPending: 1, OpenApproved: 0},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
			},
		},
		{
			codehosts: "bitbucketserver",
			name:      "single changeset open, approved, then approved and unapproved by another person",
			changesets: []*campaigns.Changeset{
				bbsChangeset(1, daysAgo(3)),
			},
			start: daysAgo(4),
			events: []Event{
				bbsActivity(1, daysAgo(2), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				bbsActivity(1, daysAgo(1), "user2", campaigns.ChangesetEventKindBitbucketServerApproved),
				bbsActivity(1, daysAgo(0), "user2", campaigns.ChangesetEventKindBitbucketServerUnapproved),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(4), Total: 0, Open: 0},
				{Time: daysAgo(3), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(2), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
				{Time: daysAgo(1), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenPending: 0, OpenApproved: 1},
			},
		},
		{
			codehosts: "github and bitbucketserver",
			name:      "multiple changesets on different code hosts in different review stages before merge",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(6)),
				bbsChangeset(2, daysAgo(6)),
				ghChangeset(3, daysAgo(6)),
				bbsChangeset(4, daysAgo(6)),
				ghChangeset(5, daysAgo(6)),
				bbsChangeset(6, daysAgo(6)),
			},
			start: daysAgo(7),
			events: []Event{
				// GitHub Events
				ghReview(1, daysAgo(5), "user1", "APPROVED"),
				fakeEvent{t: daysAgo(3), kind: campaigns.ChangesetEventKindGitHubMerged, id: 1},
				ghReview(3, daysAgo(4), "user1", "APPROVED"),
				ghReview(3, daysAgo(3), "user2", "APPROVED"),
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindGitHubMerged, id: 3},
				ghReview(5, daysAgo(2), "user1", "CHANGES_REQUESTED"),
				ghReview(5, daysAgo(1), "user2", "CHANGES_REQUESTED"),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindGitHubMerged, id: 5},
				// Bitbucket Server Events
				bbsActivity(2, daysAgo(5), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				fakeEvent{t: daysAgo(3), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 2},
				bbsActivity(4, daysAgo(4), "user1", campaigns.ChangesetEventKindBitbucketServerApproved),
				bbsActivity(4, daysAgo(3), "user2", campaigns.ChangesetEventKindBitbucketServerApproved),
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 4},
				bbsActivity(6, daysAgo(2), "user1", campaigns.ChangesetEventKindBitbucketServerReviewed),
				bbsActivity(6, daysAgo(1), "user2", campaigns.ChangesetEventKindBitbucketServerReviewed),
				fakeEvent{t: daysAgo(1), kind: campaigns.ChangesetEventKindBitbucketServerMerged, id: 6},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(7), Total: 0, Open: 0},
				{Time: daysAgo(6), Total: 6, Open: 6, OpenPending: 6},
				{Time: daysAgo(5), Total: 6, Open: 6, OpenPending: 4, OpenApproved: 2},
				{Time: daysAgo(4), Total: 6, Open: 6, OpenPending: 2, OpenApproved: 4},
				{Time: daysAgo(3), Total: 6, Open: 4, OpenPending: 2, OpenApproved: 2, Merged: 2},
				{Time: daysAgo(2), Total: 6, Open: 2, OpenPending: 0, OpenChangesRequested: 2, Merged: 4},
				{Time: daysAgo(1), Total: 6, Merged: 6},
				{Time: daysAgo(0), Total: 6, Merged: 6},
			},
		},
		{
			codehosts: "github and bitbucketserver",
			name:      "multiple changesets open and deleted",
			changesets: []*campaigns.Changeset{
				setExternalDeletedAt(ghChangeset(1, daysAgo(2)), daysAgo(1)),
				setExternalDeletedAt(bbsChangeset(1, daysAgo(2)), daysAgo(1)),
			},
			start: daysAgo(2),
			want: []*ChangesetCounts{
				{Time: daysAgo(2), Total: 2, Open: 2, OpenPending: 2},
				// We count deleted as closed
				{Time: daysAgo(1), Total: 2, Closed: 2},
				{Time: daysAgo(0), Total: 2, Closed: 2},
			},
		},
		{
			codehosts: "github and bitbucketserver",
			name:      "multiple changesets open, closed and deleted",
			changesets: []*campaigns.Changeset{
				setExternalDeletedAt(ghChangeset(1, daysAgo(3)), daysAgo(1)),
				setExternalDeletedAt(bbsChangeset(2, daysAgo(3)), daysAgo(1)),
			},
			start: daysAgo(3),
			events: []Event{
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindGitHubClosed, id: 1},
				fakeEvent{t: daysAgo(2), kind: campaigns.ChangesetEventKindBitbucketServerDeclined, id: 2},
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(3), Total: 2, Open: 2, OpenPending: 2},
				{Time: daysAgo(2), Total: 2, Closed: 2},
				// We count deleted as closed, so they stay closed
				{Time: daysAgo(1), Total: 2, Closed: 2},
				{Time: daysAgo(0), Total: 2, Closed: 2},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset with changes-requested then dismissed event by same person with dismissed state",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(1)),
			},
			start: daysAgo(1),
			events: []Event{
				// GitHub updates the state of the reviews when they're dismissed
				ghReview(1, daysAgo(0), "user1", "DISMISSED"),
				ghReviewDismissed(1, daysAgo(0), "user2", "user1"),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(1), Total: 1, Open: 1, OpenPending: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenPending: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset with approval by one person, changes-requested by another, then dismissal of changes-requested",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(2)),
			},
			start: daysAgo(2),
			events: []Event{
				ghReview(1, daysAgo(2), "user1", "APPROVED"),
				// GitHub updates the state of the changesets when they're dismissed
				ghReview(1, daysAgo(1), "user2", "DISMISSED"),
				ghReviewDismissed(1, daysAgo(1), "user3", "user2"),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(2), Total: 1, Open: 1, OpenApproved: 1},
				{Time: daysAgo(1), Total: 1, Open: 1, OpenApproved: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenApproved: 1},
			},
		},
		{
			codehosts: "github",
			name:      "single changeset with changes-requested, then another dismissed review by same person",
			changesets: []*campaigns.Changeset{
				ghChangeset(1, daysAgo(1)),
			},
			start: daysAgo(1),
			events: []Event{
				ghReview(1, daysAgo(1), "user1", "CHANGES_REQUESTED"),
				// After a dismissal, GitHub removes all of the author's
				// reviews from the overall review state, which is why we don't
				// want to fall back to "ChangesRequested" even though _that_
				// was not dismissed.
				ghReview(1, daysAgo(0), "user1", "DISMISSED"),
				ghReviewDismissed(1, daysAgo(0), "user2", "user1"),
			},
			want: []*ChangesetCounts{
				{Time: daysAgo(1), Total: 1, Open: 1, OpenChangesRequested: 1},
				{Time: daysAgo(0), Total: 1, Open: 1, OpenPending: 1},
			},
		},
	}

	for _, tc := range tests {
		if tc.codehosts != "" {
			tc.name = tc.codehosts + "/" + tc.name
		}
		t.Run(tc.name, func(t *testing.T) {
			if tc.end.IsZero() {
				tc.end = now
			}

			have, err := CalcCounts(tc.start, tc.end, tc.changesets, tc.events...)
			if err != nil {
				t.Fatal(err)
			}

			if !reflect.DeepEqual(have, tc.want) {
				t.Errorf("wrong counts calculated. diff=%s", cmp.Diff(have, tc.want))
			}
		})
	}
}

type fakeEvent struct {
	t    time.Time
	kind campaigns.ChangesetEventKind
	id   int64
}

func (e fakeEvent) Timestamp() time.Time               { return e.t }
func (e fakeEvent) Type() campaigns.ChangesetEventKind { return e.kind }
func (e fakeEvent) Changeset() int64                   { return e.id }

func ghChangeset(id int64, t time.Time) *campaigns.Changeset {
	return &campaigns.Changeset{ID: id, Metadata: &github.PullRequest{CreatedAt: t}}
}

func bbsChangeset(id int64, t time.Time) *campaigns.Changeset {
	return &campaigns.Changeset{
		ID:       id,
		Metadata: &bitbucketserver.PullRequest{CreatedDate: timeToUnixMilli(t)},
	}
}

func setExternalDeletedAt(c *campaigns.Changeset, t time.Time) *campaigns.Changeset {
	c.SetDeleted()
	c.ExternalDeletedAt = t
	return c
}

func timeToUnixMilli(t time.Time) int {
	return int(t.UnixNano()) / int(time.Millisecond)
}

func ghReview(id int64, t time.Time, login, state string) *campaigns.ChangesetEvent {
	return &campaigns.ChangesetEvent{
		ChangesetID: id,
		Kind:        campaigns.ChangesetEventKindGitHubReviewed,
		Metadata: &github.PullRequestReview{
			UpdatedAt: t,
			State:     state,
			Author: github.Actor{
				Login: login,
			},
		},
	}
}

func ghReviewDismissed(id int64, t time.Time, login, reviewer string) *campaigns.ChangesetEvent {
	return &campaigns.ChangesetEvent{
		ChangesetID: id,
		Kind:        campaigns.ChangesetEventKindGitHubReviewDismissed,
		Metadata: &github.ReviewDismissedEvent{
			CreatedAt: t,
			Actor:     github.Actor{Login: login},
			Review: github.PullRequestReview{
				Author: github.Actor{
					Login: reviewer,
				},
			},
		},
	}
}

func bbsActivity(id int64, t time.Time, username string, kind campaigns.ChangesetEventKind) *campaigns.ChangesetEvent {
	return &campaigns.ChangesetEvent{
		ChangesetID: id,
		Kind:        kind,
		Metadata: &bitbucketserver.Activity{
			CreatedDate: timeToUnixMilli(t),
			User: bitbucketserver.User{
				Name: username,
			},
		},
	}
}
