package e2eutil

import (
	"github.com/pkg/errors"
)

type SearchRepositoryResult struct {
	Name string `json:"name"`
}

type SearchRepositoryResults []*SearchRepositoryResult

// Exists returns the list of missing repositories from given names that do not exist
// in search results. If all of given names are found, it returns empty list.
func (rs SearchRepositoryResults) Exists(names ...string) []string {
	set := make(map[string]struct{}, len(names))
	for _, name := range names {
		set[name] = struct{}{}
	}
	for _, r := range rs {
		delete(set, r.Name)
	}

	missing := make([]string, 0, len(set))
	for name := range set {
		missing = append(missing, name)
	}
	return missing
}

// SearchRepositories search repositories with given query.
func (c *Client) SearchRepositories(query string) (SearchRepositoryResults, error) {
	const gqlQuery = `
query Search($query: String!) {
	search(query: $query) {
		results {
			results {
				... on Repository {
					name
				}
			}
		}
	}
}
`
	variables := map[string]interface{}{
		"query": query,
	}
	var resp struct {
		Data struct {
			Search struct {
				Results struct {
					Results []*SearchRepositoryResult `json:"results"`
				} `json:"results"`
			} `json:"search"`
		} `json:"data"`
	}
	err := c.GraphQL("", gqlQuery, variables, &resp)
	if err != nil {
		return nil, errors.Wrap(err, "request GraphQL")
	}

	return resp.Data.Search.Results.Results, nil
}

type SearchFileResult struct {
	Name string `json:"name"`
}

type SearchFileResults []*SearchFileResult

// SearchFiles search files with given query.
func (c *Client) SearchFiles(query string) (SearchFileResults, error) {
	const gqlQuery = `
query Search($query: String!) {
	search(query: $query) {
		results {
			results {
				... on FileMatch {
					file {
						name
					}
				}
			}
		}
	}
}
`
	variables := map[string]interface{}{
		"query": query,
	}
	var resp struct {
		Data struct {
			Search struct {
				Results struct {
					Results []struct {
						*SearchFileResult `json:"file"`
					} `json:"results"`
				} `json:"results"`
			} `json:"search"`
		} `json:"data"`
	}
	err := c.GraphQL("", gqlQuery, variables, &resp)
	if err != nil {
		return nil, errors.Wrap(err, "request GraphQL")
	}

	results := make([]*SearchFileResult, 0, len(resp.Data.Search.Results.Results))
	for _, r := range resp.Data.Search.Results.Results {
		results = append(results, r.SearchFileResult)
	}
	return results, nil
}
