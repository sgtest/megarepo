package types

import (
	"strconv"
	"strings"
	"time"

	"github.com/inconshreveable/log15"
	"github.com/sourcegraph/go-diff/diff"

	bbcs "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/sources/bitbucketcloud"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/bitbucketcloud"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/bitbucketserver"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitlab"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
	"github.com/sourcegraph/sourcegraph/lib/batches"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// ChangesetState defines the possible states of a Changeset.
// These are displayed in the UI as well.
type ChangesetState string

// ChangesetState constants.
const (
	ChangesetStateUnpublished ChangesetState = "UNPUBLISHED"
	ChangesetStateScheduled   ChangesetState = "SCHEDULED"
	ChangesetStateProcessing  ChangesetState = "PROCESSING"
	ChangesetStateOpen        ChangesetState = "OPEN"
	ChangesetStateDraft       ChangesetState = "DRAFT"
	ChangesetStateClosed      ChangesetState = "CLOSED"
	ChangesetStateMerged      ChangesetState = "MERGED"
	ChangesetStateDeleted     ChangesetState = "DELETED"
	ChangesetStateRetrying    ChangesetState = "RETRYING"
	ChangesetStateFailed      ChangesetState = "FAILED"
)

// Valid returns true if the given ChangesetState is valid.
func (s ChangesetState) Valid() bool {
	switch s {
	case ChangesetStateUnpublished,
		ChangesetStateScheduled,
		ChangesetStateProcessing,
		ChangesetStateOpen,
		ChangesetStateDraft,
		ChangesetStateClosed,
		ChangesetStateMerged,
		ChangesetStateDeleted,
		ChangesetStateRetrying,
		ChangesetStateFailed:
		return true
	default:
		return false
	}
}

// ChangesetPublicationState defines the possible publication states of a Changeset.
type ChangesetPublicationState string

// ChangesetPublicationState constants.
const (
	ChangesetPublicationStateUnpublished ChangesetPublicationState = "UNPUBLISHED"
	ChangesetPublicationStatePublished   ChangesetPublicationState = "PUBLISHED"
)

// Valid returns true if the given ChangesetPublicationState is valid.
func (s ChangesetPublicationState) Valid() bool {
	switch s {
	case ChangesetPublicationStateUnpublished, ChangesetPublicationStatePublished:
		return true
	default:
		return false
	}
}

// Published returns true if the given state is ChangesetPublicationStatePublished.
func (s ChangesetPublicationState) Published() bool { return s == ChangesetPublicationStatePublished }

// Unpublished returns true if the given state is ChangesetPublicationStateUnpublished.
func (s ChangesetPublicationState) Unpublished() bool {
	return s == ChangesetPublicationStateUnpublished
}

type ChangesetUiPublicationState string

var (
	ChangesetUiPublicationStateUnpublished ChangesetUiPublicationState = "UNPUBLISHED"
	ChangesetUiPublicationStateDraft       ChangesetUiPublicationState = "DRAFT"
	ChangesetUiPublicationStatePublished   ChangesetUiPublicationState = "PUBLISHED"
)

func ChangesetUiPublicationStateFromPublishedValue(value batches.PublishedValue) *ChangesetUiPublicationState {
	if value.True() {
		return &ChangesetUiPublicationStatePublished
	} else if value.Draft() {
		return &ChangesetUiPublicationStateDraft
	} else if !value.Nil() {
		return &ChangesetUiPublicationStateUnpublished
	}
	return nil
}

func (s ChangesetUiPublicationState) Valid() bool {
	switch s {
	case ChangesetUiPublicationStateUnpublished,
		ChangesetUiPublicationStateDraft,
		ChangesetUiPublicationStatePublished:
		return true
	default:
		return false
	}
}

// ReconcilerState defines the possible states of a Reconciler.
type ReconcilerState string

// ReconcilerState constants.
const (
	ReconcilerStateScheduled  ReconcilerState = "SCHEDULED"
	ReconcilerStateQueued     ReconcilerState = "QUEUED"
	ReconcilerStateProcessing ReconcilerState = "PROCESSING"
	ReconcilerStateErrored    ReconcilerState = "ERRORED"
	ReconcilerStateFailed     ReconcilerState = "FAILED"
	ReconcilerStateCompleted  ReconcilerState = "COMPLETED"
)

// Valid returns true if the given ReconcilerState is valid.
func (s ReconcilerState) Valid() bool {
	switch s {
	case ReconcilerStateScheduled,
		ReconcilerStateQueued,
		ReconcilerStateProcessing,
		ReconcilerStateErrored,
		ReconcilerStateFailed,
		ReconcilerStateCompleted:
		return true
	default:
		return false
	}
}

// ToDB returns the database representation of the reconciler state. That's
// needed because we want to use UPPERCASE ReconcilerStates in the application
// and GraphQL layer, but need to use lowercase in the database to make it work
// with workerutil.Worker.
func (s ReconcilerState) ToDB() string { return strings.ToLower(string(s)) }

// ChangesetExternalState defines the possible states of a Changeset on a code host.
type ChangesetExternalState string

// ChangesetExternalState constants.
const (
	ChangesetExternalStateDraft   ChangesetExternalState = "DRAFT"
	ChangesetExternalStateOpen    ChangesetExternalState = "OPEN"
	ChangesetExternalStateClosed  ChangesetExternalState = "CLOSED"
	ChangesetExternalStateMerged  ChangesetExternalState = "MERGED"
	ChangesetExternalStateDeleted ChangesetExternalState = "DELETED"
)

// Valid returns true if the given ChangesetExternalState is valid.
func (s ChangesetExternalState) Valid() bool {
	switch s {
	case ChangesetExternalStateOpen,
		ChangesetExternalStateDraft,
		ChangesetExternalStateClosed,
		ChangesetExternalStateMerged,
		ChangesetExternalStateDeleted:
		return true
	default:
		return false
	}
}

// ChangesetLabel represents a label applied to a changeset
type ChangesetLabel struct {
	Name        string
	Color       string
	Description string
}

// ChangesetReviewState defines the possible states of a Changeset's review.
type ChangesetReviewState string

// ChangesetReviewState constants.
const (
	ChangesetReviewStateApproved         ChangesetReviewState = "APPROVED"
	ChangesetReviewStateChangesRequested ChangesetReviewState = "CHANGES_REQUESTED"
	ChangesetReviewStatePending          ChangesetReviewState = "PENDING"
	ChangesetReviewStateCommented        ChangesetReviewState = "COMMENTED"
	ChangesetReviewStateDismissed        ChangesetReviewState = "DISMISSED"
)

// Valid returns true if the given Changeset review state is valid.
func (s ChangesetReviewState) Valid() bool {
	switch s {
	case ChangesetReviewStateApproved,
		ChangesetReviewStateChangesRequested,
		ChangesetReviewStatePending,
		ChangesetReviewStateCommented,
		ChangesetReviewStateDismissed:
		return true
	default:
		return false
	}
}

// ChangesetCheckState constants.
type ChangesetCheckState string

const (
	ChangesetCheckStateUnknown ChangesetCheckState = "UNKNOWN"
	ChangesetCheckStatePending ChangesetCheckState = "PENDING"
	ChangesetCheckStatePassed  ChangesetCheckState = "PASSED"
	ChangesetCheckStateFailed  ChangesetCheckState = "FAILED"
)

// Valid returns true if the given Changeset check state is valid.
func (s ChangesetCheckState) Valid() bool {
	switch s {
	case ChangesetCheckStateUnknown,
		ChangesetCheckStatePending,
		ChangesetCheckStatePassed,
		ChangesetCheckStateFailed:
		return true
	default:
		return false
	}
}

// BatchChangeAssoc stores the details of a association to a BatchChange.
type BatchChangeAssoc struct {
	BatchChangeID int64 `json:"-"`
	Detach        bool  `json:"detach,omitempty"`
	Archive       bool  `json:"archive,omitempty"`
	IsArchived    bool  `json:"isArchived,omitempty"`
}

// A Changeset is a changeset on a code host belonging to a Repository and many
// BatchChanges.
type Changeset struct {
	ID                  int64
	RepoID              api.RepoID
	CreatedAt           time.Time
	UpdatedAt           time.Time
	Metadata            interface{}
	BatchChanges        []BatchChangeAssoc
	ExternalID          string
	ExternalServiceType string
	// ExternalBranch should always be prefixed with refs/heads/. Call git.EnsureRefPrefix before setting this value.
	ExternalBranch string
	// ExternalForkNamespace is only set if the changeset is opened on a fork.
	ExternalForkNamespace string
	ExternalDeletedAt     time.Time
	ExternalUpdatedAt     time.Time
	ExternalState         ChangesetExternalState
	ExternalReviewState   ChangesetReviewState
	ExternalCheckState    ChangesetCheckState
	DiffStatAdded         *int32
	DiffStatChanged       *int32
	DiffStatDeleted       *int32
	SyncState             ChangesetSyncState

	// The batch change that "owns" this changeset: it can create/close
	// it on code host. If this is 0, it is imported/tracked by a batch change.
	OwnedByBatchChangeID int64

	// This is 0 if the Changeset isn't owned by Sourcegraph.
	CurrentSpecID  int64
	PreviousSpecID int64

	PublicationState   ChangesetPublicationState // "unpublished", "published"
	UiPublicationState *ChangesetUiPublicationState

	// All of the following fields are used by workerutil.Worker.
	ReconcilerState  ReconcilerState
	FailureMessage   *string
	StartedAt        time.Time
	FinishedAt       time.Time
	ProcessAfter     time.Time
	NumResets        int64
	NumFailures      int64
	SyncErrorMessage *string

	// Closing is set to true (along with the ReocncilerState) when the
	// reconciler should close the changeset.
	Closing bool
}

// RecordID is needed to implement the workerutil.Record interface.
func (c *Changeset) RecordID() int { return int(c.ID) }

// Clone returns a clone of a Changeset.
func (c *Changeset) Clone() *Changeset {
	tt := *c
	tt.BatchChanges = make([]BatchChangeAssoc, len(c.BatchChanges))
	copy(tt.BatchChanges, c.BatchChanges)
	return &tt
}

// Closeable returns whether the Changeset is already closed or merged.
func (c *Changeset) Closeable() bool {
	return c.ExternalState != ChangesetExternalStateClosed &&
		c.ExternalState != ChangesetExternalStateMerged
}

// Complete returns whether the Changeset has been published and its
// ExternalState is in a final state.
func (c *Changeset) Complete() bool {
	return c.Published() && c.ExternalState != ChangesetExternalStateOpen &&
		c.ExternalState != ChangesetExternalStateDraft
}

// Published returns whether the Changeset's PublicationState is Published.
func (c *Changeset) Published() bool { return c.PublicationState.Published() }

// Unpublished returns whether the Changeset's PublicationState is Unpublished.
func (c *Changeset) Unpublished() bool { return c.PublicationState.Unpublished() }

// IsImporting returns whether the Changeset is being imported but it's not finished yet.
func (c *Changeset) IsImporting() bool { return c.Unpublished() && c.CurrentSpecID == 0 }

// SetCurrentSpec sets the CurrentSpecID field and copies the diff stat over from the spec.
func (c *Changeset) SetCurrentSpec(spec *ChangesetSpec) {
	c.CurrentSpecID = spec.ID

	// Copy over diff stat from the spec.
	diffStat := spec.DiffStat()
	c.SetDiffStat(&diffStat)
}

// DiffStat returns a *diff.Stat if DiffStatAdded, DiffStatChanged, and
// DiffStatDeleted are set, or nil if one or more is not.
func (c *Changeset) DiffStat() *diff.Stat {
	if c.DiffStatAdded == nil || c.DiffStatChanged == nil || c.DiffStatDeleted == nil {
		return nil
	}

	return &diff.Stat{
		Added:   *c.DiffStatAdded,
		Changed: *c.DiffStatChanged,
		Deleted: *c.DiffStatDeleted,
	}
}

func (c *Changeset) SetDiffStat(stat *diff.Stat) {
	if stat == nil {
		c.DiffStatAdded = nil
		c.DiffStatChanged = nil
		c.DiffStatDeleted = nil
	} else {
		added := stat.Added
		c.DiffStatAdded = &added

		changed := stat.Changed
		c.DiffStatChanged = &changed

		deleted := stat.Deleted
		c.DiffStatDeleted = &deleted
	}
}

func (c *Changeset) SetMetadata(meta interface{}) error {
	switch pr := meta.(type) {
	case *github.PullRequest:
		c.Metadata = pr
		c.ExternalID = strconv.FormatInt(pr.Number, 10)
		c.ExternalServiceType = extsvc.TypeGitHub
		c.ExternalBranch = git.EnsureRefPrefix(pr.HeadRefName)
		c.ExternalUpdatedAt = pr.UpdatedAt

		if pr.BaseRepository.ID != pr.HeadRepository.ID {
			c.ExternalForkNamespace = pr.HeadRepository.Owner.Login
		} else {
			c.ExternalForkNamespace = ""
		}
	case *bitbucketserver.PullRequest:
		c.Metadata = pr
		c.ExternalID = strconv.FormatInt(int64(pr.ID), 10)
		c.ExternalServiceType = extsvc.TypeBitbucketServer
		c.ExternalBranch = git.EnsureRefPrefix(pr.FromRef.ID)
		c.ExternalUpdatedAt = unixMilliToTime(int64(pr.UpdatedDate))

		if pr.FromRef.Repository.ID != pr.ToRef.Repository.ID {
			c.ExternalForkNamespace = pr.FromRef.Repository.Project.Key
		} else {
			c.ExternalForkNamespace = ""
		}
	case *gitlab.MergeRequest:
		c.Metadata = pr
		c.ExternalID = strconv.FormatInt(int64(pr.IID), 10)
		c.ExternalServiceType = extsvc.TypeGitLab
		c.ExternalBranch = git.EnsureRefPrefix(pr.SourceBranch)
		c.ExternalUpdatedAt = pr.UpdatedAt.Time
		c.ExternalForkNamespace = pr.SourceProjectNamespace
	case *bbcs.AnnotatedPullRequest:
		c.Metadata = pr
		c.ExternalID = strconv.FormatInt(pr.ID, 10)
		c.ExternalServiceType = extsvc.TypeBitbucketCloud
		c.ExternalBranch = git.EnsureRefPrefix(pr.Source.Branch.Name)
		c.ExternalUpdatedAt = pr.UpdatedOn

		if pr.Source.Repo.UUID != pr.Destination.Repo.UUID {
			namespace, err := pr.Source.Repo.Namespace()
			if err != nil {
				return errors.Wrap(err, "determining fork namespace")
			}
			c.ExternalForkNamespace = namespace
		} else {
			c.ExternalForkNamespace = ""
		}
	default:
		return errors.New("unknown changeset type")
	}
	return nil
}

// RemoveBatchChangeID removes the given id from the Changesets BatchChangesIDs slice.
// If the id is not in BatchChangesIDs calling this method doesn't have an effect.
func (c *Changeset) RemoveBatchChangeID(id int64) {
	for i := len(c.BatchChanges) - 1; i >= 0; i-- {
		if c.BatchChanges[i].BatchChangeID == id {
			c.BatchChanges = append(c.BatchChanges[:i], c.BatchChanges[i+1:]...)
		}
	}
}

// Title of the Changeset.
func (c *Changeset) Title() (string, error) {
	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		return m.Title, nil
	case *bitbucketserver.PullRequest:
		return m.Title, nil
	case *gitlab.MergeRequest:
		return m.Title, nil
	case *bbcs.AnnotatedPullRequest:
		return m.Title, nil
	default:
		return "", errors.New("unknown changeset type")
	}
}

// AuthorName of the Changeset.
func (c *Changeset) AuthorName() (string, error) {
	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		return m.Author.Login, nil
	case *bitbucketserver.PullRequest:
		if m.Author.User == nil {
			return "", nil
		}
		return m.Author.User.Name, nil
	case *gitlab.MergeRequest:
		return m.Author.Username, nil
	case *bbcs.AnnotatedPullRequest:
		return m.Author.Username, nil
	default:
		return "", errors.New("unknown changeset type")
	}
}

// AuthorEmail of the Changeset.
func (c *Changeset) AuthorEmail() (string, error) {
	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		// For GitHub we can't get the email of the actor without
		// expanding the token scope by `user:email`. Since the email
		// is only a nice-to-have for mapping the GitHub user against
		// a Sourcegraph user, we wait until there is a bigger reason
		// to have users reconfigure token scopes. Once we ask users for
		// that scope as well, we should return it here.
		return "", nil
	case *bitbucketserver.PullRequest:
		if m.Author.User == nil {
			return "", nil
		}
		return m.Author.User.EmailAddress, nil
	case *gitlab.MergeRequest:
		return m.Author.Email, nil
	case *bbcs.AnnotatedPullRequest:
		// Bitbucket Cloud does not provide the e-mail of the author under any
		// circumstances.
		return "", nil
	default:
		return "", errors.New("unknown changeset type")
	}
}

// ExternalCreatedAt is when the Changeset was created on the codehost. When it
// cannot be determined when the changeset was created, a zero-value timestamp
// is returned.
func (c *Changeset) ExternalCreatedAt() time.Time {
	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		return m.CreatedAt
	case *bitbucketserver.PullRequest:
		return unixMilliToTime(int64(m.CreatedDate))
	case *gitlab.MergeRequest:
		return m.CreatedAt.Time
	case *bbcs.AnnotatedPullRequest:
		return m.CreatedOn
	default:
		return time.Time{}
	}
}

// Body of the Changeset.
func (c *Changeset) Body() (string, error) {
	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		return m.Body, nil
	case *bitbucketserver.PullRequest:
		return m.Description, nil
	case *gitlab.MergeRequest:
		return m.Description, nil
	case *bbcs.AnnotatedPullRequest:
		return m.Rendered.Description.Raw, nil
	default:
		return "", errors.New("unknown changeset type")
	}
}

// SetDeleted sets the internal state of a Changeset so that its State is
// ChangesetStateDeleted.
func (c *Changeset) SetDeleted() {
	c.ExternalDeletedAt = timeutil.Now()
}

// IsDeleted returns true when the Changeset's ExternalDeletedAt is a non-zero
// timestamp.
func (c *Changeset) IsDeleted() bool {
	return !c.ExternalDeletedAt.IsZero()
}

// HasDiff returns true when the changeset is in an open state. That is because
// currently we do not support diff rendering for historic branches, because we
// can't guarantee that we have the refs on gitserver.
func (c *Changeset) HasDiff() bool {
	return c.ExternalState == ChangesetExternalStateDraft || c.ExternalState == ChangesetExternalStateOpen
}

// URL of a Changeset.
func (c *Changeset) URL() (s string, err error) {
	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		return m.URL, nil
	case *bitbucketserver.PullRequest:
		if len(m.Links.Self) < 1 {
			return "", errors.New("bitbucketserver pull request has no self links")
		}
		selfLink := m.Links.Self[0]
		return selfLink.Href, nil
	case *gitlab.MergeRequest:
		return m.WebURL, nil
	case *bbcs.AnnotatedPullRequest:
		if link, ok := m.Links["html"]; ok {
			return link.Href, nil
		}
		// We could probably synthesise the URL based on the repo URL and the
		// pull request ID, but since the link _should_ be there, we'll error
		// instead.
		return "", errors.New("Bitbucket Cloud pull request does not have a html link")
	default:
		return "", errors.New("unknown changeset type")
	}
}

// Events returns the deduplicated list of ChangesetEvents from the Changeset's metadata.
func (c *Changeset) Events() (events []*ChangesetEvent, err error) {
	uniqueEvents := make(map[string]struct{})

	appendEvent := func(e *ChangesetEvent) {
		k := string(e.Kind) + e.Key
		if _, ok := uniqueEvents[k]; ok {
			log15.Info("dropping duplicate changeset event", "changeset_id", e.ChangesetID, "kind", e.Kind, "key", e.Key)
			return
		}
		uniqueEvents[k] = struct{}{}
		events = append(events, e)
	}

	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		events = make([]*ChangesetEvent, 0, len(m.TimelineItems))
		for _, ti := range m.TimelineItems {
			ev := ChangesetEvent{ChangesetID: c.ID}

			switch e := ti.Item.(type) {
			case *github.PullRequestReviewThread:
				for _, c := range e.Comments {
					ev := ev
					ev.Key = c.Key()
					if ev.Kind, err = ChangesetEventKindFor(c); err != nil {
						return
					}
					ev.Metadata = c
					appendEvent(&ev)
				}

			case *github.ReviewRequestedEvent:
				// If the reviewer of a ReviewRequestedEvent has been deleted,
				// the fields are blank and we cannot match the event to an
				// entry in the database and/or reliably use it, so we drop it.
				if e.ReviewerDeleted() {
					continue
				}
				ev.Key = e.Key()
				if ev.Kind, err = ChangesetEventKindFor(e); err != nil {
					return
				}
				ev.Metadata = e
				appendEvent(&ev)

			default:
				ev.Key = ti.Item.(Keyer).Key()
				if ev.Kind, err = ChangesetEventKindFor(ti.Item); err != nil {
					return
				}
				ev.Metadata = ti.Item
				appendEvent(&ev)
			}
		}

	case *bitbucketserver.PullRequest:
		events = make([]*ChangesetEvent, 0, len(m.Activities)+len(m.CommitStatus))

		addEvent := func(e Keyer) error {
			kind, err := ChangesetEventKindFor(e)
			if err != nil {
				return err
			}

			appendEvent(&ChangesetEvent{
				ChangesetID: c.ID,
				Key:         e.Key(),
				Kind:        kind,
				Metadata:    e,
			})
			return nil
		}
		for _, a := range m.Activities {
			if err = addEvent(a); err != nil {
				return
			}
		}
		for _, s := range m.CommitStatus {
			if err = addEvent(s); err != nil {
				return
			}
		}

	case *gitlab.MergeRequest:
		events = make([]*ChangesetEvent, 0, len(m.Notes)+len(m.ResourceStateEvents)+len(m.Pipelines))
		var kind ChangesetEventKind

		for _, note := range m.Notes {
			if event := note.ToEvent(); event != nil {
				if kind, err = ChangesetEventKindFor(event); err != nil {
					return
				}
				appendEvent(&ChangesetEvent{
					ChangesetID: c.ID,
					Key:         event.(Keyer).Key(),
					Kind:        kind,
					Metadata:    event,
				})
			}
		}

		for _, e := range m.ResourceStateEvents {
			if event := e.ToEvent(); event != nil {
				if kind, err = ChangesetEventKindFor(event); err != nil {
					return
				}
				appendEvent(&ChangesetEvent{
					ChangesetID: c.ID,
					Key:         event.(Keyer).Key(),
					Kind:        kind,
					Metadata:    event,
				})
			}
		}

		for _, pipeline := range m.Pipelines {
			if kind, err = ChangesetEventKindFor(pipeline); err != nil {
				return
			}
			appendEvent(&ChangesetEvent{
				ChangesetID: c.ID,
				Key:         pipeline.Key(),
				Kind:        kind,
				Metadata:    pipeline,
			})
		}

	case *bbcs.AnnotatedPullRequest:
		// There are two types of event that we create from an annotated pull
		// request: review events, based on the participants within the pull
		// request, and check events, based on the commit statuses.
		//
		// Unlike some other code host types, we don't need to handle general
		// comments, as we can access the historical data required through more
		// specialised APIs.

		var kind ChangesetEventKind

		for _, participant := range m.Participants {
			if kind, err = ChangesetEventKindFor(&participant); err != nil {
				return
			}
			appendEvent(&ChangesetEvent{
				ChangesetID: c.ID,
				// There's no unique ID within the participant structure itself,
				// but the combination of the user UUID, the repo UUID, and the
				// PR ID should be unique. We can't implement this as a Keyer on
				// the participant because it requires knowledge of things
				// outside the struct.
				Key:      m.Destination.Repo.UUID + ":" + strconv.FormatInt(m.ID, 10) + ":" + participant.User.UUID,
				Kind:     kind,
				Metadata: participant,
			})
		}

		for _, status := range m.Statuses {
			if kind, err = ChangesetEventKindFor(status); err != nil {
				return
			}
			appendEvent(&ChangesetEvent{
				ChangesetID: c.ID,
				Key:         status.UUID,
				Kind:        kind,
				Metadata:    status,
			})
		}
	}
	return events, nil
}

// HeadRefOid returns the git ObjectID of the HEAD reference associated with
// Changeset on the codehost. If the codehost doesn't include the ObjectID, an
// empty string is returned.
func (c *Changeset) HeadRefOid() (string, error) {
	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		return m.HeadRefOid, nil
	case *bitbucketserver.PullRequest:
		return "", nil
	case *gitlab.MergeRequest:
		return m.DiffRefs.HeadSHA, nil
	case *bbcs.AnnotatedPullRequest:
		return m.Source.Commit.Hash, nil
	default:
		return "", errors.New("unknown changeset type")
	}
}

// HeadRef returns the full ref (e.g. `refs/heads/my-branch`) of the
// HEAD reference associated with the Changeset on the codehost.
func (c *Changeset) HeadRef() (string, error) {
	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		return "refs/heads/" + m.HeadRefName, nil
	case *bitbucketserver.PullRequest:
		return m.FromRef.ID, nil
	case *gitlab.MergeRequest:
		return "refs/heads/" + m.SourceBranch, nil
	case *bbcs.AnnotatedPullRequest:
		return "refs/heads/" + m.Source.Branch.Name, nil
	default:
		return "", errors.New("unknown changeset type")
	}
}

// BaseRefOid returns the git ObjectID of the base reference associated with the
// Changeset on the codehost. If the codehost doesn't include the ObjectID, an
// empty string is returned.
func (c *Changeset) BaseRefOid() (string, error) {
	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		return m.BaseRefOid, nil
	case *bitbucketserver.PullRequest:
		return "", nil
	case *gitlab.MergeRequest:
		return m.DiffRefs.BaseSHA, nil
	case *bbcs.AnnotatedPullRequest:
		return m.Destination.Commit.Hash, nil
	default:
		return "", errors.New("unknown changeset type")
	}
}

// BaseRef returns the full ref (e.g. `refs/heads/my-branch`) of the base ref
// associated with the Changeset on the codehost.
func (c *Changeset) BaseRef() (string, error) {
	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		return "refs/heads/" + m.BaseRefName, nil
	case *bitbucketserver.PullRequest:
		return m.ToRef.ID, nil
	case *gitlab.MergeRequest:
		return "refs/heads/" + m.TargetBranch, nil
	case *bbcs.AnnotatedPullRequest:
		return "refs/heads/" + m.Destination.Branch.Name, nil
	default:
		return "", errors.New("unknown changeset type")
	}
}

// AttachedTo returns true if the changeset is currently attached to the batch
// change with the given batchChangeID.
func (c *Changeset) AttachedTo(batchChangeID int64) bool {
	for _, assoc := range c.BatchChanges {
		if assoc.BatchChangeID == batchChangeID {
			return true
		}
	}
	return false
}

// Attach attaches the batch change with the given ID to the changeset.
// If the batch change is already attached, this is a noop.
// If the batch change is still attached but is marked as to be detached,
// the detach flag is removed.
func (c *Changeset) Attach(batchChangeID int64) {
	for i := range c.BatchChanges {
		if c.BatchChanges[i].BatchChangeID == batchChangeID {
			c.BatchChanges[i].Detach = false
			c.BatchChanges[i].IsArchived = false
			c.BatchChanges[i].Archive = false
			return
		}
	}
	c.BatchChanges = append(c.BatchChanges, BatchChangeAssoc{BatchChangeID: batchChangeID})
}

// Detach marks the given batch change as to-be-detached. Returns true, if the
// batch change currently is attached to the batch change. This function is a noop,
// if the given batch change was not attached to the changeset.
func (c *Changeset) Detach(batchChangeID int64) bool {
	for i := range c.BatchChanges {
		if c.BatchChanges[i].BatchChangeID == batchChangeID {
			c.BatchChanges[i].Detach = true
			return true
		}
	}
	return false
}

// Archive marks the given batch change as to-be-archived. Returns true, if the
// batch change currently is attached to the batch change and *not* archived.
// This function is a noop, if the given changeset was already archived.
func (c *Changeset) Archive(batchChangeID int64) bool {
	for i := range c.BatchChanges {
		if c.BatchChanges[i].BatchChangeID == batchChangeID && !c.BatchChanges[i].IsArchived {
			c.BatchChanges[i].Archive = true
			return true
		}
	}
	return false
}

// ArchivedIn checks whether the changeset is archived in the given batch change.
func (c *Changeset) ArchivedIn(batchChangeID int64) bool {
	for i := range c.BatchChanges {
		if c.BatchChanges[i].BatchChangeID == batchChangeID && c.BatchChanges[i].IsArchived {
			return true
		}
	}
	return false
}

// SupportsLabels returns whether the code host on which the changeset is
// hosted supports labels and whether it's safe to call the
// (*Changeset).Labels() method.
func (c *Changeset) SupportsLabels() bool {
	return ExternalServiceSupports(c.ExternalServiceType, CodehostCapabilityLabels)
}

// SupportsDraft returns whether the code host on which the changeset is
// hosted supports draft changesets.
func (c *Changeset) SupportsDraft() bool {
	return ExternalServiceSupports(c.ExternalServiceType, CodehostCapabilityDraftChangesets)
}

func (c *Changeset) Labels() []ChangesetLabel {
	switch m := c.Metadata.(type) {
	case *github.PullRequest:
		labels := make([]ChangesetLabel, len(m.Labels.Nodes))
		for i, l := range m.Labels.Nodes {
			labels[i] = ChangesetLabel{
				Name:        l.Name,
				Color:       l.Color,
				Description: l.Description,
			}
		}
		return labels
	case *gitlab.MergeRequest:
		// Similarly to GitHub above, GitLab labels can have colors (foreground
		// _and_ background, in fact) and descriptions. Unfortunately, the REST
		// API only returns this level of detail on the list endpoint (with an
		// option added in GitLab 12.7), and not when retrieving individual MRs.
		//
		// When our minimum GitLab version is 12.0, we should be able to switch
		// to retrieving MRs via GraphQL, and then we can start retrieving
		// richer label data.
		labels := make([]ChangesetLabel, len(m.Labels))
		for i, l := range m.Labels {
			labels[i] = ChangesetLabel{Name: l, Color: "000000"}
		}
		return labels
	default:
		return []ChangesetLabel{}
	}
}

// ResetReconcilerState resets the failure message and reset count and sets the
// changeset's ReconcilerState to the given value.
func (c *Changeset) ResetReconcilerState(state ReconcilerState) {
	c.ReconcilerState = state
	c.NumResets = 0
	c.NumFailures = 0
	c.FailureMessage = nil
	// The reconciler syncs where needed, so we reset this message.
	c.SyncErrorMessage = nil
}

// Changesets is a slice of *Changesets.
type Changesets []*Changeset

// IDs returns the IDs of all changesets in the slice.
func (cs Changesets) IDs() []int64 {
	ids := make([]int64, len(cs))
	for i, c := range cs {
		ids[i] = c.ID
	}
	return ids
}

// IDs returns the unique RepoIDs of all changesets in the slice.
func (cs Changesets) RepoIDs() []api.RepoID {
	repoIDMap := make(map[api.RepoID]struct{})
	for _, c := range cs {
		repoIDMap[c.RepoID] = struct{}{}
	}
	repoIDs := make([]api.RepoID, 0, len(repoIDMap))
	for id := range repoIDMap {
		repoIDs = append(repoIDs, id)
	}
	return repoIDs
}

// Filter returns a new Changesets slice in which changesets have been filtered
// out for which the predicate didn't return true.
func (cs Changesets) Filter(predicate func(*Changeset) bool) (filtered Changesets) {
	for _, c := range cs {
		if predicate(c) {
			filtered = append(filtered, c)
		}
	}

	return filtered
}

// Find returns the first changeset in the slice for which the predicate
// returned true.
func (cs Changesets) Find(predicate func(*Changeset) bool) *Changeset {
	for _, c := range cs {
		if predicate(c) {
			return c
		}
	}

	return nil
}

// WithCurrentSpecID returns a predicate function that can be passed to
// Changesets.Filter/Find, etc.
func WithCurrentSpecID(id int64) func(*Changeset) bool {
	return func(c *Changeset) bool { return c.CurrentSpecID == id }
}

// WithExternalID returns a predicate function that can be passed to
// Changesets.Filter/Find, etc.
func WithExternalID(id string) func(*Changeset) bool {
	return func(c *Changeset) bool { return c.ExternalID == id }
}

type CommonChangesetsStats struct {
	Unpublished int32
	Draft       int32
	Open        int32
	Merged      int32
	Closed      int32
	Total       int32
}

// RepoChangesetsStats holds stats information on a list of changesets for a repo.
type RepoChangesetsStats struct {
	CommonChangesetsStats
}

// ChangesetsStats holds additional stats information on a list of changesets.
type ChangesetsStats struct {
	CommonChangesetsStats
	Retrying   int32
	Failed     int32
	Scheduled  int32
	Processing int32
	Deleted    int32
	Archived   int32
}

// ChangesetEventKindFor returns the ChangesetEventKind for the given
// specific code host event.
func ChangesetEventKindFor(e interface{}) (ChangesetEventKind, error) {
	switch e := e.(type) {
	case *github.AssignedEvent:
		return ChangesetEventKindGitHubAssigned, nil
	case *github.ClosedEvent:
		return ChangesetEventKindGitHubClosed, nil
	case *github.IssueComment:
		return ChangesetEventKindGitHubCommented, nil
	case *github.RenamedTitleEvent:
		return ChangesetEventKindGitHubRenamedTitle, nil
	case *github.MergedEvent:
		return ChangesetEventKindGitHubMerged, nil
	case *github.PullRequestReview:
		return ChangesetEventKindGitHubReviewed, nil
	case *github.PullRequestReviewComment:
		return ChangesetEventKindGitHubReviewCommented, nil
	case *github.ReopenedEvent:
		return ChangesetEventKindGitHubReopened, nil
	case *github.ReviewDismissedEvent:
		return ChangesetEventKindGitHubReviewDismissed, nil
	case *github.ReviewRequestRemovedEvent:
		return ChangesetEventKindGitHubReviewRequestRemoved, nil
	case *github.ReviewRequestedEvent:
		return ChangesetEventKindGitHubReviewRequested, nil
	case *github.ReadyForReviewEvent:
		return ChangesetEventKindGitHubReadyForReview, nil
	case *github.ConvertToDraftEvent:
		return ChangesetEventKindGitHubConvertToDraft, nil
	case *github.UnassignedEvent:
		return ChangesetEventKindGitHubUnassigned, nil
	case *github.PullRequestCommit:
		return ChangesetEventKindGitHubCommit, nil
	case *github.LabelEvent:
		if e.Removed {
			return ChangesetEventKindGitHubUnlabeled, nil
		}
		return ChangesetEventKindGitHubLabeled, nil
	case *github.CommitStatus:
		return ChangesetEventKindCommitStatus, nil
	case *github.CheckSuite:
		return ChangesetEventKindCheckSuite, nil
	case *github.CheckRun:
		return ChangesetEventKindCheckRun, nil
	case *bitbucketserver.Activity:
		return ChangesetEventKind("bitbucketserver:" + strings.ToLower(string(e.Action))), nil
	case *bitbucketserver.ParticipantStatusEvent:
		return ChangesetEventKind("bitbucketserver:participant_status:" + strings.ToLower(string(e.Action))), nil
	case *bitbucketserver.CommitStatus:
		return ChangesetEventKindBitbucketServerCommitStatus, nil
	case *gitlab.Pipeline:
		return ChangesetEventKindGitLabPipeline, nil
	case *gitlab.ReviewApprovedEvent:
		return ChangesetEventKindGitLabApproved, nil
	case *gitlab.ReviewUnapprovedEvent:
		return ChangesetEventKindGitLabUnapproved, nil
	case *gitlab.MarkWorkInProgressEvent:
		return ChangesetEventKindGitLabMarkWorkInProgress, nil
	case *gitlab.UnmarkWorkInProgressEvent:
		return ChangesetEventKindGitLabUnmarkWorkInProgress, nil

	case *gitlab.MergeRequestClosedEvent:
		return ChangesetEventKindGitLabClosed, nil
	case *gitlab.MergeRequestReopenedEvent:
		return ChangesetEventKindGitLabReopened, nil
	case *gitlab.MergeRequestMergedEvent:
		return ChangesetEventKindGitLabMerged, nil

	case *bitbucketcloud.Participant:
		switch e.State {
		case bitbucketcloud.ParticipantStateApproved:
			return ChangesetEventKindBitbucketCloudApproved, nil
		case bitbucketcloud.ParticipantStateChangesRequested:
			return ChangesetEventKindBitbucketCloudChangesRequested, nil
		default:
			return ChangesetEventKindBitbucketCloudReviewed, nil
		}
	case *bitbucketcloud.PullRequestStatus:
		return ChangesetEventKindBitbucketCloudCommitStatus, nil
	}

	return ChangesetEventKindInvalid, errors.Errorf("unknown changeset event kind for %T", e)
}

// NewChangesetEventMetadata returns a new metadata object for the given
// ChangesetEventKind.
func NewChangesetEventMetadata(k ChangesetEventKind) (interface{}, error) {
	switch {
	case strings.HasPrefix(string(k), "bitbucketcloud"):
		switch k {
		case ChangesetEventKindBitbucketCloudApproved,
			ChangesetEventKindBitbucketCloudChangesRequested,
			ChangesetEventKindBitbucketCloudReviewed:
			return new(bitbucketcloud.Participant), nil
		case ChangesetEventKindBitbucketCloudCommitStatus:
			return new(bitbucketcloud.PullRequestStatus), nil
		}
	case strings.HasPrefix(string(k), "bitbucketserver"):
		switch k {
		case ChangesetEventKindBitbucketServerCommitStatus:
			return new(bitbucketserver.CommitStatus), nil
		case ChangesetEventKindBitbucketServerDismissed:
			return new(bitbucketserver.ParticipantStatusEvent), nil
		default:
			return new(bitbucketserver.Activity), nil
		}
	case strings.HasPrefix(string(k), "github"):
		switch k {
		case ChangesetEventKindGitHubAssigned:
			return new(github.AssignedEvent), nil
		case ChangesetEventKindGitHubClosed:
			return new(github.ClosedEvent), nil
		case ChangesetEventKindGitHubCommented:
			return new(github.IssueComment), nil
		case ChangesetEventKindGitHubRenamedTitle:
			return new(github.RenamedTitleEvent), nil
		case ChangesetEventKindGitHubMerged:
			return new(github.MergedEvent), nil
		case ChangesetEventKindGitHubReviewed:
			return new(github.PullRequestReview), nil
		case ChangesetEventKindGitHubReviewCommented:
			return new(github.PullRequestReviewComment), nil
		case ChangesetEventKindGitHubReopened:
			return new(github.ReopenedEvent), nil
		case ChangesetEventKindGitHubReviewDismissed:
			return new(github.ReviewDismissedEvent), nil
		case ChangesetEventKindGitHubReviewRequestRemoved:
			return new(github.ReviewRequestRemovedEvent), nil
		case ChangesetEventKindGitHubReviewRequested:
			return new(github.ReviewRequestedEvent), nil
		case ChangesetEventKindGitHubReadyForReview:
			return new(github.ReadyForReviewEvent), nil
		case ChangesetEventKindGitHubConvertToDraft:
			return new(github.ConvertToDraftEvent), nil
		case ChangesetEventKindGitHubUnassigned:
			return new(github.UnassignedEvent), nil
		case ChangesetEventKindGitHubCommit:
			return new(github.PullRequestCommit), nil
		case ChangesetEventKindGitHubLabeled:
			return new(github.LabelEvent), nil
		case ChangesetEventKindGitHubUnlabeled:
			return &github.LabelEvent{Removed: true}, nil
		case ChangesetEventKindCommitStatus:
			return new(github.CommitStatus), nil
		case ChangesetEventKindCheckSuite:
			return new(github.CheckSuite), nil
		case ChangesetEventKindCheckRun:
			return new(github.CheckRun), nil
		}
	case strings.HasPrefix(string(k), "gitlab"):
		switch k {
		case ChangesetEventKindGitLabApproved:
			return new(gitlab.ReviewApprovedEvent), nil
		case ChangesetEventKindGitLabPipeline:
			return new(gitlab.Pipeline), nil
		case ChangesetEventKindGitLabUnapproved:
			return new(gitlab.ReviewUnapprovedEvent), nil
		case ChangesetEventKindGitLabMarkWorkInProgress:
			return new(gitlab.MarkWorkInProgressEvent), nil
		case ChangesetEventKindGitLabUnmarkWorkInProgress:
			return new(gitlab.UnmarkWorkInProgressEvent), nil
		case ChangesetEventKindGitLabClosed:
			return new(gitlab.MergeRequestClosedEvent), nil
		case ChangesetEventKindGitLabMerged:
			return new(gitlab.MergeRequestMergedEvent), nil
		case ChangesetEventKindGitLabReopened:
			return new(gitlab.MergeRequestReopenedEvent), nil
		}
	}
	return nil, errors.Errorf("unknown changeset event kind %q", k)
}
