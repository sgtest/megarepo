package rpc_test

import (
	"context"
	"fmt"
	"net/http/httptest"
	"net/url"
	"reflect"
	"testing"

	"github.com/sourcegraph/sourcegraph/pkg/search"
	"github.com/sourcegraph/sourcegraph/pkg/search/query"
	"github.com/sourcegraph/sourcegraph/pkg/search/rpc"
)

func TestClientServer(t *testing.T) {
	mock := &mockSearcher{
		wantSearch: query.NewAnd(mustParse("hello world|universe"), query.NewRepoSet("foo/bar", "baz/bam")),
		searchResult: &search.Result{
			Files: []search.FileMatch{
				{FileName: "bin.go"},
			},
		},
	}

	ts := httptest.NewServer(rpc.Server(mock))
	defer ts.Close()

	u, err := url.Parse(ts.URL)
	if err != nil {
		t.Fatal(err)
	}
	client := rpc.Client(u.Host)
	defer client.Close()

	r, err := client.Search(context.Background(), mock.wantSearch, &search.Options{})
	if err != nil {
		t.Fatal(err)
	}
	if !reflect.DeepEqual(r, mock.searchResult) {
		t.Fatalf("got %+v, want %+v", r, mock.searchResult)
	}
}

type mockSearcher struct {
	wantSearch   query.Q
	searchResult *search.Result
}

func (s *mockSearcher) Search(ctx context.Context, q query.Q, opts *search.Options) (*search.Result, error) {
	if q.String() != s.wantSearch.String() {
		return nil, fmt.Errorf("got query %s != %s", q.String(), s.wantSearch.String())
	}
	return s.searchResult, nil
}

func (*mockSearcher) Close() {}

func (*mockSearcher) String() string {
	return "mockSearcher"
}

func mustParse(s string) query.Q {
	q, err := query.Parse(s)
	if err != nil {
		panic(err)
	}
	return q
}
