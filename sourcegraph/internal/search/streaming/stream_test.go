package streaming

import (
	"sync"
	"testing"
	"time"

	"github.com/stretchr/testify/require"
	"go.uber.org/atomic"

	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/lib/group"
)

func BenchmarkBatchingStream(b *testing.B) {
	s := NewBatchingStream(10*time.Millisecond, StreamFunc(func(SearchEvent) {}))
	res := make(result.Matches, 1)
	for i := 0; i < b.N; i++ {
		s.Send(SearchEvent{
			Results: res,
		})
	}
	s.Done()
}

func TestBatchingStream(t *testing.T) {
	t.Run("basic walkthrough", func(t *testing.T) {
		var mu sync.Mutex
		var matches result.Matches
		s := NewBatchingStream(100*time.Millisecond, StreamFunc(func(event SearchEvent) {
			mu.Lock()
			matches = append(matches, event.Results...)
			mu.Unlock()
		}))

		for i := 0; i < 10; i++ {
			s.Send(SearchEvent{Results: make(result.Matches, 1)})
		}

		// The first event should be sent without delay, but the
		// remaining events should have been batched but unsent
		mu.Lock()
		require.Len(t, matches, 1)
		mu.Unlock()

		// After 150 milliseconds, the batch should have been flushed
		time.Sleep(150 * time.Millisecond)
		mu.Lock()
		require.Len(t, matches, 10)
		mu.Unlock()

		// Sending another event shouldn't go through immediately
		s.Send(SearchEvent{Results: make(result.Matches, 1)})
		mu.Lock()
		require.Len(t, matches, 10)
		mu.Unlock()

		// But if tell the stream we're done, it should
		s.Done()
		require.Len(t, matches, 11)
	})

	t.Run("send event before timer", func(t *testing.T) {
		var mu sync.Mutex
		var matches result.Matches
		s := NewBatchingStream(100*time.Millisecond, StreamFunc(func(event SearchEvent) {
			mu.Lock()
			matches = append(matches, event.Results...)
			mu.Unlock()
		}))

		for i := 0; i < 10; i++ {
			s.Send(SearchEvent{Results: make(result.Matches, 1)})
		}

		// The first event should be sent without delay, but the
		// remaining events should have been batched but unsent
		mu.Lock()
		require.Len(t, matches, 1)
		mu.Unlock()

		// After 150 milliseconds, all events should be sent
		time.Sleep(150 * time.Millisecond)
		mu.Lock()
		require.Len(t, matches, 10)
		mu.Unlock()

		// Sending an event should not make it through immediately
		s.Send(SearchEvent{Results: make(result.Matches, 1)})
		mu.Lock()
		require.Len(t, matches, 10)
		mu.Unlock()

		// Sending another event should be added to the batch, but still be sent
		// with the previous event because it triggered a new timer
		time.Sleep(50 * time.Millisecond)
		s.Send(SearchEvent{Results: make(result.Matches, 1)})
		mu.Lock()
		require.Len(t, matches, 10)
		mu.Unlock()

		// After 75 milliseconds, the timer from 2 events ago should have triggered
		time.Sleep(75 * time.Millisecond)
		mu.Lock()
		require.Len(t, matches, 12)
		mu.Unlock()

		s.Done()
		require.Len(t, matches, 12)
	})

	t.Run("super parallel", func(t *testing.T) {
		var count atomic.Int64
		s := NewBatchingStream(100*time.Millisecond, StreamFunc(func(event SearchEvent) {
			count.Add(int64(len(event.Results)))
		}))

		g := group.New()
		for i := 0; i < 10; i++ {
			g.Go(func() {
				s.Send(SearchEvent{Results: make(result.Matches, 1)})
			})
		}
		g.Wait()

		// One should be sent immediately
		require.Equal(t, count.Load(), int64(1))

		// The rest should be sent after flushing
		s.Done()
		require.Equal(t, count.Load(), int64(10))
	})
}

func TestDedupingStream(t *testing.T) {
	var sent []result.Match
	s := NewDedupingStream(StreamFunc(func(e SearchEvent) {
		sent = append(sent, e.Results...)
	}))

	for i := 0; i < 2; i++ {
		s.Send(SearchEvent{
			Results: []result.Match{&result.FileMatch{
				File: result.File{Path: "lombardy"},
			}},
		})
	}

	require.Equal(t, 1, len(sent))
}
