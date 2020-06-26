package query

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"

	"github.com/google/go-cmp/cmp"
)

func TestParseParameterList(t *testing.T) {
	cases := []struct {
		Name       string
		Input      string
		Want       string
		WantLabels labels
		WantRange  string
	}{
		{
			Name:       "Normal field:value",
			Input:      `file:README.md`,
			Want:       `{"field":"file","value":"README.md","negated":false}`,
			WantRange:  `{"start":{"line":0,"column":0},"end":{"line":0,"column":14}}`,
			WantLabels: None,
		},
		{
			Name:       "First char is colon",
			Input:      `:foo`,
			Want:       `{"value":":foo","negated":false}`,
			WantRange:  `{"start":{"line":0,"column":0},"end":{"line":0,"column":4}}`,
			WantLabels: None,
		},
		{
			Name:       "Last char is colon",
			Input:      `foo:`,
			Want:       `{"value":"foo:","negated":false}`,
			WantRange:  `{"start":{"line":0,"column":0},"end":{"line":0,"column":4}}`,
			WantLabels: None,
		},
		{
			Name:       "Match first colon",
			Input:      `file:bar:baz`,
			Want:       `{"field":"file","value":"bar:baz","negated":false}`,
			WantRange:  `{"start":{"line":0,"column":0},"end":{"line":0,"column":12}}`,
			WantLabels: None,
		},
		{
			Name:       "No field, start with minus",
			Input:      `-:foo`,
			Want:       `{"value":"-:foo","negated":false}`,
			WantRange:  `{"start":{"line":0,"column":0},"end":{"line":0,"column":5}}`,
			WantLabels: None,
		},
		{
			Name:       "Minus prefix on field",
			Input:      `-file:README.md`,
			Want:       `{"field":"file","value":"README.md","negated":true}`,
			WantRange:  `{"start":{"line":0,"column":0},"end":{"line":0,"column":15}}`,
			WantLabels: None,
		},
		{
			Name:       "Double minus prefix on field",
			Input:      `--foo:bar`,
			Want:       `{"value":"--foo:bar","negated":false}`,
			WantRange:  `{"start":{"line":0,"column":0},"end":{"line":0,"column":9}}`,
			WantLabels: None,
		},
		{
			Name:       "Minus in the middle is not a valid field",
			Input:      `fie-ld:bar`,
			Want:       `{"value":"fie-ld:bar","negated":false}`,
			WantRange:  `{"start":{"line":0,"column":0},"end":{"line":0,"column":10}}`,
			WantLabels: None,
		},
		{
			Name:       "Interpret escaped whitespace",
			Input:      `a\ pattern`,
			Want:       `{"value":"a pattern","negated":false}`,
			WantRange:  `{"start":{"line":0,"column":0},"end":{"line":0,"column":10}}`,
			WantLabels: None,
		},
		{
			Input:      `"quoted"`,
			Want:       `{"value":"quoted","negated":false}`,
			WantRange:  `{"start":{"line":0,"column":0},"end":{"line":0,"column":8}}`,
			WantLabels: Literal | Quoted,
		},
		{
			Input:      `'\''`,
			Want:       `{"value":"'","negated":false}`,
			WantRange:  `{"start":{"line":0,"column":0},"end":{"line":0,"column":4}}`,
			WantLabels: Literal | Quoted,
		},
	}
	for _, tt := range cases {
		t.Run(tt.Name, func(t *testing.T) {
			parser := &parser{buf: []byte(tt.Input)}
			result, err := parser.parseParameterList()
			if err != nil {
				t.Fatal("Unexpected error")
			}
			resultNode := result[0]
			got, _ := json.Marshal(resultNode)
			// Check parsed values.
			if diff := cmp.Diff(tt.Want, string(got)); diff != "" {
				t.Error(diff)
			}
			// Check ranges.
			switch n := resultNode.(type) {
			case Pattern:
				rangeStr := n.Annotation.Range.String()
				if diff := cmp.Diff(tt.WantRange, rangeStr); diff != "" {
					t.Error(diff)
				}
			case Parameter:
				rangeStr := n.Annotation.Range.String()
				if diff := cmp.Diff(tt.WantRange, rangeStr); diff != "" {
					t.Error(diff)
				}
			}
			// Check labels.
			if patternNode, ok := resultNode.(Pattern); ok {
				if diff := cmp.Diff(tt.WantLabels, patternNode.Annotation.Labels); diff != "" {
					t.Error(diff)
				}
			}
		})
	}
}

func TestScanField(t *testing.T) {
	type value struct {
		Field   string
		Advance int
	}
	cases := []struct {
		Input string
		Want  value
	}{
		// Valid field.
		{
			Input: "repo:foo",
			Want: value{
				Field:   "repo",
				Advance: 5,
			},
		},
		{
			Input: "RepO:foo",
			Want: value{
				Field:   "RepO",
				Advance: 5,
			},
		},
		{
			Input: "after:",
			Want: value{
				Field:   "after",
				Advance: 6,
			},
		},
		{
			Input: "-repo:",
			Want: value{
				Field:   "-repo",
				Advance: 6,
			},
		},
		// Invalid field.
		{
			Input: "",
			Want: value{
				Field:   "",
				Advance: 0,
			},
		},
		{
			Input: "-",
			Want: value{
				Field:   "",
				Advance: 0,
			},
		},
		{
			Input: "-:",
			Want: value{
				Field:   "",
				Advance: 0,
			},
		},
		{
			Input: ":",
			Want: value{
				Field:   "",
				Advance: 0,
			},
		},
		{
			Input: "??:foo",
			Want: value{
				Field:   "",
				Advance: 0,
			},
		},
		{
			Input: "repo",
			Want: value{
				Field:   "",
				Advance: 0,
			},
		},
		{
			Input: "-repo",
			Want: value{
				Field:   "",
				Advance: 0,
			},
		},
		{
			Input: "--repo:",
			Want: value{
				Field:   "",
				Advance: 0,
			},
		},
		{
			Input: ":foo",
			Want: value{
				Field:   "",
				Advance: 0,
			},
		},
	}
	for _, c := range cases {
		t.Run("scan field", func(t *testing.T) {
			gotField, gotAdvance := ScanField([]byte(c.Input))
			if diff := cmp.Diff(c.Want, value{gotField, gotAdvance}); diff != "" {
				t.Error(diff)
			}
		})
	}
}

func parseAndOrGrammar(in string) ([]Node, error) {
	if strings.TrimSpace(in) == "" {
		return nil, nil
	}
	parser := &parser{
		buf: []byte(in),
		// heuristics: map[heuristic]bool{parensAsPatterns: false},
	}
	nodes, err := parser.parseOr()
	if err != nil {
		return nil, err
	}
	if parser.balanced != 0 {
		return nil, errors.New("unbalanced expression")
	}
	return newOperator(nodes, And), nil
}

func TestParse(t *testing.T) {
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
			Name:          "Whitespace",
			Input:         "             ",
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
			Input:         "a oror b",
			WantGrammar:   `(concat "a" "oror" "b")`,
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
			Input:         "repo:foo main { and bar {",
			WantGrammar:   Spec(`(and (and "repo:foo" (concat "main" "{")) (concat "bar" "{"))`),
			WantHeuristic: Diff(`(and "repo:foo" (concat "main" "{") (concat "bar" "{"))`),
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
		// Keywords as patterns.
		{
			Input:         "a or",
			WantGrammar:   `(concat "a" "or")`,
			WantHeuristic: Same,
		},
		{
			Input:         "or",
			WantGrammar:   `"or"`,
			WantHeuristic: Same,
		},
		{
			Input:         "or or or",
			WantGrammar:   `(or "or" "or")`,
			WantHeuristic: Same,
		},
		{
			Input:         "and and andand or oror",
			WantGrammar:   `(or (and "and" "andand") "oror")`,
			WantHeuristic: Same,
		},
		// Errors.
		{
			Name:          "Unbalanced",
			Input:         "(foo) (bar",
			WantGrammar:   Spec("unbalanced expression"),
			WantHeuristic: Diff(`(concat "(foo)" "(bar")`),
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
			WantGrammar:   `(and "a" "OR")`,
			WantHeuristic: Same,
		},
		{
			Input:         "repo:foo or or or",
			WantGrammar:   "expected operand at 12",
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
		{
			Name:          "disable parens as patterns heuristic if containing recognized operator",
			Input:         "(() or ())",
			WantGrammar:   Spec(`""`),
			WantHeuristic: Diff(`(or "()" "()")`),
		},
		// Escaping.
		{
			Input:         `\(\)`,
			WantGrammar:   `"\\(\\)"`,
			WantHeuristic: Same,
		},
		{
			Input:         `\( \) ()`,
			WantGrammar:   Spec(`(concat "\\(" "\\)")`),
			WantHeuristic: Diff(`(concat "\\(" "\\)" "()")`),
		},
		{
			Input:         `\ `,
			WantGrammar:   `" "`,
			WantHeuristic: Same,
		},
		{
			Input:         `\  \ `,
			WantGrammar:   Spec(`(concat " " " ")`),
			WantHeuristic: Diff(`(concat " " " ")`),
		},
		// Dangling parentheses heuristic.
		{
			Input:         `(`,
			WantGrammar:   Spec(`expected operand at 1`),
			WantHeuristic: Diff(`"("`),
		},
		{
			Input:         `)(())(`,
			WantGrammar:   Spec(`unbalanced expression`),
			WantHeuristic: Diff(`"(())("`),
		},
		{
			Input:         `foo( and bar(`,
			WantGrammar:   Spec(`expected operand at 5`),
			WantHeuristic: Diff(`(and "foo(" "bar(")`),
		},
		{
			Input:         `repo:foo foo( or bar(`,
			WantGrammar:   Spec(`expected operand at 14`),
			WantHeuristic: Diff(`(and "repo:foo" (or "foo(" "bar("))`),
		},
		{
			Input:         `(a or (b and )) or d)`,
			WantGrammar:   Spec(`unbalanced expression`),
			WantHeuristic: Diff(`(or "(a" (and "(b" ")") "d)")`),
		},
		// Quotes and escape sequences.
		{
			Input:         `"`,
			WantGrammar:   `"\""`,
			WantHeuristic: Same,
		},
		{
			Input:         `repo:foo' bar'`,
			WantGrammar:   `(and "repo:foo'" "bar'")`,
			WantHeuristic: Same,
		},
		{
			Input:         `repo:'foo' 'bar'`,
			WantGrammar:   `(and "repo:foo" "bar")`,
			WantHeuristic: Same,
		},
		{
			Input:         `repo:"foo" "bar"`,
			WantGrammar:   `(and "repo:foo" "bar")`,
			WantHeuristic: Same,
		},
		{
			Input:         `repo:"foo bar" "foo bar"`,
			WantGrammar:   `(and "repo:foo bar" "foo bar")`,
			WantHeuristic: Same,
		},
		{
			Input:         `repo:"fo\"o" "bar"`,
			WantGrammar:   Spec(`(and "repo:fo\"o" "bar")`),
			WantHeuristic: Same,
		},
		{
			Input:         `repo:foo /b\/ar/`,
			WantGrammar:   `(and "repo:foo" "/b\\/ar/")`,
			WantHeuristic: Same,
		},
		{
			Input:         `repo:foo /a/file/path`,
			WantGrammar:   `(and "repo:foo" "/a/file/path")`,
			WantHeuristic: Same,
		},
		{
			Input:         `repo:foo /a/file/path/`,
			WantGrammar:   `(and "repo:foo" "/a/file/path/")`,
			WantHeuristic: Same,
		},
		{
			Input:         `repo:foo /a/ /another/path/`,
			WantGrammar:   `(and "repo:foo" (concat "/a/" "/another/path/"))`,
			WantHeuristic: Same,
		},
		{
			Input:         `\t\r\n`,
			WantGrammar:   `"\t\r\n"`,
			WantHeuristic: Same,
		},
		{
			Input:         `repo:foo\ bar \:\\`,
			WantGrammar:   `(and "repo:foo bar" ":\\")`,
			WantHeuristic: Same,
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
			result, err = ParseAndOr(tt.Input)
			if tt.WantHeuristic == Same {
				check(result, err, string(tt.WantGrammar))
			} else {
				check(result, err, string(tt.WantHeuristic))
			}
		})
	}
}

func TestScanDelimited(t *testing.T) {
	type result struct {
		Value  string
		Count  int
		ErrMsg string
	}

	cases := []struct {
		name      string
		input     string
		delimiter rune
		want      result
	}{
		{
			input:     `""`,
			delimiter: '"',
			want:      result{Value: "", Count: 2, ErrMsg: ""},
		},
		{
			input:     `"a"`,
			delimiter: '"',
			want:      result{Value: `a`, Count: 3, ErrMsg: ""},
		},
		{
			input:     `"\""`,
			delimiter: '"',
			want:      result{Value: `"`, Count: 4, ErrMsg: ""},
		},
		{
			input:     `"\\""`,
			delimiter: '"',
			want:      result{Value: `\`, Count: 4, ErrMsg: ""},
		},
		{
			input:     `"\\\"`,
			delimiter: '"',
			want:      result{Value: "", Count: 5, ErrMsg: `unterminated literal: expected "`},
		},
		{
			input:     `"\\\""`,
			delimiter: '"',
			want:      result{Value: `\"`, Count: 6, ErrMsg: ""},
		},
		{
			input:     `"a`,
			delimiter: '"',
			want:      result{Value: "", Count: 2, ErrMsg: `unterminated literal: expected "`},
		},
		{
			input:     `"\?"`,
			delimiter: '"',
			want:      result{Value: "", Count: 3, ErrMsg: `unrecognized escape sequence`},
		},
		{
			name:      "panic",
			input:     `a"`,
			delimiter: '"',
			want:      result{},
		},
		{
			input:     `/\//`,
			delimiter: '/',
			want:      result{Value: "/", Count: 4, ErrMsg: ""},
		},
	}

	for _, tt := range cases {
		if tt.name == "panic" {
			defer func() {
				if r := recover(); r == nil {
					t.Errorf("expected panic for ScanDelimited")
				}
			}()
			_, _, _ = ScanDelimited([]byte(tt.input), tt.delimiter)
		}

		t.Run(tt.name, func(t *testing.T) {
			value, count, err := ScanDelimited([]byte(tt.input), tt.delimiter)
			var errMsg string
			if err != nil {
				errMsg = err.Error()
			}
			got := result{value, count, errMsg}
			if diff := cmp.Diff(tt.want, got); diff != "" {
				t.Error(diff)
			}
		})
	}
}
