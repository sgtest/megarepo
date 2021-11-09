package run

import (
	"context"
	"crypto/md5"
	"encoding/binary"
	"os"
	"reflect"
	"regexp"
	"sort"
	"strconv"
	"testing"

	"github.com/cockroachdb/errors"
	"github.com/google/go-cmp/cmp"
	"github.com/google/go-cmp/cmp/cmpopts"
	"github.com/google/zoekt"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/search"
	searchbackend "github.com/sourcegraph/sourcegraph/internal/search/backend"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
	"github.com/sourcegraph/sourcegraph/internal/search/streaming"
	"github.com/sourcegraph/sourcegraph/internal/search/unindexed"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestSearchRepositories(t *testing.T) {
	if os.Getenv("CI") != "" {
		// #25936: Some unit tests rely on external services that break
		// in CI but not locally. They should be removed or improved.
		t.Skip("TestSeachRepositories only works in local dev and is not reliable in CI")
	}
	repositories := []*search.RepositoryRevisions{
		{Repo: types.MinimalRepo{ID: 123, Name: "foo/one"}, Revs: []search.RevisionSpecifier{{RevSpec: ""}}},
		{Repo: types.MinimalRepo{ID: 456, Name: "foo/no-match"}, Revs: []search.RevisionSpecifier{{RevSpec: ""}}},
		{Repo: types.MinimalRepo{ID: 789, Name: "bar/one"}, Revs: []search.RevisionSpecifier{{RevSpec: ""}}},
	}

	zoekt := &searchbackend.FakeSearcher{}

	unindexed.MockSearchFilesInRepos = func() ([]result.Match, *streaming.Stats, error) {
		repoName := repositories[0].Repo.Name
		rev := "1a2b3c"
		switch repoName {
		case "foo/one":
			return []result.Match{&result.FileMatch{
				File: result.File{
					Repo:     types.MinimalRepo{ID: 123, Name: repoName},
					InputRev: &rev,
					Path:     "f.go",
				},
			}}, &streaming.Stats{}, nil
		case "bar/one":
			return []result.Match{&result.FileMatch{
				File: result.File{
					Repo:     types.MinimalRepo{ID: 789, Name: repoName},
					InputRev: &rev,
					Path:     "f.go",
				},
			}}, &streaming.Stats{}, nil
		case "foo/no-match":
			return []result.Match{}, &streaming.Stats{}, nil
		default:
			return nil, &streaming.Stats{}, errors.New("Unexpected repo")
		}
	}
	defer func() { unindexed.MockSearchFilesInRepos = nil }()

	cases := []struct {
		name string
		q    string
		want []string
	}{{
		name: "all",
		q:    "type:repo",
		want: []string{"bar/one", "foo/no-match", "foo/one"},
	}, {
		name: "pattern filter",
		q:    "type:repo foo/one",
		want: []string{"foo/one"},
	}, {
		name: "repohasfile",
		q:    "foo type:repo repohasfile:f.go",
		want: []string{"foo/one"},
	}, {
		name: "case yes match",
		q:    "foo case:yes",
		want: []string{"foo/no-match", "foo/one"},
	}, {
		name: "case no match",
		q:    "Foo case:no",
		want: []string{"foo/no-match", "foo/one"},
	}, {
		name: "case exclude all",
		q:    "Foo case:yes",
		want: []string{},
	}}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			q, _ := query.ParseLiteral(tc.q)
			b, _ := query.ToBasicQuery(q)
			pattern := search.ToTextPatternInfo(b, search.Batch, query.Identity)
			matches, _, err := searchRepositoriesBatch(context.Background(), &search.TextParameters{
				PatternInfo: pattern,
				Repos:       repositories,
				Query:       q,
				Zoekt:       zoekt,
			}, 100)
			if err != nil {
				t.Fatal(err)
			}

			var got []string
			for _, res := range matches {
				r := res.(*result.RepoMatch)
				got = append(got, string(r.Name))
			}
			sort.Strings(got)

			if !cmp.Equal(tc.want, got, cmpopts.EquateEmpty()) {
				t.Errorf("mismatch (-want +got):\n%s", cmp.Diff(tc.want, got))
			}
		})
	}
}

func searchRepositoriesBatch(ctx context.Context, args *search.TextParameters, limit int) ([]result.Match, streaming.Stats, error) {
	return streaming.CollectStream(func(stream streaming.Sender) error {
		return SearchRepositories(ctx, args, limit, stream)
	})
}

func TestRepoShouldBeAdded(t *testing.T) {
	if os.Getenv("CI") != "" {
		// #25936: Some unit tests rely on external services that break
		// in CI but not locally. They should be removed or improved.
		t.Skip("TestRepoShouldBeAdded only works in local dev and is not reliable in CI")
	}
	zoekt := &searchbackend.FakeSearcher{}

	t.Run("repo should be included in results, query has repoHasFile filter", func(t *testing.T) {
		repo := &search.RepositoryRevisions{Repo: types.MinimalRepo{ID: 123, Name: "foo/one"}, Revs: []search.RevisionSpecifier{{RevSpec: ""}}}
		unindexed.MockSearchFilesInRepos = func() ([]result.Match, *streaming.Stats, error) {
			rev := "1a2b3c"
			return []result.Match{&result.FileMatch{
				File: result.File{
					Repo:     types.MinimalRepo{ID: 123, Name: repo.Repo.Name},
					InputRev: &rev,
					Path:     "foo.go",
				},
			}}, &streaming.Stats{}, nil
		}
		pat := &search.TextPatternInfo{
			Pattern:                      "",
			FilePatternsReposMustInclude: []string{"foo"},
			IsRegExp:                     true,
			FileMatchLimit:               1,
			PathPatternsAreCaseSensitive: false,
			PatternMatchesContent:        true,
			PatternMatchesPath:           true,
		}
		shouldBeAdded, err := repoShouldBeAdded(context.Background(), zoekt, repo, pat)
		if err != nil {
			t.Fatal(err)
		}
		if !shouldBeAdded {
			t.Errorf("Expected shouldBeAdded for repo %v to be true, but got false", repo)
		}
	})

	t.Run("repo shouldn't be included in results, query has repoHasFile filter ", func(t *testing.T) {
		repo := &search.RepositoryRevisions{Repo: types.MinimalRepo{Name: "foo/no-match"}, Revs: []search.RevisionSpecifier{{RevSpec: ""}}}
		unindexed.MockSearchFilesInRepos = func() ([]result.Match, *streaming.Stats, error) {
			return nil, &streaming.Stats{}, nil
		}
		pat := &search.TextPatternInfo{
			Pattern:                      "",
			FilePatternsReposMustInclude: []string{"foo"},
			IsRegExp:                     true,
			FileMatchLimit:               1,
			PathPatternsAreCaseSensitive: false,
			PatternMatchesContent:        true,
			PatternMatchesPath:           true,
		}
		shouldBeAdded, err := repoShouldBeAdded(context.Background(), zoekt, repo, pat)
		if err != nil {
			t.Fatal(err)
		}
		if shouldBeAdded {
			t.Errorf("Expected shouldBeAdded for repo %v to be false, but got true", repo)
		}
	})

	t.Run("repo shouldn't be included in results, query has -repoHasFile filter", func(t *testing.T) {
		repo := &search.RepositoryRevisions{Repo: types.MinimalRepo{ID: 123, Name: "foo/one"}, Revs: []search.RevisionSpecifier{{RevSpec: ""}}}
		unindexed.MockSearchFilesInRepos = func() ([]result.Match, *streaming.Stats, error) {
			rev := "1a2b3c"
			return []result.Match{&result.FileMatch{
				File: result.File{
					Repo:     types.MinimalRepo{ID: 123, Name: repo.Repo.Name},
					InputRev: &rev,
					Path:     "foo.go",
				},
			}}, &streaming.Stats{}, nil
		}
		pat := &search.TextPatternInfo{
			Pattern:                      "",
			FilePatternsReposMustExclude: []string{"foo"},
			IsRegExp:                     true,
			FileMatchLimit:               1,
			PathPatternsAreCaseSensitive: false,
			PatternMatchesContent:        true,
			PatternMatchesPath:           true,
		}
		shouldBeAdded, err := repoShouldBeAdded(context.Background(), zoekt, repo, pat)
		if err != nil {
			t.Fatal(err)
		}
		if shouldBeAdded {
			t.Errorf("Expected shouldBeAdded for repo %v to be false, but got true", repo)
		}
	})

	t.Run("repo should be included in results, query has -repoHasFile filter", func(t *testing.T) {
		repo := &search.RepositoryRevisions{Repo: types.MinimalRepo{Name: "foo/no-match"}, Revs: []search.RevisionSpecifier{{RevSpec: ""}}}
		unindexed.MockSearchFilesInRepos = func() ([]result.Match, *streaming.Stats, error) {
			return nil, &streaming.Stats{}, nil
		}
		pat := &search.TextPatternInfo{
			Pattern:                      "",
			FilePatternsReposMustExclude: []string{"foo"},
			IsRegExp:                     true,
			FileMatchLimit:               1,
			PathPatternsAreCaseSensitive: false,
			PatternMatchesContent:        true,
			PatternMatchesPath:           true,
		}
		shouldBeAdded, err := repoShouldBeAdded(context.Background(), zoekt, repo, pat)
		if err != nil {
			t.Fatal(err)
		}
		if !shouldBeAdded {
			t.Errorf("Expected shouldBeAdded for repo %v to be true, but got false", repo)
		}
	})
}

// repoShouldBeAdded determines whether a repository should be included in the result set based on whether the repository fits in the subset
// of repostiories specified in the query's `repohasfile` and `-repohasfile` fields if they exist.
func repoShouldBeAdded(ctx context.Context, zoekt zoekt.Streamer, repo *search.RepositoryRevisions, pattern *search.TextPatternInfo) (bool, error) {
	repos := []*search.RepositoryRevisions{repo}
	args := search.TextParameters{
		PatternInfo: pattern,
		Zoekt:       zoekt,
	}
	rsta, err := reposToAdd(ctx, &args, repos)
	if err != nil {
		return false, err
	}
	return len(rsta) == 1, nil
}

func TestMatchRepos(t *testing.T) {
	want := makeRepositoryRevisions("foo/bar", "abc/foo")
	in := append(want, makeRepositoryRevisions("beef/bam", "qux/bas")...)
	pattern := regexp.MustCompile("foo")

	results := make(chan []*search.RepositoryRevisions)
	go func() {
		defer close(results)
		matchRepos(pattern, in, results)
	}()
	var repos []*search.RepositoryRevisions
	for matched := range results {
		repos = append(repos, matched...)
	}

	// because of the concurrency we cannot rely on the order of "repos" to be the
	// same as "want". Hence we create map of repo names and compare those.
	toMap := func(reporevs []*search.RepositoryRevisions) map[string]struct{} {
		out := map[string]struct{}{}
		for _, r := range reporevs {
			out[string(r.Repo.Name)] = struct{}{}
		}
		return out
	}
	if !reflect.DeepEqual(toMap(repos), toMap(want)) {
		t.Fatalf("expected %v, got %v", want, repos)
	}
}

func BenchmarkSearchRepositories(b *testing.B) {
	n := 200 * 1000
	repos := make([]*search.RepositoryRevisions, n)
	for i := 0; i < n; i++ {
		repo := types.MinimalRepo{Name: api.RepoName("github.com/org/repo" + strconv.Itoa(i))}
		repos[i] = &search.RepositoryRevisions{Repo: repo, Revs: []search.RevisionSpecifier{{}}}
	}
	q, _ := query.ParseLiteral("context.WithValue")
	bq, _ := query.ToBasicQuery(q)
	pattern := search.ToTextPatternInfo(bq, search.Batch, query.Identity)
	tp := search.TextParameters{
		PatternInfo: pattern,
		Repos:       repos,
		Query:       q,
	}
	for i := 0; i < b.N; i++ {
		_, _, err := searchRepositoriesBatch(context.Background(), &tp, int(tp.PatternInfo.FileMatchLimit))
		if err != nil {
			b.Fatal(err)
		}
	}
}

func makeRepositoryRevisions(repos ...string) []*search.RepositoryRevisions {
	r := make([]*search.RepositoryRevisions, len(repos))
	for i, repospec := range repos {
		repoName, revs := search.ParseRepositoryRevisions(repospec)
		if len(revs) == 0 {
			// treat empty list as preferring master
			revs = []search.RevisionSpecifier{{RevSpec: ""}}
		}
		r[i] = &search.RepositoryRevisions{Repo: mkRepos(repoName)[0], Revs: revs}
	}
	return r
}

func mkRepos(names ...string) []types.MinimalRepo {
	var repos []types.MinimalRepo
	for _, name := range names {
		sum := md5.Sum([]byte(name))
		id := api.RepoID(binary.BigEndian.Uint64(sum[:]))
		if id < 0 {
			id = -(id / 2)
		}
		if id == 0 {
			id++
		}
		repos = append(repos, types.MinimalRepo{ID: id, Name: api.RepoName(name)})
	}
	return repos
}
