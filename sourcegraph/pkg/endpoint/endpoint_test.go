package endpoint

import (
	"fmt"
	"strings"
	"testing"
	"testing/quick"
)

func TestStatic(t *testing.T) {
	m := New("http://test")
	expectEndpoints(t, m, nil, "http://test")
}

func TestExclude(t *testing.T) {
	endpoints := []string{"http://test-1", "http://test-2", "http://test-3", "http://test-4"}
	m := New(strings.Join(endpoints, " "))

	exclude := map[string]bool{}
	for len(endpoints) > 0 {
		expectEndpoints(t, m, exclude, endpoints...)

		exclude[endpoints[len(endpoints)-1]] = true
		endpoints = endpoints[:len(endpoints)-1]
	}
}

func expectEndpoints(t *testing.T, m *Map, exclude map[string]bool, endpoints ...string) {
	t.Helper()

	// We ask for the URL of a large number of keys, we expect to see every
	// endpoint and only those endpoints.
	count := map[string]int{}
	for _, e := range endpoints {
		count[e] = 0
	}
	for i := 0; i < len(endpoints)*10; i++ {
		v, err := m.Get(fmt.Sprintf("test-%d", i), exclude)
		if err != nil {
			t.Fatalf("Get failed: %v", err)
		}
		if _, ok := count[v]; !ok {
			t.Fatalf("map returned unexpected endpoint %v. Valid: %v", v, endpoints)
		}
		count[v] = count[v] + 1
	}
	t.Logf("counts: %v", count)
	for e, c := range count {
		if c == 0 {
			t.Fatalf("map never returned %v", e)
		}
	}
}

func TestGetAll(t *testing.T) {
	m := New("http://test-1 http://test-2 http://test-3 http://test-4")
	f := func(keys []string) bool {
		values, err := m.GetAll(keys, nil)
		if err != nil {
			t.Fatal(err)
		}
		for i := range keys {
			if v, err := m.Get(keys[i], nil); err != nil {
				t.Fatal(err)
			} else if v != values[i] {
				return false
			}
		}
		return len(keys) == len(values)
	}
	if err := quick.Check(f, nil); err != nil {
		t.Error(err)
	}
}
