package graphqlbackend

import (
	"context"
	"crypto/md5"
	"encoding/binary"
	"fmt"
	"reflect"
	"sort"
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/pkg/errors"

	"github.com/sourcegraph/sourcegraph/cmd/searcher/protocol"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/endpoint"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/search"
	searchbackend "github.com/sourcegraph/sourcegraph/internal/search/backend"
	"github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/internal/search/searcher"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/vcs"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestSearchFilesInRepos(t *testing.T) {
	mockSearchFilesInRepo = func(ctx context.Context, repo *types.RepoName, gitserverRepo api.RepoName, rev string, info *search.TextPatternInfo, fetchTimeout time.Duration) (matches []*FileMatchResolver, limitHit bool, err error) {
		repoName := repo.Name
		switch repoName {
		case "foo/one":
			return []*FileMatchResolver{
				{
					uri: "git://" + string(repoName) + "?" + rev + "#" + "main.go",
				},
			}, false, nil
		case "foo/two":
			return []*FileMatchResolver{
				{
					uri: "git://" + string(repoName) + "?" + rev + "#" + "main.go",
				},
			}, false, nil
		case "foo/empty":
			return nil, false, nil
		case "foo/cloning":
			return nil, false, &vcs.RepoNotExistError{Repo: repoName, CloneInProgress: true}
		case "foo/missing":
			return nil, false, &vcs.RepoNotExistError{Repo: repoName}
		case "foo/missing-database":
			return nil, false, &errcode.Mock{Message: "repo not found: foo/missing-database", IsNotFound: true}
		case "foo/timedout":
			return nil, false, context.DeadlineExceeded
		case "foo/no-rev":
			// TODO we do not specify a rev when searching "foo/no-rev", so it
			// is treated as an empty repository. We need to test the fatal
			// case of trying to search a revision which doesn't exist.
			return nil, false, &gitserver.RevisionNotFoundError{Repo: repoName, Spec: "missing"}
		default:
			return nil, false, errors.New("Unexpected repo")
		}
	}
	defer func() { mockSearchFilesInRepo = nil }()

	zoekt := &searchbackend.Zoekt{Client: &searchbackend.FakeSearcher{}}

	q, err := query.ParseLiteral("foo")
	if err != nil {
		t.Fatal(err)
	}
	repoRevs := makeRepositoryRevisions("foo/one", "foo/two", "foo/empty", "foo/cloning", "foo/missing", "foo/missing-database", "foo/timedout", "foo/no-rev")
	args := &search.TextParameters{
		PatternInfo: &search.TextPatternInfo{
			FileMatchLimit: defaultMaxSearchResults,
			Pattern:        "foo",
		},
		RepoPromise:  (&search.Promise{}).Resolve(repoRevs),
		Query:        q,
		Zoekt:        zoekt,
		SearcherURLs: endpoint.Static("test"),
	}
	results, common, err := searchFilesInReposBatch(context.Background(), args)
	if err != nil {
		t.Fatal(err)
	}
	if len(results) != 2 {
		t.Errorf("expected two results, got %d", len(results))
	}
	repoNames := map[api.RepoID]string{}
	for _, rr := range repoRevs {
		repoNames[rr.Repo.ID] = string(rr.Repo.Name)
	}
	assertReposStatus(t, repoNames, common.Status, map[string]search.RepoStatus{
		"foo/cloning":          search.RepoStatusCloning,
		"foo/empty":            search.RepoStatusSearched,
		"foo/missing":          search.RepoStatusMissing,
		"foo/missing-database": search.RepoStatusMissing,
		"foo/no-rev":           0,
		"foo/one":              search.RepoStatusSearched,
		"foo/timedout":         search.RepoStatusTimedout,
		"foo/two":              search.RepoStatusSearched,
	})

	// If we specify a rev and it isn't found, we fail the whole search since
	// that should be checked earlier.
	args = &search.TextParameters{
		PatternInfo: &search.TextPatternInfo{
			FileMatchLimit: defaultMaxSearchResults,
			Pattern:        "foo",
		},
		RepoPromise:  (&search.Promise{}).Resolve(makeRepositoryRevisions("foo/no-rev@dev")),
		Query:        q,
		Zoekt:        zoekt,
		SearcherURLs: endpoint.Static("test"),
	}

	_, _, err = searchFilesInReposBatch(context.Background(), args)
	if !gitserver.IsRevisionNotFound(errors.Cause(err)) {
		t.Fatalf("searching non-existent rev expected to fail with RevisionNotFoundError got: %v", err)
	}
}

func TestSearchFilesInReposStream(t *testing.T) {
	mockSearchFilesInRepo = func(ctx context.Context, repo *types.RepoName, gitserverRepo api.RepoName, rev string, info *search.TextPatternInfo, fetchTimeout time.Duration) (matches []*FileMatchResolver, limitHit bool, err error) {
		repoName := repo.Name
		switch repoName {
		case "foo/one":
			return []*FileMatchResolver{
				{
					uri: "git://" + string(repoName) + "?" + rev + "#" + "main.go",
				},
			}, false, nil
		case "foo/two":
			return []*FileMatchResolver{
				{
					uri: "git://" + string(repoName) + "?" + rev + "#" + "main.go",
				},
			}, false, nil
		case "foo/three":
			return []*FileMatchResolver{
				{
					uri: "git://" + string(repoName) + "?" + rev + "#" + "main.go",
				},
			}, false, nil
		default:
			return nil, false, errors.New("Unexpected repo")
		}
	}
	defer func() { mockSearchFilesInRepo = nil }()

	zoekt := &searchbackend.Zoekt{Client: &searchbackend.FakeSearcher{}}

	q, err := query.ParseLiteral("foo")
	if err != nil {
		t.Fatal(err)
	}
	args := &search.TextParameters{
		PatternInfo: &search.TextPatternInfo{
			FileMatchLimit: defaultMaxSearchResults,
			Pattern:        "foo",
		},
		RepoPromise:  (&search.Promise{}).Resolve(makeRepositoryRevisions("foo/one", "foo/two", "foo/three")),
		Query:        q,
		Zoekt:        zoekt,
		SearcherURLs: endpoint.Static("test"),
	}

	results, _, err := searchFilesInReposBatch(context.Background(), args)
	if err != nil {
		t.Fatal(err)
	}

	if len(results) != 3 {
		t.Errorf("expected three results, got %d", len(results))
	}
}

func assertReposStatus(t *testing.T, repoNames map[api.RepoID]string, got search.RepoStatusMap, want map[string]search.RepoStatus) {
	t.Helper()
	gotM := map[string]search.RepoStatus{}
	got.Iterate(func(id api.RepoID, mask search.RepoStatus) {
		name := repoNames[id]
		if name == "" {
			name = fmt.Sprintf("UNKNOWNREPO{ID=%d}", id)
		}
		gotM[name] = mask
	})
	if diff := cmp.Diff(want, gotM); diff != "" {
		t.Errorf("RepoStatusMap mismatch (-want +got):\n%s", diff)
	}
}

func mkStatusMap(m map[string]search.RepoStatus) search.RepoStatusMap {
	var rsm search.RepoStatusMap
	for name, status := range m {
		rsm.Update(mkRepos(name)[0].ID, status)
	}
	return rsm
}

func TestSearchFilesInRepos_multipleRevsPerRepo(t *testing.T) {
	mockSearchFilesInRepo = func(ctx context.Context, repo *types.RepoName, gitserverRepo api.RepoName, rev string, info *search.TextPatternInfo, fetchTimeout time.Duration) (matches []*FileMatchResolver, limitHit bool, err error) {
		repoName := repo.Name
		switch repoName {
		case "foo":
			return []*FileMatchResolver{
				{
					uri: "git://" + string(repoName) + "?" + rev + "#" + "main.go",
				},
			}, false, nil
		default:
			panic("unexpected repo")
		}
	}
	defer func() { mockSearchFilesInRepo = nil }()

	trueVal := true
	conf.Mock(&conf.Unified{SiteConfiguration: schema.SiteConfiguration{
		ExperimentalFeatures: &schema.ExperimentalFeatures{SearchMultipleRevisionsPerRepository: &trueVal},
	}})
	defer conf.Mock(nil)

	zoekt := &searchbackend.Zoekt{Client: &searchbackend.FakeSearcher{}}

	q, err := query.ParseLiteral("foo")
	if err != nil {
		t.Fatal(err)
	}
	args := &search.TextParameters{
		PatternInfo: &search.TextPatternInfo{
			FileMatchLimit: defaultMaxSearchResults,
			Pattern:        "foo",
		},
		RepoPromise:  (&search.Promise{}).Resolve(makeRepositoryRevisions("foo@master:mybranch:*refs/heads/")),
		Query:        q,
		Zoekt:        zoekt,
		SearcherURLs: endpoint.Static("test"),
	}
	repos, _ := getRepos(context.Background(), args.RepoPromise)
	repos[0].ListRefs = func(context.Context, api.RepoName) ([]git.Ref, error) {
		return []git.Ref{{Name: "refs/heads/branch3"}, {Name: "refs/heads/branch4"}}, nil
	}
	results, _, err := searchFilesInReposBatch(context.Background(), args)
	if err != nil {
		t.Fatal(err)
	}

	resultURIs := make([]string, len(results))
	for i, result := range results {
		resultURIs[i] = result.uri
	}
	sort.Strings(resultURIs)

	wantResultURIs := []string{
		"git://foo?branch3#main.go",
		"git://foo?branch4#main.go",
		"git://foo?master#main.go",
		"git://foo?mybranch#main.go",
	}
	if !reflect.DeepEqual(resultURIs, wantResultURIs) {
		t.Errorf("got %v, want %v", resultURIs, wantResultURIs)
	}
}

func TestRepoShouldBeSearched(t *testing.T) {
	searcher.MockSearch = func(ctx context.Context, repo api.RepoName, commit api.CommitID, p *search.TextPatternInfo, fetchTimeout time.Duration) (matches []*protocol.FileMatch, limitHit bool, err error) {
		repoName := repo
		switch repoName {
		case "foo/one":
			return []*protocol.FileMatch{{Path: "main.go"}}, false, nil
		case "foo/no-filematch":
			return []*protocol.FileMatch{}, false, nil
		default:
			return nil, false, errors.New("Unexpected repo")
		}
	}
	defer func() { searcher.MockSearch = nil }()
	info := &search.TextPatternInfo{
		FileMatchLimit:               defaultMaxSearchResults,
		Pattern:                      "foo",
		FilePatternsReposMustInclude: []string{"main"},
	}

	shouldBeSearched, err := repoShouldBeSearched(context.Background(), nil, info, "foo/one", "1a2b3c", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	if !shouldBeSearched {
		t.Errorf("expected repo to be searched, got shouldn't be searched")
	}

	shouldBeSearched, err = repoShouldBeSearched(context.Background(), nil, info, "foo/no-filematch", "1a2b3c", time.Minute)
	if err != nil {
		t.Fatal(err)
	}
	if shouldBeSearched {
		t.Errorf("expected repo to not be searched, got should be searched")
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

func mkRepos(names ...string) []*types.RepoName {
	var repos []*types.RepoName
	for _, name := range names {
		sum := md5.Sum([]byte(name))
		id := api.RepoID(binary.BigEndian.Uint64(sum[:]))
		if id < 0 {
			id = -(id / 2)
		}
		if id == 0 {
			id++
		}
		repos = append(repos, &types.RepoName{ID: id, Name: api.RepoName(name)})
	}
	return repos
}

func TestLimitSearcherRepos(t *testing.T) {
	repoRevs := func(repoRevs ...string) []*search.RepositoryRevisions {
		var result []*search.RepositoryRevisions
		for _, repoRev := range repoRevs {
			split := strings.Split(repoRev, "@")
			repo, rev := split[0], split[1]

			found := false
			for _, existing := range result {
				if string(existing.Repo.Name) == repo {
					existing.Revs = append(existing.Revs, search.RevisionSpecifier{RevSpec: rev})
					found = true
					break
				}
			}
			if found {
				continue
			}
			result = append(result, &search.RepositoryRevisions{
				Repo: mkRepos(repo)[0],
				Revs: []search.RevisionSpecifier{{RevSpec: rev}},
			})
		}
		return result
	}

	tests := []struct {
		name        string
		limit       int
		input       []*search.RepositoryRevisions
		want        []*search.RepositoryRevisions
		wantLimited []*types.RepoName
	}{
		{
			name:        "non_limited",
			limit:       5,
			input:       repoRevs("a@1", "a@2", "b@1", "c@1"),
			want:        repoRevs("a@1", "a@2", "b@1", "c@1"),
			wantLimited: nil,
		},
		{
			name:        "limited",
			limit:       5,
			input:       repoRevs("a@1", "b@1", "c@1", "d@1", "e@1", "f@1", "g@1"),
			want:        repoRevs("a@1", "b@1", "c@1", "d@1", "e@1"),
			wantLimited: mkRepos("f", "g"),
		},
		{
			name:        "rev_limited",
			limit:       6,
			input:       repoRevs("a@1", "a@2", "b@1", "c@1", "d@1", "e@1", "f@1", "g@1"),
			want:        repoRevs("a@1", "a@2", "b@1", "c@1", "d@1", "e@1"),
			wantLimited: mkRepos("f", "g"),
		},
		{
			name:        "rev_limited_duplication",
			limit:       6,
			input:       repoRevs("a@1", "a@2", "b@1", "c@1", "d@1", "e@1", "f@1", "f@2", "g@1"),
			want:        repoRevs("a@1", "a@2", "b@1", "c@1", "d@1", "e@1"),
			wantLimited: mkRepos("f", "g"),
		},
	}
	for _, tst := range tests {
		t.Run(tst.name, func(t *testing.T) {
			got, gotLimited := limitSearcherRepos(tst.input, tst.limit)
			if !reflect.DeepEqual(got, tst.want) {
				t.Errorf("got %+v want %+v", got, tst.limit)
			}
			if !reflect.DeepEqual(gotLimited, tst.wantLimited) {
				t.Errorf("got limited %+v want %+v", gotLimited, tst.wantLimited)
			}
		})
	}
}
