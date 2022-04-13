package github

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"sort"
	"strings"
	"testing"

	"github.com/sourcegraph/sourcegraph/internal/extsvc/auth"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
	"github.com/sourcegraph/sourcegraph/internal/rcache"
)

func TestSplitRepositoryNameWithOwner(t *testing.T) {
	owner, name, err := SplitRepositoryNameWithOwner("a/b")
	if err != nil {
		t.Fatal(err)
	}
	if want := "a"; owner != want {
		t.Errorf("got owner %q, want %q", owner, want)
	}
	if want := "b"; name != want {
		t.Errorf("got name %q, want %q", name, want)
	}
}

type mockHTTPResponseBody struct {
	count        int
	responseBody string
	status       int
}

func (s *mockHTTPResponseBody) Do(req *http.Request) (*http.Response, error) {
	s.count++
	status := s.status
	if status == 0 {
		status = http.StatusOK
	}
	return &http.Response{
		Request:    req,
		StatusCode: status,
		Body:       io.NopCloser(strings.NewReader(s.responseBody)),
	}, nil
}

type mockHTTPEmptyResponse struct {
	statusCode int
}

func (s mockHTTPEmptyResponse) Do(req *http.Request) (*http.Response, error) {
	return &http.Response{
		Request:    req,
		StatusCode: s.statusCode,
		Body:       io.NopCloser(bytes.NewReader(nil)),
	}, nil
}

func newTestClient(t *testing.T, cli httpcli.Doer) *V3Client {
	return newTestClientWithAuthenticator(t, nil, cli)
}

func newTestClientWithAuthenticator(t *testing.T, auth auth.Authenticator, cli httpcli.Doer) *V3Client {
	rcache.SetupForTest(t)

	apiURL := &url.URL{Scheme: "https", Host: "example.com", Path: "/"}
	return NewV3Client("Test", apiURL, auth, cli)
}

// TestClient_GetRepository tests the behavior of GetRepository.
func TestClient_GetRepository(t *testing.T) {
	cli, save := newV3TestClient(t, "GetRepository")
	defer save()

	t.Run("first run", func(t *testing.T) {
		ctx := context.Background()
		repo, err := cli.GetRepository(ctx, "sourcegraph-vcr-repos", "private-org-repo-1")
		if err != nil {
			t.Fatal(err)
		}
		if repo == nil {
			t.Fatal("expected repo, got nil")
		}
	})
	t.Run("second run", func(t *testing.T) {
		ctx := context.Background()
		repo, err := cli.GetRepository(ctx, "sourcegraph-vcr-repos", "private-org-repo-1")
		if err != nil {
			t.Fatal(err)
		}
		if repo == nil {
			t.Fatal("expected repo, got nil")
		}
	})
}

// TestClient_GetRepository_nonexistent tests the behavior of GetRepository when called
// on a repository that does not exist.
func TestClient_GetRepository_nonexistent(t *testing.T) {
	mock := mockHTTPEmptyResponse{http.StatusNotFound}
	c := newTestClient(t, &mock)

	repo, err := c.GetRepository(context.Background(), "owner", "repo")
	if !IsNotFound(err) {
		t.Errorf("got err == %v, want IsNotFound(err) == true", err)
	}
	if err != ErrRepoNotFound {
		t.Errorf("got err == %q, want ErrNotFound", err)
	}
	if repo != nil {
		t.Error("repo != nil")
	}
}

func TestClient_ListOrgRepositories(t *testing.T) {
	mock := mockHTTPResponseBody{
		responseBody: `[
  {
    "node_id": "i",
    "full_name": "o/r",
    "description": "d",
    "html_url": "https://github.example.com/o/r",
    "fork": true
  },
  {
    "node_id": "j",
    "full_name": "o/b",
    "description": "c",
    "html_url": "https://github.example.com/o/b",
    "fork": false
  }
]
`,
	}

	c := newTestClient(t, &mock)
	wantRepos := []*Repository{
		{
			ID:            "i",
			NameWithOwner: "o/r",
			Description:   "d",
			URL:           "https://github.example.com/o/r",
			IsFork:        true,
		},
		{
			ID:            "j",
			NameWithOwner: "o/b",
			Description:   "c",
			URL:           "https://github.example.com/o/b",
			IsFork:        false,
		},
	}

	repos, hasNextPage, _, err := c.ListOrgRepositories(context.Background(), "o", 1, "all")
	if err != nil {
		t.Fatal(err)
	}
	if !repoListsAreEqual(repos, wantRepos) {
		t.Errorf("got repositories:\n%s\nwant:\n%s", stringForRepoList(repos), stringForRepoList(wantRepos))
	}
	if !hasNextPage {
		t.Errorf("got hasNextPage: false want: true")
	}
}

func stringForRepoList(repos []*Repository) string {
	repoStrings := []string{}
	for _, repo := range repos {
		repoStrings = append(repoStrings, fmt.Sprintf("%#v", repo))
	}
	return "{\n" + strings.Join(repoStrings, ",\n") + "}\n"
}

func repoListsAreEqual(a []*Repository, b []*Repository) bool {
	if len(a) != len(b) {
		return false
	}
	for i := 0; i < len(a); i++ {
		if *a[i] != *b[i] {
			return false
		}
	}
	return true
}

func TestClient_ListRepositoriesForSearch(t *testing.T) {
	mock := mockHTTPResponseBody{
		responseBody: `
{
  "total_count": 2,
  "incomplete_results": false,
  "items": [
    {
      "node_id": "i",
      "full_name": "o/r",
      "description": "d",
      "html_url": "https://github.example.com/o/r",
      "fork": true
    },
    {
      "node_id": "j",
      "full_name": "a/b",
      "description": "c",
      "html_url": "https://github.example.com/a/b",
      "fork": false
    }
  ]
}
`,
	}
	c := newTestClient(t, &mock)

	wantRepos := []*Repository{
		{
			ID:            "i",
			NameWithOwner: "o/r",
			Description:   "d",
			URL:           "https://github.example.com/o/r",
			IsFork:        true,
		},
		{
			ID:            "j",
			NameWithOwner: "a/b",
			Description:   "c",
			URL:           "https://github.example.com/a/b",
			IsFork:        false,
		},
	}

	reposPage, err := c.ListRepositoriesForSearch(context.Background(), "org:sourcegraph", 1)
	if err != nil {
		t.Fatal(err)
	}
	if !repoListsAreEqual(reposPage.Repos, wantRepos) {
		t.Errorf("got repositories:\n%s\nwant:\n%s", stringForRepoList(reposPage.Repos), stringForRepoList(wantRepos))
	}
}

func TestClient_ListRepositoriesForSearch_incomplete(t *testing.T) {
	mock := mockHTTPResponseBody{
		responseBody: `
{
  "total_count": 2,
  "incomplete_results": true,
  "items": [
    {
      "node_id": "i",
      "full_name": "o/r",
      "description": "d",
      "html_url": "https://github.example.com/o/r",
      "fork": true
    },
    {
      "node_id": "j",
      "full_name": "a/b",
      "description": "c",
      "html_url": "https://github.example.com/a/b",
      "fork": false
    }
  ]
}
`,
	}
	c := newTestClient(t, &mock)

	// If we have incomplete results we want to fail. Our syncer requires all
	// repositories to be returned, otherwise it will delete the missing
	// repositories.
	_, err := c.ListRepositoriesForSearch(context.Background(), "org:sourcegraph", 1)

	if have, want := err, ErrIncompleteResults; want != have {
		t.Errorf("\nhave: %s\nwant: %s", have, want)
	}
}

func TestClient_buildGetRepositoriesBatchQuery(t *testing.T) {
	repos := []string{
		"sourcegraph/grapher-tutorial",
		"sourcegraph/clojure-grapher",
		"sourcegraph/programming-challenge",
		"sourcegraph/annotate",
		"sourcegraph/sourcegraph-sublime-old",
		"sourcegraph/makex",
		"sourcegraph/pydep",
		"sourcegraph/vcsstore",
		"sourcegraph/contains.dot",
	}

	wantIncluded := `
repo0: repository(owner: "sourcegraph", name: "grapher-tutorial") { ... on Repository { ...RepositoryFields } }
repo1: repository(owner: "sourcegraph", name: "clojure-grapher") { ... on Repository { ...RepositoryFields } }
repo2: repository(owner: "sourcegraph", name: "programming-challenge") { ... on Repository { ...RepositoryFields } }
repo3: repository(owner: "sourcegraph", name: "annotate") { ... on Repository { ...RepositoryFields } }
repo4: repository(owner: "sourcegraph", name: "sourcegraph-sublime-old") { ... on Repository { ...RepositoryFields } }
repo5: repository(owner: "sourcegraph", name: "makex") { ... on Repository { ...RepositoryFields } }
repo6: repository(owner: "sourcegraph", name: "pydep") { ... on Repository { ...RepositoryFields } }
repo7: repository(owner: "sourcegraph", name: "vcsstore") { ... on Repository { ...RepositoryFields } }
repo8: repository(owner: "sourcegraph", name: "contains.dot") { ... on Repository { ...RepositoryFields } }`

	mock := mockHTTPResponseBody{responseBody: ""}
	apiURL := &url.URL{Scheme: "https", Host: "example.com", Path: "/"}
	c := NewV4Client("Test", apiURL, nil, &mock)
	query, err := c.buildGetReposBatchQuery(context.Background(), repos)
	if err != nil {
		t.Fatal(err)
	}

	if !strings.Contains(query, wantIncluded) {
		t.Fatalf("query does not contain repository query. query=%q, want=%q", query, wantIncluded)
	}
}

func TestClient_GetReposByNameWithOwner(t *testing.T) {
	namesWithOwners := []string{
		"sourcegraph/grapher-tutorial",
		"sourcegraph/clojure-grapher",
	}

	grapherTutorialRepo := &Repository{
		ID:               "MDEwOlJlcG9zaXRvcnkxNDYwMTc5OA==",
		DatabaseID:       14601798,
		NameWithOwner:    "sourcegraph/grapher-tutorial",
		Description:      "monkey language",
		URL:              "https://github.com/sourcegraph/grapher-tutorial",
		IsPrivate:        true,
		IsFork:           false,
		IsArchived:       true,
		IsLocked:         true,
		ViewerPermission: "ADMIN",
		Visibility:       "internal",
	}

	clojureGrapherRepo := &Repository{
		ID:               "MDEwOlJlcG9zaXRvcnkxNTc1NjkwOA==",
		DatabaseID:       15756908,
		NameWithOwner:    "sourcegraph/clojure-grapher",
		Description:      "clojure grapher",
		URL:              "https://github.com/sourcegraph/clojure-grapher",
		IsPrivate:        true,
		IsFork:           false,
		IsArchived:       true,
		IsDisabled:       true,
		ViewerPermission: "ADMIN",
		Visibility:       "private",
	}

	testCases := []struct {
		name             string
		mockResponseBody string
		wantRepos        []*Repository
		err              string
	}{

		{
			name: "found",
			mockResponseBody: `
{
  "data": {
    "repo_sourcegraph_grapher_tutorial": {
      "id": "MDEwOlJlcG9zaXRvcnkxNDYwMTc5OA==",
      "databaseId": 14601798,
      "nameWithOwner": "sourcegraph/grapher-tutorial",
      "description": "monkey language",
      "url": "https://github.com/sourcegraph/grapher-tutorial",
      "isPrivate": true,
      "isFork": false,
      "isArchived": true,
      "isLocked": true,
      "viewerPermission": "ADMIN",
      "visibility": "internal"
    },
    "repo_sourcegraph_clojure_grapher": {
      "id": "MDEwOlJlcG9zaXRvcnkxNTc1NjkwOA==",
	  "databaseId": 15756908,
      "nameWithOwner": "sourcegraph/clojure-grapher",
      "description": "clojure grapher",
      "url": "https://github.com/sourcegraph/clojure-grapher",
      "isPrivate": true,
      "isFork": false,
      "isArchived": true,
      "isDisabled": true,
      "viewerPermission": "ADMIN",
      "visibility": "private"
    }
  }
}
`,
			wantRepos: []*Repository{grapherTutorialRepo, clojureGrapherRepo},
		},
		{
			name: "not found",
			mockResponseBody: `
{
  "data": {
    "repo_sourcegraph_grapher_tutorial": {
      "id": "MDEwOlJlcG9zaXRvcnkxNDYwMTc5OA==",
      "databaseId": 14601798,
      "nameWithOwner": "sourcegraph/grapher-tutorial",
      "description": "monkey language",
      "url": "https://github.com/sourcegraph/grapher-tutorial",
      "isPrivate": true,
      "isFork": false,
      "isArchived": true,
      "isLocked": true,
      "viewerPermission": "ADMIN",
      "visibility": "internal"
    },
    "repo_sourcegraph_clojure_grapher": null
  },
  "errors": [
    {
      "type": "NOT_FOUND",
      "path": [
        "repo_sourcegraph_clojure_grapher"
      ],
      "locations": [
        {
          "line": 13,
          "column": 3
        }
      ],
      "message": "Could not resolve to a Repository with the name 'clojure-grapher'."
    }
  ]
}
`,
			wantRepos: []*Repository{grapherTutorialRepo},
		},
		{
			name: "error",
			mockResponseBody: `
{
  "errors": [
    {
      "path": [
        "fragment RepositoryFields",
        "foobar"
      ],
      "extensions": {
        "code": "undefinedField",
        "typeName": "Repository",
        "fieldName": "foobar"
      },
      "locations": [
        {
          "line": 10,
          "column": 3
        }
      ],
      "message": "Field 'foobar' doesn't exist on type 'Repository'"
    }
  ]
}
`,
			wantRepos: []*Repository{},
			err:       "error in GraphQL response: Field 'foobar' doesn't exist on type 'Repository'",
		},
	}

	for _, tc := range testCases {
		t.Run(tc.name, func(t *testing.T) {
			mock := mockHTTPResponseBody{responseBody: tc.mockResponseBody}
			apiURL := &url.URL{Scheme: "https", Host: "example.com", Path: "/"}
			c := NewV4Client("Test", apiURL, nil, &mock)

			repos, err := c.GetReposByNameWithOwner(context.Background(), namesWithOwners...)
			if have, want := fmt.Sprint(err), fmt.Sprint(tc.err); tc.err != "" && have != want {
				t.Errorf("error:\nhave: %v\nwant: %v", have, want)
			}

			if want, have := len(tc.wantRepos), len(repos); want != have {
				t.Errorf("wrong number of repos. want=%d, have=%d", want, have)
			}

			newSortFunc := func(s []*Repository) func(int, int) bool {
				return func(i, j int) bool { return s[i].ID < s[j].ID }
			}

			sort.Slice(tc.wantRepos, newSortFunc(tc.wantRepos))
			sort.Slice(repos, newSortFunc(repos))

			if !repoListsAreEqual(repos, tc.wantRepos) {
				t.Errorf("got repositories:\n%s\nwant:\n%s", stringForRepoList(repos), stringForRepoList(tc.wantRepos))
			}
		})
	}
}
