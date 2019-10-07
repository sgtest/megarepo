package db

import (
	"context"
	"fmt"
	"math/rand"
	"reflect"
	"testing"

	"github.com/sourcegraph/sourcegraph/internal/db/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/db/dbtesting"
)

func TestRecentSearches_Log(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	ctx := context.Background()
	q := fmt.Sprintf("fake query with random number %d", rand.Int())
	rs := &RecentSearches{}
	if err := rs.Log(ctx, q); err != nil {
		t.Fatal(err)
	}
	ss, err := rs.List(ctx)
	if err != nil {
		t.Fatal(err)
	}
	if len(ss) != 1 {
		t.Fatalf("%d searches returned, want exactly 1", len(ss))
	}
	if ss[0] != q {
		t.Errorf("query is '%s', want '%s'", ss[0], q)
	}
}

func TestRecentSearches_Cleanup(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	rs := &RecentSearches{}
	t.Run("empty case", func(t *testing.T) {
		dbtesting.SetupGlobalTestDB(t)
		ctx := context.Background()
		if err := rs.Cleanup(ctx, 1); err != nil {
			t.Error(err)
		}
	})
	t.Run("single case", func(t *testing.T) {
		dbtesting.SetupGlobalTestDB(t)
		ctx := context.Background()
		q := "fake query"
		if err := rs.Log(ctx, q); err != nil {
			t.Fatal(err)
		}
		if err := rs.Cleanup(ctx, 2); err != nil {
			t.Error(err)
		}
		ss, err := rs.List(ctx)
		if err != nil {
			t.Fatal(err)
		}
		if len(ss) != 1 {
			t.Errorf("recent_searches table has %d rows, want %d", len(ss), 1)
		}
	})
	t.Run("simple case", func(t *testing.T) {
		dbtesting.SetupGlobalTestDB(t)
		ctx := context.Background()
		limit := 10
		for i := 1; i <= limit+1; i++ {
			q := fmt.Sprintf("fake query for i = %d", i)
			if err := rs.Log(ctx, q); err != nil {
				t.Fatal(err)
			}
		}
		if err := rs.Cleanup(ctx, limit); err != nil {
			t.Fatal(err)
		}
		ss, err := rs.List(ctx)
		if err != nil {
			t.Fatal(err)
		}
		if len(ss) != limit {
			t.Errorf("recent_searches table has %d rows, want %d", len(ss), limit)
		}
	})
	t.Run("id gap", func(t *testing.T) {
		dbtesting.SetupGlobalTestDB(t)
		ctx := context.Background()
		addQueryWithRandomId := func(q string) {
			insert := `INSERT INTO recent_searches (id, query) VALUES ((1e6*RANDOM())::int, $1)`
			if _, err := dbconn.Global.ExecContext(ctx, insert, q); err != nil {
				t.Fatalf("inserting '%s' into recent_searches table: %v", q, err)
			}
		}
		limit := 10
		for i := 1; i <= limit+1; i++ {
			q := fmt.Sprintf("fake query for i = %d", i)
			addQueryWithRandomId(q)
		}
		if err := rs.Cleanup(ctx, limit); err != nil {
			t.Fatal(err)
		}
		ss, err := rs.List(ctx)
		if err != nil {
			t.Fatal(err)
		}
		if len(ss) != limit {
			t.Errorf("recent_searches table has %d rows, want %d", len(ss), limit)
		}
	})
}

func BenchmarkRecentSearches_LogAndCleanup(b *testing.B) {
	rs := &RecentSearches{}
	dbtesting.SetupGlobalTestDB(b)
	ctx := context.Background()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		q := fmt.Sprintf("fake query for i = %d", i)
		if err := rs.Log(ctx, q); err != nil {
			b.Fatal(err)
		}
		if err := rs.Cleanup(ctx, b.N); err != nil {
			b.Fatal(err)
		}
	}
}

func TestRecentSearches_Top(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	tests := []struct {
		name              string
		queries           []string
		n                 int32
		wantUniqueQueries []string
		wantCounts        []int32
		wantErr           bool
	}{
		{
			name:              "empty case",
			queries:           nil,
			n:                 10,
			wantUniqueQueries: nil,
			wantCounts:        nil,
			wantErr:           false,
		},
		{
			name:              "a",
			queries:           []string{"a"},
			n:                 10,
			wantUniqueQueries: []string{"a"},
			wantCounts:        []int32{1},
			wantErr:           false,
		},
		{
			name:              "a a",
			queries:           []string{"a", "a"},
			n:                 10,
			wantUniqueQueries: []string{"a"},
			wantCounts:        []int32{2},
			wantErr:           false,
		},
		{
			name:              "a b",
			queries:           []string{"a", "b"},
			n:                 10,
			wantUniqueQueries: []string{"a", "b"},
			wantCounts:        []int32{1, 1},
			wantErr:           false,
		},
		{
			name:              "c c b a a",
			queries:           []string{"c", "c", "b", "a", "a"},
			n:                 10,
			wantUniqueQueries: []string{"a", "c", "b"},
			wantCounts:        []int32{2, 2, 1},
			wantErr:           false,
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			rs := &RecentSearches{}
			dbtesting.SetupGlobalTestDB(t)
			ctx := context.Background()
			for _, q := range tt.queries {
				if err := rs.Log(ctx, q); err != nil {
					t.Fatal(err)
				}
			}
			gotUniqueQueries, gotCounts, err := rs.Top(ctx, tt.n)
			if (err != nil) != tt.wantErr {
				t.Errorf("RecentSearches.Top() error = %v, wantErr %v", err, tt.wantErr)
				return
			}
			if !reflect.DeepEqual(gotUniqueQueries, tt.wantUniqueQueries) {
				t.Errorf("RecentSearches.Top() queries = %v, want %v", gotUniqueQueries, tt.wantUniqueQueries)
			}
			if !reflect.DeepEqual(gotCounts, tt.wantCounts) {
				t.Errorf("RecentSearches.Top() counts = %v, want %v", gotCounts, tt.wantCounts)
			}
		})
	}
}
