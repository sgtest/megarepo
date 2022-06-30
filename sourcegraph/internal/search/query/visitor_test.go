package query

import (
	"testing"

	"github.com/hexops/autogold"
)

func TestSubstitute(t *testing.T) {
	test := func(input string) string {
		q, _ := ParseLiteral(input)
		var result string
		VisitPredicate(q, func(field, name, value string) {
			if field == FieldRepo && name == "contains" {
				result = "contains value is " + value
			}
		})
		return result
	}

	autogold.Want("VisitPredicate visits predicates",
		"contains value is file:foo").
		Equal(t, test("repo:contains(file:foo)"))
}

func TestVisitTypedPredicate(t *testing.T) {
	cases := []struct {
		query  string
		output autogold.Value
	}{{
		"repo:test",
		autogold.Want("no predicates", []*RepoContainsFilePredicate{}),
	}, {
		"repo:test repo:contains.file(test)",
		autogold.Want("one predicate", []*RepoContainsFilePredicate{{Pattern: "test"}}),
	}}

	for _, tc := range cases {
		t.Run(tc.output.Name(), func(t *testing.T) {
			q, _ := ParseLiteral(tc.query)
			var result []*RepoContainsFilePredicate
			VisitTypedPredicate(q, func(pred *RepoContainsFilePredicate, negated bool) {
				result = append(result, pred)
			})
			tc.output.Equal(t, result)
		})
	}
}
