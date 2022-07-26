package gqltestutil

import (
	"github.com/graph-gophers/graphql-go"

	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// ScheduleRepositoryPermissionsSync schedules a permissions syncing request for
// the given repository.
func (c *Client) ScheduleRepositoryPermissionsSync(id string) error {
	const query = `
mutation ScheduleRepositoryPermissionsSync($repository: ID!) {
	scheduleRepositoryPermissionsSync(repository: $repository) {
		alwaysNil
	}
}
`
	variables := map[string]any{
		"repository": id,
	}
	err := c.GraphQL("", query, variables, nil)
	if err != nil {
		return errors.Wrap(err, "request GraphQL")
	}
	return nil
}

type BitbucketProjectPermsSyncArgs struct {
	ProjectKey      string
	CodeHost        string
	UserPermissions []types.UserPermission
	Unrestricted    *bool
}

type UserPermission struct {
	BindID     string `json:"bindID"`
	Permission string `json:"permission"`
}

// SetRepositoryPermissionsForBitbucketProject requests to set repo permissions for given Bitbucket Project and users.
func (c *Client) SetRepositoryPermissionsForBitbucketProject(args BitbucketProjectPermsSyncArgs) error {
	const query = `
mutation SetRepositoryPermissionsForBitbucketProject($projectKey: String!, $codeHost: ID!, $userPermissions: [UserPermissionInput!]!, $unrestricted: Boolean) {
	setRepositoryPermissionsForBitbucketProject(
		projectKey: $projectKey
		codeHost: $codeHost
		userPermissions: $userPermissions
		unrestricted: $unrestricted
	) {
		alwaysNil
	}
}
`
	variables := map[string]any{
		"projectKey":      args.ProjectKey,
		"codeHost":        graphql.ID(args.CodeHost),
		"userPermissions": args.UserPermissions,
		"unrestricted":    args.Unrestricted,
	}
	err := c.GraphQL("", query, variables, nil)
	if err != nil {
		return errors.Wrap(err, "request GraphQL")
	}
	return nil
}

// GetLastBitbucketProjectPermissionJob returns a status of the most recent
// BitbucketProjectPermissionJob for given projectKey
func (c *Client) GetLastBitbucketProjectPermissionJob(projectKey string) (string, error) {
	const query = `
query BitbucketProjectPermissionJobs($projectKeys: [String!], $status: String, $count: Int) {
	bitbucketProjectPermissionJobs(projectKeys: $projectKeys, status: $status, count: $count) {
		totalCount,
   		nodes {
			State
   		}
	}
}
`
	variables := map[string]any{
		"projectKeys": []string{projectKey},
	}
	var resp struct {
		Data struct {
			Jobs struct {
				TotalCount int `json:"totalCount"`
				Nodes      []struct {
					State string `json:"state"`
				} `json:"nodes"`
			} `json:"bitbucketProjectPermissionJobs"`
		} `json:"data"`
	}
	err := c.GraphQL("", query, variables, &resp)
	if err != nil {
		return "", errors.Wrap(err, "request GraphQL")
	}

	if resp.Data.Jobs.TotalCount < 1 {
		return "", nil
	} else {
		return resp.Data.Jobs.Nodes[0].State, nil
	}
}

// UsersWithPendingPermissions returns bind IDs of users with pending permissions
func (c *Client) UsersWithPendingPermissions() ([]string, error) {
	const query = `
query {
	usersWithPendingPermissions
}
`
	var resp struct {
		Data struct {
			UsersWithPendingPermissions []string `json:"usersWithPendingPermissions"`
		} `json:"data"`
	}
	err := c.GraphQL("", query, nil, &resp)
	if err != nil {
		return nil, errors.Wrap(err, "request GraphQL")
	}

	return resp.Data.UsersWithPendingPermissions, nil
}
