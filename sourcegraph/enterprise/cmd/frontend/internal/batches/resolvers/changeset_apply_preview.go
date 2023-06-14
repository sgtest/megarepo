package resolvers

import (
	"context"
	"sync"
	"time"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/reconciler"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/rewirer"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/service"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/batches"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type changesetApplyPreviewResolver struct {
	store           *store.Store
	gitserverClient gitserver.Client
	logger          log.Logger

	mapping              *btypes.RewirerMapping
	preloadedNextSync    time.Time
	preloadedBatchChange *btypes.BatchChange
	batchSpecID          int64
	publicationStates    publicationStateMap
}

var _ graphqlbackend.ChangesetApplyPreviewResolver = &changesetApplyPreviewResolver{}

func (r *changesetApplyPreviewResolver) repoAccessible() bool {
	// The repo is accessible when it was returned by the database when the mapping was hydrated.
	return r.mapping.Repo != nil
}

func (r *changesetApplyPreviewResolver) ToVisibleChangesetApplyPreview() (graphqlbackend.VisibleChangesetApplyPreviewResolver, bool) {
	if r.repoAccessible() {
		return &visibleChangesetApplyPreviewResolver{
			store:                r.store,
			gitserverClient:      r.gitserverClient,
			logger:               r.logger,
			mapping:              r.mapping,
			preloadedNextSync:    r.preloadedNextSync,
			preloadedBatchChange: r.preloadedBatchChange,
			batchSpecID:          r.batchSpecID,
			publicationStates:    r.publicationStates,
		}, true
	}
	return nil, false
}

func (r *changesetApplyPreviewResolver) ToHiddenChangesetApplyPreview() (graphqlbackend.HiddenChangesetApplyPreviewResolver, bool) {
	if !r.repoAccessible() {
		return &hiddenChangesetApplyPreviewResolver{
			store:             r.store,
			gitserverClient:   r.gitserverClient,
			mapping:           r.mapping,
			preloadedNextSync: r.preloadedNextSync,
		}, true
	}
	return nil, false
}

type hiddenChangesetApplyPreviewResolver struct {
	store           *store.Store
	gitserverClient gitserver.Client

	mapping           *btypes.RewirerMapping
	preloadedNextSync time.Time
}

var _ graphqlbackend.HiddenChangesetApplyPreviewResolver = &hiddenChangesetApplyPreviewResolver{}

func (r *hiddenChangesetApplyPreviewResolver) Operations(ctx context.Context) ([]string, error) {
	// If the repo is inaccessible, no operations would be taken, since the changeset is not created/updated.
	return []string{}, nil
}

func (r *hiddenChangesetApplyPreviewResolver) Delta(ctx context.Context) (graphqlbackend.ChangesetSpecDeltaResolver, error) {
	// If the repo is inaccessible, no comparison is made, since the changeset is not created/updated.
	return &changesetSpecDeltaResolver{}, nil
}

func (r *hiddenChangesetApplyPreviewResolver) Targets() graphqlbackend.HiddenApplyPreviewTargetsResolver {
	return &hiddenApplyPreviewTargetsResolver{
		store:             r.store,
		gitserverClient:   r.gitserverClient,
		mapping:           r.mapping,
		preloadedNextSync: r.preloadedNextSync,
	}
}

type hiddenApplyPreviewTargetsResolver struct {
	store           *store.Store
	gitserverClient gitserver.Client
	logger          log.Logger

	mapping           *btypes.RewirerMapping
	preloadedNextSync time.Time
}

var _ graphqlbackend.HiddenApplyPreviewTargetsResolver = &hiddenApplyPreviewTargetsResolver{}
var _ graphqlbackend.HiddenApplyPreviewTargetsAttachResolver = &hiddenApplyPreviewTargetsResolver{}
var _ graphqlbackend.HiddenApplyPreviewTargetsUpdateResolver = &hiddenApplyPreviewTargetsResolver{}
var _ graphqlbackend.HiddenApplyPreviewTargetsDetachResolver = &hiddenApplyPreviewTargetsResolver{}

func (r *hiddenApplyPreviewTargetsResolver) ToHiddenApplyPreviewTargetsAttach() (graphqlbackend.HiddenApplyPreviewTargetsAttachResolver, bool) {
	if r.mapping.Changeset == nil {
		return r, true
	}
	return nil, false
}
func (r *hiddenApplyPreviewTargetsResolver) ToHiddenApplyPreviewTargetsUpdate() (graphqlbackend.HiddenApplyPreviewTargetsUpdateResolver, bool) {
	if r.mapping.Changeset != nil && r.mapping.ChangesetSpec != nil {
		return r, true
	}
	return nil, false
}
func (r *hiddenApplyPreviewTargetsResolver) ToHiddenApplyPreviewTargetsDetach() (graphqlbackend.HiddenApplyPreviewTargetsDetachResolver, bool) {
	if r.mapping.ChangesetSpec == nil {
		return r, true
	}
	return nil, false
}

func (r *hiddenApplyPreviewTargetsResolver) ChangesetSpec(ctx context.Context) (graphqlbackend.HiddenChangesetSpecResolver, error) {
	if r.mapping.ChangesetSpec == nil {
		return nil, nil
	}
	return NewChangesetSpecResolverWithRepo(r.store, nil, r.mapping.ChangesetSpec), nil
}

func (r *hiddenApplyPreviewTargetsResolver) Changeset(ctx context.Context) (graphqlbackend.HiddenExternalChangesetResolver, error) {
	if r.mapping.Changeset == nil {
		return nil, nil
	}
	return NewChangesetResolverWithNextSync(r.store, r.gitserverClient, r.logger, r.mapping.Changeset, nil, r.preloadedNextSync), nil
}

type visibleChangesetApplyPreviewResolver struct {
	store           *store.Store
	gitserverClient gitserver.Client
	logger          log.Logger

	mapping              *btypes.RewirerMapping
	preloadedNextSync    time.Time
	preloadedBatchChange *btypes.BatchChange
	batchSpecID          int64
	publicationStates    map[string]batches.PublishedValue

	planOnce sync.Once
	plan     *reconciler.Plan
	planErr  error

	batchChangeOnce sync.Once
	batchChange     *btypes.BatchChange
	batchChangeErr  error
}

var _ graphqlbackend.VisibleChangesetApplyPreviewResolver = &visibleChangesetApplyPreviewResolver{}

func (r *visibleChangesetApplyPreviewResolver) Operations(ctx context.Context) ([]string, error) {
	plan, err := r.computePlan(ctx)
	if err != nil {
		return nil, err
	}
	ops := plan.Ops.ExecutionOrder()
	strOps := make([]string, 0, len(ops))
	for _, op := range ops {
		strOps = append(strOps, string(op))
	}
	return strOps, nil
}

func (r *visibleChangesetApplyPreviewResolver) Delta(ctx context.Context) (graphqlbackend.ChangesetSpecDeltaResolver, error) {
	plan, err := r.computePlan(ctx)
	if err != nil {
		return nil, err
	}
	if plan.Delta == nil {
		return &changesetSpecDeltaResolver{}, nil
	}
	return &changesetSpecDeltaResolver{delta: *plan.Delta}, nil
}

func (r *visibleChangesetApplyPreviewResolver) Targets() graphqlbackend.VisibleApplyPreviewTargetsResolver {
	return &visibleApplyPreviewTargetsResolver{
		store:             r.store,
		gitserverClient:   r.gitserverClient,
		logger:            r.logger,
		mapping:           r.mapping,
		preloadedNextSync: r.preloadedNextSync,
	}
}

func (r *visibleChangesetApplyPreviewResolver) computePlan(ctx context.Context) (*reconciler.Plan, error) {
	r.planOnce.Do(func() {
		batchChange, err := r.computeBatchChange(ctx)
		if err != nil {
			r.planErr = err
			return
		}

		// Clone all entities to ensure they're not modified when used
		// by the changeset and changeset spec resolvers. Otherwise, the
		// changeset always appears as "processing".
		var (
			mappingChangeset     *btypes.Changeset
			mappingChangesetSpec *btypes.ChangesetSpec
			mappingRepo          *types.Repo
		)
		if r.mapping.Changeset != nil {
			mappingChangeset = r.mapping.Changeset.Clone()
		}
		if r.mapping.ChangesetSpec != nil {
			mappingChangesetSpec = r.mapping.ChangesetSpec.Clone()
		}
		if r.mapping.Repo != nil {
			mappingRepo = r.mapping.Repo.Clone()
		}

		// Then, dry-run the rewirer to simulate how the changeset would look like _after_ an apply operation.
		changesetRewirer := rewirer.New(btypes.RewirerMappings{{
			ChangesetSpecID: r.mapping.ChangesetSpecID,
			ChangesetID:     r.mapping.ChangesetID,
			RepoID:          r.mapping.RepoID,

			ChangesetSpec: mappingChangesetSpec,
			Changeset:     mappingChangeset,
			Repo:          mappingRepo,
		}}, batchChange.ID)
		newChangesets, updatedChangesets, err := changesetRewirer.Rewire()
		if err != nil {
			r.planErr = err
			return
		}

		// For a preview, we do not care if the changesets are new or being updated. When applying the changes, we do
		// want to differentiate to make life easier.
		wantedChangesets := append(newChangesets, updatedChangesets...)

		if len(wantedChangesets) != 1 {
			r.planErr = errors.New("rewirer did not return changeset")
			return
		}
		wantedChangeset := wantedChangesets[0]

		// Set the changeset UI publication state if necessary.
		if r.publicationStates != nil && mappingChangesetSpec != nil {
			if state, ok := r.publicationStates[mappingChangesetSpec.RandID]; ok {
				if !mappingChangesetSpec.Published.Nil() {
					r.planErr = errors.Newf("changeset spec %q has the published field set in its spec", mappingChangesetSpec.RandID)
					return
				}
				wantedChangeset.UiPublicationState = btypes.ChangesetUiPublicationStateFromPublishedValue(state)
			}
		}

		// Detached changesets would still appear here, but since they'll never match one of the new specs, they don't actually appear here.
		// Once we have a way to have changeset specs for detached changesets, this would be the place to do a "will be detached" check.
		// TBD: How we represent that in the API.

		// The rewirer takes previous and current spec into account to determine actions to take,
		// so we need to find out which specs we need to pass to the planner.

		// This means that we currently won't show "attach to tracking changeset" and "detach changeset" in this preview API. Close and import non-existing work, though.
		var previousSpec, currentSpec *btypes.ChangesetSpec
		if wantedChangeset.PreviousSpecID != 0 {
			previousSpec, err = r.store.GetChangesetSpecByID(ctx, wantedChangeset.PreviousSpecID)
			if err != nil {
				r.planErr = err
				return
			}
		}
		if wantedChangeset.CurrentSpecID != 0 {
			if r.mapping.ChangesetSpec != nil {
				// If the current spec was not unset by the rewirer, it will be this resolvers spec.
				currentSpec = r.mapping.ChangesetSpec
			} else {
				currentSpec, err = r.store.GetChangesetSpecByID(ctx, wantedChangeset.CurrentSpecID)
				if err != nil {
					r.planErr = err
					return
				}
			}
		}
		r.plan, r.planErr = reconciler.DeterminePlan(previousSpec, currentSpec, r.mapping.Changeset, wantedChangeset)
	})
	return r.plan, r.planErr
}

func (r *visibleChangesetApplyPreviewResolver) computeBatchChange(ctx context.Context) (*btypes.BatchChange, error) {
	r.batchChangeOnce.Do(func() {
		if r.preloadedBatchChange != nil {
			r.batchChange = r.preloadedBatchChange
			return
		}
		svc := service.New(r.store)
		batchSpec, err := r.store.GetBatchSpec(ctx, store.GetBatchSpecOpts{ID: r.batchSpecID})
		if err != nil {
			r.planErr = err
			return
		}
		// Dry-run reconcile the batch  with the new batch spec.
		r.batchChange, _, r.batchChangeErr = svc.ReconcileBatchChange(ctx, batchSpec)
	})
	return r.batchChange, r.batchChangeErr
}

type visibleApplyPreviewTargetsResolver struct {
	store           *store.Store
	gitserverClient gitserver.Client
	logger          log.Logger

	mapping           *btypes.RewirerMapping
	preloadedNextSync time.Time
}

var _ graphqlbackend.VisibleApplyPreviewTargetsResolver = &visibleApplyPreviewTargetsResolver{}
var _ graphqlbackend.VisibleApplyPreviewTargetsAttachResolver = &visibleApplyPreviewTargetsResolver{}
var _ graphqlbackend.VisibleApplyPreviewTargetsUpdateResolver = &visibleApplyPreviewTargetsResolver{}
var _ graphqlbackend.VisibleApplyPreviewTargetsDetachResolver = &visibleApplyPreviewTargetsResolver{}

func (r *visibleApplyPreviewTargetsResolver) ToVisibleApplyPreviewTargetsAttach() (graphqlbackend.VisibleApplyPreviewTargetsAttachResolver, bool) {
	if r.mapping.Changeset == nil {
		return r, true
	}
	return nil, false
}
func (r *visibleApplyPreviewTargetsResolver) ToVisibleApplyPreviewTargetsUpdate() (graphqlbackend.VisibleApplyPreviewTargetsUpdateResolver, bool) {
	if r.mapping.Changeset != nil && r.mapping.ChangesetSpec != nil {
		return r, true
	}
	return nil, false
}
func (r *visibleApplyPreviewTargetsResolver) ToVisibleApplyPreviewTargetsDetach() (graphqlbackend.VisibleApplyPreviewTargetsDetachResolver, bool) {
	if r.mapping.ChangesetSpec == nil {
		return r, true
	}
	return nil, false
}

func (r *visibleApplyPreviewTargetsResolver) ChangesetSpec(ctx context.Context) (graphqlbackend.VisibleChangesetSpecResolver, error) {
	if r.mapping.ChangesetSpec == nil {
		return nil, nil
	}
	return NewChangesetSpecResolverWithRepo(r.store, r.mapping.Repo, r.mapping.ChangesetSpec), nil
}

func (r *visibleApplyPreviewTargetsResolver) Changeset(ctx context.Context) (graphqlbackend.ExternalChangesetResolver, error) {
	if r.mapping.Changeset == nil {
		return nil, nil
	}
	return NewChangesetResolverWithNextSync(r.store, r.gitserverClient, r.logger, r.mapping.Changeset, r.mapping.Repo, r.preloadedNextSync), nil
}

type changesetSpecDeltaResolver struct {
	delta reconciler.ChangesetSpecDelta
}

var _ graphqlbackend.ChangesetSpecDeltaResolver = &changesetSpecDeltaResolver{}

func (c *changesetSpecDeltaResolver) TitleChanged() bool {
	return c.delta.TitleChanged
}
func (c *changesetSpecDeltaResolver) BodyChanged() bool {
	return c.delta.BodyChanged
}
func (c *changesetSpecDeltaResolver) Undraft() bool {
	return c.delta.Undraft
}
func (c *changesetSpecDeltaResolver) BaseRefChanged() bool {
	return c.delta.BaseRefChanged
}
func (c *changesetSpecDeltaResolver) DiffChanged() bool {
	return c.delta.DiffChanged
}
func (c *changesetSpecDeltaResolver) CommitMessageChanged() bool {
	return c.delta.CommitMessageChanged
}
func (c *changesetSpecDeltaResolver) AuthorNameChanged() bool {
	return c.delta.AuthorNameChanged
}
func (c *changesetSpecDeltaResolver) AuthorEmailChanged() bool {
	return c.delta.AuthorEmailChanged
}
