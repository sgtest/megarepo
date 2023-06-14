package resolvers

import (
	"context"
	"strconv"
	"sync"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/batches/store"
	btypes "github.com/sourcegraph/sourcegraph/enterprise/internal/batches/types"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
)

type bulkOperationConnectionResolver struct {
	store           *store.Store
	logger          log.Logger
	batchChangeID   int64
	opts            store.ListBulkOperationsOpts
	gitserverClient gitserver.Client

	// Cache results because they are used by multiple fields
	once           sync.Once
	bulkOperations []*btypes.BulkOperation
	next           int64
	err            error
}

var _ graphqlbackend.BulkOperationConnectionResolver = &bulkOperationConnectionResolver{}

func (r *bulkOperationConnectionResolver) TotalCount(ctx context.Context) (int32, error) {
	count, err := r.store.CountBulkOperations(ctx, store.CountBulkOperationsOpts{
		BatchChangeID: r.batchChangeID,
		CreatedAfter:  r.opts.CreatedAfter,
	})
	if err != nil {
		return 0, err
	}
	return int32(count), nil
}

func (r *bulkOperationConnectionResolver) PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error) {
	_, next, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}

	if next != 0 {
		return graphqlutil.NextPageCursor(strconv.Itoa(int(next))), nil
	}

	return graphqlutil.HasNextPage(false), nil
}

func (r *bulkOperationConnectionResolver) Nodes(ctx context.Context) ([]graphqlbackend.BulkOperationResolver, error) {
	bulkOperations, _, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}

	resolvers := make([]graphqlbackend.BulkOperationResolver, 0, len(bulkOperations))
	for _, b := range bulkOperations {
		resolvers = append(resolvers, &bulkOperationResolver{store: r.store, gitserverClient: r.gitserverClient, logger: r.logger, bulkOperation: b})
	}

	return resolvers, nil
}

func (r *bulkOperationConnectionResolver) compute(ctx context.Context) ([]*btypes.BulkOperation, int64, error) {
	r.once.Do(func() {
		opts := r.opts
		opts.BatchChangeID = r.batchChangeID
		r.bulkOperations, r.next, r.err = r.store.ListBulkOperations(ctx, opts)
	})

	return r.bulkOperations, r.next, r.err
}
