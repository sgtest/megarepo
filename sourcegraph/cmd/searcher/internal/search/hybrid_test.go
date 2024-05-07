package search_test

import (
	"bytes"
	"context"
	"io"
	"net/http"
	"net/http/httptest"
	"sort"
	"strings"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/log/logtest"
	"github.com/sourcegraph/zoekt"
	zoektgrpc "github.com/sourcegraph/zoekt/cmd/zoekt-webserver/grpc/server"
	"google.golang.org/grpc"

	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/gitdomain"

	internalgrpc "github.com/sourcegraph/sourcegraph/internal/grpc"
	"github.com/sourcegraph/sourcegraph/internal/grpc/defaults"
	proto "github.com/sourcegraph/sourcegraph/internal/searcher/v1"

	webproto "github.com/sourcegraph/zoekt/grpc/protos/zoekt/webserver/v1"
	"github.com/sourcegraph/zoekt/query"
	"github.com/sourcegraph/zoekt/web"

	"github.com/sourcegraph/sourcegraph/cmd/searcher/internal/search"
	"github.com/sourcegraph/sourcegraph/cmd/searcher/protocol"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/search/backend"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func TestHybridSearch(t *testing.T) {
	// TODO maybe we should create a real git repo and then have FetchTar/etc
	// all work against it. That would make me feel more confident in
	// implementation.

	files := map[string]struct {
		body string
		typ  fileType
	}{
		"added.md": {`hello world I am added`, typeFile},

		"changed.go": {`package main

import "fmt"

func main() {
	fmt.Println("Hello world")
}
`, typeFile},

		"unchanged.md": {`# Hello World

Hello world example in go`, typeFile},
	}

	filesIndexed := map[string]struct {
		body string
		typ  fileType
	}{
		"changed.go": {`
This result should not appear even though it contains "world" since the file has changed.
`, typeFile},

		"removed.md": {`
This result should not appear even though it contains "world" since the file has been removed.
`, typeFile},

		"unchanged.md": {`# Hello World

Hello world example in go`, typeFile},
	}

	// We explicitly remove "unchanged.md" from files so the test has to rely
	// on the results from Zoekt.
	if unchanged := "unchanged.md"; files[unchanged] != filesIndexed[unchanged] {
		t.Fatal()
	} else {
		delete(files, unchanged)
	}

	s := newStore(t, files)

	// explictly remove FetchTar since we should only be using FetchTarByPath
	s.FetchTar = nil

	// Ensure we don't ask for unchanged
	fetchTarPaths := s.FetchTarPaths
	s.FetchTarPaths = func(ctx context.Context, repo api.RepoName, commit api.CommitID, paths []string) (io.ReadCloser, error) {
		for _, p := range paths {
			if strings.Contains(p, "unchanged") {
				return nil, errors.Errorf("should not ask for unchanged path: %s", p)
			}
		}
		return fetchTarPaths(ctx, repo, commit, paths)
	}

	zoektURL := newZoekt(t, &zoekt.Repository{
		Name: "foo",
		ID:   123,
		Branches: []zoekt.RepositoryBranch{{
			Name:    "HEAD",
			Version: "indexedfdeadbeefdeadbeefdeadbeefdeadbeef",
		}},
	}, filesIndexed)

	// we expect one command against git, lets just fake it.
	service := &search.Service{
		GitChangedFiles: func(ctx context.Context, repo api.RepoName, commitA, commitB api.CommitID) (gitserver.ChangedFilesIterator, error) {
			if commitA != "indexedfdeadbeefdeadbeefdeadbeefdeadbeef" {
				return nil, errors.Errorf("expected first commit to be indexed, got: %s", commitA)
			}
			if commitB != "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef" {
				return nil, errors.Errorf("expected first commit to be unindexed, got: %s", commitB)
			}

			return gitserver.NewChangedFilesIteratorFromSlice([]gitdomain.PathStatus{
				{Status: gitdomain.ModifiedAMD, Path: "changed.go"},
				{Status: gitdomain.AddedAMD, Path: "added.md"},
				{Status: gitdomain.DeletedAMD, Path: "removed.md"},
			}), nil
		},
		MaxTotalPathsLength: 100_000,

		Store:   s,
		Indexed: backend.ZoektDial(zoektURL),
		Log:     logtest.Scoped(t),
	}

	grpcServer := defaults.NewServer(logtest.Scoped(t))
	proto.RegisterSearcherServiceServer(grpcServer, &search.Server{
		Service: service,
	})

	handler := internalgrpc.MultiplexHandlers(grpcServer, http.HandlerFunc(http.NotFound))

	ts := httptest.NewServer(handler)

	t.Cleanup(func() {
		ts.Close()
		grpcServer.Stop()
	})

	cases := []struct {
		Name    string
		Pattern protocol.PatternInfo
		Want    string
	}{{
		Name:    "all",
		Pattern: protocol.PatternInfo{Query: &protocol.PatternNode{Value: "world"}},
		Want: `
added.md:1:1:
hello world I am added
// No newline at end of chunk
changed.go:6:6:
	fmt.Println("Hello world")
unchanged.md:1:1:
# Hello World
unchanged.md:3:3:
Hello world example in go
// No newline at end of chunk
`,
	}, {
		Name: "added",
		Pattern: protocol.PatternInfo{
			Query:        &protocol.PatternNode{Value: "world"},
			IncludePaths: []string{"added"},
		},
		Want: `
added.md:1:1:
hello world I am added
// No newline at end of chunk
`,
	}, {
		Name: "example",
		Pattern: protocol.PatternInfo{
			Query: &protocol.PatternNode{Value: "example"},
		},
		Want: `
unchanged.md:3:3:
Hello world example in go
// No newline at end of chunk
`,
	}, {
		Name: "boolean query",
		Pattern: protocol.PatternInfo{
			Query: &protocol.AndNode{
				Children: []protocol.QueryNode{
					&protocol.OrNode{
						Children: []protocol.QueryNode{
							&protocol.PatternNode{Value: "w.rld", IsRegExp: true},
							&protocol.PatternNode{Value: "package"},
						},
					},
					&protocol.PatternNode{Value: "nonexistent", IsNegated: true},
				},
			},
		},
		Want: `
added.md:1:1:
hello world I am added
// No newline at end of chunk
changed.go:1:1:
package main
changed.go:6:6:
	fmt.Println("Hello world")
unchanged.md:1:1:
# Hello World
unchanged.md:3:3:
Hello world example in go
// No newline at end of chunk
`,
	}, {
		Name: "negated-pattern-example",
		Pattern: protocol.PatternInfo{
			Query: &protocol.PatternNode{Value: "example", IsNegated: true},
		},
		Want: `
added.md
changed.go
`,
	}, {
		Name: "path-include",
		Pattern: protocol.PatternInfo{
			Query:        &protocol.PatternNode{Value: ""},
			IncludePaths: []string{"^added"},
		},
		Want: `
added.md
`,
	}, {
		Name: "path-exclude-added",
		Pattern: protocol.PatternInfo{
			Query:        &protocol.PatternNode{Value: ""},
			ExcludePaths: "added",
		},
		Want: `
changed.go
unchanged.md
`,
	}, {
		Name: "path-exclude-unchanged",
		Pattern: protocol.PatternInfo{
			Query:        &protocol.PatternNode{Value: ""},
			ExcludePaths: "unchanged",
		},
		Want: `
added.md
changed.go
`,
	}, {
		Name: "path-all",
		Pattern: protocol.PatternInfo{
			Query:        &protocol.PatternNode{Value: ""},
			IncludePaths: []string{"."},
		},
		Want: `
added.md
changed.go
unchanged.md
`,
	}, {
		Name: "pattern-path",
		Pattern: protocol.PatternInfo{
			Query:                 &protocol.PatternNode{Value: "go"},
			PatternMatchesContent: true,
			PatternMatchesPath:    true,
		},
		Want: `
changed.go
unchanged.md:3:3:
Hello world example in go
// No newline at end of chunk
`,
	}, {
		Name: "negated-pattern-path",
		Pattern: protocol.PatternInfo{
			Query:              &protocol.PatternNode{Value: "go", IsNegated: true},
			PatternMatchesPath: true,
		},
		Want: `
added.md
unchanged.md
`,
	}, {
		Name: "lang-filters-include",
		Pattern: protocol.PatternInfo{
			Query:        &protocol.PatternNode{Value: ""},
			IncludeLangs: []string{"Markdown"},
		},
		Want: `
added.md
unchanged.md
`,
	}, {
		Name: "lang-filters-exclude",
		Pattern: protocol.PatternInfo{
			Query:        &protocol.PatternNode{Value: ""},
			ExcludeLangs: []string{"Markdown"},
		},
		Want: `
changed.go
`,
	}, {
		Name: "lang-filters-with-paths",
		Pattern: protocol.PatternInfo{
			Query:        &protocol.PatternNode{Value: ""},
			IncludePaths: []string{"change"},
			ExcludeLangs: []string{"Markdown"},
		},
		Want: `
changed.go
`,
	},
	}

	for _, tc := range cases {
		t.Run(tc.Name, func(t *testing.T) {
			req := protocol.Request{
				Repo:         "foo",
				RepoID:       123,
				URL:          "u",
				Commit:       "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
				PatternInfo:  tc.Pattern,
				FetchTimeout: fetchTimeoutForCI(t),
			}

			m, err := doSearch(t, ts.URL, &req)
			if err != nil {
				t.Fatal(err)
			}

			sort.Sort(sortByPath(m))
			got := strings.TrimSpace(toString(m))
			want := strings.TrimSpace(tc.Want)
			if d := cmp.Diff(want, got); d != "" {
				t.Fatalf("mismatch (-want, +got):\n%s", d)
			}
		})
	}
}

func newZoekt(t *testing.T, repo *zoekt.Repository, files map[string]struct {
	body string
	typ  fileType
}) string {
	var docs []zoekt.Document
	for name, file := range files {
		docs = append(docs, zoekt.Document{
			Name:     name,
			Content:  []byte(file.body),
			Branches: []string{"HEAD"},
		})
	}
	sort.Slice(docs, func(i, j int) bool {
		return docs[i].Name < docs[j].Name
	})

	b, err := zoekt.NewIndexBuilder(repo)
	if err != nil {
		t.Fatal(err)
	}
	for _, d := range docs {
		if err := b.Add(d); err != nil {
			t.Fatal(err)
		}
	}

	var buf bytes.Buffer
	if err := b.Write(&buf); err != nil {
		t.Fatal(err)
	}
	f := &memSeeker{data: buf.Bytes()}

	searcher, err := zoekt.NewSearcher(f)
	if err != nil {
		t.Fatal(err)
	}

	streamer := &streamer{Searcher: searcher}

	h, err := web.NewMux(&web.Server{
		Searcher: streamer,
		RPC:      true,
		Top:      web.Top,
	})
	if err != nil {
		t.Fatal(err)
	}

	s := grpc.NewServer()
	grpcServer := zoektgrpc.NewServer(streamer)
	webproto.RegisterWebserverServiceServer(s, grpcServer)

	handler := internalgrpc.MultiplexHandlers(s, h)

	ts := httptest.NewServer(handler)
	t.Cleanup(ts.Close)

	return ts.Listener.Addr().String()
}

type streamer struct {
	zoekt.Searcher
}

func (s *streamer) StreamSearch(ctx context.Context, q query.Q, opts *zoekt.SearchOptions, sender zoekt.Sender) (err error) {
	res, err := s.Searcher.Search(ctx, q, opts)
	if err != nil {
		return err
	}
	sender.Send(res)
	return nil
}

type memSeeker struct {
	data []byte
}

func (s *memSeeker) Name() string {
	return "memseeker"
}

func (s *memSeeker) Close() {}
func (s *memSeeker) Read(off, sz uint32) ([]byte, error) {
	return s.data[off : off+sz], nil
}

func (s *memSeeker) Size() (uint32, error) {
	return uint32(len(s.data)), nil
}
