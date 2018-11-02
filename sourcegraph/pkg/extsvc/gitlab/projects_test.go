package gitlab

import (
	"bytes"
	"context"
	"io/ioutil"
	"net/http"
	"net/url"
	"reflect"
	"strings"
	"testing"

	"github.com/sourcegraph/sourcegraph/pkg/ratelimit"
	"github.com/sourcegraph/sourcegraph/pkg/rcache"
)

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
		baseURL:    &url.URL{Scheme: "https", Host: "example.com", Path: "/"},
		httpClient: &http.Client{},
		RateLimit:  &ratelimit.Monitor{},
		projCache:  rcache.NewWithTTL("__test__gl_proj", 1000),
	}
}

// TestClient_GetProject tests the behavior of GetProject.
func TestClient_GetProject(t *testing.T) {
	mock := mockHTTPResponseBody{
		responseBody: `
{
	"id": 1,
	"path_with_namespace": "n1/n2/r",
	"description": "d",
	"web_url": "https://gitlab.example.com/n1/n2/r",
	"http_url_to_repo": "https://gitlab.example.com/n1/n2/r.git",
	"ssh_url_to_repo": "git@gitlab.example.com:n1/n2/r.git"
}
`}
	c := newTestClient(t)
	c.httpClient.Transport = &mock

	want := Project{
		ProjectCommon: ProjectCommon{
			ID:                1,
			PathWithNamespace: "n1/n2/r",
			Description:       "d",
			WebURL:            "https://gitlab.example.com/n1/n2/r",
			HTTPURLToRepo:     "https://gitlab.example.com/n1/n2/r.git",
			SSHURLToRepo:      "git@gitlab.example.com:n1/n2/r.git",
		},
	}

	proj, err := c.GetProject(context.Background(), 0, "n1/n2/r")
	if err != nil {
		t.Fatal(err)
	}
	if proj == nil {
		t.Error("proj == nil")
	}
	if mock.count != 1 {
		t.Errorf("mock.count == %d, expected to miss cache once", mock.count)
	}
	if !reflect.DeepEqual(proj, &want) {
		t.Errorf("got project %+v, want %+v", proj, &want)
	}

	// Test that proj is cached (and therefore NOT fetched) from client on second request.
	proj, err = c.GetProject(context.Background(), 0, "n1/n2/r")
	if err != nil {
		t.Fatal(err)
	}
	if proj == nil {
		t.Error("proj == nil")
	}
	if mock.count != 1 {
		t.Errorf("mock.count == %d, expected to hit cache", mock.count)
	}
	if !reflect.DeepEqual(proj, &want) {
		t.Errorf("got project %+v, want %+v", proj, &want)
	}
}

// TestClient_GetProject_nonexistent tests the behavior of GetProject when called
// on a project that does not exist.
func TestClient_GetProject_nonexistent(t *testing.T) {
	mock := mockHTTPEmptyResponse{http.StatusNotFound}
	c := newTestClient(t)
	c.httpClient.Transport = &mock

	proj, err := c.GetProject(context.Background(), 0, "doesnt/exist")
	if !IsNotFound(err) {
		t.Errorf("got err == %v, want IsNotFound(err) == true", err)
	}
	if proj != nil {
		t.Error("proj != nil")
	}
}
