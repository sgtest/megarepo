package campaigns

import (
	"container/heap"
	"context"
	"fmt"
	"strconv"
	"sync"
	"time"

	"github.com/inconshreveable/log15"
	"github.com/pkg/errors"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
	"github.com/sourcegraph/sourcegraph/cmd/repo-updater/repos"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/campaigns"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
)

// SyncRegistry manages a ChangesetSyncer per code host
type SyncRegistry struct {
	Ctx         context.Context
	SyncStore   SyncStore
	RepoStore   RepoStore
	HTTPFactory *httpcli.Factory

	// Used to receive high priority sync requests
	priorityNotify chan []int64

	mu sync.Mutex
	// key is the external service id
	syncers map[int64]*ChangesetSyncer
}

type RepoStore interface {
	ListExternalServices(context.Context, repos.StoreListExternalServicesArgs) ([]*repos.ExternalService, error)
	ListRepos(context.Context, repos.StoreListReposArgs) ([]*repos.Repo, error)
}

// NewSyncRegistry creates a new sync registry which starts a syncer for each code host and will update them
// when external services are changed, added or removed.
func NewSyncRegistry(ctx context.Context, store SyncStore, repoStore RepoStore, cf *httpcli.Factory) *SyncRegistry {
	r := &SyncRegistry{
		Ctx:            ctx,
		SyncStore:      store,
		RepoStore:      repoStore,
		HTTPFactory:    cf,
		priorityNotify: make(chan []int64, 500),
		syncers:        make(map[int64]*ChangesetSyncer),
	}

	services, err := repoStore.ListExternalServices(ctx, repos.StoreListExternalServicesArgs{})
	if err != nil {
		log15.Error("Fetching initial external services", "err", err)
	}

	// Add and start syncers
	for _, service := range services {
		r.Add(service)
	}

	go r.handlePriorityItems()

	return r
}

// Add adds a syncer for the code host associated with the supplied external service if the syncer hasn't
// already been added and starts it.
func (s *SyncRegistry) Add(extSvc *repos.ExternalService) {
	if !campaigns.IsKindSupported(extSvc.Kind) {
		log15.Info("External service not support by campaigns", "kind", extSvc.Kind)
		return
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	if _, ok := s.syncers[extSvc.ID]; ok {
		// Already added
		return
	}

	baseURL, err := extsvc.ExtractBaseURL(extSvc.Kind, extSvc.Config)
	if err != nil {
		log15.Error("getting normalized URL from service", err)
		return
	}

	sourcer := repos.NewSourcer(s.HTTPFactory)
	source, err := buildChangesetSource(sourcer, extSvc)
	if err != nil {
		log15.Error(err.Error())
		return
	}

	// We need to be able to cancel the syncer if the service is removed
	ctx, cancel := context.WithCancel(s.Ctx)

	syncer := &ChangesetSyncer{
		SyncStore:      s.SyncStore,
		ReposStore:     s.RepoStore,
		source:         source,
		codeHostURL:    baseURL.String(),
		extSvcID:       extSvc.ID,
		cancel:         cancel,
		priorityNotify: make(chan []int64, 500),
	}

	s.syncers[extSvc.ID] = syncer

	go syncer.Run(ctx)
}

// handlePriorityItems fetches changesets in the priority queue from the db and passes them
// to the appropriate syncer.
func (s *SyncRegistry) handlePriorityItems() {
	fetchSyncData := func(ids []int64) ([]*campaigns.ChangesetSyncData, error) {
		ctx, cancel := context.WithTimeout(s.Ctx, 10*time.Second)
		defer cancel()
		return s.SyncStore.ListChangesetSyncData(ctx, ListChangesetSyncDataOpts{ChangesetIDs: ids})
	}
	for {
		select {
		case <-s.Ctx.Done():
			return
		case ids := <-s.priorityNotify:
			syncData, err := fetchSyncData(ids)
			if err != nil {
				log15.Error("Fetching sync data", "err", err)
				continue
			}

			// Group changesets by external service
			changesetByService := make(map[int64][]int64)
			for _, d := range syncData {
				changesetByService[d.RepoExternalServiceID] = append(changesetByService[d.RepoExternalServiceID], d.ChangesetID)
			}

			// Anonymous func so we can use defer
			func() {
				s.mu.Lock()
				defer s.mu.Unlock()
				for svc, changesets := range changesetByService {
					syncer, ok := s.syncers[svc]
					if !ok {
						continue
					}

					select {
					case syncer.priorityNotify <- changesets:
					default:
					}
				}
			}()
		}
	}
}

// EnqueueChangesetSyncs will enqueue the changesets with the supplied ids for high priority syncing.
// An error indicates that no changesets have been enqueued.
func (s *SyncRegistry) EnqueueChangesetSyncs(ctx context.Context, ids []int64) error {
	// The channel below is buffered so we'll usually send without blocking.
	// It is important not to block here as this method is called from the UI
	select {
	case s.priorityNotify <- ids:
	default:
		return errors.New("high priority sync capacity reached")
	}
	return nil
}

// HandleExternalServiceSync handles changes to external services.
func (s *SyncRegistry) HandleExternalServiceSync(es api.ExternalService) {
	s.mu.Lock()
	syncer, exists := s.syncers[es.ID]
	s.mu.Unlock()

	if timeIsNilOrZero(es.DeletedAt) && !exists {
		s.Add(extsvcTypeApiToRepos(&es))
	}

	s.mu.Lock()
	defer s.mu.Unlock()
	if es.DeletedAt != nil && exists {
		delete(s.syncers, es.ID)
		syncer.cancel()
	}
}

func timeIsNilOrZero(t *time.Time) bool {
	if t == nil {
		return true
	}
	return t.IsZero()
}

// A ChangesetSyncer periodically syncs metadata of changesets
// saved in the database.
type ChangesetSyncer struct {
	SyncStore  SyncStore
	ReposStore RepoStore

	source repos.ChangesetSource

	codeHostURL string
	extSvcID    int64

	// scheduleInterval determines how often a new schedule will be computed.
	// NOTE: It involves a DB query but no communication with code hosts.
	scheduleInterval time.Duration

	queue          *changesetPriorityQueue
	priorityNotify chan []int64

	// Replaceable for testing
	syncFunc func(ctx context.Context, id int64) error
	clock    func() time.Time

	// cancel should be called to stop this syncer
	cancel context.CancelFunc
}

var syncerMetrics = struct {
	syncs                   *prometheus.CounterVec
	priorityQueued          *prometheus.CounterVec
	syncDuration            *prometheus.HistogramVec
	computeScheduleDuration *prometheus.HistogramVec
	scheduleSize            *prometheus.GaugeVec
	behindSchedule          *prometheus.GaugeVec
}{}

func init() {
	syncerMetrics.syncs = promauto.NewCounterVec(prometheus.CounterOpts{
		Name: "src_repoupdater_changeset_syncer_syncs",
		Help: "Total number of changeset syncs",
	}, []string{"codehost", "success"})
	syncerMetrics.priorityQueued = promauto.NewCounterVec(prometheus.CounterOpts{
		Name: "src_repoupdater_changeset_syncer_priority_queued",
		Help: "Total number of priority items added to queue",
	}, []string{"codehost"})
	syncerMetrics.syncDuration = promauto.NewHistogramVec(prometheus.HistogramOpts{
		Name:    "src_repoupdater_changeset_syncer_sync_duration_seconds",
		Help:    "Time spent syncing changesets",
		Buckets: []float64{1, 2, 5, 10, 30, 60, 120},
	}, []string{"codehost", "success"})
	syncerMetrics.computeScheduleDuration = promauto.NewHistogramVec(prometheus.HistogramOpts{
		Name:    "src_repoupdater_changeset_syncer_compute_schedule_duration_seconds",
		Help:    "Time spent computing changeset schedule",
		Buckets: []float64{1, 2, 5, 10, 30, 60, 120},
	}, []string{"codehost", "success"})
	syncerMetrics.scheduleSize = promauto.NewGaugeVec(prometheus.GaugeOpts{
		Name: "src_repoupdater_changeset_syncer_schedule_size",
		Help: "The number of changesets scheduled to sync",
	}, []string{"codehost"})
	syncerMetrics.behindSchedule = promauto.NewGaugeVec(prometheus.GaugeOpts{
		Name: "src_repoupdater_changeset_syncer_behind_schedule",
		Help: "The number of changesets behind schedule",
	}, []string{"codehost"})
}

type SyncStore interface {
	ListChangesetSyncData(context.Context, ListChangesetSyncDataOpts) ([]*campaigns.ChangesetSyncData, error)
	GetChangeset(context.Context, GetChangesetOpts) (*campaigns.Changeset, error)
	ListChangesets(context.Context, ListChangesetsOpts) (campaigns.Changesets, int64, error)
	UpdateChangeset(ctx context.Context, cs *campaigns.Changeset) error
	UpsertChangesetEvents(ctx context.Context, cs ...*campaigns.ChangesetEvent) error
	Transact(context.Context) (*Store, error)
}

// Run will start the process of changeset syncing. It is long running
// and is expected to be launched once at startup.
func (s *ChangesetSyncer) Run(ctx context.Context) {
	scheduleInterval := s.scheduleInterval
	if scheduleInterval == 0 {
		scheduleInterval = 2 * time.Minute
	}
	if s.syncFunc == nil {
		s.syncFunc = s.SyncChangeset
	}
	if s.clock == nil {
		s.clock = time.Now
	}
	s.queue = newChangesetPriorityQueue()
	// How often to refresh the schedule
	scheduleTicker := time.NewTicker(scheduleInterval)

	// Get initial schedule
	if sched, err := s.computeSchedule(ctx); err != nil {
		// Non fatal as we'll try again later in the main loop
		log15.Error("Computing schedule", "err", err)
	} else {
		s.queue.Upsert(sched...)
	}

	// Prioritize changesets without diffstats on startup.
	if err := s.prioritizeChangesetsWithoutDiffStats(ctx); err != nil {
		log15.Error("Prioritizing changesets", "err", err)
	}

	var next scheduledSync
	var ok bool

	// NOTE: All mutations of the queue should be done is this loop as operations on the queue
	// are not safe for concurrent use
	for {
		var timer *time.Timer
		var timerChan <-chan time.Time
		next, ok = s.queue.Peek()

		if ok {
			// Queue isn't empty
			if next.priority == priorityHigh {
				// Fire ASAP
				timer = time.NewTimer(0)
			} else {
				// Use scheduled time
				timer = time.NewTimer(time.Until(next.nextSync))
			}
			timerChan = timer.C
		}

		select {
		case <-ctx.Done():
			return
		case <-scheduleTicker.C:
			if timer != nil {
				timer.Stop()
			}
			start := s.clock()
			schedule, err := s.computeSchedule(ctx)
			labelValues := []string{s.codeHostURL, strconv.FormatBool(err == nil)}
			syncerMetrics.computeScheduleDuration.WithLabelValues(labelValues...).Observe(s.clock().Sub(start).Seconds())
			if err != nil {
				log15.Error("Computing queue", "err", err)
				continue
			}
			syncerMetrics.scheduleSize.WithLabelValues(s.codeHostURL).Set(float64(len(schedule)))
			s.queue.Upsert(schedule...)
			var behindSchedule int
			now := s.clock()
			for _, ss := range schedule {
				if ss.nextSync.Before(now) {
					behindSchedule++
				}
			}
			syncerMetrics.behindSchedule.WithLabelValues(s.codeHostURL).Set(float64(behindSchedule))
		case <-timerChan:
			start := s.clock()
			err := s.syncFunc(ctx, next.changesetID)
			labelValues := []string{s.codeHostURL, strconv.FormatBool(err == nil)}
			syncerMetrics.syncDuration.WithLabelValues(labelValues...).Observe(s.clock().Sub(start).Seconds())
			syncerMetrics.syncs.WithLabelValues(labelValues...).Add(1)

			if err != nil {
				log15.Error("Syncing changeset", "err", err)
				// We'll continue and remove it as it'll get retried on next schedule
			}

			// Remove item now that it has been processed
			s.queue.Remove(next.changesetID)
			syncerMetrics.scheduleSize.WithLabelValues(s.codeHostURL).Dec()
		case ids := <-s.priorityNotify:
			if timer != nil {
				timer.Stop()
			}
			for _, id := range ids {
				item, ok := s.queue.Get(id)
				if !ok {
					// Item has been recently synced and removed or we have an invalid id
					// We have no way of telling the difference without making a DB call so
					// add a new item anyway which will just lead to a harmless error later
					item = scheduledSync{
						changesetID: id,
						nextSync:    time.Time{},
					}
				}
				item.priority = priorityHigh
				s.queue.Upsert(item)
				syncerMetrics.scheduleSize.WithLabelValues(s.codeHostURL).Inc()
			}
			syncerMetrics.priorityQueued.WithLabelValues(s.codeHostURL).Add(float64(len(ids)))
		}
	}
}

var (
	minSyncDelay = 2 * time.Minute
	maxSyncDelay = 8 * time.Hour
)

// NextSync computes the time we want the next sync to happen.
func NextSync(clock func() time.Time, h *campaigns.ChangesetSyncData) time.Time {
	lastSync := h.UpdatedAt

	if lastSync.IsZero() {
		// Edge case where we've never synced
		return clock()
	}

	var lastChange time.Time
	// When we perform a sync, event timestamps are all updated even if nothing has changed.
	// We should fall back to h.ExternalUpdated if the diff is small
	// TODO: This is a workaround while we try to implement syncing without always updating events. See: https://github.com/sourcegraph/sourcegraph/pull/8771
	// Once the above issue is fixed we can simply use maxTime(h.ExternalUpdatedAt, h.LatestEvent)
	if diff := h.LatestEvent.Sub(lastSync); !h.LatestEvent.IsZero() && absDuration(diff) < minSyncDelay {
		lastChange = h.ExternalUpdatedAt
	} else {
		lastChange = maxTime(h.ExternalUpdatedAt, h.LatestEvent)
	}

	// Simple linear backoff for now
	diff := lastSync.Sub(lastChange)

	// If the last change has happened AFTER our last sync this indicates a webhook
	// has arrived. In this case, we should check again in minSyncDelay after
	// the hook arrived. If multiple webhooks arrive in close succession this will
	// cause us to wait for a quiet period of at least minSyncDelay
	if diff < 0 {
		return lastChange.Add(minSyncDelay)
	}

	if diff > maxSyncDelay {
		diff = maxSyncDelay
	}
	if diff < minSyncDelay {
		diff = minSyncDelay
	}
	return lastSync.Add(diff)
}

func maxTime(a, b time.Time) time.Time {
	if a.After(b) {
		return a
	}
	return b
}

func absDuration(d time.Duration) time.Duration {
	if d >= 0 {
		return d
	}
	return -1 * d
}

func (s *ChangesetSyncer) computeSchedule(ctx context.Context) ([]scheduledSync, error) {
	syncData, err := s.SyncStore.ListChangesetSyncData(ctx, ListChangesetSyncDataOpts{ExternalServiceID: s.extSvcID})
	if err != nil {
		return nil, errors.Wrap(err, "listing changeset sync data")
	}

	ss := make([]scheduledSync, len(syncData))
	for i := range syncData {
		nextSync := NextSync(s.clock, syncData[i])

		ss[i] = scheduledSync{
			changesetID: syncData[i].ChangesetID,
			nextSync:    nextSync,
		}
	}

	return ss, nil
}

func (s *ChangesetSyncer) prioritizeChangesetsWithoutDiffStats(ctx context.Context) error {
	published := campaigns.ChangesetPublicationStatePublished
	changesets, _, err := s.SyncStore.ListChangesets(ctx, ListChangesetsOpts{
		OnlyWithoutDiffStats: true,
		ExternalServiceID:    s.extSvcID,
		PublicationState:     &published,
		ReconcilerStates:     []campaigns.ReconcilerState{campaigns.ReconcilerStateCompleted},
	})
	if err != nil {
		return err
	}

	if len(changesets) == 0 {
		return nil
	}

	ids := make([]int64, 0, len(changesets))
	for _, cs := range changesets {
		ids = append(ids, cs.ID)
	}
	s.priorityNotify <- ids

	return nil
}

// SyncChangeset will sync a single changeset given its id.
func (s *ChangesetSyncer) SyncChangeset(ctx context.Context, id int64) error {
	log15.Debug("SyncChangeset", "codeHostURL", s.codeHostURL, "extSvcID", s.extSvcID, "id", id)
	cs, err := s.SyncStore.GetChangeset(ctx, GetChangesetOpts{
		ID: id,
	})
	if err != nil {
		return err
	}
	return SyncChangeset(ctx, s.ReposStore, s.SyncStore, s.source, cs)
}

// SyncChangeset refreshes the metadata of the given changeset and
// updates them in the database.
func SyncChangeset(ctx context.Context, repoStore RepoStore, syncStore SyncStore, source repos.ChangesetSource, c *campaigns.Changeset) (err error) {
	rs, err := repoStore.ListRepos(ctx, repos.StoreListReposArgs{
		IDs: []api.RepoID{c.RepoID},
	})
	if err != nil {
		return err
	}
	repo := rs[0]

	repoChangeset := &repos.Changeset{Repo: repo, Changeset: c}
	if err := source.LoadChangeset(ctx, repoChangeset); err != nil {
		_, ok := err.(repos.ChangesetNotFoundError)
		if !ok {
			return err
		}

		if !c.IsDeleted() {
			c.SetDeleted()
		}
	}

	events := c.Events()
	SetDerivedState(ctx, c, events)

	tx, err := syncStore.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Done(err) }()

	c.Unsynced = false
	if err := tx.UpdateChangeset(ctx, c); err != nil {
		return err
	}

	return tx.UpsertChangesetEvents(ctx, events...)
}

// buildChangesetSource returns a ChangesetSource for the given external service.
func buildChangesetSource(
	sourcer repos.Sourcer,
	extSvc *repos.ExternalService,
) (repos.ChangesetSource, error) {
	sources, err := sourcer(extSvc)
	if err != nil {
		return nil, err
	}

	source, ok := sources[0].(repos.ChangesetSource)
	if !ok {
		return nil, fmt.Errorf("ChangesetSource cannot be created from external service %q", extSvc.Kind)
	}

	return source, nil
}

type scheduledSync struct {
	changesetID int64
	nextSync    time.Time
	priority    priority
}

// changesetPriorityQueue is a min heap that sorts syncs by priority
// and time of next sync. It is not safe for concurrent use.
type changesetPriorityQueue struct {
	items []scheduledSync
	index map[int64]int // changesetID -> index
}

// newChangesetPriorityQueue creates a new queue for holding changeset sync instructions in chronological order.
// items with a high priority will always appear at the front of the queue.
func newChangesetPriorityQueue() *changesetPriorityQueue {
	q := &changesetPriorityQueue{
		items: make([]scheduledSync, 0),
		index: make(map[int64]int),
	}
	heap.Init(q)
	return q
}

// The following methods implement heap.Interface based on the priority queue example:
// https://golang.org/pkg/container/heap/#example__priorityQueue

func (pq *changesetPriorityQueue) Len() int { return len(pq.items) }

func (pq *changesetPriorityQueue) Less(i, j int) bool {
	// We want items ordered by priority, then NextSync
	// Order by priority and then NextSync
	a := pq.items[i]
	b := pq.items[j]

	if a.priority != b.priority {
		// Greater than here since we want high priority items to be ranked before low priority
		return a.priority > b.priority
	}
	if !a.nextSync.Equal(b.nextSync) {
		return a.nextSync.Before(b.nextSync)
	}
	return a.changesetID < b.changesetID
}

func (pq *changesetPriorityQueue) Swap(i, j int) {
	pq.items[i], pq.items[j] = pq.items[j], pq.items[i]
	pq.index[pq.items[i].changesetID] = i
	pq.index[pq.items[j].changesetID] = j
}

// Push is here to implement the Heap interface, please use Upsert
func (pq *changesetPriorityQueue) Push(x interface{}) {
	n := len(pq.items)
	item := x.(scheduledSync)
	pq.index[item.changesetID] = n
	pq.items = append(pq.items, item)
}

// Pop is not to be used directly, use heap.Pop(pq)
func (pq *changesetPriorityQueue) Pop() interface{} {
	item := pq.items[len(pq.items)-1]
	delete(pq.index, item.changesetID)
	pq.items = pq.items[:len(pq.items)-1]
	return item
}

// End of heap methods

// Peek fetches the highest priority item without removing it.
func (pq *changesetPriorityQueue) Peek() (scheduledSync, bool) {
	if len(pq.items) == 0 {
		return scheduledSync{}, false
	}
	return pq.items[0], true
}

// Upsert modifies at item if it exists or adds a new item if not.
// NOTE: If an existing item is high priority, it will not be changed back
// to normal. This allows high priority items to stay that way through reschedules.
func (pq *changesetPriorityQueue) Upsert(ss ...scheduledSync) {
	for _, s := range ss {
		i, ok := pq.index[s.changesetID]
		if !ok {
			heap.Push(pq, s)
			continue
		}
		oldPriority := pq.items[i].priority
		pq.items[i] = s
		if oldPriority == priorityHigh {
			pq.items[i].priority = priorityHigh
		}
		heap.Fix(pq, i)
	}
}

// Get fetches the item with the supplied id without removing it.
func (pq *changesetPriorityQueue) Get(id int64) (scheduledSync, bool) {
	i, ok := pq.index[id]
	if !ok {
		return scheduledSync{}, false
	}
	item := pq.items[i]
	return item, true
}

func (pq *changesetPriorityQueue) Remove(id int64) {
	i, ok := pq.index[id]
	if !ok {
		return
	}
	heap.Remove(pq, i)
}

type priority int

const (
	priorityNormal priority = iota
	priorityHigh
)

// extsvcTypeApiToRepos converts the api.ExternalService to repos.ExternalService.
// They're basically the same, just some pointers here and there.
func extsvcTypeApiToRepos(es *api.ExternalService) *repos.ExternalService {
	extSvc := &repos.ExternalService{
		ID:          es.ID,
		Kind:        es.Kind,
		DisplayName: es.DisplayName,
		Config:      es.Config,
		CreatedAt:   es.CreatedAt,
		UpdatedAt:   es.UpdatedAt,
	}
	if es.DeletedAt != nil {
		extSvc.DeletedAt = *es.DeletedAt
	}
	if es.LastSyncAt != nil {
		extSvc.LastSyncAt = *es.LastSyncAt
	}
	if es.NextSyncAt != nil {
		extSvc.NextSyncAt = *es.NextSyncAt
	}
	if es.NamespaceUserID != nil {
		extSvc.NamespaceUserID = *es.NamespaceUserID
	}
	return extSvc
}
