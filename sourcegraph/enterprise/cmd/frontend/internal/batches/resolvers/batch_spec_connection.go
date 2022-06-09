package resolvers

import (
	"context"
	"strconv"
	"sync"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
)

type batchSpecConnectionResolver struct {
	store *store.Store
	opts  store.ListBatchSpecsOpts

	// Cache results because they are used by multiple fields.
	once       sync.Once
	batchSpecs []*btypes.BatchSpec
	next       int64
	err        error
}

var _ graphqlbackend.BatchSpecConnectionResolver = &batchSpecConnectionResolver{}

func (r *batchSpecConnectionResolver) Nodes(ctx context.Context) ([]graphqlbackend.BatchSpecResolver, error) {
	nodes, _, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}
	resolvers := make([]graphqlbackend.BatchSpecResolver, 0, len(nodes))
	for _, c := range nodes {
		resolvers = append(resolvers, &batchSpecResolver{store: r.store, batchSpec: c})
	}
	return resolvers, nil
}

func (r *batchSpecConnectionResolver) TotalCount(ctx context.Context) (int32, error) {
	count, err := r.store.CountBatchSpecs(ctx, store.CountBatchSpecsOpts{
		BatchChangeID:                       r.opts.BatchChangeID,
		ExcludeCreatedFromRawNotOwnedByUser: r.opts.ExcludeCreatedFromRawNotOwnedByUser,
		IncludeLocallyExecutedSpecs:         r.opts.IncludeLocallyExecutedSpecs,
	})
	return int32(count), err
}

func (r *batchSpecConnectionResolver) PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error) {
	_, next, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}
	if next != 0 {
		return graphqlutil.NextPageCursor(strconv.Itoa(int(next))), nil
	}
	return graphqlutil.HasNextPage(false), nil
}

func (r *batchSpecConnectionResolver) compute(ctx context.Context) ([]*btypes.BatchSpec, int64, error) {
	r.once.Do(func() {
		r.batchSpecs, r.next, r.err = r.store.ListBatchSpecs(ctx, r.opts)
	})
	return r.batchSpecs, r.next, r.err
}
