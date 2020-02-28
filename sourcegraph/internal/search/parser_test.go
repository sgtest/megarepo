package search

import (
	"strings"
	"testing"

	"github.com/google/go-cmp/cmp"
)

func Test_Parse(t *testing.T) {
	cases := []struct {
		Name  string
		Input string
		Want  string
	}{
		{
			Name:  "Empty string",
			Input: "",
			Want:  "",
		},
		{
			Name:  "Single",
			Input: "a",
			Want:  "a",
		},
		{
			Name:  "Whitespace basic",
			Input: "a b",
			Want:  "(and a b)",
		},
		{
			Name:  "Basic",
			Input: "a and b and c",
			Want:  "(and a b c)",
		},
		{
			Name:  "Reduced complex query mixed caps",
			Input: "a and b AND c or d and (e OR f) g h i or j",
			Want:  "(or (and a b c) (and d (or e f) g h i) j)",
		},
		{
			Name:  "Basic reduced complex query",
			Input: "a and b or c and d or e",
			Want:  "(or (and a b) (and c d) e)",
		},
		{
			Name:  "Reduced complex query, reduction over parens",
			Input: "(a and b or c and d) or e",
			Want:  "(or (and a b) (and c d) e)",
		},
		{
			Name:  "Reduced complex query, nested 'or' trickles up",
			Input: "(a and b or c) or d",
			Want:  "(or (and a b) c d)",
		},
		{
			Name:  "Reduced complex query, nested nested 'or' trickles up",
			Input: "(a and b or (c and d or f)) or e",
			Want:  "(or (and a b) (and c d) f e)",
		},
		{
			Name:  "No reduction on precedence defined by parens",
			Input: "(a and (b or c) and d) or e",
			Want:  "(or (and a (or b c) d) e)",
		},
		{
			Name:  "Paren reduction over operators",
			Input: "(((a b c))) and d",
			Want:  "(and a b c d)",
		},
		// Errors.
		{
			Name:  "Unbalanced",
			Input: "(foo) (bar",
			Want:  "unbalanced expression",
		},
		{
			Name:  "Incomplete expression",
			Input: "a or",
			Want:  "expected operand at 4",
		},
		{
			Name:  "Illegal expression on the right",
			Input: "a or or b",
			Want:  "expected operand at 5",
		},
		{
			Name:  "Illegal expression on the right, mixed operators",
			Input: "a and OR",
			Want:  "expected operand at 6",
		},
		{
			Name:  "Illegal expression on the left",
			Input: "or",
			Want:  "expected operand at 0",
		},
		{
			Name:  "Illegal expression on the left, multiple operators",
			Input: "or or or",
			Want:  "expected operand at 0",
		},
		// Reduction.
		{
			Name:  "paren reduction with ands",
			Input: "(a and b) and (c and d)",
			Want:  "(and a b c d)",
		},
		{
			Name:  "paren reduction with ors",
			Input: "(a or b) or (c or d)",
			Want:  "(or a b c d)",
		},
		{
			Name:  "nested paren reduction with whitespace",
			Input: "(((a b c))) d",
			Want:  "(and a b c d)",
		},
		{
			Name:  "left paren reduction with whitespace",
			Input: "(a b) c d",
			Want:  "(and a b c d)",
		},
		{
			Name:  "right paren reduction with whitespace",
			Input: "a b (c d)",
			Want:  "(and a b c d)",
		},
		{
			Name:  "grouped paren reduction with whitespace",
			Input: "(a b) (c d)",
			Want:  "(and a b c d)",
		},
		{
			Name:  "multiple grouped paren reduction with whitespace",
			Input: "(a b) (c d) (e f)",
			Want:  "(and a b c d e f)",
		},
		{
			Name:  "interpolated grouped paren reduction",
			Input: "(a b) c d (e f)",
			Want:  "(and a b c d e f)",
		},
		{
			Name:  "mixed interpolated grouped paren reduction",
			Input: "(a and b and (z or q)) and (c and d) and (e and f)",
			Want:  "(and a b (or z q) c d e f)",
		},
		// Parentheses.
		{
			Name:  "empty paren",
			Input: "()",
			Want:  "",
		},
		{
			Name:  "nested empty paren",
			Input: "(x())",
			Want:  "x",
		},
		{
			Name:  "interpolated nested empty paren",
			Input: "(()x(  )(())())",
			Want:  "x",
		},
		{
			Name:  "empty paren on or",
			Input: "() or ()",
			Want:  "",
		},
		{
			Name:  "empty left paren on or",
			Input: "() or (x)",
			Want:  "x",
		},
		{
			Name:  "complex interpolated nested empty paren",
			Input: "(()x(  )(y or () or (f))())",
			Want:  "(and x (or y f))",
		},
	}
	for _, tt := range cases {
		t.Run(tt.Name, func(t *testing.T) {
			result, err := Parse(tt.Input)
			if err != nil {
				if diff := cmp.Diff(tt.Want, err.Error()); diff != "" {
					t.Fatal(diff)
				}
				return
			}
			var resultStr []string
			for _, node := range result {
				resultStr = append(resultStr, node.String())
			}
			got := strings.Join(resultStr, " ")
			if diff := cmp.Diff(tt.Want, got); diff != "" {
				t.Error(diff)
			}
		})
	}
}
