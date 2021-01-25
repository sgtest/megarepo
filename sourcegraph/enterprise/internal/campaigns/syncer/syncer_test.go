package syncer

import (
	"container/heap"
	"context"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/campaigns/store"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/campaigns"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestNextSync(t *testing.T) {
	t.Parallel()

	clock := func() time.Time { return time.Date(2020, 01, 01, 01, 01, 01, 01, time.UTC) }
	tests := []struct {
		name string
		h    *campaigns.ChangesetSyncData
		want time.Time
	}{
		{
			name: "No time passed",
			h: &campaigns.ChangesetSyncData{
				UpdatedAt:         clock(),
				ExternalUpdatedAt: clock(),
			},
			want: clock().Add(minSyncDelay),
		},
		{
			name: "Linear backoff",
			h: &campaigns.ChangesetSyncData{
				UpdatedAt:         clock(),
				ExternalUpdatedAt: clock().Add(-1 * time.Hour),
			},
			want: clock().Add(1 * time.Hour),
		},
		{
			name: "Use max of ExternalUpdateAt and LatestEvent",
			h: &campaigns.ChangesetSyncData{
				UpdatedAt:         clock(),
				ExternalUpdatedAt: clock().Add(-2 * time.Hour),
				LatestEvent:       clock().Add(-1 * time.Hour),
			},
			want: clock().Add(1 * time.Hour),
		},
		{
			name: "Diff max is capped",
			h: &campaigns.ChangesetSyncData{
				UpdatedAt:         clock(),
				ExternalUpdatedAt: clock().Add(-2 * maxSyncDelay),
			},
			want: clock().Add(maxSyncDelay),
		},
		{
			name: "Diff min is capped",
			h: &campaigns.ChangesetSyncData{
				UpdatedAt:         clock(),
				ExternalUpdatedAt: clock().Add(-1 * minSyncDelay / 2),
			},
			want: clock().Add(minSyncDelay),
		},
		{
			name: "Event arrives after sync",
			h: &campaigns.ChangesetSyncData{
				UpdatedAt:         clock(),
				ExternalUpdatedAt: clock().Add(-1 * maxSyncDelay / 2),
				LatestEvent:       clock().Add(10 * time.Minute),
			},
			want: clock().Add(10 * time.Minute).Add(minSyncDelay),
		},
		{
			name: "Never synced",
			h:    &campaigns.ChangesetSyncData{},
			want: clock(),
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := NextSync(clock, tt.h)
			if diff := cmp.Diff(got, tt.want); diff != "" {
				t.Fatal(diff)
			}
		})
	}
}

func TestChangesetPriorityQueue(t *testing.T) {
	t.Parallel()

	assertOrder := func(t *testing.T, q *changesetPriorityQueue, expected []int64) {
		t.Helper()
		ids := make([]int64, len(q.items))
		for i := range ids {
			ids[i] = q.items[i].changesetID
		}
		if diff := cmp.Diff(expected, ids); diff != "" {
			t.Fatal(diff)
		}
	}

	now := time.Now()
	q := newChangesetPriorityQueue()

	items := []scheduledSync{
		{
			changesetID: 1,
			nextSync:    now,
			priority:    priorityNormal,
		},
		{
			changesetID: 2,
			nextSync:    now,
			priority:    priorityHigh,
		},
		{
			changesetID: 3,
			nextSync:    now.Add(-1 * time.Minute),
			priority:    priorityNormal,
		},
		{
			changesetID: 4,
			nextSync:    now.Add(-2 * time.Hour),
			priority:    priorityNormal,
		},
		{
			changesetID: 5,
			nextSync:    now.Add(1 * time.Hour),
			priority:    priorityNormal,
		},
	}

	for i := range items {
		q.Upsert(items[i])
	}

	assertOrder(t, q, []int64{2, 4, 3, 1, 5})

	// Set item to high priority
	q.Upsert(scheduledSync{
		changesetID: 4,
		nextSync:    now.Add(-2 * time.Hour),
		priority:    priorityHigh,
	})

	assertOrder(t, q, []int64{4, 2, 3, 1, 5})

	// Can't reduce priority of existing item
	q.Upsert(scheduledSync{
		changesetID: 4,
		nextSync:    now.Add(-2 * time.Hour),
		priority:    priorityNormal,
	})

	if q.Len() != len(items) {
		t.Fatalf("Expected %d, got %d", q.Len(), len(items))
	}

	assertOrder(t, q, []int64{4, 2, 3, 1, 5})

	for i := 0; i < len(items); i++ {
		peeked, ok := q.Peek()
		if !ok {
			t.Fatalf("Queue should not be empty")
		}
		item := heap.Pop(q).(scheduledSync)
		if peeked.changesetID != item.changesetID {
			t.Fatalf("Peeked and Popped item should have the same id")
		}
	}

	// Len() should be zero after all items popped
	if q.Len() != 0 {
		t.Fatalf("Expected %d, got %d", q.Len(), 0)
	}
}

func TestSyncerRun(t *testing.T) {
	t.Run("Sync due", func(t *testing.T) {
		ctx, cancel := context.WithCancel(context.Background())
		now := time.Now()
		store := MockSyncStore{
			listChangesetSyncData: func(ctx context.Context, opts store.ListChangesetSyncDataOpts) ([]*campaigns.ChangesetSyncData, error) {
				return []*campaigns.ChangesetSyncData{
					{
						ChangesetID:       1,
						UpdatedAt:         now.Add(-2 * maxSyncDelay),
						LatestEvent:       now.Add(-2 * maxSyncDelay),
						ExternalUpdatedAt: now.Add(-2 * maxSyncDelay),
					},
				}, nil
			},
		}
		syncFunc := func(ctx context.Context, ids int64) error {
			cancel()
			return nil
		}
		syncer := &changesetSyncer{
			syncStore:        store,
			scheduleInterval: 10 * time.Minute,
			syncFunc:         syncFunc,
		}
		go syncer.Run(ctx)
		select {
		case <-ctx.Done():
		case <-time.After(100 * time.Millisecond):
			t.Fatal("Sync should have been triggered")
		}
	})

	t.Run("Sync due but reenqueued for reconciler", func(t *testing.T) {
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Millisecond)
		defer cancel()
		now := time.Now()
		updateCalled := false
		store := MockSyncStore{
			getChangeset: func(context.Context, store.GetChangesetOpts) (*campaigns.Changeset, error) {
				// Return ErrNoResults, which is the result you get when the changeset preconditions aren't met anymore.
				// The sync data checks for the reconciler state and if it changed since the sync data was loaded,
				// we don't get back the changeset here and skip it.
				//
				// If we don't return ErrNoResults, the rest of the test will fail, because not all
				// methods of sync store are mocked.
				return nil, store.ErrNoResults
			},
			updateChangeset: func(context.Context, *campaigns.Changeset) error {
				updateCalled = true
				return nil
			},
			listChangesetSyncData: func(ctx context.Context, opts store.ListChangesetSyncDataOpts) ([]*campaigns.ChangesetSyncData, error) {
				return []*campaigns.ChangesetSyncData{
					{
						ChangesetID:       1,
						UpdatedAt:         now.Add(-2 * maxSyncDelay),
						LatestEvent:       now.Add(-2 * maxSyncDelay),
						ExternalUpdatedAt: now.Add(-2 * maxSyncDelay),
					},
				}, nil
			},
		}
		syncer := &changesetSyncer{
			syncStore:        store,
			scheduleInterval: 10 * time.Minute,
		}
		syncer.Run(ctx)
		if updateCalled {
			t.Fatal("Called UpdateChangeset, but shouldn't have")
		}
	})

	t.Run("Sync not due", func(t *testing.T) {
		ctx, cancel := context.WithTimeout(context.Background(), 10*time.Millisecond)
		defer cancel()
		now := time.Now()
		store := MockSyncStore{
			listChangesetSyncData: func(ctx context.Context, opts store.ListChangesetSyncDataOpts) ([]*campaigns.ChangesetSyncData, error) {
				return []*campaigns.ChangesetSyncData{
					{
						ChangesetID:       1,
						UpdatedAt:         now,
						LatestEvent:       now,
						ExternalUpdatedAt: now,
					},
				}, nil
			},
		}
		var syncCalled bool
		syncFunc := func(ctx context.Context, ids int64) error {
			syncCalled = true
			return nil
		}
		syncer := &changesetSyncer{
			syncStore:        store,
			scheduleInterval: 10 * time.Minute,
			syncFunc:         syncFunc,
		}
		syncer.Run(ctx)
		if syncCalled {
			t.Fatal("Sync should not have been triggered")
		}
	})

	t.Run("Priority added", func(t *testing.T) {
		// Empty schedule but then we add an item
		ctx, cancel := context.WithCancel(context.Background())
		store := MockSyncStore{
			listChangesetSyncData: func(ctx context.Context, opts store.ListChangesetSyncDataOpts) ([]*campaigns.ChangesetSyncData, error) {
				return []*campaigns.ChangesetSyncData{}, nil
			},
		}
		syncFunc := func(ctx context.Context, ids int64) error {
			cancel()
			return nil
		}
		syncer := &changesetSyncer{
			syncStore:        store,
			scheduleInterval: 10 * time.Minute,
			syncFunc:         syncFunc,
			priorityNotify:   make(chan []int64, 1),
		}
		syncer.priorityNotify <- []int64{1}
		go syncer.Run(ctx)
		select {
		case <-ctx.Done():
		case <-time.After(100 * time.Millisecond):
			t.Fatal("Sync not called")
		}
	})
}

func TestSyncRegistry(t *testing.T) {
	t.Parallel()

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	now := time.Now()

	extSvc := &types.ExternalService{
		ID:          1,
		Kind:        extsvc.KindGitHub,
		DisplayName: "",
		Config:      `{"url": "https://example.com/"}`,
		CreatedAt:   time.Time{},
		UpdatedAt:   time.Time{},
	}

	var repoStore MockRepoStore

	esStore := MockExternalServiceStore{
		list: func(context.Context, database.ExternalServicesListOptions) ([]*types.ExternalService, error) {
			return []*types.ExternalService{
				extSvc,
			}, nil
		},
	}

	syncStore := MockSyncStore{
		listChangesetSyncData: func(ctx context.Context, opts store.ListChangesetSyncDataOpts) (data []*campaigns.ChangesetSyncData, err error) {
			return []*campaigns.ChangesetSyncData{
				{
					ChangesetID:           1,
					UpdatedAt:             now,
					RepoExternalServiceID: "https://example.com/",
				},
			}, nil
		},
	}

	r := NewSyncRegistry(ctx, syncStore, repoStore, esStore, nil)

	assertSyncerCount := func(want int) {
		r.mu.Lock()
		if len(r.syncers) != want {
			t.Fatalf("Expected %d syncer, got %d", want, len(r.syncers))
		}
		r.mu.Unlock()
	}

	assertSyncerCount(1)

	// Adding it again should have no effect
	r.Add(extSvc)
	assertSyncerCount(1)

	// Simulate a service being removed
	r.HandleExternalServiceSync(api.ExternalService{
		ID:        1,
		Kind:      extsvc.KindGitHub,
		Config:    `{"url": "https://example.com/"}`,
		DeletedAt: now,
	})
	assertSyncerCount(0)

	// And added again
	r.HandleExternalServiceSync(api.ExternalService{
		ID:   1,
		Kind: extsvc.KindGitHub,
	})
	assertSyncerCount(1)

	syncChan := make(chan int64, 1)

	// In order to test that priority items are delivered we'll inject our own syncer
	// with a custom sync func
	syncer := &changesetSyncer{
		syncStore:   syncStore,
		reposStore:  repoStore,
		codeHostURL: "https://example.com/",
		syncFunc: func(ctx context.Context, id int64) error {
			syncChan <- id
			return nil
		},
		priorityNotify: make(chan []int64, 1),
	}
	go syncer.Run(ctx)

	// Set the syncer
	r.mu.Lock()
	r.syncers["https://example.com/"] = syncer
	r.mu.Unlock()

	// Send priority items
	err := r.EnqueueChangesetSyncs(ctx, []int64{1, 2})
	if err != nil {
		t.Fatal(err)
	}

	select {
	case id := <-syncChan:
		if id != 1 {
			t.Fatalf("Expected 1, got %d", id)
		}
	case <-time.After(1 * time.Second):
		t.Fatal("Timed out waiting for sync")
	}
}

type MockSyncStore struct {
	listChangesetSyncData func(context.Context, store.ListChangesetSyncDataOpts) ([]*campaigns.ChangesetSyncData, error)
	getChangeset          func(context.Context, store.GetChangesetOpts) (*campaigns.Changeset, error)
	updateChangeset       func(context.Context, *campaigns.Changeset) error
	upsertChangesetEvents func(context.Context, ...*campaigns.ChangesetEvent) error
	transact              func(context.Context) (*store.Store, error)
}

func (m MockSyncStore) ListChangesetSyncData(ctx context.Context, opts store.ListChangesetSyncDataOpts) ([]*campaigns.ChangesetSyncData, error) {
	return m.listChangesetSyncData(ctx, opts)
}

func (m MockSyncStore) GetChangeset(ctx context.Context, opts store.GetChangesetOpts) (*campaigns.Changeset, error) {
	return m.getChangeset(ctx, opts)
}

func (m MockSyncStore) UpdateChangeset(ctx context.Context, c *campaigns.Changeset) error {
	return m.updateChangeset(ctx, c)
}

func (m MockSyncStore) UpsertChangesetEvents(ctx context.Context, cs ...*campaigns.ChangesetEvent) error {
	return m.upsertChangesetEvents(ctx, cs...)
}

func (m MockSyncStore) Transact(ctx context.Context) (*store.Store, error) {
	return m.transact(ctx)
}

type MockRepoStore struct {
	get func(ctx context.Context, id api.RepoID) (*types.Repo, error)
}

func (m MockRepoStore) Get(ctx context.Context, id api.RepoID) (*types.Repo, error) {
	return m.get(ctx, id)
}

type MockExternalServiceStore struct {
	list func(context.Context, database.ExternalServicesListOptions) ([]*types.ExternalService, error)
}

func (m MockExternalServiceStore) List(ctx context.Context, args database.ExternalServicesListOptions) ([]*types.ExternalService, error) {
	return m.list(ctx, args)
}
