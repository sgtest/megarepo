package resolvers

import (
	"context"
	"encoding/json"
	"testing"

	"github.com/hexops/autogold"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/compute"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/search/result"
)

func TestToResultResolverList(t *testing.T) {
	matches := []result.Match{
		&result.FileMatch{
			LineMatches: []*result.LineMatch{
				{Preview: "a"},
				{Preview: "b"},
			},
		},
	}
	test := func(input string) string {
		computeQuery, _ := compute.Parse(input)
		resolvers, _ := toResultResolverList(
			context.Background(),
			computeQuery.Command,
			matches,
			database.NewMockDB(),
		)
		results := make([]string, 0, len(resolvers))
		for _, r := range resolvers {
			if rr, ok := r.ToComputeMatchContext(); ok {
				matches := rr.Matches()
				for _, m := range matches {
					results = append(results, m.Value())
				}
			}
		}
		v, _ := json.Marshal(results)
		return string(v)
	}

	autogold.Want("resolver copies all match results", `["a","b"]`).Equal(t, test("a|b"))
}
