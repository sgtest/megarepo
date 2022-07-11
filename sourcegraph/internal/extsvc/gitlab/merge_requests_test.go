package gitlab

import (
	"context"
	"net/http"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/stretchr/testify/assert"

	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func TestWIP(t *testing.T) {
	t.Run("SetWIP", func(t *testing.T) {
		tests := []struct{ title, want string }{
			{title: "My perfect changeset", want: "WIP: My perfect changeset"},
			{title: "WIP: My perfect changeset", want: "WIP: My perfect changeset"},
			{title: "Draft: My perfect changeset", want: "Draft: My perfect changeset"},
		}
		for _, tc := range tests {
			if have, want := SetWIP(tc.title), tc.want; have != want {
				t.Errorf("incorrect title generated from SetWIP: have=%q want=%q", have, want)
			}
		}
	})
	t.Run("UnsetWIP", func(t *testing.T) {
		tests := []struct{ title, want string }{
			{title: "WIP: My perfect changeset", want: "My perfect changeset"},
			{title: "Draft: My perfect changeset", want: "My perfect changeset"},
			{title: "My perfect changeset", want: "My perfect changeset"},
		}
		for _, tc := range tests {
			if have, want := UnsetWIP(tc.title), tc.want; have != want {
				t.Errorf("incorrect title generated from UnsetWIP: have=%q want=%q", have, want)
			}
		}
	})
}

func TestCreateMergeRequest(t *testing.T) {
	ctx := context.Background()
	project := &Project{}

	t.Run("merge request already exists", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPEmptyResponse{http.StatusConflict}

		mr, err := client.CreateMergeRequest(ctx, project, CreateMergeRequestOpts{})
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if want := ErrMergeRequestAlreadyExists; want != err {
			t.Errorf("unexpected error: have %+v; want %+v", err, want)
		}
	})

	t.Run("non-conflict error", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPEmptyResponse{http.StatusInternalServerError}

		mr, err := client.CreateMergeRequest(ctx, project, CreateMergeRequestOpts{})
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if err == nil {
			t.Error("unexpected nil error")
		} else if err == ErrMergeRequestAlreadyExists {
			t.Errorf("unexpected error value: %+v", err)
		}
	})

	t.Run("malformed response", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `this is not valid JSON`,
		}

		mr, err := client.CreateMergeRequest(ctx, project, CreateMergeRequestOpts{})
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if err == nil {
			t.Error("unexpected nil error")
		} else if err == ErrMergeRequestAlreadyExists {
			t.Errorf("unexpected error value: %+v", err)
		}
	})

	t.Run("invalid response", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `{"id":"the id cannot be a string"}`,
		}

		mr, err := client.CreateMergeRequest(ctx, project, CreateMergeRequestOpts{})
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if err == nil {
			t.Error("unexpected nil error")
		} else if err == ErrMergeRequestAlreadyExists {
			t.Errorf("unexpected error value: %+v", err)
		}
	})

	t.Run("success", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `{"iid":42}`,
		}

		mr, err := client.CreateMergeRequest(ctx, project, CreateMergeRequestOpts{})
		if mr == nil {
			t.Error("unexpected nil merge request")
		} else if diff := cmp.Diff(mr, &MergeRequest{IID: 42}); diff != "" {
			t.Errorf("unexpected merge request: %s", diff)
		}
		if err != nil {
			t.Errorf("unexpected non-nil error: %+v", err)
		}
	})
}

func TestCreateMergeRequest_Archived(t *testing.T) {
	ctx := context.Background()
	client := createTestClient(t)

	project := &Project{ProjectCommon: ProjectCommon{ID: 37741563}}
	opts := CreateMergeRequestOpts{
		SourceBranch: "branch-without-pr",
		TargetBranch: "main",
		Title:        "This MR should never be created",
		Description:  "This merge request was created by a test against an archived repository, and should therefore not exist.",
	}
	mr, err := client.CreateMergeRequest(ctx, project, opts)
	assert.Nil(t, mr)
	assert.Error(t, err)
	assert.True(t, errcode.IsArchived(err))
}

func TestGetMergeRequest(t *testing.T) {
	ctx := context.Background()
	project := &Project{}

	t.Run("error status code", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPEmptyResponse{http.StatusNotFound}

		mr, err := client.GetMergeRequest(ctx, project, 1)
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if err == nil {
			t.Error("unexpected nil error")
		}
	})

	t.Run("malformed response", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `this is not valid JSON`,
		}

		mr, err := client.GetMergeRequest(ctx, project, 1)
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if err == nil {
			t.Error("unexpected nil error")
		}
	})

	t.Run("invalid response", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `{"id":"the id cannot be a string"}`,
		}

		mr, err := client.GetMergeRequest(ctx, project, 1)
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if err == nil {
			t.Error("unexpected nil error")
		}
	})

	t.Run("success", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `{"iid":42}`,
		}

		mr, err := client.GetMergeRequest(ctx, project, 1)
		if mr == nil {
			t.Error("unexpected nil merge request")
		} else if diff := cmp.Diff(mr, &MergeRequest{IID: 42}); diff != "" {
			t.Errorf("unexpected merge request: %s", diff)
		}
		if err != nil {
			t.Errorf("unexpected non-nil error: %+v", err)
		}
	})
}

func TestGetOpenMergeRequestByRefs(t *testing.T) {
	ctx := context.Background()
	project := &Project{}

	t.Run("error status code", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPEmptyResponse{http.StatusNotFound}

		mr, err := client.GetOpenMergeRequestByRefs(ctx, project, "source", "target")
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if err == nil || err == ErrTooManyMergeRequests || err == ErrMergeRequestNotFound {
			t.Errorf("unexpected error: %+v", err)
		}
	})

	t.Run("malformed response", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `this is not valid JSON`,
		}

		mr, err := client.GetOpenMergeRequestByRefs(ctx, project, "source", "target")
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if err == nil || err == ErrTooManyMergeRequests || err == ErrMergeRequestNotFound {
			t.Errorf("unexpected error: %+v", err)
		}
	})

	t.Run("invalid response", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `[{"id":"the id cannot be a string"}]`,
		}

		mr, err := client.GetOpenMergeRequestByRefs(ctx, project, "source", "target")
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if err == nil || err == ErrTooManyMergeRequests || err == ErrMergeRequestNotFound {
			t.Errorf("unexpected error: %+v", err)
		}
	})

	t.Run("zero merge requests", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `[]`,
		}

		mr, err := client.GetOpenMergeRequestByRefs(ctx, project, "source", "target")
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if err != ErrMergeRequestNotFound {
			t.Errorf("unexpected error: %+v", err)
		}
	})

	t.Run("too many merge requests", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `[{"iid":1},{"iid":2}]`,
		}

		mr, err := client.GetOpenMergeRequestByRefs(ctx, project, "source", "target")
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if err != ErrTooManyMergeRequests {
			t.Errorf("unexpected error: %+v", err)
		}
	})

	t.Run("success", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `[{"iid":42}]`,
		}

		// Since this will invoke GetMergeRequest, we need to mock that. (But,
		// on the bright side, that allows us to verify the parameters it's
		// given.)
		want := &MergeRequest{}
		MockGetMergeRequest = func(mc *Client, mctx context.Context, mproject *Project, miid ID) (*MergeRequest, error) {
			if client != mc {
				t.Errorf("unexpected client: have %+v; want %+v", mc, client)
			}
			if ctx != mctx {
				t.Errorf("unexpected context: have %+v; want %+v", mctx, ctx)
			}
			if project != mproject {
				t.Errorf("unexpected project: have %+v; want %+v", mproject, project)
			}
			if want := ID(42); miid != want {
				t.Errorf("unexpected IID: have %d; want %d", miid, want)
			}

			return want, nil
		}
		defer func() { MockGetMergeRequest = nil }()

		mr, err := client.GetOpenMergeRequestByRefs(ctx, project, "source", "target")
		if mr != want {
			t.Errorf("unexpected merge request: have %+v; want %+v", mr, want)
		}
		if err != nil {
			t.Errorf("unexpected non-nil error: %+v", err)
		}
	})
}

func TestUpdateMergeRequest(t *testing.T) {
	ctx := context.Background()
	empty := &MergeRequest{}
	opts := UpdateMergeRequestOpts{}
	project := &Project{}

	t.Run("error status code", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPEmptyResponse{http.StatusNotFound}

		mr, err := client.UpdateMergeRequest(ctx, project, empty, opts)
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if err == nil {
			t.Error("unexpected nil error")
		}
	})

	t.Run("malformed response", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `this is not valid JSON`,
		}

		mr, err := client.UpdateMergeRequest(ctx, project, empty, opts)
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if err == nil {
			t.Error("unexpected nil error")
		}
	})

	t.Run("invalid response", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `{"id":"the id cannot be a string"}`,
		}

		mr, err := client.UpdateMergeRequest(ctx, project, empty, opts)
		if mr != nil {
			t.Errorf("unexpected non-nil merge request: %+v", mr)
		}
		if err == nil {
			t.Error("unexpected nil error")
		}
	})

	t.Run("success", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `{"iid":42}`,
		}

		mr, err := client.UpdateMergeRequest(ctx, project, empty, opts)
		if mr == nil {
			t.Error("unexpected nil merge request")
		} else if diff := cmp.Diff(mr, &MergeRequest{IID: 42}); diff != "" {
			t.Errorf("unexpected merge request: %s", diff)
		}
		if err != nil {
			t.Errorf("unexpected non-nil error: %+v", err)
		}
	})
}

func TestUpdateMergeRequest_Archived(t *testing.T) {
	ctx := context.Background()
	client := createTestClient(t)

	project := &Project{ProjectCommon: ProjectCommon{ID: 37741563}}
	mr := &MergeRequest{IID: 1}
	opts := UpdateMergeRequestOpts{
		Title: "This title should never change",
	}
	mr, err := client.UpdateMergeRequest(ctx, project, mr, opts)
	assert.Nil(t, mr)
	assert.Error(t, err)
	assert.True(t, errcode.IsArchived(err))
}

func TestCreateMergeRequestNote(t *testing.T) {
	ctx := context.Background()
	empty := &MergeRequest{}
	project := &Project{}

	t.Run("error status code", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPEmptyResponse{http.StatusNotFound}

		err := client.CreateMergeRequestNote(ctx, project, empty, "test-comment")
		if err == nil {
			t.Error("unexpected nil error")
		}
	})

	t.Run("malformed response", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `this is not valid JSON`,
		}

		err := client.CreateMergeRequestNote(ctx, project, empty, "test-comment")
		if err == nil {
			t.Error("unexpected nil error")
		}
	})

	t.Run("invalid response", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `{"id":"the id cannot be a string"}`,
		}

		err := client.CreateMergeRequestNote(ctx, project, empty, "test-comment")
		if err == nil {
			t.Error("unexpected nil error")
		}
	})

	t.Run("success", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `{"body":"test-comment"}`,
		}

		err := client.CreateMergeRequestNote(ctx, project, empty, "test-comment")
		if err != nil {
			t.Errorf("unexpected non-nil error: %+v", err)
		}
	})
}

func TestMergeMergeRequest(t *testing.T) {
	ctx := context.Background()
	empty := &MergeRequest{}
	project := &Project{}

	t.Run("error status code", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPEmptyResponse{http.StatusNotFound}

		_, err := client.MergeMergeRequest(ctx, project, empty, false)
		if err == nil {
			t.Error("unexpected nil error")
		}
	})

	t.Run("malformed response", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `this is not valid JSON`,
		}

		_, err := client.MergeMergeRequest(ctx, project, empty, false)
		if err == nil {
			t.Error("unexpected nil error")
		}
	})

	t.Run("invalid response", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `{"id":"the id cannot be a string"}`,
		}

		_, err := client.MergeMergeRequest(ctx, project, empty, false)
		if err == nil {
			t.Error("unexpected nil error")
		}
	})

	t.Run("not mergeable", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPEmptyResponse{
			statusCode: 405,
		}

		_, err := client.MergeMergeRequest(ctx, project, empty, false)
		if err == nil {
			t.Error("unexpected nil error")
		}
		if !errors.Is(err, ErrNotMergeable) {
			t.Errorf("invalid error, want=%v have=%v", ErrNotMergeable, err)
		}
	})

	t.Run("success", func(t *testing.T) {
		client := newTestClient(t)
		client.httpClient = &mockHTTPResponseBody{
			responseBody: `{"body":"test-merge"}`,
		}

		_, err := client.MergeMergeRequest(ctx, project, empty, false)
		if err != nil {
			t.Errorf("unexpected non-nil error: %+v", err)
		}
	})
}
