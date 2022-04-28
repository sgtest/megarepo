package query

import (
	"encoding/json"
	"testing"

	"github.com/hexops/autogold"
)

func TestPipelineStructural(t *testing.T) {
	test := func(input string) string {
		pipelinePlan, _ := Pipeline(InitStructural(input))
		return pipelinePlan.ToParseTree().String()
	}

	autogold.Want("contains(...) spans newlines", `"repo:contains.file(\nfoo\n)"`).Equal(t, test("repo:contains.file(\nfoo\n)"))
}

func jsonFormatted(nodes []Node) string {
	var jsons []interface{}
	for _, node := range nodes {
		jsons = append(jsons, toJSON(node))
	}
	json, err := json.MarshalIndent(jsons, "", "  ")
	if err != nil {
		return ""
	}
	return string(json)
}

func TestSubstituteSearchContexts(t *testing.T) {
	test := func(input string, verbose bool) string {
		lookup := func(string) (string, error) {
			return "repo:primary or repo:secondary", nil
		}
		plan, err := Pipeline(InitLiteral(input), SubstituteSearchContexts(lookup))
		if err != nil {
			return err.Error()
		}

		if verbose {
			return jsonFormatted(plan.ToParseTree())
		}
		return plan.ToParseTree().String()
	}

	autogold.Want("basic", `(or (and "repo:primary" "scamaz") (and "repo:secondary" "scamaz"))`).Equal(t, test("context:gordo scamaz", false))

	autogold.Want("preserve predicate label", `[
  {
    "or": [
      {
        "and": [
          {
            "field": "repo",
            "value": "primary",
            "negated": false,
            "labels": [
              "None"
            ]
          },
          {
            "field": "repo",
            "value": "contains.file(gordo)",
            "negated": false,
            "labels": [
              "IsPredicate"
            ]
          }
        ]
      },
      {
        "and": [
          {
            "field": "repo",
            "value": "secondary",
            "negated": false,
            "labels": [
              "None"
            ]
          },
          {
            "field": "repo",
            "value": "contains.file(gordo)",
            "negated": false,
            "labels": [
              "IsPredicate"
            ]
          }
        ]
      }
    ]
  }
]`).
		Equal(t, test("context:gordo repo:contains.file(gordo)", true))
}
