package search

import (
	"archive/zip"
	"bytes"
	"context"
	"io"
	"os"
	"reflect"
	"sort"
	"strconv"
	"testing"
	"testing/iotest"

	"github.com/grafana/regexp"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/cmd/searcher/protocol"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func BenchmarkSearchRegex_large_fixed(b *testing.B) {
	benchSearchRegex(b, &protocol.Request{
		Repo:   "github.com/golang/go",
		Commit: "0ebaca6ba27534add5930a95acffa9acff182e2b",
		PatternInfo: protocol.PatternInfo{
			Pattern: "error handler",
		},
	})
}

func BenchmarkSearchRegex_rare_fixed(b *testing.B) {
	benchSearchRegex(b, &protocol.Request{
		Repo:   "github.com/golang/go",
		Commit: "0ebaca6ba27534add5930a95acffa9acff182e2b",
		PatternInfo: protocol.PatternInfo{
			Pattern: "REBOOT_CMD",
		},
	})
}

func BenchmarkSearchRegex_large_fixed_casesensitive(b *testing.B) {
	benchSearchRegex(b, &protocol.Request{
		Repo:   "github.com/golang/go",
		Commit: "0ebaca6ba27534add5930a95acffa9acff182e2b",
		PatternInfo: protocol.PatternInfo{
			Pattern:         "error handler",
			IsCaseSensitive: true,
		},
	})
}

func BenchmarkSearchRegex_large_re_dotstar(b *testing.B) {
	benchSearchRegex(b, &protocol.Request{
		Repo:   "github.com/golang/go",
		Commit: "0ebaca6ba27534add5930a95acffa9acff182e2b",
		PatternInfo: protocol.PatternInfo{
			Pattern:  ".*",
			IsRegExp: true,
		},
	})
}

func BenchmarkSearchRegex_large_re_common(b *testing.B) {
	benchSearchRegex(b, &protocol.Request{
		Repo:   "github.com/golang/go",
		Commit: "0ebaca6ba27534add5930a95acffa9acff182e2b",
		PatternInfo: protocol.PatternInfo{
			Pattern:         "func +[A-Z]",
			IsRegExp:        true,
			IsCaseSensitive: true,
		},
	})
}

func BenchmarkSearchRegex_large_re_anchor(b *testing.B) {
	// TODO(keegan) PERF regex engine performs poorly since LiteralPrefix
	// is empty when ^. We can improve this by:
	// * Transforming the regex we use to prune a file to be more
	// performant/permissive.
	// * Searching for any literal (Rabin-Karp aka bytes.Index) or group
	// of literals (Aho-Corasick).
	benchSearchRegex(b, &protocol.Request{
		Repo:   "github.com/golang/go",
		Commit: "0ebaca6ba27534add5930a95acffa9acff182e2b",
		PatternInfo: protocol.PatternInfo{
			Pattern:         "^func +[A-Z]",
			IsRegExp:        true,
			IsCaseSensitive: true,
		},
	})
}

func BenchmarkSearchRegex_large_capture_group(b *testing.B) {
	benchSearchRegex(b, &protocol.Request{
		Repo:   "github.com/golang/go",
		Commit: "0ebaca6ba27534add5930a95acffa9acff182e2b",
		PatternInfo: protocol.PatternInfo{
			Pattern:         "(TODO|FIXME)",
			IsRegExp:        true,
			IsCaseSensitive: true,
		},
	})
}

func BenchmarkSearchRegex_large_path(b *testing.B) {
	do := func(b *testing.B, content, path bool) {
		benchSearchRegex(b, &protocol.Request{
			Repo:   "github.com/golang/go",
			Commit: "0ebaca6ba27534add5930a95acffa9acff182e2b",
			PatternInfo: protocol.PatternInfo{
				Pattern:               "http.*client",
				IsRegExp:              true,
				IsCaseSensitive:       true,
				PatternMatchesContent: content,
				PatternMatchesPath:    path,
			},
		})
	}
	b.Run("path only", func(b *testing.B) { do(b, false, true) })
	b.Run("content only", func(b *testing.B) { do(b, true, false) })
	b.Run("both path and content", func(b *testing.B) { do(b, true, true) })
}

func BenchmarkSearchRegex_small_fixed(b *testing.B) {
	benchSearchRegex(b, &protocol.Request{
		Repo:   "github.com/sourcegraph/go-langserver",
		Commit: "4193810334683f87b8ed5d896aa4753f0dfcdf20",
		PatternInfo: protocol.PatternInfo{
			Pattern: "object not found",
		},
	})
}

func BenchmarkSearchRegex_small_fixed_casesensitive(b *testing.B) {
	benchSearchRegex(b, &protocol.Request{
		Repo:   "github.com/sourcegraph/go-langserver",
		Commit: "4193810334683f87b8ed5d896aa4753f0dfcdf20",
		PatternInfo: protocol.PatternInfo{
			Pattern:         "object not found",
			IsCaseSensitive: true,
		},
	})
}

func BenchmarkSearchRegex_small_re_dotstar(b *testing.B) {
	benchSearchRegex(b, &protocol.Request{
		Repo:   "github.com/sourcegraph/go-langserver",
		Commit: "4193810334683f87b8ed5d896aa4753f0dfcdf20",
		PatternInfo: protocol.PatternInfo{
			Pattern:  ".*",
			IsRegExp: true,
		},
	})
}

func BenchmarkSearchRegex_small_re_common(b *testing.B) {
	benchSearchRegex(b, &protocol.Request{
		Repo:   "github.com/sourcegraph/go-langserver",
		Commit: "4193810334683f87b8ed5d896aa4753f0dfcdf20",
		PatternInfo: protocol.PatternInfo{
			Pattern:         "func +[A-Z]",
			IsRegExp:        true,
			IsCaseSensitive: true,
		},
	})
}

func BenchmarkSearchRegex_small_re_anchor(b *testing.B) {
	benchSearchRegex(b, &protocol.Request{
		Repo:   "github.com/sourcegraph/go-langserver",
		Commit: "4193810334683f87b8ed5d896aa4753f0dfcdf20",
		PatternInfo: protocol.PatternInfo{
			Pattern:         "^func +[A-Z]",
			IsRegExp:        true,
			IsCaseSensitive: true,
		},
	})
}

func BenchmarkSearchRegex_small_capture_group(b *testing.B) {
	benchSearchRegex(b, &protocol.Request{
		Repo:   "github.com/sourcegraph/go-langserver",
		Commit: "4193810334683f87b8ed5d896aa4753f0dfcdf20",
		PatternInfo: protocol.PatternInfo{
			Pattern:         "(TODO|FIXME)",
			IsRegExp:        true,
			IsCaseSensitive: true,
		},
	})
}

func benchSearchRegex(b *testing.B, p *protocol.Request) {
	if testing.Short() {
		b.Skip("")
	}
	b.ReportAllocs()

	err := validateParams(p)
	if err != nil {
		b.Fatal(err)
	}

	m, err := compilePattern(&p.PatternInfo)
	if err != nil {
		b.Fatal(err)
	}

	pm, err := compilePathPatterns(&p.PatternInfo)
	if err != nil {
		b.Fatal(err)
	}

	ctx := context.Background()
	path, err := githubStore.PrepareZip(ctx, p.Repo, p.Commit, nil)
	if err != nil {
		b.Fatal(err)
	}

	var zc zipCache
	zf, err := zc.Get(path)
	if err != nil {
		b.Fatal(err)
	}
	defer zf.Close()

	b.ResetTimer()

	for n := 0; n < b.N; n++ {
		_, _, err := regexSearchBatch(ctx, m, pm, zf, 99999999, p.PatternMatchesContent, p.PatternMatchesPath, p.IsCaseSensitive, 0)
		if err != nil {
			b.Fatal(err)
		}
	}
}

func TestReadAll(t *testing.T) {
	input := []byte("Hello World")

	// If we are the same size as input, it should work
	b := make([]byte, len(input))
	n, err := readAll(bytes.NewReader(input), b)
	if err != nil {
		t.Fatal(err)
	}
	if n != len(input) {
		t.Fatalf("want to read in %d bytes, read %d", len(input), n)
	}
	if string(b[:n]) != string(input) {
		t.Fatalf("got %s, want %s", string(b[:n]), string(input))
	}

	// If we are larger then it should work
	b = make([]byte, len(input)*2)
	n, err = readAll(bytes.NewReader(input), b)
	if err != nil {
		t.Fatal(err)
	}
	if n != len(input) {
		t.Fatalf("want to read in %d bytes, read %d", len(input), n)
	}
	if string(b[:n]) != string(input) {
		t.Fatalf("got %s, want %s", string(b[:n]), string(input))
	}

	// Same size, but modify reader to return 1 byte per call to ensure
	// our loop works.
	b = make([]byte, len(input))
	n, err = readAll(iotest.OneByteReader(bytes.NewReader(input)), b)
	if err != nil {
		t.Fatal(err)
	}
	if n != len(input) {
		t.Fatalf("want to read in %d bytes, read %d", len(input), n)
	}
	if string(b[:n]) != string(input) {
		t.Fatalf("got %s, want %s", string(b[:n]), string(input))
	}

	// If we are too small it should fail
	b = make([]byte, 1)
	_, err = readAll(bytes.NewReader(input), b)
	if err == nil {
		t.Fatal("expected to fail on small buffer")
	}
}

func TestMaxMatches(t *testing.T) {
	t.Skip("TODO: Disabled because it's flaky. See: https://github.com/sourcegraph/sourcegraph/issues/22560")

	pattern := "foo"

	// Create a zip archive which contains our limits + 1
	buf := new(bytes.Buffer)
	zw := zip.NewWriter(buf)
	maxMatches := 33
	for i := 0; i < maxMatches+1; i++ {
		w, err := zw.CreateHeader(&zip.FileHeader{
			Name:   strconv.Itoa(i),
			Method: zip.Store,
		})
		if err != nil {
			t.Fatal(err)
		}
		for j := 0; j < 10; j++ {
			_, _ = w.Write([]byte(pattern))
			_, _ = w.Write([]byte{' '})
			_, _ = w.Write([]byte{'\n'})
		}
	}
	err := zw.Close()
	if err != nil {
		t.Fatal(err)
	}
	zf, err := mockZipFile(buf.Bytes())
	if err != nil {
		t.Fatal(err)
	}

	p := &protocol.PatternInfo{Pattern: pattern}
	m, err := compilePattern(p)
	if err != nil {
		t.Fatal(err)
	}

	pm, err := compilePathPatterns(p)
	if err != nil {
		t.Fatal(err)
	}

	fileMatches, limitHit, err := regexSearchBatch(context.Background(), m, pm, zf, maxMatches, true, false, false, 0)
	if err != nil {
		t.Fatal(err)
	}
	if !limitHit {
		t.Fatalf("expected limitHit on regexSearch")
	}

	totalMatches := 0
	for _, match := range fileMatches {
		totalMatches += match.MatchCount()
	}

	if totalMatches != maxMatches {
		t.Fatalf("expected %d file matches, got %d", maxMatches, totalMatches)
	}
}

// Tests that:
//
// - IncludePatterns can match the path in any order
// - A path must match all (not any) of the IncludePatterns
// - An empty pattern is allowed
func TestPathMatches(t *testing.T) {
	zipData, err := createZip(map[string]string{
		"a":   "",
		"a/b": "",
		"a/c": "",
		"ab":  "",
		"b/a": "",
		"ba":  "",
		"c/d": "",
	})
	if err != nil {
		t.Fatal(err)
	}
	zf, err := mockZipFile(zipData)
	if err != nil {
		t.Fatal(err)
	}

	patternInfo := &protocol.PatternInfo{
		Pattern:         "",
		IncludePatterns: []string{"a", "b"},
	}
	m, err := compilePattern(patternInfo)
	if err != nil {
		t.Fatal(err)
	}
	pm, err := compilePathPatterns(patternInfo)
	if err != nil {
		t.Fatal(err)
	}

	fileMatches, _, err := regexSearchBatch(context.Background(), m, pm, zf, 10, true, true, false, 0)
	if err != nil {
		t.Fatal(err)
	}

	want := []string{"a/b", "ab", "b/a", "ba"}
	got := make([]string, len(fileMatches))
	for i, fm := range fileMatches {
		got[i] = fm.Path
	}
	sort.Strings(got)
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("got file matches %v, want %v", got, want)
	}
}

// githubStore fetches from github and caches across test runs.
var githubStore = &Store{
	GitserverClient: gitserver.NewClient("test"),
	FetchTar:        fetchTarFromGithub,
	Path:            "/tmp/search_test/store",
	Log:             observation.TestContext.Logger,
	ObservationCtx:  &observation.TestContext,
}

func fetchTarFromGithub(ctx context.Context, repo api.RepoName, commit api.CommitID) (io.ReadCloser, error) {
	r, err := fetchTarFromGithubWithPaths(ctx, repo, commit, []string{})
	return r, err
}

func init() {
	// Clear out store so we pick up changes in our store writing code.
	os.RemoveAll(githubStore.Path)
}

func TestRegexSearch(t *testing.T) {
	pm, err := compilePathPatterns(&protocol.PatternInfo{
		IncludePatterns: []string{`a\.go`},
		ExcludePattern:  `README\.md`,
	})
	if err != nil {
		t.Fatal(err)
	}

	zipData, _ := createZip(map[string]string{
		"a.go":      "aaaaa11111",
		"b.go":      "bbbbb22222",
		"c.go":      "ccccc3333",
		"README.md": "important info on go",
	})
	file, _ := mockZipFile(zipData)

	type args struct {
		ctx                   context.Context
		m                     matchTree
		pm                    *pathMatcher
		zf                    *zipFile
		limit                 int
		patternMatchesContent bool
		patternMatchesPaths   bool
	}
	tests := []struct {
		name    string
		args    args
		wantFm  []protocol.FileMatch
		wantErr bool
	}{
		{
			name: "nil matchTree returns a FileMatch with no LineMatches",
			args: args{
				ctx: context.Background(),
				// Check this case specifically.
				m:                     &allMatchTree{},
				pm:                    pm,
				zf:                    file,
				patternMatchesPaths:   false,
				patternMatchesContent: true,
				limit:                 5,
			},
			wantFm: []protocol.FileMatch{{Path: "a.go"}},
		},
		{
			name: "'and' matchTree with matches",
			args: args{
				ctx: context.Background(),
				m: &orMatchTree{
					children: []matchTree{
						&regexMatchTree{
							re: regexp.MustCompile("aaaaa"),
						},
						&regexMatchTree{
							re: regexp.MustCompile("11111"),
						},
					}},
				pm:                    &pathMatcher{},
				zf:                    file,
				patternMatchesPaths:   false,
				patternMatchesContent: true,
				limit:                 5,
			},
			wantFm: []protocol.FileMatch{{
				Path: "a.go",
				ChunkMatches: []protocol.ChunkMatch{{
					Content:      "aaaaa11111",
					ContentStart: protocol.Location{0, 0, 0},
					Ranges: []protocol.Range{{
						Start: protocol.Location{0, 0, 0},
						End:   protocol.Location{5, 0, 5},
					}, {
						Start: protocol.Location{5, 0, 5},
						End:   protocol.Location{10, 0, 10},
					}},
				}},
			}},
		},
		{
			name: "'and' matchTree with no match",
			args: args{
				ctx: context.Background(),
				m: &andMatchTree{
					children: []matchTree{
						&regexMatchTree{
							re: regexp.MustCompile("aaaaa"),
						},
						&regexMatchTree{
							re: regexp.MustCompile("22222"),
						},
					}},
				pm:                    &pathMatcher{},
				zf:                    file,
				patternMatchesPaths:   false,
				patternMatchesContent: true,
				limit:                 5,
			},
			wantFm: nil,
		},
		{
			name: "empty 'and' matchTree",
			args: args{
				ctx:                   context.Background(),
				m:                     &andMatchTree{},
				pm:                    pm,
				zf:                    file,
				patternMatchesPaths:   false,
				patternMatchesContent: true,
				limit:                 5,
			},
			wantFm: []protocol.FileMatch{{Path: "a.go"}},
		},
		{
			name: "'or' matchTree with matches",
			args: args{
				ctx: context.Background(),
				m: &orMatchTree{
					children: []matchTree{
						&regexMatchTree{
							re: regexp.MustCompile("aaaaa"),
						},
						&regexMatchTree{
							re: regexp.MustCompile("99999"),
						},
					}},
				pm:                    &pathMatcher{},
				zf:                    file,
				patternMatchesPaths:   false,
				patternMatchesContent: true,
				limit:                 5,
			},
			wantFm: []protocol.FileMatch{{
				Path: "a.go",
				ChunkMatches: []protocol.ChunkMatch{{
					Content:      "aaaaa11111",
					ContentStart: protocol.Location{0, 0, 0},
					Ranges: []protocol.Range{{
						Start: protocol.Location{0, 0, 0},
						End:   protocol.Location{5, 0, 5},
					}},
				}},
			}},
		},
		{
			name: "'or' matchTree with no match",
			args: args{
				ctx: context.Background(),
				m: &orMatchTree{
					children: []matchTree{
						&regexMatchTree{
							re: regexp.MustCompile("jjjjj"),
						},
						&regexMatchTree{
							re: regexp.MustCompile("99999"),
						},
					}},
				pm:                    &pathMatcher{},
				zf:                    file,
				patternMatchesPaths:   false,
				patternMatchesContent: true,
				limit:                 5,
			},
			wantFm: nil,
		},
		{
			name: "empty 'or' matchTree",
			args: args{
				ctx:                   context.Background(),
				m:                     &orMatchTree{},
				pm:                    &pathMatcher{},
				zf:                    file,
				patternMatchesPaths:   false,
				patternMatchesContent: true,
				limit:                 5,
			},
			wantFm: nil,
		},
		{
			name: "matchTree matches on content AND path",
			args: args{
				ctx: context.Background(),
				m: &regexMatchTree{
					re: regexp.MustCompile("go"),
				},
				pm:                    pm,
				zf:                    file,
				patternMatchesPaths:   true,
				patternMatchesContent: true,
				limit:                 5,
			},
			wantFm: []protocol.FileMatch{{Path: "a.go"}},
		},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			gotFm, _, err := regexSearchBatch(tt.args.ctx, tt.args.m, tt.args.pm, tt.args.zf, tt.args.limit, tt.args.patternMatchesContent, tt.args.patternMatchesPaths, false, 0)
			if (err != nil) != tt.wantErr {
				t.Errorf("regexSearch() error = %v, wantErr %v", err, tt.wantErr)
				return
			}
			if !reflect.DeepEqual(gotFm, tt.wantFm) {
				t.Errorf("regexSearch() gotFm = %v, want %v", gotFm, tt.wantFm)
			}
		})
	}
}

func Test_locsToRanges(t *testing.T) {
	cases := []struct {
		buf    string
		locs   [][]int
		ranges []protocol.Range
	}{{
		// simple multimatch
		buf:  "0.2.4.6.8.",
		locs: [][]int{{0, 2}, {4, 8}},
		ranges: []protocol.Range{{
			Start: protocol.Location{0, 0, 0},
			End:   protocol.Location{2, 0, 2},
		}, {
			Start: protocol.Location{4, 0, 4},
			End:   protocol.Location{8, 0, 8},
		}},
	}, {
		// multibyte match
		buf:  "0.2.🔧.8.",
		locs: [][]int{{2, 8}},
		ranges: []protocol.Range{{
			Start: protocol.Location{2, 0, 2},
			End:   protocol.Location{8, 0, 5},
		}},
	}, {
		// match crosses newlines and ends on a newline
		buf:  "0.2.4.6.\n9.11.14.17",
		locs: [][]int{{2, 9}},
		ranges: []protocol.Range{{
			Start: protocol.Location{2, 0, 2},
			End:   protocol.Location{9, 1, 0},
		}},
	}, {
		// match starts on a newline
		buf:  "0.2.4.6.\n9.11.14.17",
		locs: [][]int{{8, 11}},
		ranges: []protocol.Range{{
			Start: protocol.Location{8, 0, 8},
			End:   protocol.Location{11, 1, 2},
		}},
	}, {
		// match crosses a few lines and has multibyte chars
		buf:  "0.2.🔧.9.\n12.15.18.\n22.25.28.",
		locs: [][]int{{0, 25}},
		ranges: []protocol.Range{{
			Start: protocol.Location{0, 0, 0},
			End:   protocol.Location{25, 2, 3},
		}},
	}, {
		// multiple matches on different lines
		buf:  "0.2.🔧.9.\n12.15.18.\n22.25.28.",
		locs: [][]int{{0, 2}, {2, 3}, {10, 14}, {23, 28}},
		ranges: []protocol.Range{{
			Start: protocol.Location{0, 0, 0},
			End:   protocol.Location{2, 0, 2},
		}, {
			Start: protocol.Location{2, 0, 2},
			End:   protocol.Location{3, 0, 3},
		}, {
			Start: protocol.Location{10, 0, 7},
			End:   protocol.Location{14, 1, 2},
		}, {
			Start: protocol.Location{23, 2, 1},
			End:   protocol.Location{28, 2, 6},
		}},
	}, {
		// multiple matches with overlap
		buf:  "0.2.🔧.9.\n12.15.18.\n22.25.28.",
		locs: [][]int{{1, 8}, {2, 3}, {8, 11}, {8, 9}, {13, 16}, {14, 17}},
		ranges: []protocol.Range{{
			Start: protocol.Location{1, 0, 1},
			End:   protocol.Location{8, 0, 5},
		}, {
			Start: protocol.Location{2, 0, 2},
			End:   protocol.Location{3, 0, 3},
		}, {
			Start: protocol.Location{8, 0, 5},
			End:   protocol.Location{11, 0, 8},
		}, {
			Start: protocol.Location{8, 0, 5},
			End:   protocol.Location{9, 0, 6},
		}, {
			Start: protocol.Location{13, 1, 1},
			End:   protocol.Location{16, 1, 4},
		}, {
			Start: protocol.Location{14, 1, 2},
			End:   protocol.Location{17, 1, 5},
		}}},
	}

	for _, tc := range cases {
		t.Run("", func(t *testing.T) {
			got := locsToRanges([]byte(tc.buf), tc.locs)
			require.Equal(t, tc.ranges, got)
		})
	}
}
