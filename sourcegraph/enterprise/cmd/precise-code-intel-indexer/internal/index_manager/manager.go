package indexmanager

import (
	"context"
	"fmt"
	"sync"
	"time"

	"github.com/efritz/glock"
	"github.com/hashicorp/go-multierror"
	"github.com/inconshreveable/log15"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/store"
	"github.com/sourcegraph/sourcegraph/internal/workerutil"
	"github.com/teivah/onecontext"
)

// Manager tracks which index records are assigned to which indexers.
type Manager interface {
	// Start runs the routine that handles expiration of transactions for indexers which have become
	// unresponsive. This method blocks until Stop has been called.
	Start()

	// Stop will cause Start to exit after the current request. This method blocks until Start has
	// returned. All active transactions known by the manager will be rolled back. Requests to the
	// Dequeue, Complete, or Heartbeat methods should not occur after this method has been called.
	Stop()

	// Dequeue pulls an unprocessed index record from the database and assigns the transaction that
	// locks that record to the given indexer.
	Dequeue(ctx context.Context, indexerName string) (store.Index, bool, error)

	// Complete marks the target index record as complete or errored depending on the existence of
	// an error message, then finalizes the transaction that locks that record.
	Complete(ctx context.Context, indexerName string, indexID int, errorMessage string) (bool, error)

	// Heartbeat bumps the last updated time of the indexer and closes any transactions locking
	// records whose identifiers were not supplied.
	Heartbeat(ctx context.Context, indexerName string, indexIDs []int) error
}

// ThreadedManager is a manager that handles requests that modify database transactions from a single
// goroutine to simplify bookkeeping of live transactions.
type ThreadedManager interface {
	Manager
}

type ManagerOptions struct {
	// MaximumTransactions is the maximum number of active records that can be given out to indexers. The
	// manager dequeue method will stop returning records while the number of outstanding transactions is
	// at this threshold.
	MaximumTransactions int

	// RequeueDelay controls how far into the future to make an indexer's records visible to another
	// agent once it becomes unresponsive.
	RequeueDelay time.Duration

	// CleanupInterval is the duration between cleanup invocations, in which the index records assigned
	// to dead indexers are requeued.
	CleanupInterval time.Duration

	// UnreportedMaxAge is the maximum time between an index record being dequeued and it appearing in
	// the indexer's heartbeat requests before it being considered lost.
	UnreportedIndexMaxAge time.Duration

	// DeathThreshold is the minimum time since the last indexerheartbeat before the indexer can be
	// considered as unresponsive. This should be configured to be longer than the indexer's heartbeat
	// interval.
	DeathThreshold time.Duration
}

type manager struct {
	store            workerutil.Store
	options          ManagerOptions
	clock            glock.Clock
	indexers         map[string]*indexerMeta
	dequeueSemaphore chan struct{}   // tracks available dequeue slots
	m                sync.Mutex      // protects indexers
	ctx              context.Context // root context passed to the database
	cancel           func()          // cancels the root context
	finished         chan struct{}   // signals that Start has finished
}

var _ Manager = &manager{}

// indexerMeta tracks the last request time of an indexer along with the set of index records which it
// is currently processing.
type indexerMeta struct {
	lastUpdate time.Time
	metas      []indexMeta
}

// indexMeta wraps an index record and the tranaction that is currently locking it for processing.
type indexMeta struct {
	index   store.Index
	tx      workerutil.Store
	started time.Time
}

// New creates a new manager with the given store and options.
func New(store workerutil.Store, options ManagerOptions) ThreadedManager {
	return newManager(store, options, glock.NewRealClock())
}

func newManager(store workerutil.Store, options ManagerOptions, clock glock.Clock) ThreadedManager {
	ctx, cancel := context.WithCancel(context.Background())

	dequeueSemaphore := make(chan struct{}, options.MaximumTransactions)
	for i := 0; i < options.MaximumTransactions; i++ {
		dequeueSemaphore <- struct{}{}
	}

	return &manager{
		store:            store,
		options:          options,
		clock:            clock,
		dequeueSemaphore: dequeueSemaphore,
		indexers:         map[string]*indexerMeta{},
		ctx:              ctx,
		cancel:           cancel,
		finished:         make(chan struct{}),
	}
}

// Start runs the routine that handles expiration of transactions for indexers which have become
// unresponsive. This method blocks until Stop has been called.
func (m *manager) Start() {
	defer close(m.finished)

loop:
	for {
		m.cleanup()

		select {
		case <-m.clock.After(m.options.CleanupInterval):
		case <-m.ctx.Done():
			break loop
		}
	}

	m.m.Lock()
	defer m.m.Unlock()

	for _, indexer := range m.indexers {
		for _, meta := range indexer.metas {
			if err := meta.tx.Done(m.ctx.Err()); err != nil && err != m.ctx.Err() {
				log15.Error(fmt.Sprintf("Failed to close transaction holding index %d", meta.index.ID), "err", err)
			}
		}
	}
}

// cleanup requeues every locked index record assigned to indexers which have not been updated
// for longer than the death threshold.
func (m *manager) cleanup() {
	if err := m.requeueIndexes(m.ctx, m.pruneIndexers()); err != nil {
		log15.Error("Failed to requeue indexes", "err", err)
	}
}

// pruneIndexers removes the data associated with indexers which have not been updated for longer
// than the death threshold and returns all index meta values assigned to removed indexers.
func (m *manager) pruneIndexers() (metas []indexMeta) {
	m.m.Lock()
	defer m.m.Unlock()

	for name, indexer := range m.indexers {
		if m.clock.Now().Sub(indexer.lastUpdate) <= m.options.DeathThreshold {
			continue
		}

		metas = append(metas, indexer.metas...)
		delete(m.indexers, name)
	}

	return metas
}

// Stop will cause Start to exit after the current request. This method blocks until Start has
// returned. All active transactions known by the manager will be rolled back. Requests to the
// Dequeue, Complete, or Heartbeat methods should not occur after this method has been called.
func (m *manager) Stop() {
	m.cancel()
	<-m.finished
}

// Dequeue pulls an unprocessed index record from the database and assigns the transaction that
// locks that record to the given indexer.
func (m *manager) Dequeue(ctx context.Context, indexerName string) (_ store.Index, dequeued bool, _ error) {
	ctx, cancel := onecontext.Merge(ctx, m.ctx)
	defer cancel()

	select {
	case <-m.dequeueSemaphore:
	default:
		return store.Index{}, false, nil
	}
	defer func() {
		if !dequeued {
			// Ensure that if we do not dequeue a record successfully we do not
			// leak from the semaphore. This will happen if the dequeue call fails
			// or if there are no records to process
			m.dequeueSemaphore <- struct{}{}
		}
	}()

	record, tx, dequeued, err := m.store.DequeueWithIndependentTransactionContext(ctx, nil)
	if err != nil {
		return store.Index{}, false, err
	}
	if !dequeued {
		return store.Index{}, false, nil
	}

	now := m.clock.Now()
	index := record.(store.Index)
	m.addMeta(indexerName, indexMeta{index: index, tx: tx, started: now})
	return index, true, nil
}

// addMeta removes the given index to the given indexer. This method also updates the last
// updated time of the indexer.
func (m *manager) addMeta(indexerName string, meta indexMeta) {
	m.m.Lock()
	defer m.m.Unlock()

	indexer, ok := m.indexers[indexerName]
	if !ok {
		indexer = &indexerMeta{}
		m.indexers[indexerName] = indexer
	}

	now := m.clock.Now()
	indexer.metas = append(indexer.metas, meta)
	indexer.lastUpdate = now
}

// Complete marks the target index record as complete or errored depending on the existence of
// an error message, then finalizes the transaction that locks that record.
func (m *manager) Complete(ctx context.Context, indexerName string, indexID int, errorMessage string) (bool, error) {
	ctx, cancel := onecontext.Merge(ctx, m.ctx)
	defer cancel()

	index, ok := m.findMeta(indexerName, indexID)
	if !ok {
		return false, nil
	}

	if err := m.completeIndex(ctx, index, errorMessage); err != nil {
		return false, err
	}

	return true, nil
}

// findMeta finds and returns an index meta value matching the given index identifier. If found,
// the meta value is removed from the indexer.
func (m *manager) findMeta(indexerName string, indexID int) (indexMeta, bool) {
	m.m.Lock()
	defer m.m.Unlock()

	indexer, ok := m.indexers[indexerName]
	if !ok {
		return indexMeta{}, false
	}

	for i, meta := range indexer.metas {
		if meta.index.ID != indexID {
			continue
		}

		l := len(indexer.metas) - 1
		indexer.metas[i] = indexer.metas[l]
		indexer.metas = indexer.metas[:l]
		return meta, true
	}

	return indexMeta{}, false
}

// completeIndex marks the target index record as complete or errored depending on the existence
// of an error message, then finalizes the transaction that locks that record.
func (m *manager) completeIndex(ctx context.Context, meta indexMeta, errorMessage string) (err error) {
	defer func() { m.dequeueSemaphore <- struct{}{} }()

	if errorMessage == "" {
		_, err = meta.tx.MarkComplete(ctx, meta.index.ID)
	} else {
		_, err = meta.tx.MarkErrored(ctx, meta.index.ID, errorMessage)
	}

	return meta.tx.Done(err)
}

// Heartbeat bumps the last updated time of the indexer and closes any transactions locking
// records whose identifiers were not supplied.
func (m *manager) Heartbeat(ctx context.Context, indexerName string, indexIDs []int) error {
	return m.requeueIndexes(ctx, m.pruneIndexes(indexerName, indexIDs))
}

// pruneIndexes removes the indexes whose identifier is not in the given list from the given indexer.
// This method returns the index meta values which were removed. Index meta values which were created
// very recently will be counted as live to account for the time between when the record is dequeued
// in this service and when it is added to the heartbeat requests from the indexer. This method also
// updates the last updated time of the indexer.
func (m *manager) pruneIndexes(indexerName string, ids []int) (dead []indexMeta) {
	now := m.clock.Now()

	idMap := map[int]struct{}{}
	for _, id := range ids {
		idMap[id] = struct{}{}
	}

	m.m.Lock()
	defer m.m.Unlock()

	indexer, ok := m.indexers[indexerName]
	if !ok {
		indexer = &indexerMeta{}
		m.indexers[indexerName] = indexer
	}

	var live []indexMeta
	for _, meta := range indexer.metas {
		if _, ok := idMap[meta.index.ID]; ok || now.Sub(meta.started) < m.options.UnreportedIndexMaxAge {
			live = append(live, meta)
		} else {
			dead = append(dead, meta)
		}
	}

	indexer.metas = live
	indexer.lastUpdate = now
	return dead
}

// requeueIndexes requeues the given index records.
func (m *manager) requeueIndexes(ctx context.Context, metas []indexMeta) (errs error) {
	for _, meta := range metas {
		if err := m.requeueIndex(ctx, meta); err != nil {
			errs = multierror.Append(errs, err)
		}
	}

	return errs
}

// requeueIndex requeues the given index record , then finalizes the transaction that locks that record.
func (m *manager) requeueIndex(ctx context.Context, meta indexMeta) error {
	defer func() { m.dequeueSemaphore <- struct{}{} }()

	err := meta.tx.Requeue(ctx, meta.index.ID, m.clock.Now().Add(m.options.RequeueDelay))
	return meta.tx.Done(err)
}
