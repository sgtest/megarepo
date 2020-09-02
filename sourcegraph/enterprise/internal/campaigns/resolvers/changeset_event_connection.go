package resolvers

import (
	"context"
	"strconv"
	"sync"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	ee "github.com/sourcegraph/sourcegraph/enterprise/internal/campaigns"
	"github.com/sourcegraph/sourcegraph/internal/campaigns"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
)

type changesetEventsConnectionResolver struct {
	store             *ee.Store
	httpFactory       *httpcli.Factory
	changesetResolver *changesetResolver
	first             int
	cursor            int64

	// cache results because they are used by multiple fields
	once            sync.Once
	changesetEvents []*campaigns.ChangesetEvent
	next            int64
	err             error
}

func (r *changesetEventsConnectionResolver) Nodes(ctx context.Context) ([]graphqlbackend.ChangesetEventResolver, error) {
	changesetEvents, _, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}
	resolvers := make([]graphqlbackend.ChangesetEventResolver, 0, len(changesetEvents))
	for _, c := range changesetEvents {
		resolvers = append(resolvers, &changesetEventResolver{
			store:             r.store,
			httpFactory:       r.httpFactory,
			changesetResolver: r.changesetResolver,
			ChangesetEvent:    c,
		})
	}
	return resolvers, nil
}

func (r *changesetEventsConnectionResolver) TotalCount(ctx context.Context) (int32, error) {
	opts := ee.CountChangesetEventsOpts{ChangesetID: r.changesetResolver.changeset.ID}
	count, err := r.store.CountChangesetEvents(ctx, opts)
	return int32(count), err
}

func (r *changesetEventsConnectionResolver) PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error) {
	_, next, err := r.compute(ctx)
	if err != nil {
		return nil, err
	}
	if next != 0 {
		return graphqlutil.NextPageCursor(strconv.Itoa(int(next))), nil
	}
	return graphqlutil.HasNextPage(false), nil
}

func (r *changesetEventsConnectionResolver) compute(ctx context.Context) ([]*campaigns.ChangesetEvent, int64, error) {
	r.once.Do(func() {
		opts := ee.ListChangesetEventsOpts{
			ChangesetIDs: []int64{r.changesetResolver.changeset.ID},
			LimitOpts:    ee.LimitOpts{Limit: r.first},
			Cursor:       r.cursor,
		}
		r.changesetEvents, r.next, r.err = r.store.ListChangesetEvents(ctx, opts)
	})
	return r.changesetEvents, r.next, r.err
}
