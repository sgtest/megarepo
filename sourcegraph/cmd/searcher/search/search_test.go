package search_test

import (
	"archive/tar"
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/ioutil"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"os/exec"
	"sort"
	"strconv"
	"strings"
	"testing"

	"github.com/sourcegraph/sourcegraph/cmd/searcher/search"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/gitserver"
	"github.com/sourcegraph/sourcegraph/pkg/searcher/protocol"
)

func TestSearch(t *testing.T) {
	// Create byte buffer of binary file
	miltonPNG := bytes.Repeat([]byte{0x00}, 32*1024)

	files := map[string]string{
		"README.md": `# Hello World

Hello world example in go`,
		"main.go": `package main

import "fmt"

func main() {
	fmt.Println("Hello world")
}
`,
		"abc.txt":    "w",
		"milton.png": string(miltonPNG),
	}

	cases := []struct {
		arg  protocol.PatternInfo
		want string
	}{
		{protocol.PatternInfo{Pattern: "foo"}, ""},

		{protocol.PatternInfo{Pattern: "World", IsCaseSensitive: true}, `
README.md:1:# Hello World
`},

		{protocol.PatternInfo{Pattern: "world", IsCaseSensitive: true}, `
README.md:3:Hello world example in go
main.go:6:	fmt.Println("Hello world")
`},

		{protocol.PatternInfo{Pattern: "world"}, `
README.md:1:# Hello World
README.md:3:Hello world example in go
main.go:6:	fmt.Println("Hello world")
`},

		{protocol.PatternInfo{Pattern: "func.*main"}, ""},

		{protocol.PatternInfo{Pattern: "func.*main", IsRegExp: true}, `
main.go:5:func main() {
`},

		// https://github.com/sourcegraph/sourcegraph/issues/8155
		{protocol.PatternInfo{Pattern: "^func", IsRegExp: true}, `
main.go:5:func main() {
`},
		{protocol.PatternInfo{Pattern: "^FuNc", IsRegExp: true}, `
main.go:5:func main() {
`},

		{protocol.PatternInfo{Pattern: "mai", IsWordMatch: true}, ""},

		{protocol.PatternInfo{Pattern: "main", IsWordMatch: true}, `
main.go:1:package main
main.go:5:func main() {
`},

		// Ensure we handle CaseInsensitive regexp searches with
		// special uppercase chars in pattern.
		{protocol.PatternInfo{Pattern: `printL\B`, IsRegExp: true}, `
main.go:6:	fmt.Println("Hello world")
`},

		{protocol.PatternInfo{Pattern: "world", ExcludePattern: "README.md"}, `
main.go:6:	fmt.Println("Hello world")
`},
		{protocol.PatternInfo{Pattern: "world", IncludePattern: "*.md"}, `
README.md:1:# Hello World
README.md:3:Hello world example in go
`},

		{protocol.PatternInfo{Pattern: "w", IncludePatterns: []string{"*.{md,txt}", "*.txt"}}, `
abc.txt:1:w
`},

		{protocol.PatternInfo{Pattern: "world", ExcludePattern: "README\\.md", PathPatternsAreRegExps: true}, `
main.go:6:	fmt.Println("Hello world")
`},
		{protocol.PatternInfo{Pattern: "world", IncludePattern: "\\.md", PathPatternsAreRegExps: true}, `
README.md:1:# Hello World
README.md:3:Hello world example in go
`},

		{protocol.PatternInfo{Pattern: "w", IncludePatterns: []string{"\\.(md|txt)", "README"}, PathPatternsAreRegExps: true}, `
README.md:1:# Hello World
README.md:3:Hello world example in go
`},

		{protocol.PatternInfo{Pattern: "world", IncludePattern: "*.{MD,go}", PathPatternsAreCaseSensitive: true}, `
main.go:6:	fmt.Println("Hello world")
`},
		{protocol.PatternInfo{Pattern: "world", IncludePattern: `\.(MD|go)`, PathPatternsAreRegExps: true, PathPatternsAreCaseSensitive: true}, `
main.go:6:	fmt.Println("Hello world")
`},

		{protocol.PatternInfo{Pattern: "doesnotmatch"}, ""},
		{protocol.PatternInfo{Pattern: "", IsRegExp: false, IncludePatterns: []string{"\\.png"}, PathPatternsAreRegExps: true, PatternMatchesPath: true}, `
milton.png
`},
	}

	store, cleanup, err := newStore(files)
	if err != nil {
		t.Fatal(err)
	}
	defer cleanup()
	ts := httptest.NewServer(&search.Service{Store: store})
	defer ts.Close()

	for _, test := range cases {
		test.arg.PatternMatchesContent = true
		req := protocol.Request{
			Repo:         "foo",
			URL:          "u",
			Commit:       "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
			PatternInfo:  test.arg,
			FetchTimeout: "500ms",
		}
		m, err := doSearch(ts.URL, &req)
		if err != nil {
			t.Errorf("%v failed: %s", test.arg, err)
			continue
		}
		sort.Sort(sortByPath(m))
		got := toString(m)
		err = sanityCheckSorted(m)
		if err != nil {
			t.Errorf("%v malformed response: %s\n%s", test.arg, err, got)
		}
		// We have an extra newline to make expected readable
		if len(test.want) > 0 {
			test.want = test.want[1:]
		}
		if got != test.want {
			d, err := diff(test.want, got)
			if err != nil {
				t.Fatal(err)
			}
			t.Errorf("%v unexpected response:\n%s", test.arg, d)
		}
	}
}

func TestSearch_badrequest(t *testing.T) {
	cases := []protocol.Request{
		// Bad regexp
		{
			Repo:   "foo",
			URL:    "u",
			Commit: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
			PatternInfo: protocol.PatternInfo{
				Pattern:  `\F`,
				IsRegExp: true,
			},
		},

		// Unsupported regex
		{
			Repo:   "foo",
			URL:    "u",
			Commit: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
			PatternInfo: protocol.PatternInfo{
				Pattern:  `(?!id)entity`,
				IsRegExp: true,
			},
		},

		// No repo
		{
			URL:    "u",
			Commit: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
			PatternInfo: protocol.PatternInfo{
				Pattern: "test",
			},
		},

		// No commit
		{
			Repo: "foo",
			URL:  "u",
			PatternInfo: protocol.PatternInfo{
				Pattern: "test",
			},
		},

		// Non-absolute commit
		{
			Repo:   "foo",
			URL:    "u",
			Commit: "HEAD",
			PatternInfo: protocol.PatternInfo{
				Pattern: "test",
			},
		},

		// Bad include glob
		{
			Repo:   "foo",
			URL:    "u",
			Commit: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
			PatternInfo: protocol.PatternInfo{
				Pattern:        "test",
				IncludePattern: "[c-a]",
			},
		},

		// Bad exclude glob
		{
			Repo:   "foo",
			URL:    "u",
			Commit: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
			PatternInfo: protocol.PatternInfo{
				Pattern:        "test",
				ExcludePattern: "[c-a]",
			},
		},

		// Bad include regexp
		{
			Repo:   "foo",
			URL:    "u",
			Commit: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
			PatternInfo: protocol.PatternInfo{
				Pattern:                "test",
				IncludePattern:         "**",
				PathPatternsAreRegExps: true,
			},
		},

		// Bad exclude regexp
		{
			Repo:   "foo",
			URL:    "u",
			Commit: "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
			PatternInfo: protocol.PatternInfo{
				Pattern:                "test",
				ExcludePattern:         "**",
				PathPatternsAreRegExps: true,
			},
		},
	}

	store, cleanup, err := newStore(nil)
	if err != nil {
		t.Fatal(err)
	}
	defer cleanup()
	ts := httptest.NewServer(&search.Service{Store: store})
	defer ts.Close()

	for _, p := range cases {
		p.PatternInfo.PatternMatchesContent = true
		_, err := doSearch(ts.URL, &p)
		if err == nil {
			t.Fatalf("%v expected to fail", p)
		}
		if !strings.HasPrefix(err.Error(), "non-200 response: code=400 ") {
			t.Fatalf("%v expected to have HTTP 400 response. Got %s", p, err)
		}
	}
}

func doSearch(u string, p *protocol.Request) ([]protocol.FileMatch, error) {
	form := url.Values{
		"Repo":            []string{string(p.Repo)},
		"URL":             []string{string(p.URL)},
		"Commit":          []string{string(p.Commit)},
		"Pattern":         []string{p.Pattern},
		"IncludePatterns": p.IncludePatterns,
		"IncludePattern":  []string{p.IncludePattern},
		"ExcludePattern":  []string{p.ExcludePattern},
	}
	if p.IsRegExp {
		form.Set("IsRegExp", "true")
	}
	if p.IsWordMatch {
		form.Set("IsWordMatch", "true")
	}
	if p.IsCaseSensitive {
		form.Set("IsCaseSensitive", "true")
	}
	if p.PathPatternsAreRegExps {
		form.Set("PathPatternsAreRegExps", "true")
	}
	if p.PathPatternsAreCaseSensitive {
		form.Set("PathPatternsAreCaseSensitive", "true")
	}
	if p.PatternMatchesContent {
		form.Set("PatternMatchesContent", "true")
	}
	if p.PatternMatchesPath {
		form.Set("PatternMatchesPath", "true")
	}
	resp, err := http.PostForm(u, form)
	if err != nil {
		return nil, err
	}

	body, err := ioutil.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}
	if resp.StatusCode != 200 {
		return nil, fmt.Errorf("non-200 response: code=%d body=%s", resp.StatusCode, string(body))
	}

	var r protocol.Response
	err = json.Unmarshal(body, &r)
	if err != nil {
		return nil, err
	}
	return r.Matches, err
}

func newStore(files map[string]string) (*search.Store, func(), error) {
	buf := new(bytes.Buffer)
	w := tar.NewWriter(buf)
	for name, body := range files {
		hdr := &tar.Header{
			Name: name,
			Mode: 0600,
			Size: int64(len(body)),
		}
		if err := w.WriteHeader(hdr); err != nil {
			return nil, nil, err
		}
		if _, err := w.Write([]byte(body)); err != nil {
			return nil, nil, err
		}
	}
	// git-archive usually includes a pax header we should ignore.
	// use a body which matches a test case. Ensures we don't return this
	// false entry as a result.
	if err := addpaxheader(w, "Hello world\n"); err != nil {
		return nil, nil, err
	}

	err := w.Close()
	if err != nil {
		return nil, nil, err
	}
	d, err := ioutil.TempDir("", "search_test")
	if err != nil {
		return nil, nil, err
	}
	return &search.Store{
		FetchTar: func(ctx context.Context, repo gitserver.Repo, commit api.CommitID) (io.ReadCloser, error) {
			return ioutil.NopCloser(bytes.NewReader(buf.Bytes())), nil
		},
		Path: d,
	}, func() { os.RemoveAll(d) }, nil
}

func toString(m []protocol.FileMatch) string {
	buf := new(bytes.Buffer)
	for _, f := range m {
		if len(f.LineMatches) == 0 {
			buf.WriteString(f.Path)
			buf.WriteByte('\n')
		}
		for _, l := range f.LineMatches {
			buf.WriteString(f.Path)
			buf.WriteByte(':')
			buf.WriteString(strconv.Itoa(l.LineNumber + 1))
			buf.WriteByte(':')
			buf.WriteString(l.Preview)
			buf.WriteByte('\n')
		}

	}
	return buf.String()
}

func sanityCheckSorted(m []protocol.FileMatch) error {
	if !sort.IsSorted(sortByPath(m)) {
		return errors.New("unsorted file matches, please sortByPath")
	}
	for i := range m {
		if i > 0 && m[i].Path == m[i-1].Path {
			return fmt.Errorf("duplicate FileMatch on %s", m[i].Path)
		}
		lm := m[i].LineMatches
		if !sort.IsSorted(sortByLineNumber(lm)) {
			return fmt.Errorf("unsorted LineMatches for %s", m[i].Path)
		}
		for j := range lm {
			if j > 0 && lm[j].LineNumber == lm[j-1].LineNumber {
				return fmt.Errorf("duplicate LineNumber on %s:%d", m[i].Path, lm[j].LineNumber)
			}
		}
	}
	return nil
}

func diff(b1, b2 string) (string, error) {
	f1, err := ioutil.TempFile("", "search_test")
	if err != nil {
		return "", err
	}
	defer os.Remove(f1.Name())
	defer f1.Close()

	f2, err := ioutil.TempFile("", "search_test")
	if err != nil {
		return "", err
	}
	defer os.Remove(f2.Name())
	defer f2.Close()

	f1.WriteString(b1)
	f2.WriteString(b2)

	data, err := exec.Command("diff", "-u", f1.Name(), f2.Name()).CombinedOutput()
	if len(data) > 0 {
		err = nil
	}
	return string(data), err
}

type sortByPath []protocol.FileMatch

func (m sortByPath) Len() int           { return len(m) }
func (m sortByPath) Less(i, j int) bool { return m[i].Path < m[j].Path }
func (m sortByPath) Swap(i, j int)      { m[i], m[j] = m[j], m[i] }

type sortByLineNumber []protocol.LineMatch

func (m sortByLineNumber) Len() int           { return len(m) }
func (m sortByLineNumber) Less(i, j int) bool { return m[i].LineNumber < m[j].LineNumber }
func (m sortByLineNumber) Swap(i, j int)      { m[i], m[j] = m[j], m[i] }
