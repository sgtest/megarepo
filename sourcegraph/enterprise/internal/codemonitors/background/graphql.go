package background

import (
	"bytes"
	"context"
	"encoding/json"
	"log"
	"net/http"
	"net/url"
	"runtime"
	"strconv"
	"time"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/api/internalapi"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"

	"github.com/cockroachdb/errors"
)

type graphQLQuery struct {
	Query     string      `json:"query"`
	Variables interface{} `json:"variables"`
}

const gqlSearchQuery = `query CodeMonitorSearch(
	$query: String!,
) {
	search(query: $query) {
		results {
			approximateResultCount
			limitHit
			cloning { name }
			timedout { name }
			results {
				__typename
				... on FileMatch {
					limitHit
					lineMatches {
						preview
						lineNumber
						offsetAndLengths
					}
				}
				... on CommitSearchResult {
					refs {
						name
						displayName
						prefix
						repository {
							name
						}
					}
					sourceRefs {
						name
						displayName
						prefix
						repository {
							name
						}
					}
					messagePreview {
						value
						highlights {
							line
							character
							length
						}
					}
					diffPreview {
						value
						highlights {
							line
							character
							length
						}
					}
					commit {
						repository {
							name
						}
						oid
						abbreviatedOID
						author {
							person {
								displayName
								avatarURL
							}
							date
						}
						message
					}
				}
			}
			alert {
				title
				description
				proposedQueries {
					description
					query
				}
			}
		}
	}
}`

type gqlSearchVars struct {
	Query string `json:"query"`
}

type gqlSearchResponse struct {
	Data struct {
		Search struct {
			Results struct {
				ApproximateResultCount string
				Cloning                []*api.Repo
				Timedout               []*api.Repo
				Results                []interface{}
			}
		}
	}
	Errors []interface{}
}

func search(ctx context.Context, query string, userID int32) (*gqlSearchResponse, error) {
	var buf bytes.Buffer
	err := json.NewEncoder(&buf).Encode(graphQLQuery{
		Query:     gqlSearchQuery,
		Variables: gqlSearchVars{Query: query},
	})
	if err != nil {
		return nil, errors.Wrap(err, "Encode")
	}

	url, err := gqlURL("CodeMonitorSearch")
	if err != nil {
		return nil, errors.Wrap(err, "constructing frontend URL")
	}

	req, err := http.NewRequest("POST", url, &buf)
	if err != nil {
		return nil, errors.Wrap(err, "Post")
	}

	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Sourcegraph-User-ID", strconv.FormatInt(int64(userID), 10))
	resp, err := httpcli.InternalDoer.Do(req.WithContext(ctx))
	if err != nil {
		return nil, errors.Wrap(err, "Post")
	}
	defer resp.Body.Close()

	var res *gqlSearchResponse
	if err := json.NewDecoder(resp.Body).Decode(&res); err != nil {
		return nil, errors.Wrap(err, "Decode")
	}
	if len(res.Errors) > 0 {
		return res, errors.Errorf("graphql: errors: %v", res.Errors)
	}
	return res, nil
}

func gqlURL(queryName string) (string, error) {
	u, err := url.Parse(internalapi.Client.URL)
	if err != nil {
		return "", err
	}
	u.Path = "/.internal/graphql"
	u.RawQuery = queryName
	return u.String(), nil
}

// extractTime extracts the time from the given search result.
func extractTime(result interface{}) (t *time.Time, err error) {
	// Use recover because we assume the data structure here a lot, for less
	// error checking.
	defer func() {
		if r := recover(); r != nil {
			// Same as net/http
			const size = 64 << 10
			buf := make([]byte, size)
			buf = buf[:runtime.Stack(buf, false)]
			log.Printf("failed to extract time from search result: %v\n%s", r, buf)
			err = errors.Errorf("failed to extract time from search result")
		}
	}()

	m := result.(map[string]interface{})
	typeName := m["__typename"].(string)
	switch typeName {
	case "CommitSearchResult":
		commit := m["commit"].(map[string]interface{})
		author := commit["author"].(map[string]interface{})
		date := author["date"].(string)

		// This relies on the date format that our API returns. It was previously broken
		// and should be checked first in case date extraction stops working.
		t, err := time.Parse(time.RFC3339, date)
		if err != nil {
			return nil, err
		}
		return &t, nil
	default:
		return nil, errors.Errorf("unexpected result __typename %q", typeName)
	}
}
