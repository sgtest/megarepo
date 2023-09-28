package scheduler

import (
	"container/heap"
	"sync"

	"github.com/sourcegraph/sourcegraph/internal/api"
)

// updateQueue is a priority queue of repos to update.
// A repo can't have more than one location in the queue.
// Implements heap.Interface and sort.Interface.
type updateQueue struct {
	mu sync.Mutex

	heap  []*repoUpdate
	index map[api.RepoID]*repoUpdate

	seq uint64

	// The queue performs a non-blocking send on this channel
	// when a new value is enqueued so that the update loop
	// can wake up if it is idle.
	notifyEnqueue chan struct{}
}

func (q *updateQueue) reset() {
	q.mu.Lock()
	defer q.mu.Unlock()

	q.heap = q.heap[:0]
	q.index = map[api.RepoID]*repoUpdate{}
	q.seq = 0
	q.notifyEnqueue = make(chan struct{}, notifyChanBuffer)

	schedUpdateQueueLength.Set(0)
}

// enqueue adds the repo to the queue with the given priority.
//
// If the repo is already in the queue and it isn't yet updating,
// the repo is updated.
//
// If the given priority is higher than the one in the queue,
// the repo's position in the queue is updated accordingly.
func (q *updateQueue) enqueue(repo configuredRepo, p priority) (updated bool) {
	if repo.ID == 0 {
		panic("repo.id is zero")
	}

	q.mu.Lock()
	defer q.mu.Unlock()

	update := q.index[repo.ID]
	if update == nil {
		heap.Push(q, &repoUpdate{
			Repo:     repo,
			Priority: p,
		})
		notify(q.notifyEnqueue)
		return false
	}

	if update.Updating {
		return false
	}

	update.Repo = repo
	if p <= update.Priority {
		// Repo is already in the queue with at least as good priority.
		return true
	}

	// Repo is in the queue at a lower priority.
	update.Priority = p      // bump the priority
	update.Seq = q.nextSeq() // put it after all existing updates with this priority
	heap.Fix(q, update.Index)
	notify(q.notifyEnqueue)

	return true
}

// nextSeq increments and returns the next sequence number.
// The caller must hold the lock on q.mu.
func (q *updateQueue) nextSeq() uint64 {
	q.seq++
	return q.seq
}

// remove removes the repo from the queue if the repo.Updating matches the updating argument.
func (q *updateQueue) remove(repo configuredRepo, updating bool) (removed bool) {
	if repo.ID == 0 {
		panic("repo.id is zero")
	}

	q.mu.Lock()
	defer q.mu.Unlock()

	update := q.index[repo.ID]
	if update != nil && update.Updating == updating {
		heap.Remove(q, update.Index)
		return true
	}

	return false
}

// acquireNext acquires the next repo for update.
// The acquired repo must be removed from the queue
// when the update finishes (independent of success or failure).
func (q *updateQueue) acquireNext() (configuredRepo, bool) {
	q.mu.Lock()
	defer q.mu.Unlock()
	if len(q.heap) == 0 {
		return configuredRepo{}, false
	}
	update := q.heap[0]
	if update.Updating {
		// Everything in the queue is already updating.
		return configuredRepo{}, false
	}
	update.Updating = true
	heap.Fix(q, update.Index)
	return update.Repo, true
}

// The following methods implement heap.Interface based on the priority queue example:
// https://golang.org/pkg/container/heap/#example__priorityQueue
// These methods are not safe for concurrent use. Therefore, it is the caller's
// responsibility to ensure they're being guarded by a mutex during any heap operation,
// i.e. heap.Fix, heap.Remove, heap.Push, heap.Pop.

func (q *updateQueue) Len() int {
	n := len(q.heap)
	schedUpdateQueueLength.Set(float64(n))
	return n
}

func (q *updateQueue) Less(i, j int) bool {
	qi := q.heap[i]
	qj := q.heap[j]
	if qi.Updating != qj.Updating {
		// Repos that are already updating are sorted last.
		return qj.Updating
	}
	if qi.Priority != qj.Priority {
		// We want Pop to give us the highest, not lowest, priority so we use greater than here.
		return qi.Priority > qj.Priority
	}
	// Queue semantics for items with the same priority.
	return qi.Seq < qj.Seq
}

func (q *updateQueue) Swap(i, j int) {
	q.heap[i], q.heap[j] = q.heap[j], q.heap[i]
	q.heap[i].Index = i
	q.heap[j].Index = j
}

func (q *updateQueue) Push(x any) {
	n := len(q.heap)
	item := x.(*repoUpdate)
	item.Index = n
	item.Seq = q.nextSeq()
	q.heap = append(q.heap, item)
	q.index[item.Repo.ID] = item
}

func (q *updateQueue) Pop() any {
	n := len(q.heap)
	item := q.heap[n-1]
	item.Index = -1 // for safety
	q.heap = q.heap[0 : n-1]
	delete(q.index, item.Repo.ID)
	return item
}
