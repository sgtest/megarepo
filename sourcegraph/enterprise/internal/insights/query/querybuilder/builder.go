package querybuilder

import (
	"fmt"
	"strings"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/compute"

	"github.com/grafana/regexp"

	searchquery "github.com/sourcegraph/sourcegraph/internal/search/query"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// withDefaults builds a Sourcegraph query from a base input query setting default fields if they are not specified
// in the base query. For example an input query of `repo:myrepo test` might be provided a default `archived:no`,
// and the result would be generated as `repo:myrepo test archive:no`. This preserves the semantics of the original query
// by fully parsing and reconstructing the tree, and does **not** overwrite user supplied values for the default fields.
func withDefaults(inputQuery string, defaults searchquery.Parameters) (string, error) {
	plan, err := searchquery.Pipeline(searchquery.Init(inputQuery, searchquery.SearchTypeRegex))
	if err != nil {
		return "", errors.Wrap(err, "Pipeline")
	}
	modified := make(searchquery.Plan, 0, len(plan))

	for _, basic := range plan {
		p := make(searchquery.Parameters, 0, len(basic.Parameters)+len(defaults))

		for _, defaultParam := range defaults {
			if !basic.Parameters.Exists(defaultParam.Field) {
				p = append(p, defaultParam)
			}
		}
		p = append(p, basic.Parameters...)
		modified = append(modified, basic.MapParameters(p))
	}

	return searchquery.StringHuman(modified.ToQ()), nil
}

// CodeInsightsQueryDefaults returns the default query parameters for a Code Insights generated Sourcegraph query.
func CodeInsightsQueryDefaults(allReposInsight bool) searchquery.Parameters {
	forkArchiveValue := searchquery.No
	if !allReposInsight {
		forkArchiveValue = searchquery.Yes
	}
	return []searchquery.Parameter{
		{
			Field:      searchquery.FieldFork,
			Value:      string(forkArchiveValue),
			Negated:    false,
			Annotation: searchquery.Annotation{},
		},
		{
			Field:      searchquery.FieldArchived,
			Value:      string(forkArchiveValue),
			Negated:    false,
			Annotation: searchquery.Annotation{},
		},
	}
}

// withCountAll appends a count all argument to a query if one isn't already provided.
func withCountAll(s string) string {
	if strings.Contains(s, "count:") {
		return s
	}
	return s + " count:all"
}

// forRepoRevision appends the `repo@rev` target for a Code Insight query.
func forRepoRevision(query, repo, revision string) string {
	return fmt.Sprintf("%s repo:^%s$@%s", query, regexp.QuoteMeta(repo), revision)
}

// forRepos appends a single repo filter making an OR condition for all repos passed
func forRepos(query string, repos []string) string {
	escapedRepos := make([]string, len(repos))
	for i, repo := range repos {
		escapedRepos[i] = regexp.QuoteMeta(repo)
	}
	return fmt.Sprintf("%s repo:^(%s)$", query, (strings.Join(escapedRepos, "|")))
}

// SingleRepoQuery generates a Sourcegraph query with the provided default values given a user specified query and a repository / revision target. The repository string
// should be provided in plain text, and will be escaped for regexp before being added to the query.
func SingleRepoQuery(query, repo, revision string, defaultParams searchquery.Parameters) (string, error) {
	modified := withCountAll(query)
	modified, err := withDefaults(modified, defaultParams)
	if err != nil {
		return "", errors.Wrap(err, "WithDefaults")
	}
	modified = forRepoRevision(modified, repo, revision)

	return modified, nil
}

// GlobalQuery generates a Sourcegraph query with the provided default values given a user specified query. This query will be global (against all visible repositories).
func GlobalQuery(query string, defaultParams searchquery.Parameters) (string, error) {
	modified := withCountAll(query)
	modified, err := withDefaults(modified, defaultParams)
	if err != nil {
		return "", errors.Wrap(err, "WithDefaults")
	}
	return modified, nil
}

// MultiRepoQuery generates a Sourcegraph query with the provided default values given a user specified query and slice of repositories.
// Repositories should be provided in plain text, and will be escaped for regexp and OR'ed together before being added to the query.
func MultiRepoQuery(query string, repos []string, defaultParams searchquery.Parameters) (string, error) {
	modified := withCountAll(query)
	modified, err := withDefaults(modified, defaultParams)
	if err != nil {
		return "", errors.Wrap(err, "WithDefaults")
	}
	modified = forRepos(modified, repos)

	return modified, nil
}

type MapType string

const (
	Lang   MapType = "lang"
	Repo   MapType = "repo"
	Path   MapType = "path"
	Author MapType = "author"
	Date   MapType = "date"
)

// This is the compute command that corresponds to the execution for Code Insights.
const insightsComputeCommand = "output.extra"

// ComputeInsightCommandQuery will convert a standard Sourcegraph search query into a compute "map type" insight query. This command type will group by
// certain fields. The original search query semantic should be preserved, although any new limitations or restrictions in Compute will apply.
func ComputeInsightCommandQuery(query string, mapType MapType) (string, error) {
	q, err := compute.Parse(query)
	if err != nil {
		return "", err
	}
	pattern := q.Command.ToSearchPattern()
	return searchquery.AddRegexpField(q.Parameters, searchquery.FieldContent, fmt.Sprintf("%s(%s -> $%s)", insightsComputeCommand, pattern, mapType)), nil
}
