package resolvers

import (
	"context"
	"sort"
	"strconv"
	"sync"
	"time"

	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/state"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var _ graphqlbackend.BatchChangeResolver = &batchChangeResolver{}

type batchChangeResolver struct {
	store *store.Store

	batchChange *btypes.BatchChange

	// Cache the namespace on the resolver, since it's accessed more than once.
	namespaceOnce sync.Once
	namespace     graphqlbackend.NamespaceResolver
	namespaceErr  error

	batchSpecOnce sync.Once
	batchSpec     *btypes.BatchSpec
	batchSpecErr  error
}

const batchChangeIDKind = "BatchChange"

func marshalBatchChangeID(id int64) graphql.ID {
	return relay.MarshalID(batchChangeIDKind, id)
}

func unmarshalBatchChangeID(id graphql.ID) (batchChangeID int64, err error) {
	err = relay.UnmarshalSpec(id, &batchChangeID)
	return
}

func (r *batchChangeResolver) ID() graphql.ID {
	return marshalBatchChangeID(r.batchChange.ID)
}

func (r *batchChangeResolver) Name() string {
	return r.batchChange.Name
}

func (r *batchChangeResolver) Description() *string {
	if r.batchChange.Description == "" {
		return nil
	}
	return &r.batchChange.Description
}

func (r *batchChangeResolver) State() string {
	var state btypes.BatchChangeState
	if r.batchChange.Closed() {
		state = btypes.BatchChangeStateClosed
	} else if r.batchChange.IsDraft() {
		state = btypes.BatchChangeStateDraft
	} else {
		state = btypes.BatchChangeStateOpen
	}

	return state.ToGraphQL()
}

func (r *batchChangeResolver) InitialApplier(ctx context.Context) (*graphqlbackend.UserResolver, error) {
	return r.Creator(ctx)
}

func (r *batchChangeResolver) Creator(ctx context.Context) (*graphqlbackend.UserResolver, error) {
	user, err := graphqlbackend.UserByIDInt32(ctx, r.store.DatabaseDB(), r.batchChange.CreatorID)
	if errcode.IsNotFound(err) {
		return nil, nil
	}
	return user, err
}

func (r *batchChangeResolver) LastApplier(ctx context.Context) (*graphqlbackend.UserResolver, error) {
	if r.batchChange.LastApplierID == 0 {
		return nil, nil
	}

	user, err := graphqlbackend.UserByIDInt32(ctx, r.store.DatabaseDB(), r.batchChange.LastApplierID)
	if errcode.IsNotFound(err) {
		return nil, nil
	}

	return user, err
}

func (r *batchChangeResolver) LastAppliedAt() *graphqlbackend.DateTime {
	if r.batchChange.LastAppliedAt.IsZero() {
		return nil
	}

	return &graphqlbackend.DateTime{Time: r.batchChange.LastAppliedAt}
}

func (r *batchChangeResolver) SpecCreator(ctx context.Context) (*graphqlbackend.UserResolver, error) {
	spec, err := r.computeBatchSpec(ctx)
	if err != nil {
		return nil, err
	}

	user, err := graphqlbackend.UserByIDInt32(ctx, r.store.DatabaseDB(), spec.UserID)
	if errcode.IsNotFound(err) {
		return nil, nil
	}

	return user, err
}

func (r *batchChangeResolver) ViewerCanAdminister(ctx context.Context) (bool, error) {
	return checkSiteAdminOrSameUser(ctx, r.store.DatabaseDB(), r.batchChange.CreatorID)
}

func (r *batchChangeResolver) URL(ctx context.Context) (string, error) {
	n, err := r.Namespace(ctx)
	if err != nil {
		return "", err
	}
	return batchChangeURL(n, r), nil
}

func (r *batchChangeResolver) Namespace(ctx context.Context) (graphqlbackend.NamespaceResolver, error) {
	return r.computeNamespace(ctx)
}

func (r *batchChangeResolver) computeNamespace(ctx context.Context) (graphqlbackend.NamespaceResolver, error) {
	r.namespaceOnce.Do(func() {
		if r.batchChange.NamespaceUserID != 0 {
			r.namespace.Namespace, r.namespaceErr = graphqlbackend.UserByIDInt32(
				ctx,
				r.store.DatabaseDB(),
				r.batchChange.NamespaceUserID,
			)
		} else {
			r.namespace.Namespace, r.namespaceErr = graphqlbackend.OrgByIDInt32(
				ctx,
				r.store.DatabaseDB(),
				r.batchChange.NamespaceOrgID,
			)
		}
		if errcode.IsNotFound(r.namespaceErr) {
			r.namespace.Namespace = nil
			r.namespaceErr = errors.New("namespace of batch change has been deleted")
		}
	})

	return r.namespace, r.namespaceErr
}

func (r *batchChangeResolver) computeBatchSpec(ctx context.Context) (*btypes.BatchSpec, error) {
	r.batchSpecOnce.Do(func() {
		r.batchSpec, r.batchSpecErr = r.store.GetBatchSpec(ctx, store.GetBatchSpecOpts{
			ID: r.batchChange.BatchSpecID,
		})
	})

	return r.batchSpec, r.batchSpecErr
}

func (r *batchChangeResolver) CreatedAt() graphqlbackend.DateTime {
	return graphqlbackend.DateTime{Time: r.batchChange.CreatedAt}
}

func (r *batchChangeResolver) UpdatedAt() graphqlbackend.DateTime {
	return graphqlbackend.DateTime{Time: r.batchChange.UpdatedAt}
}

func (r *batchChangeResolver) ClosedAt() *graphqlbackend.DateTime {
	if !r.batchChange.Closed() {
		return nil
	}
	return &graphqlbackend.DateTime{Time: r.batchChange.ClosedAt}
}

func (r *batchChangeResolver) ChangesetsStats(ctx context.Context) (graphqlbackend.ChangesetsStatsResolver, error) {
	stats, err := r.store.GetChangesetsStats(ctx, r.batchChange.ID)
	if err != nil {
		return nil, err
	}
	return &changesetsStatsResolver{stats: stats}, nil
}

func (r *batchChangeResolver) Changesets(
	ctx context.Context,
	args *graphqlbackend.ListChangesetsArgs,
) (graphqlbackend.ChangesetsConnectionResolver, error) {
	opts, safe, err := listChangesetOptsFromArgs(args, r.batchChange.ID)
	if err != nil {
		return nil, err
	}
	opts.BatchChangeID = r.batchChange.ID
	return &changesetsConnectionResolver{
		store:    r.store,
		opts:     opts,
		optsSafe: safe,
	}, nil
}

func (r *batchChangeResolver) ChangesetCountsOverTime(
	ctx context.Context,
	args *graphqlbackend.ChangesetCountsArgs,
) ([]graphqlbackend.ChangesetCountsResolver, error) {
	publishedState := btypes.ChangesetPublicationStatePublished
	opts := store.ListChangesetsOpts{
		BatchChangeID:   r.batchChange.ID,
		IncludeArchived: args.IncludeArchived,
		// Only load fully-synced changesets, so that the data we use for computing the changeset counts is complete.
		PublicationState: &publishedState,
	}
	cs, _, err := r.store.ListChangesets(ctx, opts)
	if err != nil {
		return nil, err
	}

	var es []*btypes.ChangesetEvent
	changesetIDs := cs.IDs()
	if len(changesetIDs) > 0 {
		eventsOpts := store.ListChangesetEventsOpts{ChangesetIDs: changesetIDs, Kinds: state.RequiredEventTypesForHistory}
		es, _, err = r.store.ListChangesetEvents(ctx, eventsOpts)
		if err != nil {
			return nil, err
		}
	}
	// Sort all events once by their timestamps, CalcCounts depends on it.
	events := state.ChangesetEvents(es)
	sort.Sort(events)

	// Determine timeframe.
	now := r.store.Clock()()
	weekAgo := now.Add(-7 * 24 * time.Hour)
	start := r.batchChange.CreatedAt.UTC()
	if len(events) > 0 {
		start = events[0].Timestamp().UTC()
	}
	// At least a week lookback, more if the batch change was created earlier.
	if start.After(weekAgo) {
		start = weekAgo
	}
	if args.From != nil {
		start = args.From.Time.UTC()
	}
	end := now.UTC()
	if args.To != nil && args.To.Time.Before(end) {
		end = args.To.Time.UTC()
	}

	counts, err := state.CalcCounts(start, end, cs, es...)
	if err != nil {
		return nil, err
	}

	resolvers := make([]graphqlbackend.ChangesetCountsResolver, 0, len(counts))
	for _, c := range counts {
		resolvers = append(resolvers, &changesetCountsResolver{counts: c})
	}

	return resolvers, nil
}

func (r *batchChangeResolver) DiffStat(ctx context.Context) (*graphqlbackend.DiffStat, error) {
	diffStat, err := r.store.GetBatchChangeDiffStat(ctx, store.GetBatchChangeDiffStatOpts{BatchChangeID: r.batchChange.ID})
	if err != nil {
		return nil, err
	}
	return graphqlbackend.NewDiffStat(*diffStat), nil
}

func (r *batchChangeResolver) CurrentSpec(ctx context.Context) (graphqlbackend.BatchSpecResolver, error) {
	batchSpec, err := r.computeBatchSpec(ctx)
	if err != nil {
		// This spec should always exist, so fail hard on not found errors as well.
		return nil, err
	}

	return &batchSpecResolver{store: r.store, batchSpec: batchSpec}, nil
}

func (r *batchChangeResolver) BulkOperations(
	ctx context.Context,
	args *graphqlbackend.ListBatchChangeBulkOperationArgs,
) (graphqlbackend.BulkOperationConnectionResolver, error) {
	if err := validateFirstParamDefaults(args.First); err != nil {
		return nil, err
	}
	opts := store.ListBulkOperationsOpts{
		LimitOpts: store.LimitOpts{
			Limit: int(args.First),
		},
	}
	if args.After != nil {
		id, err := strconv.Atoi(*args.After)
		if err != nil {
			return nil, err
		}
		opts.Cursor = int64(id)
	}

	if args.CreatedAfter != nil {
		opts.CreatedAfter = args.CreatedAfter.Time
	}

	return &bulkOperationConnectionResolver{
		store:         r.store,
		batchChangeID: r.batchChange.ID,
		opts:          opts,
	}, nil
}

func (r *batchChangeResolver) BatchSpecs(
	ctx context.Context,
	args *graphqlbackend.ListBatchSpecArgs,
) (graphqlbackend.BatchSpecConnectionResolver, error) {
	if err := validateFirstParamDefaults(args.First); err != nil {
		return nil, err
	}
	opts := store.ListBatchSpecsOpts{
		BatchChangeID: r.batchChange.ID,
		LimitOpts: store.LimitOpts{
			Limit: int(args.First),
		},
		// We want the batch spec connection to always show the latest one first.
		NewestFirst: true,
	}

	if args.IncludeLocallyExecutedSpecs != nil {
		opts.IncludeLocallyExecutedSpecs = *args.IncludeLocallyExecutedSpecs
	}

	if err := backend.CheckCurrentUserIsSiteAdmin(ctx, r.store.DatabaseDB()); err != nil {
		opts.ExcludeCreatedFromRawNotOwnedByUser = actor.FromContext(ctx).UID
	}

	if args.After != nil {
		id, err := strconv.Atoi(*args.After)
		if err != nil {
			return nil, err
		}
		opts.Cursor = int64(id)
	}

	return &batchSpecConnectionResolver{store: r.store, opts: opts}, nil
}
