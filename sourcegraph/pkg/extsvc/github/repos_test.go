package github

import (
	"bytes"
	"context"
	"fmt"
	"io/ioutil"
	"net/http"
	"net/url"
	"reflect"
	"strings"
	"testing"

	"github.com/sourcegraph/sourcegraph/pkg/ratelimit"
	"github.com/sourcegraph/sourcegraph/pkg/rcache"
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
}

func (s *mockHTTPResponseBody) RoundTrip(req *http.Request) (*http.Response, error) {
	s.count++
	return &http.Response{
		Request:    req,
		StatusCode: http.StatusOK,
		Body:       ioutil.NopCloser(strings.NewReader(s.responseBody)),
	}, nil
}

type mockHTTPEmptyResponse struct {
	statusCode int
}

func (s mockHTTPEmptyResponse) RoundTrip(req *http.Request) (*http.Response, error) {
	return &http.Response{
		Request:    req,
		StatusCode: s.statusCode,
		Body:       ioutil.NopCloser(bytes.NewReader(nil)),
	}, nil
}

func newTestClient(t *testing.T) *Client {
	rcache.SetupForTest(t)
	return &Client{
		apiURL:     &url.URL{Scheme: "https", Host: "example.com", Path: "/"},
		httpClient: &http.Client{},
		RateLimit:  &ratelimit.Monitor{},
		repoCache:  rcache.NewWithTTL("__test__gh_repo", 1000),
	}
}

// TestClient_GetRepository tests the behavior of GetRepository.
func TestClient_GetRepository(t *testing.T) {
	mock := mockHTTPResponseBody{
		responseBody: `
{
	"node_id": "i",
	"full_name": "o/r",
	"description": "d",
	"html_url": "https://github.example.com/o/r",
	"fork": true
}
`}
	c := newTestClient(t)
	c.httpClient.Transport = &mock

	want := Repository{
		ID:            "i",
		NameWithOwner: "o/r",
		Description:   "d",
		URL:           "https://github.example.com/o/r",
		IsFork:        true,
	}

	repo, err := c.GetRepository(context.Background(), "owner", "repo")
	if err != nil {
		t.Fatal(err)
	}
	if repo == nil {
		t.Error("repo == nil")
	}
	if mock.count != 1 {
		t.Errorf("mock.count == %d, expected to miss cache once", mock.count)
	}
	if !reflect.DeepEqual(repo, &want) {
		t.Errorf("got repository %+v, want %+v", repo, &want)
	}

	// Test that repo is cached (and therefore NOT fetched) from client on second request.
	repo, err = c.GetRepository(context.Background(), "owner", "repo")
	if err != nil {
		t.Fatal(err)
	}
	if repo == nil {
		t.Error("repo == nil")
	}
	if mock.count != 1 {
		t.Errorf("mock.count == %d, expected to hit cache", mock.count)
	}
	if !reflect.DeepEqual(repo, &want) {
		t.Errorf("got repository %+v, want %+v", repo, &want)
	}
}

// TestClient_GetRepository_nonexistent tests the behavior of GetRepository when called
// on a repository that does not exist.
func TestClient_GetRepository_nonexistent(t *testing.T) {
	mock := mockHTTPEmptyResponse{http.StatusNotFound}
	c := newTestClient(t)
	c.httpClient.Transport = &mock

	repo, err := c.GetRepository(context.Background(), "owner", "repo")
	if !IsNotFound(err) {
		t.Errorf("got err == %v, want IsNotFound(err) == true", err)
	}
	if repo != nil {
		t.Error("repo != nil")
	}
}

// TestClient_GetRepositoryByNodeID tests the behavior of GetRepositoryByNodeID.
func TestClient_GetRepositoryByNodeID(t *testing.T) {
	mock := mockHTTPResponseBody{
		responseBody: `
{
	"data": {
		"node": {
			"id": "i",
			"nameWithOwner": "o/r",
			"description": "d",
			"url": "https://github.example.com/o/r",
			"isFork": true
		}
	}
}
`}
	c := newTestClient(t)
	c.httpClient.Transport = &mock

	want := Repository{
		ID:            "i",
		NameWithOwner: "o/r",
		Description:   "d",
		URL:           "https://github.example.com/o/r",
		IsFork:        true,
	}

	repo, err := c.GetRepositoryByNodeID(context.Background(), "i")
	if err != nil {
		t.Fatal(err)
	}
	if repo == nil {
		t.Error("repo == nil")
	}
	if mock.count != 1 {
		t.Errorf("mock.count == %d, expected to miss cache once", mock.count)
	}
	if !reflect.DeepEqual(repo, &want) {
		t.Errorf("got repository %+v, want %+v", repo, &want)
	}

	// Test that repo is cached (and therefore NOT fetched) from client on second request.
	repo, err = c.GetRepositoryByNodeID(context.Background(), "i")
	if err != nil {
		t.Fatal(err)
	}
	if repo == nil {
		t.Error("repo == nil")
	}
	if mock.count != 1 {
		t.Errorf("mock.count == %d, expected to hit cache", mock.count)
	}
	if !reflect.DeepEqual(repo, &want) {
		t.Errorf("got repository %+v, want %+v", repo, &want)
	}
}

// TestClient_GetRepositoryByNodeID_nonexistent tests the behavior of GetRepositoryByNodeID when called
// on a repository that does not exist.
func TestClient_GetRepositoryByNodeID_nonexistent(t *testing.T) {
	mock := mockHTTPResponseBody{
		responseBody: `
{
	"data": {
		"node": null
	}
}
`}
	c := newTestClient(t)
	c.httpClient.Transport = &mock

	repo, err := c.GetRepositoryByNodeID(context.Background(), "i")
	if !IsNotFound(err) {
		t.Errorf("got err == %v, want IsNotFound(err) == true", err)
	}
	if repo != nil {
		t.Error("repo != nil")
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
`}
	c := newTestClient(t)
	c.httpClient.Transport = &mock

	wantRepos := []*Repository{
		&Repository{
			ID:            "i",
			NameWithOwner: "o/r",
			Description:   "d",
			URL:           "https://github.example.com/o/r",
			IsFork:        true,
		},
		&Repository{
			ID:            "j",
			NameWithOwner: "a/b",
			Description:   "c",
			URL:           "https://github.example.com/a/b",
			IsFork:        false,
		},
	}

	repos, _, _, err :=
		c.ListRepositoriesForSearch(context.Background(), "org:sourcegraph", 1)

	if err != nil {
		t.Fatal(err)
	}
	if !repoListsAreEqual(repos, wantRepos) {
		t.Errorf("got repositories:\n%s\nwant:\n%s", stringForRepoList(repos), stringForRepoList(wantRepos))
	}
}
