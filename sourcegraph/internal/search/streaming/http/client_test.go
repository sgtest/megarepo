package http

import (
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming/api"
)

func TestFrontendClient(t *testing.T) {
	type Event struct {
		Name  string
		Value any
	}

	want := []Event{{
		Name: "progress",
		Value: &api.Progress{
			MatchCount: 5,
		},
	}, {
		Name: "progress",
		Value: &api.Progress{
			MatchCount: 10,
		},
	}, {
		Name: "matches",
		Value: []EventMatch{
			&EventContentMatch{
				Type: ContentMatchType,
				Path: "test",
			},
			&EventPathMatch{
				Type: PathMatchType,
				Path: "test",
			},
			&EventRepoMatch{
				Type:       RepoMatchType,
				Repository: "test",
			},
			&EventSymbolMatch{
				Type: SymbolMatchType,
				Path: "test",
			},
			&EventCommitMatch{
				Type:   CommitMatchType,
				Detail: "test",
			},
		},
	}, {
		Name: "filters",
		Value: []*EventFilter{{
			Value: "filter-1",
		}, {
			Value: "filter-2",
		}},
	}, {
		Name: "alert",
		Value: &EventAlert{
			Title: "alert",
		},
	}, {
		Name: "error",
		Value: &EventError{
			Message: "error",
		},
	}}

	ts := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		ew, err := NewWriter(w)
		if err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		for _, e := range want {
			ew.Event(e.Name, e.Value)
		}
		ew.Event("done", struct{}{})
	}))
	defer ts.Close()

	req, err := NewRequest(ts.URL, "hello world")
	if err != nil {
		t.Fatal(err)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()

	var got []Event
	err = FrontendStreamDecoder{
		OnProgress: func(d *api.Progress) {
			got = append(got, Event{Name: "progress", Value: d})
		},
		OnMatches: func(d []EventMatch) {
			got = append(got, Event{Name: "matches", Value: d})
		},
		OnFilters: func(d []*EventFilter) {
			got = append(got, Event{Name: "filters", Value: d})
		},
		OnAlert: func(d *EventAlert) {
			got = append(got, Event{Name: "alert", Value: d})
		},
		OnError: func(d *EventError) {
			got = append(got, Event{Name: "error", Value: d})
		},
		OnUnknown: func(event, data []byte) {
			t.Fatalf("got unexpected event: %s %s", event, data)
		},
	}.ReadAll(resp.Body)
	if err != nil {
		t.Fatal(err)
	}

	if d := cmp.Diff(want, got); d != "" {
		t.Fatalf("mismatch (-want +got):\n%s", d)
	}
}
