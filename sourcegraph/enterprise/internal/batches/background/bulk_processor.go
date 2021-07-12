package background

import (
	"context"
	"fmt"

	"github.com/cockroachdb/errors"
	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/global"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/service"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/sources"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/state"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

// unknownJobTypeErr is returned when a ChangesetJob record is of an unknown type
// and hence cannot be executed.
type unknownJobTypeErr struct {
	jobType string
}

func (e unknownJobTypeErr) Error() string {
	return fmt.Sprintf("invalid job type %q", e.jobType)
}

func (e unknownJobTypeErr) NonRetryable() bool {
	return true
}

type bulkProcessor struct {
	tx      *store.Store
	sourcer sources.Sourcer

	css  sources.ChangesetSource
	repo *types.Repo
	ch   *btypes.Changeset
}

func (b *bulkProcessor) process(ctx context.Context, job *btypes.ChangesetJob) (err error) {
	// Use the acting user for the operation to enforce repository permissions.
	ctx = actor.WithActor(ctx, actor.FromUser(job.UserID))

	// Load changeset.
	b.ch, err = b.tx.GetChangeset(ctx, store.GetChangesetOpts{ID: job.ChangesetID})
	if err != nil {
		return errors.Wrap(err, "loading changeset")
	}

	// Load repo.
	b.repo, err = b.tx.Repos().Get(ctx, b.ch.RepoID)
	if err != nil {
		return errors.Wrap(err, "loading repo")
	}

	// Construct changeset source.
	b.css, err = b.sourcer.ForRepo(ctx, b.tx, b.repo)
	if err != nil {
		return errors.Wrap(err, "loading ChangesetSource")
	}
	b.css, err = sources.WithAuthenticatorForUser(ctx, b.tx, b.css, job.UserID, b.repo)
	if err != nil {
		return errors.Wrap(err, "authenticating ChangesetSource")
	}

	log15.Info("processing changeset job", "type", job.JobType)

	switch job.JobType {

	case btypes.ChangesetJobTypeComment:
		return b.comment(ctx, job)
	case btypes.ChangesetJobTypeDetach:
		return b.detach(ctx, job)
	case btypes.ChangesetJobTypeReenqueue:
		return b.reenqueueChangeset(ctx, job)
	case btypes.ChangesetJobTypeMerge:
		return b.mergeChangeset(ctx, job)
	case btypes.ChangesetJobTypeClose:
		return b.closeChangeset(ctx, job)

	default:
		return &unknownJobTypeErr{jobType: string(job.JobType)}
	}
}

func (b *bulkProcessor) comment(ctx context.Context, job *btypes.ChangesetJob) error {
	typedPayload, ok := job.Payload.(*btypes.ChangesetJobCommentPayload)
	if !ok {
		return errors.Errorf("invalid payload type for changeset_job, want=%T have=%T", &btypes.ChangesetJobCommentPayload{}, job.Payload)
	}
	cs := &sources.Changeset{
		Changeset: b.ch,
		Repo:      b.repo,
	}
	return b.css.CreateComment(ctx, cs, typedPayload.Message)
}

func (b *bulkProcessor) detach(ctx context.Context, job *btypes.ChangesetJob) error {
	// Try to detach the changeset from the batch change of the job.
	var detached bool
	for i, assoc := range b.ch.BatchChanges {
		if assoc.BatchChangeID == job.BatchChangeID {
			if !b.ch.BatchChanges[i].Detach {
				b.ch.BatchChanges[i].Detach = true
				detached = true
			}
		}
	}

	if !detached {
		return nil
	}

	// If we successfully marked the record as to-be-detached, trigger a reconciler run.
	b.ch.ResetReconcilerState(global.DefaultReconcilerEnqueueState())
	return b.tx.UpdateChangeset(ctx, b.ch)
}

func (b *bulkProcessor) reenqueueChangeset(ctx context.Context, job *btypes.ChangesetJob) error {
	svc := service.New(b.tx)
	_, _, err := svc.ReenqueueChangeset(ctx, b.ch.ID)
	return err
}

func (b *bulkProcessor) mergeChangeset(ctx context.Context, job *btypes.ChangesetJob) (err error) {
	typedPayload, ok := job.Payload.(*btypes.ChangesetJobMergePayload)
	if !ok {
		return errors.Errorf("invalid payload type for changeset_job, want=%T have=%T", &btypes.ChangesetJobMergePayload{}, job.Payload)
	}

	cs := &sources.Changeset{
		Changeset: b.ch,
		Repo:      b.repo,
	}
	if err := b.css.MergeChangeset(ctx, cs, typedPayload.Squash); err != nil {
		return err
	}

	events, err := cs.Changeset.Events()
	if err != nil {
		log15.Error("Events", "err", err)
		return errcode.MakeNonRetryable(err)
	}
	state.SetDerivedState(ctx, b.tx.Repos(), cs.Changeset, events)

	if err := b.tx.UpsertChangesetEvents(ctx, events...); err != nil {
		log15.Error("UpsertChangesetEvents", "err", err)
		return errcode.MakeNonRetryable(err)
	}

	if err := b.tx.UpdateChangeset(ctx, cs.Changeset); err != nil {
		log15.Error("UpdateChangeset", "err", err)
		return errcode.MakeNonRetryable(err)
	}

	return nil
}

func (b *bulkProcessor) closeChangeset(ctx context.Context, job *btypes.ChangesetJob) (err error) {
	cs := &sources.Changeset{
		Changeset: b.ch,
		Repo:      b.repo,
	}
	if err := b.css.CloseChangeset(ctx, cs); err != nil {
		return err
	}

	events, err := cs.Changeset.Events()
	if err != nil {
		log15.Error("Events", "err", err)
		return errcode.MakeNonRetryable(err)
	}
	state.SetDerivedState(ctx, b.tx.Repos(), cs.Changeset, events)

	if err := b.tx.UpsertChangesetEvents(ctx, events...); err != nil {
		log15.Error("UpsertChangesetEvents", "err", err)
		return errcode.MakeNonRetryable(err)
	}

	if err := b.tx.UpdateChangeset(ctx, cs.Changeset); err != nil {
		log15.Error("UpdateChangeset", "err", err)
		return errcode.MakeNonRetryable(err)
	}

	return nil
}
