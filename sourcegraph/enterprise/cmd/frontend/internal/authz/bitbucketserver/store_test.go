package bitbucketserver

import (
	"context"
	"database/sql"
	"fmt"
	"reflect"
	"sync/atomic"
	"testing"
	"time"

	"github.com/RoaringBitmap/roaring"
	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/authz"
	"github.com/sourcegraph/sourcegraph/pkg/db/dbtest"
)

func BenchmarkStore(b *testing.B) {
	b.StopTimer()
	b.ResetTimer()

	db, cleanup := dbtest.NewDB(b, *dsn)
	defer cleanup()

	ids := make([]uint32, 30000)
	for i := range ids {
		ids[i] = uint32(i)
	}

	update := func(context.Context) ([]uint32, error) {
		time.Sleep(2 * time.Second) // Emulate slow code host
		return ids, nil
	}

	ctx := context.Background()

	b.Run("ttl=0", func(b *testing.B) {
		s := newStore(db, 0, DefaultHardTTL, clock, newCache())
		s.block = true

		ps := &Permissions{
			UserID: 99,
			Perm:   authz.Read,
			Type:   "repos",
		}

		for i := 0; i < b.N; i++ {
			err := s.LoadPermissions(ctx, &ps, update)
			if err != nil {
				b.Fatal(err)
			}
		}
	})

	b.Run("ttl=60s/no-in-memory-cache", func(b *testing.B) {
		s := newStore(db, 60*time.Second, DefaultHardTTL, clock, nil)
		s.block = true

		ps := &Permissions{
			UserID: 99,
			Perm:   authz.Read,
			Type:   "repos",
		}

		for i := 0; i < b.N; i++ {
			err := s.LoadPermissions(ctx, &ps, update)
			if err != nil {
				b.Fatal(err)
			}
		}
	})

	b.Run("ttl=60s/in-memory-cache", func(b *testing.B) {
		s := newStore(db, 60*time.Second, DefaultHardTTL, clock, newCache())
		s.block = true

		ps := &Permissions{
			UserID: 99,
			Perm:   authz.Read,
			Type:   "repos",
		}

		for i := 0; i < b.N; i++ {
			err := s.LoadPermissions(ctx, &ps, update)
			if err != nil {
				b.Fatal(err)
			}
		}
	})
}

func testStore(db *sql.DB) func(*testing.T) {
	equal := func(t testing.TB, name string, have, want interface{}) {
		t.Helper()
		if !reflect.DeepEqual(have, want) {
			t.Fatalf("%q: %s", name, cmp.Diff(have, want))
		}
	}

	return func(t *testing.T) {
		now := time.Now().UTC().UnixNano()
		ttl := time.Second
		hardTTL := 10 * time.Second

		clock := func() time.Time {
			return time.Unix(0, atomic.LoadInt64(&now)).Truncate(time.Microsecond)
		}

		s := newStore(db, ttl, hardTTL, clock, newCache())
		s.updates = make(chan *Permissions)

		ids := []uint32{1, 2, 3}
		e := error(nil)
		update := func(context.Context) ([]uint32, error) {
			return ids, e
		}

		ctx := context.Background()

		ps := &Permissions{UserID: 42, Perm: authz.Read, Type: "repos"}
		load := func(s *store) (*Permissions, error) {
			ps := *ps
			p := &ps
			return p, s.LoadPermissions(ctx, &p, update)
		}

		array := func(ids *roaring.Bitmap) []uint32 {
			if ids == nil {
				return nil
			}
			return ids.ToArray()
		}

		{
			// Not cached, nor stored.
			ps, err := load(s)
			equal(t, "err", err, &StalePermissionsError{Permissions: ps})
			equal(t, "ids", array(ps.IDs), []uint32(nil))
		}

		<-s.updates

		{
			// Hard TTL elapsed
			atomic.AddInt64(&now, int64(hardTTL))

			ps, err := load(s)
			equal(t, "err", err, &StalePermissionsError{Permissions: ps})
			equal(t, "ids", array(ps.IDs), ids)
		}

		<-s.updates

		{
			// Not cached, but stored by the background update.
			// After loading, stored permissions are cached in memory.
			equal(t, "cache", s.cache.cache[newCacheKey(ps)], (*Permissions)(nil))

			ps, err := load(s)

			equal(t, "err", err, nil)
			equal(t, "ids", array(ps.IDs), ids)
			equal(t, "cache", s.cache.cache[newCacheKey(ps)], ps)
		}

		ids = append(ids, 4, 5, 6)

		{
			// Source of truth changed (i.e. ids variable), but
			// cached permissions are not expired, so previous permissions
			// version is returned and no background update is started.
			ps, err := load(s)
			equal(t, "err", err, nil)
			equal(t, "ids", array(ps.IDs), ids[:3])
		}

		{
			// Cache expired, update called in the background, but stale
			// permissions are returned immediatelly.
			atomic.AddInt64(&now, int64(ttl))
			ps, err := load(s)
			equal(t, "err", err, nil)
			equal(t, "ids", array(ps.IDs), ids[:3])
		}

		// Wait for background update.
		<-s.updates

		{
			// Update is done, so we now have fresh permissions returned.
			ps, err := load(s)
			equal(t, "err", err, nil)
			equal(t, "ids", array(ps.IDs), ids)
		}

		ids = append(ids, 7)

		{
			// Cache expired, and source of truth changed. Here we test
			// that no concurrent updates are performed.
			atomic.AddInt64(&now, int64(2*ttl))

			delay := make(chan struct{})
			update = func(context.Context) ([]uint32, error) {
				<-delay
				return ids, e
			}

			type op struct {
				id  int
				ps  *Permissions
				err error
			}

			ch := make(chan op, 30)
			updates := make(chan *Permissions)

			for i := 0; i < cap(ch); i++ {
				go func(i int) {
					s := newStore(db, ttl, hardTTL, clock, newCache())
					s.updates = updates
					ps, err := load(s)
					ch <- op{i, ps, err}
				}(i)
			}

			results := make([]op, 0, cap(ch))
			for i := 0; i < cap(ch); i++ {
				results = append(results, <-ch)
			}

			for _, r := range results {
				equal(t, fmt.Sprintf("%d.err", r.id), r.err, nil)
				equal(t, fmt.Sprintf("%d.ids", r.id), array(r.ps.IDs), ids[:6])
			}

			close(delay)
			calls := 0
			timeout := time.After(500 * time.Millisecond)

		wait:
			for {
				select {
				case p := <-updates:
					calls++
					equal(t, "updated.ids", array(p.IDs), ids)
				case <-timeout:
					break wait
				}
			}

			equal(t, "updates", calls, 1)
		}
	}
}
