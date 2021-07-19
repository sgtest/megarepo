package backend

import (
	"container/heap"
	"context"
	"fmt"
	"math"
	"sort"
	"sync"
	"time"

	"github.com/google/zoekt"
	"github.com/google/zoekt/query"
	"github.com/google/zoekt/stream"
	"github.com/inconshreveable/log15"
	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promauto"
	"golang.org/x/sync/errgroup"
)

var reorderQueueSize = promauto.NewHistogramVec(prometheus.HistogramOpts{
	Name:    "src_zoekt_reorder_queue_size",
	Help:    "Maximum size of result reordering buffer for a request.",
	Buckets: prometheus.ExponentialBuckets(4, 2, 10),
}, nil)

// HorizontalSearcher is a Streamer which aggregates searches over
// Map. It manages the connections to Map as the endpoints come and go.
type HorizontalSearcher struct {
	// Map is a subset of EndpointMap only using the Endpoints function. We
	// use this to find the endpoints to dial over time.
	Map interface {
		Endpoints() (map[string]struct{}, error)
	}
	Dial func(endpoint string) zoekt.Streamer

	mu      sync.RWMutex
	clients map[string]zoekt.Streamer // addr -> client
}

// StreamSearch does a search which merges the stream from every endpoint in Map, reordering results to produce a sorted stream.
func (s *HorizontalSearcher) StreamSearch(ctx context.Context, q query.Q, opts *zoekt.SearchOptions, streamer zoekt.Sender) error {
	clients, err := s.searchers()
	if err != nil {
		return err
	}

	// During rebalancing a repository can appear on more than one replica.
	var mu sync.Mutex
	dedupper := dedupper{}

	// The results from each endpoint are mostly sorted by priority, with bounded errors described
	// by SearchResult.Stats.MaxPendingPriority. Each backend will dispatch searches in parallel against
	// its shards in priority order, but the actual return order of those searches is not constrained.
	//
	// Instead, the backend will report the maximum priority shard that it still has pending along with
	// the results that it returns, so we accumulate results in a heap and only pop when the top item
	// has a priority greater than the maximum of all endpoints' pending results.
	endpointMaxPendingPriority := map[string]float64{}
	resultQueue := priorityQueue{}
	resultQueueMaxLength := 0 // for a prometheus metric

	// To start, initialize every endpoint's maxPending to +inf since we don't yet know the bounds.
	for endpoint := range clients {
		endpointMaxPendingPriority[endpoint] = math.Inf(1)
	}

	g, ctx := errgroup.WithContext(ctx)
	for endpoint, c := range clients {
		endpoint := endpoint
		c := c
		g.Go(func() error {
			err := c.StreamSearch(ctx, q, opts, stream.SenderFunc(func(sr *zoekt.SearchResult) {
				// This shouldn't happen, but skip event if sr is nil.
				if sr == nil {
					return
				}

				mu.Lock()
				sr.Files = dedupper.Dedup(endpoint, sr.Files)

				// Note the endpoint's updated MaxPendingPriority, and recompute
				// it across all endpoints to determine what search results are stable.
				endpointMaxPendingPriority[endpoint] = sr.Progress.MaxPendingPriority
				maxPending := math.Inf(-1)
				for _, pri := range endpointMaxPendingPriority {
					if pri > maxPending {
						maxPending = pri
					}
				}

				// Pop and send search results where it is guaranteed that no higher-priority result
				// is possible, because there are no pending shards with a greater priority.
				resultQueue.add(sr)
				if resultQueue.Len() > resultQueueMaxLength {
					resultQueueMaxLength = resultQueue.Len()
				}
				for resultQueue.isTopAbove(maxPending) {
					streamer.Send(heap.Pop(&resultQueue).(*zoekt.SearchResult))
				}

				mu.Unlock()
			}))
			mu.Lock()
			// Clear pending priority because the endpoint is done sending results--
			// otherwise, an endpoint with 0 results could delay results returning,
			// because it would never set its maxPendingPriority to 0 in the StreamSearch
			// callback.
			delete(endpointMaxPendingPriority, endpoint)
			mu.Unlock()
			return err
		})
	}
	err = g.Wait()
	if err != nil {
		return err
	}
	reorderQueueSize.WithLabelValues().Observe(float64(resultQueueMaxLength))
	if len(resultQueue) > 0 {
		log15.Warn("HorizontalSearcher.Streamsearch: results not sent in core loop", len(resultQueue))
		for len(resultQueue) > 0 {
			streamer.Send(heap.Pop(&resultQueue).(*zoekt.SearchResult))
		}
	}
	return nil
}

// priorityQueue modified from https://golang.org/pkg/container/heap/
// A priorityQueue implements heap.Interface and holds Items.
// All Exported methods are part of the container.heap interface, and
// unexported methods are local helpers.
type priorityQueue []*zoekt.SearchResult

func (pq *priorityQueue) add(sr *zoekt.SearchResult) {
	heap.Push(pq, sr)
}

func (pq *priorityQueue) isTopAbove(limit float64) bool {
	return len(*pq) > 0 && (*pq)[0].Progress.Priority >= limit
}

func (pq priorityQueue) Len() int { return len(pq) }

func (pq priorityQueue) Less(i, j int) bool {
	// We want Pop to give us the highest, not lowest, priority so we use greater than here.
	return pq[i].Progress.Priority > pq[j].Progress.Priority
}

func (pq priorityQueue) Swap(i, j int) {
	pq[i], pq[j] = pq[j], pq[i]
}

func (pq *priorityQueue) Push(x interface{}) {
	*pq = append(*pq, x.(*zoekt.SearchResult))
}

func (pq *priorityQueue) Pop() interface{} {
	old := *pq
	n := len(old)
	item := old[n-1]
	old[n-1] = nil // avoid memory leak
	*pq = old[0 : n-1]
	return item
}

// Search aggregates search over every endpoint in Map.
func (s *HorizontalSearcher) Search(ctx context.Context, q query.Q, opts *zoekt.SearchOptions) (*zoekt.SearchResult, error) {
	return AggregateStreamSearch(ctx, s.StreamSearch, q, opts)
}

// AggregateStreamSearch aggregates the stream events into a single batch
// result.
func AggregateStreamSearch(ctx context.Context, streamSearch func(context.Context, query.Q, *zoekt.SearchOptions, zoekt.Sender) error, q query.Q, opts *zoekt.SearchOptions) (*zoekt.SearchResult, error) {
	start := time.Now()

	var mu sync.Mutex
	aggregate := &zoekt.SearchResult{}

	ctx, cancel := context.WithCancel(ctx)
	defer cancel()

	err := streamSearch(ctx, q, opts, ZoektStreamFunc(func(event *zoekt.SearchResult) {
		mu.Lock()
		defer mu.Unlock()
		aggregate.Files = append(aggregate.Files, event.Files...)
		aggregate.Stats.Add(event.Stats)
	}))
	if err != nil {
		return nil, err
	}

	aggregate.Duration = time.Since(start)

	return aggregate, nil
}

// List aggregates list over every endpoint in Map.
func (s *HorizontalSearcher) List(ctx context.Context, q query.Q, opts *zoekt.ListOptions) (*zoekt.RepoList, error) {
	clients, err := s.searchers()
	if err != nil {
		return nil, err
	}

	var cancel context.CancelFunc
	ctx, cancel = context.WithCancel(ctx)
	defer cancel()

	type result struct {
		rl  *zoekt.RepoList
		err error
	}
	results := make(chan result, len(clients))
	for _, c := range clients {
		go func(c zoekt.Streamer) {
			rl, err := c.List(ctx, q, opts)
			results <- result{rl: rl, err: err}
		}(c)
	}

	// PERF: We don't deduplicate Repos since the only user of List already
	// does deduplication.

	aggregate := zoekt.RepoList{
		Minimal: make(map[uint32]*zoekt.MinimalRepoListEntry),
	}
	for range clients {
		r := <-results
		if r.err != nil {
			return nil, r.err
		}

		aggregate.Repos = append(aggregate.Repos, r.rl.Repos...)
		aggregate.Crashes += r.rl.Crashes

		for k, v := range r.rl.Minimal {
			aggregate.Minimal[k] = v
		}
	}

	return &aggregate, nil
}

// Close will close all connections in Map.
func (s *HorizontalSearcher) Close() {
	s.mu.Lock()
	clients := s.clients
	s.clients = nil
	s.mu.Unlock()
	for _, c := range clients {
		c.Close()
	}
}

func (s *HorizontalSearcher) String() string {
	s.mu.RLock()
	clients := s.clients
	s.mu.RUnlock()
	addrs := make([]string, 0, len(clients))
	for addr := range clients {
		addrs = append(addrs, addr)
	}
	sort.Strings(addrs)
	return fmt.Sprintf("HorizontalSearcher{%v}", addrs)
}

// searchers returns the list of clients to aggregate over.
func (s *HorizontalSearcher) searchers() (map[string]zoekt.Streamer, error) {
	eps, err := s.Map.Endpoints()
	if err != nil {
		return nil, err
	}

	// Fast-path, check if Endpoints matches addrs. If it does we can use
	// s.clients.
	//
	// We structure our state to optimize for the fast-path.
	s.mu.RLock()
	clients := s.clients
	s.mu.RUnlock()
	if equalKeys(clients, eps) {
		return clients, nil
	}

	// Slow-path, need to remove/connect.
	return s.syncSearchers()
}

// syncSearchers syncs the set of clients with the set of endpoints. It is the
// slow-path of "searchers" since it obtains an write lock on the state before
// proceeding.
func (s *HorizontalSearcher) syncSearchers() (map[string]zoekt.Streamer, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	// Double check someone didn't beat us to the update
	eps, err := s.Map.Endpoints()
	if err != nil {
		return nil, err
	}
	if equalKeys(s.clients, eps) {
		return s.clients, nil
	}

	// Disconnect first
	for addr, client := range s.clients {
		if _, ok := eps[addr]; !ok {
			client.Close()
		}
	}

	// Use new map to avoid read conflicts
	clients := make(map[string]zoekt.Streamer, len(eps))
	for addr := range eps {
		// Try re-use
		client, ok := s.clients[addr]
		if !ok {
			client = s.Dial(addr)
		}
		clients[addr] = client
	}
	s.clients = clients

	return s.clients, nil
}

func equalKeys(a map[string]zoekt.Streamer, b map[string]struct{}) bool {
	if len(a) != len(b) {
		return false
	}
	for k := range a {
		if _, ok := b[k]; !ok {
			return false
		}
	}
	return true
}

type dedupper map[string]string // repoName -> endpoint

// Dedup will in-place filter out matches on Repositories we have already
// seen. A Repository has been seen if a previous call to Dedup had a match in
// it with a different endpoint.
func (repoEndpoint dedupper) Dedup(endpoint string, fms []zoekt.FileMatch) []zoekt.FileMatch {
	if len(fms) == 0 { // handles fms being nil
		return fms
	}

	// PERF: Normally fms is sorted by Repository. So we can avoid the map
	// lookup if we just did it for the previous entry.
	lastRepo := ""
	lastSeen := false

	// Remove entries for repos we have already seen.
	dedup := fms[:0]
	for _, fm := range fms {
		if lastRepo == fm.Repository {
			if lastSeen {
				continue
			}
		} else if ep, ok := repoEndpoint[fm.Repository]; ok && ep != endpoint {
			lastRepo = fm.Repository
			lastSeen = true
			continue
		}

		lastRepo = fm.Repository
		lastSeen = false
		dedup = append(dedup, fm)
	}

	// Update seenRepo now, so the next call of dedup will contain the
	// repos.
	lastRepo = ""
	for _, fm := range dedup {
		if lastRepo != fm.Repository {
			lastRepo = fm.Repository
			repoEndpoint[fm.Repository] = endpoint
		}
	}

	return dedup
}
