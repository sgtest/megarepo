package query

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"

	"github.com/google/go-cmp/cmp"
)

func Test_ScanParameter(t *testing.T) {
	cases := []struct {
		Name  string
		Input string
		Want  string
	}{
		{
			Name:  "Normal field:value",
			Input: `file:README.md`,
			Want:  `{"field":"file","value":"README.md","negated":false}`,
		},

		{
			Name:  "First char is colon",
			Input: `:foo`,
			Want:  `{"field":"","value":":foo","negated":false}`,
		},
		{
			Name:  "Last char is colon",
			Input: `foo:`,
			Want:  `{"field":"foo","value":"","negated":false}`,
		},
		{
			Name:  "Match first colon",
			Input: `foo:bar:baz`,
			Want:  `{"field":"foo","value":"bar:baz","negated":false}`,
		},
		{
			Name:  "No field, start with minus",
			Input: `-:foo`,
			Want:  `{"field":"","value":"-:foo","negated":false}`,
		},
		{
			Name:  "Minus prefix on field",
			Input: `-file:README.md`,
			Want:  `{"field":"file","value":"README.md","negated":true}`,
		},
		{
			Name:  "Double minus prefix on field",
			Input: `--foo:bar`,
			Want:  `{"field":"","value":"--foo:bar","negated":false}`,
		},
		{
			Name:  "Minus in the middle is not a valid field",
			Input: `fie-ld:bar`,
			Want:  `{"field":"","value":"fie-ld:bar","negated":false}`,
		},
		{
			Name:  "No effect on escaped whitespace",
			Input: `a\ pattern`,
			Want:  `{"field":"","value":"a\\ pattern","negated":false}`,
		},
	}
	for _, tt := range cases {
		t.Run(tt.Name, func(t *testing.T) {
			parser := &parser{buf: []byte(tt.Input)}
			result := parser.ParseParameter()
			got, _ := json.Marshal(result)
			if diff := cmp.Diff(tt.Want, string(got)); diff != "" {
				t.Error(diff)
			}
		})
	}
}

func parseAndOrGrammar(in string) ([]Node, error) {
	if in == "" {
		return nil, nil
	}
	parser := &parser{buf: []byte(in), heuristic: false}
	nodes, err := parser.parseOr()
	if err != nil {
		return nil, err
	}
	if parser.balanced != 0 {
		return nil, errors.New("unbalanced expression")
	}
	return newOperator(nodes, And), nil
}

func Test_Parse(t *testing.T) {
	type relation string         // a relation for comparing test outputs of queries parsed according to grammar and heuristics.
	const Same relation = "Same" // a constant that says heuristic output is interpreted the same as the grammar spec.
	type Spec = relation         // constructor for expected output of the grammar spec without heuristics.
	type Diff = relation         // constructor for expected heuristic output when different to the grammar spec.

	cases := []struct {
		Name          string
		Input         string
		WantGrammar   relation
		WantHeuristic relation
	}{
		{
			Name:          "Empty string",
			Input:         "",
			WantGrammar:   "",
			WantHeuristic: Same,
		},
		{
			Name:          "Single",
			Input:         "a",
			WantGrammar:   `"a"`,
			WantHeuristic: Same,
		},
		{
			Name:          "Whitespace basic",
			Input:         "a b",
			WantGrammar:   `(concat "a" "b")`,
			WantHeuristic: Same,
		},
		{
			Name:          "Basic",
			Input:         "a and b and c",
			WantGrammar:   `(and "a" "b" "c")`,
			WantHeuristic: Same,
		},
		{
			Input:         "(f(x)oo((a|b))bar)",
			WantGrammar:   Spec(`(concat "f" "x" "oo" "a|b" "bar")`),
			WantHeuristic: Diff(`"(f(x)oo((a|b))bar)"`),
		},
		{
			Input:         "aorb",
			WantGrammar:   `"aorb"`,
			WantHeuristic: Same,
		},
		{
			Input:         "aANDb",
			WantGrammar:   `"aANDb"`,
			WantHeuristic: Same,
		},
		{
			Name:          "Reduced complex query mixed caps",
			Input:         "a and b AND c or d and (e OR f) g h i or j",
			WantGrammar:   `(or (and "a" "b" "c") (and "d" (concat (or "e" "f") "g" "h" "i")) "j")`,
			WantHeuristic: Same,
		},
		{
			Name:          "Basic reduced complex query",
			Input:         "a and b or c and d or e",
			WantGrammar:   `(or (and "a" "b") (and "c" "d") "e")`,
			WantHeuristic: Same,
		},
		{
			Name:          "Reduced complex query, reduction over parens",
			Input:         "(a and b or c and d) or e",
			WantGrammar:   `(or (and "a" "b") (and "c" "d") "e")`,
			WantHeuristic: Same,
		},
		{
			Name:          "Reduced complex query, nested 'or' trickles up",
			Input:         "(a and b or c) or d",
			WantGrammar:   `(or (and "a" "b") "c" "d")`,
			WantHeuristic: Same,
		},
		{
			Name:          "Reduced complex query, nested nested 'or' trickles up",
			Input:         "(a and b or (c and d or f)) or e",
			WantGrammar:   `(or (and "a" "b") (and "c" "d") "f" "e")`,
			WantHeuristic: Same,
		},
		{
			Name:          "No reduction on precedence defined by parens",
			Input:         "(a and (b or c) and d) or e",
			WantGrammar:   `(or (and "a" (or "b" "c") "d") "e")`,
			WantHeuristic: Same,
		},
		{
			Name:          "Paren reduction over operators",
			Input:         "(((a b c))) and d",
			WantGrammar:   Spec(`(and (concat "a" "b" "c") "d")`),
			WantHeuristic: Diff(`(and (concat "(((a" "b" "c)))") "d")`),
		},
		// Partition parameters and concatenated patterns.
		{
			Input:         "a (b and c) d",
			WantGrammar:   `(concat "a" (and "b" "c") "d")`,
			WantHeuristic: Same,
		},
		{
			Input:         "(a b c) and (d e f) and (g h i)",
			WantGrammar:   Spec(`(and (concat "a" "b" "c") (concat "d" "e" "f") (concat "g" "h" "i"))`),
			WantHeuristic: Diff(`(and (concat "(a" "b" "c)") (concat "(d" "e" "f)") (concat "(g" "h" "i)"))`),
		},
		{
			Input:         "(a) repo:foo (b)",
			WantGrammar:   Spec(`(and "repo:foo" (concat "a" "b"))`),
			WantHeuristic: Diff(`(and "repo:foo" (concat "(a)" "(b)"))`),
		},
		{
			Input:         "a b (repo:foo c d)",
			WantGrammar:   `(concat "a" "b" (and "repo:foo" (concat "c" "d")))`,
			WantHeuristic: Same,
		},
		{
			Input:         "a repo:b repo:c (d repo:e repo:f)",
			WantGrammar:   `(and "repo:b" "repo:c" (concat "a" (and "repo:e" "repo:f" "d")))`,
			WantHeuristic: Same,
		},
		{
			Input:         "a repo:b repo:c (repo:e repo:f (repo:g repo:h))",
			WantGrammar:   `(and "repo:b" "repo:c" "repo:e" "repo:f" "repo:g" "repo:h" "a")`,
			WantHeuristic: Same,
		},
		{
			Input:         "a repo:b repo:c (repo:e repo:f (repo:g repo:h)) b",
			WantGrammar:   `(and "repo:b" "repo:c" "repo:e" "repo:f" "repo:g" "repo:h" (concat "a" "b"))`,
			WantHeuristic: Same,
		},
		{
			Input:         "a repo:b repo:c (repo:e repo:f (repo:g repo:h b)) ",
			WantGrammar:   `(and "repo:b" "repo:c" (concat "a" (and "repo:e" "repo:f" "repo:g" "repo:h" "b")))`,
			WantHeuristic: Same,
		},
		{
			Input:         "(repo:foo a (repo:bar b (repo:qux c)))",
			WantGrammar:   `(and "repo:foo" (concat "a" (and "repo:bar" (concat "b" (and "repo:qux" "c")))))`,
			WantHeuristic: Same,
		},
		{
			Input:         "a repo:b repo:c (d repo:e repo:f e)",
			WantGrammar:   `(and "repo:b" "repo:c" (concat "a" (and "repo:e" "repo:f" (concat "d" "e"))))`,
			WantHeuristic: Same,
		},
		// Errors.
		{
			Name:          "Unbalanced",
			Input:         "(foo) (bar",
			WantGrammar:   "unbalanced expression",
			WantHeuristic: Same,
		},
		{
			Name:          "Incomplete expression",
			Input:         "a or",
			WantGrammar:   "expected operand at 4",
			WantHeuristic: Same,
		},
		{
			Name:          "Illegal expression on the right",
			Input:         "a or or b",
			WantGrammar:   "expected operand at 5",
			WantHeuristic: Same,
		},
		{
			Name:          "Illegal expression on the right, mixed operators",
			Input:         "a and OR",
			WantGrammar:   "expected operand at 6",
			WantHeuristic: Same,
		},
		{
			Name:          "Illegal expression on the left",
			Input:         "or",
			WantGrammar:   "expected operand at 0",
			WantHeuristic: Same,
		},
		{
			Name:          "Illegal expression on the left, multiple operators",
			Input:         "or or or",
			WantGrammar:   "expected operand at 0",
			WantHeuristic: Same,
		},
		// Reduction.
		{
			Name:          "paren reduction with ands",
			Input:         "(a and b) and (c and d)",
			WantGrammar:   `(and "a" "b" "c" "d")`,
			WantHeuristic: Same,
		},
		{
			Name:          "paren reduction with ors",
			Input:         "(a or b) or (c or d)",
			WantGrammar:   `(or "a" "b" "c" "d")`,
			WantHeuristic: Same,
		},
		{
			Name:          "nested paren reduction with whitespace",
			Input:         "(((a b c))) d",
			WantGrammar:   Spec(`(concat "a" "b" "c" "d")`),
			WantHeuristic: Diff(`(concat "(((a" "b" "c)))" "d")`),
		},
		{
			Name:          "left paren reduction with whitespace",
			Input:         "(a b) c d",
			WantGrammar:   Spec(`(concat "a" "b" "c" "d")`),
			WantHeuristic: Diff(`(concat "(a" "b)" "c" "d")`),
		},
		{
			Name:          "right paren reduction with whitespace",
			Input:         "a b (c d)",
			WantGrammar:   Spec(`(concat "a" "b" "c" "d")`),
			WantHeuristic: Diff(`(concat "a" "b" "(c" "d)")`),
		},
		{
			Name:          "grouped paren reduction with whitespace",
			Input:         "(a b) (c d)",
			WantGrammar:   Spec(`(concat "a" "b" "c" "d")`),
			WantHeuristic: Diff(`(concat "(a" "b)" "(c" "d)")`),
		},
		{
			Name:          "multiple grouped paren reduction with whitespace",
			Input:         "(a b) (c d) (e f)",
			WantGrammar:   Spec(`(concat "a" "b" "c" "d" "e" "f")`),
			WantHeuristic: Diff(`(concat "(a" "b)" "(c" "d)" "(e" "f)")`),
		},
		{
			Name:          "interpolated grouped paren reduction",
			Input:         "(a b) c d (e f)",
			WantGrammar:   Spec(`(concat "a" "b" "c" "d" "e" "f")`),
			WantHeuristic: Diff(`(concat "(a" "b)" "c" "d" "(e" "f)")`),
		},
		{
			Name:          "mixed interpolated grouped paren reduction",
			Input:         "(a and b and (z or q)) and (c and d) and (e and f)",
			WantGrammar:   `(and "a" "b" (or "z" "q") "c" "d" "e" "f")`,
			WantHeuristic: Same,
		},
		// Parentheses.
		{
			Name:          "empty paren",
			Input:         "()",
			WantGrammar:   Spec(`""`),
			WantHeuristic: Diff(`"()"`),
		},
		{
			Name:          "paren inside contiguous string",
			Input:         "foo()bar",
			WantGrammar:   Spec(`(concat "foo" "bar")`),
			WantHeuristic: Diff(`"foo()bar"`),
		},
		{
			Name:          "paren containing whitespace inside contiguous string",
			Input:         "foo(   )bar",
			WantGrammar:   Diff(`(concat "foo" "bar")`),
			WantHeuristic: Spec(`(concat "foo(" ")bar")`),
		},
		{
			Name:          "nested empty paren",
			Input:         "(x())",
			WantGrammar:   Spec(`"x"`),
			WantHeuristic: Diff(`"(x())"`),
		},
		{
			Name:          "interpolated nested empty paren",
			Input:         "(()x(  )(())())",
			WantGrammar:   Spec(`"x"`),
			WantHeuristic: Diff(`(concat "(()x(" ")(())())")`),
		},
		{
			Name:          "empty paren on or",
			Input:         "() or ()",
			WantGrammar:   Spec(`""`),
			WantHeuristic: Diff(`(or "()" "()")`),
		},
		{
			Name:          "empty left paren on or",
			Input:         "() or (x)",
			WantGrammar:   Spec(`"x"`),
			WantHeuristic: Diff(`(or "()" "(x)")`),
		},
		{
			Name:          "empty left paren on or",
			Input:         "() or (x)",
			WantGrammar:   Spec(`"x"`),
			WantHeuristic: Diff(`(or "()" "(x)")`),
		},
		{
			Name:          "complex interpolated nested empty paren",
			Input:         "(()x(  )(y or () or (f))())",
			WantGrammar:   Spec(`(concat "x" (or "y" "f"))`),
			WantHeuristic: Diff(`(concat "()" "x" "()" (or "y" "()" "f") "()")`),
		},
	}
	for _, tt := range cases {
		t.Run(tt.Name, func(t *testing.T) {
			check := func(result []Node, err error, want string) {
				var resultStr []string
				if err != nil {
					if diff := cmp.Diff(want, err.Error()); diff != "" {
						t.Fatal(diff)
					}
					return
				}
				for _, node := range result {
					resultStr = append(resultStr, node.String())
				}
				got := strings.Join(resultStr, " ")
				if diff := cmp.Diff(want, got); diff != "" {
					t.Error(diff)
				}
			}
			var result []Node
			var err error
			result, err = parseAndOrGrammar(tt.Input) // Parse without heuristic.
			check(result, err, string(tt.WantGrammar))
			result, err = parseAndOr(tt.Input)
			if tt.WantHeuristic == Same {
				check(result, err, string(tt.WantGrammar))
			} else {
				check(result, err, string(tt.WantHeuristic))
			}
		})
	}
}
