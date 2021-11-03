package httpapi

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"

	"github.com/cockroachdb/errors"
	"github.com/google/go-cmp/cmp"
	"github.com/google/zoekt"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestReposIndex(t *testing.T) {
	allRepos := []types.MinimalRepo{
		{ID: 1, Name: "github.com/popular/foo"},
		{ID: 2, Name: "github.com/popular/bar"},
		{ID: 3, Name: "github.com/alice/foo"},
		{ID: 4, Name: "github.com/alice/bar"},
	}

	indexableRepos := allRepos[:2]

	cases := []struct {
		name      string
		indexable []types.MinimalRepo
		body      string
		want      []string
	}{{
		name:      "indexers",
		indexable: allRepos,
		body:      `{"Hostname": "foo"}`,
		want:      []string{"github.com/popular/foo", "github.com/alice/foo"},
	}, {
		name:      "indexed",
		indexable: allRepos,
		body:      `{"Hostname": "foo", "Indexed": ["github.com/alice/bar"]}`,
		want:      []string{"github.com/popular/foo", "github.com/alice/foo", "github.com/alice/bar"},
	}, {
		name:      "indexedids",
		indexable: allRepos,
		body:      `{"Hostname": "foo", "IndexedIDs": [4]}`,
		want:      []string{"github.com/popular/foo", "github.com/alice/foo", "github.com/alice/bar"},
	}, {
		name:      "dot-com indexers",
		indexable: indexableRepos,
		body:      `{"Hostname": "foo"}`,
		want:      []string{"github.com/popular/foo"},
	}, {
		name:      "dot-com indexed",
		indexable: indexableRepos,
		body:      `{"Hostname": "foo", "Indexed": ["github.com/popular/bar"]}`,
		want:      []string{"github.com/popular/foo", "github.com/popular/bar"},
	}, {
		name:      "dot-com indexedids",
		indexable: indexableRepos,
		body:      `{"Hostname": "foo", "IndexedIDs": [2]}`,
		want:      []string{"github.com/popular/foo", "github.com/popular/bar"},
	}, {
		name:      "none",
		indexable: allRepos,
		body:      `{"Hostname": "baz"}`,
		want:      []string{},
	}}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			srv := &reposListServer{
				ListIndexable:      fakeListIndexable(tc.indexable),
				StreamMinimalRepos: fakeStreamMinimalRepos(allRepos),
				Indexers:           suffixIndexers(true),
			}

			req := httptest.NewRequest("POST", "/", bytes.NewReader([]byte(tc.body)))
			w := httptest.NewRecorder()
			if err := srv.serveIndex(w, req); err != nil {
				t.Fatal(err)
			}

			resp := w.Result()
			body, _ := io.ReadAll(resp.Body)

			if resp.StatusCode != http.StatusOK {
				t.Errorf("got status %v", resp.StatusCode)
			}

			var data struct {
				RepoNames []string
				RepoIDs   []api.RepoID
			}
			if err := json.Unmarshal(body, &data); err != nil {
				t.Fatal(err)
			}
			got := data.RepoNames

			if !cmp.Equal(tc.want, got) {
				t.Fatalf("names mismatch (-want +got):\n%s", cmp.Diff(tc.want, got))
			}

			wantIDs := make([]api.RepoID, len(tc.want))
			for i, name := range tc.want {
				for _, repo := range allRepos {
					if string(repo.Name) == name {
						wantIDs[i] = repo.ID
					}
				}
			}
			if d := cmp.Diff(wantIDs, data.RepoIDs); d != "" {
				t.Fatalf("ids mismatch (-want +got):\n%s", d)
			}
		})
	}
}

func fakeListIndexable(indexable []types.MinimalRepo) func(context.Context) ([]types.MinimalRepo, error) {
	return func(context.Context) ([]types.MinimalRepo, error) {
		return indexable, nil
	}
}

func fakeStreamMinimalRepos(repos []types.MinimalRepo) func(context.Context, database.ReposListOptions, func(*types.MinimalRepo)) error {
	return func(ctx context.Context, opt database.ReposListOptions, cb func(*types.MinimalRepo)) error {
		names := make(map[string]bool, len(opt.Names))
		for _, name := range opt.Names {
			names[name] = true
		}

		ids := make(map[api.RepoID]bool, len(opt.IDs))
		for _, id := range opt.IDs {
			ids[id] = true
		}

		for i := range repos {
			r := &repos[i]
			if names[string(r.Name)] || ids[r.ID] {
				cb(&repos[i])
			}
		}

		return nil
	}
}

// suffixIndexers mocks Indexers. ReposSubset will return all repoNames with
// the suffix of hostname.
type suffixIndexers bool

func (b suffixIndexers) ReposSubset(ctx context.Context, hostname string, indexed map[uint32]*zoekt.MinimalRepoListEntry, indexable []types.MinimalRepo) ([]types.MinimalRepo, error) {
	if !b.Enabled() {
		return nil, errors.New("indexers disabled")
	}
	if hostname == "" {
		return nil, errors.New("empty hostname")
	}

	var filter []types.MinimalRepo
	for _, r := range indexable {
		if strings.HasSuffix(string(r.Name), hostname) {
			filter = append(filter, r)
		} else if _, ok := indexed[uint32(r.ID)]; ok {
			filter = append(filter, r)
		}
	}
	return filter, nil
}

func (b suffixIndexers) Enabled() bool {
	return bool(b)
}

func TestRepoRankFromConfig(t *testing.T) {
	cases := []struct {
		name       string
		rankScores map[string]float64
		want       float64
	}{
		{"gh.test/sg/sg", nil, 0},
		{"gh.test/sg/sg", map[string]float64{"gh.test": 100}, 100},
		{"gh.test/sg/sg", map[string]float64{"gh.test": 100, "gh.test/sg": 50}, 150},
		{"gh.test/sg/sg", map[string]float64{"gh.test": 100, "gh.test/sg": 50, "gh.test/sg/sg": -20}, 130},
		{"gh.test/sg/ex", map[string]float64{"gh.test": 100, "gh.test/sg": 50, "gh.test/sg/sg": -20}, 150},
	}
	for _, tc := range cases {
		config := schema.SiteConfiguration{ExperimentalFeatures: &schema.ExperimentalFeatures{
			Ranking: &schema.Ranking{
				RepoScores: tc.rankScores,
			},
		}}
		got := repoRankFromConfig(config, tc.name)
		if got != tc.want {
			t.Errorf("got score %v, want %v, repo %q config %v", got, tc.want, tc.name, tc.rankScores)
		}
	}
}
