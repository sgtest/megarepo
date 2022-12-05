package diskcache

import (
	"bytes"
	"context"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func TestOpen(t *testing.T) {
	dir := t.TempDir()

	store := &store{
		dir:       dir,
		component: "test",
		observe:   newOperations(&observation.TestContext, "test"),
	}

	do := func() (*File, bool) {
		want := "foobar"
		calledFetcher := false
		f, err := store.Open(context.Background(), []string{"key"}, func(ctx context.Context) (io.ReadCloser, error) {
			calledFetcher = true
			return io.NopCloser(bytes.NewReader([]byte(want))), nil
		})
		if err != nil {
			t.Fatal(err)
		}
		got, err := io.ReadAll(f.File)
		if err != nil {
			t.Fatal(err)
		}
		f.Close()
		if string(got) != want {
			t.Fatalf("did not return fetcher output. got %q, want %q", string(got), want)
		}
		return f, !calledFetcher
	}

	// Cache should be empty
	_, usedCache := do()
	if usedCache {
		t.Fatal("Expected fetcher to be called on empty cache")
	}

	// Redo, now we should use the cache
	f, usedCache := do()
	if !usedCache {
		t.Fatal("Expected fetcher to not be called when cached")
	}

	// Evict, then we should not use the cache
	os.Remove(f.Path)
	_, usedCache = do()
	if usedCache {
		t.Fatal("Item was not properly evicted")
	}
}

func TestMultiKeyEviction(t *testing.T) {
	dir := t.TempDir()

	store := &store{
		dir:       dir,
		component: "test",
		observe:   newOperations(&observation.TestContext, "test"),
	}

	f, err := store.Open(context.Background(), []string{"key1", "key2"}, func(ctx context.Context) (io.ReadCloser, error) {
		return io.NopCloser(bytes.NewReader([]byte("blah"))), nil
	})
	if err != nil {
		t.Fatal(err)
	}
	f.Close()

	stats, err := store.Evict(0)
	if err != nil {
		t.Fatal(err)
	}
	if stats.Evicted != 1 {
		t.Fatal("Expected to evict 1 item, evicted", stats.Evicted)
	}
}

func TestEvict(t *testing.T) {
	dir := t.TempDir()

	store := &store{
		dir:       dir,
		component: "test",
		observe:   newOperations(&observation.TestContext, "test"),
	}

	for _, name := range []string{
		"key-first",
		"key-second",
		"not-managed.txt",
		"key-third",
		"key-fourth",
	} {
		if strings.HasPrefix(name, "key-") {
			f, err := store.Open(context.Background(), []string{name}, func(ctx context.Context) (io.ReadCloser, error) {
				return io.NopCloser(bytes.NewReader([]byte("x"))), nil
			})
			if err != nil {
				t.Fatal(err)
			}
			f.Close()
		} else {
			if err := os.WriteFile(filepath.Join(dir, name), []byte("x"), 0o600); err != nil {
				t.Fatal(err)
			}
		}
	}

	evict := func(maxCacheSizeBytes int64) EvictStats {
		t.Helper()
		stats, err := store.Evict(maxCacheSizeBytes)
		if err != nil {
			t.Fatal(err)
		}
		return stats
	}

	expect := func(maxCacheSizeBytes int64, cacheSize int64, evicted int) {
		t.Helper()
		before := evict(10000) // just get cache size before
		stats := evict(maxCacheSizeBytes)
		after := evict(10000)

		if before.CacheSize != stats.CacheSize {
			t.Fatalf("expected evict to return cache size before evictions: got=%d want=%d", stats.CacheSize, before.CacheSize)
		}
		if after.CacheSize != cacheSize {
			t.Fatalf("unexpected cache size: got=%d want=%d", stats.CacheSize, cacheSize)
		}
		if stats.Evicted != evicted {
			t.Fatalf("unexpected evicted: got=%d want=%d", stats.Evicted, evicted)
		}
	}

	// we have 5 files with size 1 each.
	expect(10000, 5, 0)

	// our cachesize is 5, so making it 4 will evict one.
	expect(4, 4, 1)

	// we have 4 files left, but 1 can't be evicted since it isn't managed by
	// disckcache.
	expect(0, 1, 3)
}
