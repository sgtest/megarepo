package bitbucketcloud

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"

	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type PullRequestInput struct {
	Title        string
	Description  string
	SourceBranch string

	// The following fields are optional.
	//
	// If SourceRepo is provided, only FullName is actually used.
	SourceRepo        *Repo
	DestinationBranch *string
}

// CreatePullRequest opens a new pull request.
//
// Invoking CreatePullRequest with the same repo and options will succeed: the
// same PR will be returned each time, and will be updated accordingly on
// Bitbucket with any changed information in the options.
func (c *Client) CreatePullRequest(ctx context.Context, repo *Repo, input PullRequestInput) (*PullRequest, error) {
	data, err := json.Marshal(&input)
	if err != nil {
		return nil, errors.Wrap(err, "marshalling request")
	}

	req, err := http.NewRequest("POST", fmt.Sprintf("/2.0/repositories/%s/pullrequests", repo.FullName), bytes.NewBuffer(data))
	if err != nil {
		return nil, errors.Wrap(err, "creating request")
	}

	var pr PullRequest
	if err := c.do(ctx, req, &pr); err != nil {
		return nil, errors.Wrap(err, "sending request")
	}

	return &pr, nil
}

// DeclinePullRequest declines (closes without merging) a pull request.
//
// Invoking DeclinePullRequest on an already declined PR will error.
func (c *Client) DeclinePullRequest(ctx context.Context, repo *Repo, id int64) (*PullRequest, error) {
	req, err := http.NewRequest("POST", fmt.Sprintf("/2.0/repositories/%s/pullrequests/%d/decline", repo.FullName, id), nil)
	if err != nil {
		return nil, errors.Wrap(err, "creating request")
	}

	var pr PullRequest
	if err := c.do(ctx, req, &pr); err != nil {
		return nil, errors.Wrap(err, "sending request")
	}

	return &pr, nil
}

// GetPullRequest retrieves a single pull request.
func (c *Client) GetPullRequest(ctx context.Context, repo *Repo, id int64) (*PullRequest, error) {
	req, err := http.NewRequest("GET", fmt.Sprintf("/2.0/repositories/%s/pullrequests/%d", repo.FullName, id), nil)
	if err != nil {
		return nil, errors.Wrap(err, "creating request")
	}

	var pr PullRequest
	if err := c.do(ctx, req, &pr); err != nil {
		return nil, errors.Wrap(err, "sending request")
	}

	return &pr, nil
}

// GetPullRequestStatuses retrieves the statuses for a pull request.
//
// Each item in the result set is a *PullRequestStatus.
func (c *Client) GetPullRequestStatuses(repo *Repo, id int64) (*PaginatedResultSet, error) {
	u, err := url.Parse(fmt.Sprintf("/2.0/repositories/%s/pullrequests/%d/statuses", repo.FullName, id))
	if err != nil {
		return nil, errors.Wrap(err, "parsing URL")
	}

	return newResultSet(c, u, func(ctx context.Context, req *http.Request) (*PageToken, []interface{}, error) {
		var page struct {
			*PageToken
			Values []*PullRequestStatus `json:"values"`
		}

		if err := c.do(ctx, req, &page); err != nil {
			return nil, nil, err
		}

		values := []interface{}{}
		for _, value := range page.Values {
			values = append(values, value)
		}

		return page.PageToken, values, nil
	}), nil
}

// UpdatePullRequest updates a pull request.
func (c *Client) UpdatePullRequest(ctx context.Context, repo *Repo, id int64, input PullRequestInput) (*PullRequest, error) {
	data, err := json.Marshal(&input)
	if err != nil {
		return nil, errors.Wrap(err, "marshalling request")
	}

	req, err := http.NewRequest("PUT", fmt.Sprintf("/2.0/repositories/%s/pullrequests/%d", repo.FullName, id), bytes.NewBuffer(data))
	if err != nil {
		return nil, errors.Wrap(err, "creating request")
	}

	var updated PullRequest
	if err := c.do(ctx, req, &updated); err != nil {
		return nil, errors.Wrap(err, "sending request")
	}

	return &updated, nil
}

type CommentInput struct {
	// The content, as Markdown.
	Content string
}

// CreatePullRequestComment adds a comment to a pull request.
func (c *Client) CreatePullRequestComment(ctx context.Context, repo *Repo, id int64, input CommentInput) (*Comment, error) {
	data, err := json.Marshal(&input)
	if err != nil {
		return nil, errors.Wrap(err, "marshalling request")
	}

	req, err := http.NewRequest("POST", fmt.Sprintf("/2.0/repositories/%s/pullrequests/%d/comments", repo.FullName, id), bytes.NewBuffer(data))
	if err != nil {
		return nil, errors.Wrap(err, "creating request")
	}

	var comment Comment
	if err := c.do(ctx, req, &comment); err != nil {
		return nil, errors.Wrap(err, "sending request")
	}

	return &comment, nil
}

// MergePullRequestOpts are the options available when merging a pull request.
//
// All fields are optional.
type MergePullRequestOpts struct {
	Message           *string        `json:"message,omitempty"`
	CloseSourceBranch *bool          `json:"close_source_branch,omitempty"`
	MergeStrategy     *MergeStrategy `json:"merge_strategy,omitempty"`
}

// MergePullRequest merges the given pull request.
func (c *Client) MergePullRequest(ctx context.Context, repo *Repo, id int64, opts MergePullRequestOpts) (*PullRequest, error) {
	data, err := json.Marshal(&opts)
	if err != nil {
		return nil, errors.Wrap(err, "marshalling request")
	}

	req, err := http.NewRequest("POST", fmt.Sprintf("/2.0/repositories/%s/pullrequests/%d/merge", repo.FullName, id), bytes.NewBuffer(data))
	if err != nil {
		return nil, errors.Wrap(err, "creating request")
	}

	var pr PullRequest
	if err := c.do(ctx, req, &pr); err != nil {
		return nil, errors.Wrap(err, "sending request")
	}

	return &pr, nil
}

var _ json.Marshaler = &PullRequestInput{}

func (input *PullRequestInput) MarshalJSON() ([]byte, error) {
	type branch struct {
		Name string `json:"name"`
	}

	type repository struct {
		FullName string `json:"full_name"`
	}

	type source struct {
		Branch     branch      `json:"branch"`
		Repository *repository `json:"repository,omitempty"`
	}

	type request struct {
		Title       string  `json:"title"`
		Description string  `json:"description,omitempty"`
		Source      source  `json:"source"`
		Destination *source `json:"destination,omitempty"`
	}

	req := request{
		Title:       input.Title,
		Description: input.Description,
		Source: source{
			Branch: branch{Name: input.SourceBranch},
		},
	}
	if input.SourceRepo != nil {
		req.Source.Repository = &repository{
			FullName: input.SourceRepo.FullName,
		}
	}
	if input.DestinationBranch != nil {
		req.Destination = &source{
			Branch: branch{Name: *input.DestinationBranch},
		}
	}

	return json.Marshal(&req)
}

var _ json.Marshaler = &CommentInput{}

func (ci *CommentInput) MarshalJSON() ([]byte, error) {
	type content struct {
		Raw string `json:"raw"`
	}
	type comment struct {
		Content content `json:"content"`
	}

	return json.Marshal(&comment{
		Content: content{
			Raw: ci.Content,
		},
	})
}
