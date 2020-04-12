package query

import (
	"errors"
	"fmt"
	"io"
	"strconv"
	"strings"
	"unicode"
	"unicode/utf8"

	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"
)

/*
Parser implements a parser for the following grammar:

OrTerm     → AndTerm { OR AndTerm }
AndTerm    → Term { AND Term }
Term       → (OrTerm) | Parameters
Parameters → Parameter { " " Parameter }
*/

type Node interface {
	String() string
	node()
}

// All terms that implement Node.
func (Parameter) node() {}
func (Operator) node()  {}

// Parameter is a leaf node of expressions.
type Parameter struct {
	Field   string `json:"field"`   // The repo part in repo:sourcegraph.
	Value   string `json:"value"`   // The sourcegraph part in repo:sourcegraph.
	Negated bool   `json:"negated"` // True if the - prefix exists, as in -repo:sourcegraph.
}

type operatorKind int

const (
	Or operatorKind = iota
	And
	Concat
)

// Operator is a nonterminal node of kind Kind with child nodes Operands.
type Operator struct {
	Kind     operatorKind
	Operands []Node
}

func (node Parameter) String() string {
	var v string
	switch {
	case node.Field == "":
		v = node.Value
	case node.Negated:
		v = fmt.Sprintf("-%s:%s", node.Field, node.Value)
	default:
		v = fmt.Sprintf("%s:%s", node.Field, node.Value)
	}
	return strconv.Quote(v)
}

func (node Operator) String() string {
	var result []string
	for _, child := range node.Operands {
		result = append(result, child.String())
	}
	var kind string
	switch node.Kind {
	case Or:
		kind = "or"
	case And:
		kind = "and"
	case Concat:
		kind = "concat"
	}

	return fmt.Sprintf("(%s %s)", kind, strings.Join(result, " "))
}

type keyword string

// Reserved keyword syntax.
const (
	AND    keyword = "and"
	OR     keyword = "or"
	LPAREN keyword = "("
	RPAREN keyword = ")"
)

func isSpace(buf []byte) bool {
	r, _ := utf8.DecodeRune(buf)
	return unicode.IsSpace(r)
}

// skipSpace returns the number of whitespace bytes skipped from the beginning of a buffer buf.
func skipSpace(buf []byte) int {
	count := 0
	for len(buf) > 0 {
		r, advance := utf8.DecodeRune(buf)
		if !unicode.IsSpace(r) {
			break
		}
		count += advance
		buf = buf[advance:]
	}
	return count
}

type parser struct {
	buf       []byte
	pos       int
	balanced  int
	heuristic bool // if true, activates parsing parens as patterns rather than expression groups.
}

func (p *parser) done() bool {
	return p.pos >= len(p.buf)
}

func (p *parser) next() rune {
	if p.done() {
		panic("eof")
	}
	r, advance := utf8.DecodeRune(p.buf[p.pos:])
	p.pos += advance
	return r
}

// peek looks ahead n runes in the input and returns a string if it succeeds, or
// an error if the length exceeds what's available in the buffer.
func (p *parser) peek(n int) (string, error) {
	start := p.pos
	defer func() {
		p.pos = start // backtrack
	}()

	var result []rune
	for i := 0; i < n; i++ {
		if p.done() {
			return "", io.ErrShortBuffer
		}
		next := p.next()
		result = append(result, next)
	}
	return string(result), nil
}

// match returns whether it succeeded matching a keyword at the current
// position. It does not advance the position.
func (p *parser) match(keyword keyword) bool {
	v, err := p.peek(len(string(keyword)))
	if err != nil {
		return false
	}
	return strings.ToLower(v) == string(keyword)
}

// expect returns the result of match, and advances the position if it succeeds.
func (p *parser) expect(keyword keyword) bool {
	if !p.match(keyword) {
		return false
	}
	p.pos += len(string(keyword))
	return true
}

// skipSpaces advances the input and places the parser position at the next
// non-space value.
func (p *parser) skipSpaces() error {
	if p.pos > len(p.buf) {
		return io.ErrShortBuffer
	}

	p.pos += skipSpace(p.buf[p.pos:])
	if p.pos > len(p.buf) {
		return io.ErrShortBuffer
	}
	return nil
}

var fieldValuePattern = lazyregexp.New("(^-?[a-zA-Z0-9]+):(.*)")

// ScanParameter returns a leaf node value usable by _any_ kind of search (e.g.,
// literal or regexp, or...) and always succeeds.
//
// A parameter is a contiguous sequence of characters, where the following two forms are distinguished:
// (1) a string of syntax field:<string> where : matches the first encountered colon, and field must match ^-?[a-zA-Z0-9]+
// (2) <string>
//
// When a parameter is of form (1), the <string> corresponds to Parameter.Value, field corresponds to Parameter.Field and Parameter.Negated is set if Field starts with '-'.
// When form (1) does not match, Value corresponds to <string> and Field is the empty string.
//
// The value parameter in the parse tree is only distinguished with respect to
// the two forms above. There is no restriction on values that <string> may take
// on. Notably, there is no interpretation of quoting or escaping, which may vary
// depending on the search being performed. All validation with respect to such
// properties, and how these should be interpretted, is thus context dependent
// and handled appropriately within those contexts.
func ScanParameter(parameter []byte) Parameter {
	result := fieldValuePattern.FindSubmatch(parameter)
	if result != nil {
		if result[1][0] == '-' {
			return Parameter{
				Field:   string(result[1][1:]),
				Value:   string(result[2]),
				Negated: true,
			}
		}
		return Parameter{Field: string(result[1]), Value: string(result[2])}
	}
	return Parameter{Field: "", Value: string(parameter)}
}

// ScanSearchPatternHeuristic scans for a pattern using a heuristic that allows it to
// contain parentheses, if balanced, with appropriate lexical handling for
// traditional escape sequences, escaped parentheses, and escaped whitespace.
func ScanSearchPatternHeuristic(buf []byte) ([]string, int, bool) {
	var count, advance, balanced int
	var r rune
	var piece []rune
	var pieces []string

	next := func() rune {
		r, advance := utf8.DecodeRune(buf)
		count += advance
		buf = buf[advance:]
		return r
	}

loop:
	for len(buf) > 0 {
		r = next()
		switch {
		case unicode.IsSpace(r) && balanced == 0:
			// Stop scanning a potential pattern when we see
			// whitespace in a balanced state.
			break loop
		case r == '(':
			balanced++
			piece = append(piece, r)
		case r == ')':
			balanced--
			piece = append(piece, r)
		case unicode.IsSpace(r):
			// We see a space and the pattern is unbalanced, so assume this
			// terminates a piece of an incomplete search pattern.
			if len(piece) > 0 {
				pieces = append(pieces, string(piece))
			}
			piece = piece[:0]
		case r == '\\':
			// Handle escape sequence.
			if len(buf[advance:]) > 0 {
				r = next()
				switch r {
				case 'a', 'b', 'f', 'n', 'r', 't', 'v', '\\', '"', '\'', '(', ')':
					piece = append(piece, '\\', r)
				default:
					// Unrecognized escape sequence.
					return pieces, count, false
				}
			} else {
				// Unterminated escape sequence.
				return pieces, count, false
			}
		default:
			piece = append(piece, r)
		}

	}
	if len(piece) > 0 {
		pieces = append(pieces, string(piece))
	}
	return pieces, count, balanced == 0
}

// ParseSearchPatternHeuristic heuristically parses a search pattern containing
// parentheses at the current position. There are cases where we want to
// interpret parentheses as part of a search pattern, rather than an and/or
// expression group. For example, In the regex foo(a|b)bar, we want to preserve
// parentheses as part of the pattern.
func (p *parser) ParseSearchPatternHeuristic() (Node, bool) {
	if !p.heuristic {
		return Parameter{Field: "", Value: ""}, false
	}
	start := p.pos
	pieces, advance, ok := ScanSearchPatternHeuristic(p.buf[p.pos:])
	end := start + advance
	if !ok || len(p.buf[start:end]) == 0 || !isPureSearchPattern(p.buf[start:end]) {
		// We tried validating the pattern but it is either unbalanced
		// or malformed, empty, or an invalid and/or expression.
		return Parameter{Field: "", Value: ""}, false
	}
	// The heuristic succeeds: we can process the string as a pure search pattern.
	p.pos += advance
	if len(pieces) == 1 {
		return Parameter{Field: "", Value: pieces[0]}, true
	}
	parameters := []Node{}
	for _, piece := range pieces {
		parameters = append(parameters, Parameter{Field: "", Value: piece})
	}
	return Operator{Kind: Concat, Operands: parameters}, true
}

// ParseParameter returns valid leaf node values for AND/OR queries, taking into
// account escape sequences for special syntax: whitespace and parentheses.
func (p *parser) ParseParameter() Parameter {
	start := p.pos
	for {
		if p.expect(`\ `) || p.expect(`\(`) || p.expect(`\)`) {
			continue
		}
		if p.match(LPAREN) || p.match(RPAREN) {
			break
		}
		if p.done() {
			break
		}
		if isSpace(p.buf[p.pos:]) {
			break
		}
		p.pos++
	}
	return ScanParameter(p.buf[start:p.pos])
}

// containsPattern returns true if any descendent of nodes is a search pattern
// (i.e., a parameter where the field is the empty string).
func containsPattern(node Node) bool {
	var result bool
	VisitField([]Node{node}, "", func(_ string, _ bool) {
		result = true
	})
	return result
}

// returns true if descendent of node contains and/or expressions.
func containsAndOrExpression(nodes []Node) bool {
	var result bool
	VisitOperator(nodes, func(kind operatorKind, _ []Node) {
		if kind == And || kind == Or {
			result = true
		}
	})
	return result
}

// partitionParameters constructs a parse tree to distinguish terms where
// ordering is insignificant (e.g., "repo:foo file:bar") versus terms where
// ordering may be significant (e.g., search patterns like "foo bar"). Search
// patterns are parameters whose field is the empty string.
//
// The resulting tree defines an ordering relation on nodes in the following cases:
// (1) When more than one search patterns exist at the same operator level, they
// are concatenated in order.
// (2) Any nonterminal node is concatenated (ordered in the tree) if its
// descendents contain one or more search patterns.
func partitionParameters(nodes []Node) []Node {
	var patterns, unorderedParams []Node
	for _, n := range nodes {
		switch v := n.(type) {
		case Parameter:
			if v.Field == "" {
				patterns = append(patterns, n)
			} else {
				unorderedParams = append(unorderedParams, n)
			}
		case Operator:
			if containsPattern(n) {
				patterns = append(patterns, n)
			} else {
				unorderedParams = append(unorderedParams, n)
			}
		}
	}
	if len(patterns) > 1 {
		orderedPatterns := newOperator(patterns, Concat)
		return newOperator(append(unorderedParams, orderedPatterns...), And)
	}
	return newOperator(append(unorderedParams, patterns...), And)
}

// parseParameterParameterList scans for consecutive leaf nodes.
func (p *parser) parseParameterList() ([]Node, error) {
	var nodes []Node
loop:
	for {
		if err := p.skipSpaces(); err != nil {
			return nil, err
		}
		if p.done() {
			break loop
		}
		switch {
		case p.match(LPAREN):
			// First try parse a parameter as a search pattern containing parens.
			if patterns, ok := p.ParseSearchPatternHeuristic(); ok {
				nodes = append(nodes, patterns)
			} else {
				// If the above failed, we treat this paren
				// group as part of an and/or expression.
				_ = p.expect(LPAREN) // Guaranteed to succeed.
				p.balanced++
				result, err := p.parseOr()
				if err != nil {
					return nil, err
				}
				nodes = append(nodes, result...)
			}
		case p.expect(RPAREN):
			p.balanced--
			if len(nodes) == 0 {
				// We parsed "()".
				if p.heuristic {
					// Interpret literally.
					nodes = []Node{Parameter{Value: "()"}}
				} else {
					// Interpret as a group: return an empty non-nil node.
					nodes = []Node{Parameter{Value: ""}}
				}
			}
			break loop
		case p.match(AND), p.match(OR):
			// Caller advances.
			break loop
		default:
			// First try parse a parameter as a search pattern containing parens.
			if parameter, ok := p.ParseSearchPatternHeuristic(); ok {
				nodes = append(nodes, parameter)
			} else {
				parameter := p.ParseParameter()
				nodes = append(nodes, parameter)
			}
		}
	}
	return partitionParameters(nodes), nil
}

// reduce takes lists of left and right nodes and reduces them if possible. For example,
// (and a (b and c))       => (and a b c)
// (((a and b) or c) or d) => (or (and a b) c d)
func reduce(left, right []Node, kind operatorKind) ([]Node, bool) {
	if param, ok := left[0].(Parameter); ok && param.Value == "" {
		// Remove empty string parameter.
		return right, true
	}

	switch term := right[0].(type) {
	case Operator:
		if kind == term.Kind {
			// Reduce right node.
			left = append(left, term.Operands...)
			if len(right) > 1 {
				left = append(left, right[1:]...)
			}
			return left, true
		}
	case Parameter:
		if term.Value == "" {
			// Remove empty string parameter.
			if len(right) > 1 {
				return append(left, right[1:]...), true
			}
			return left, true
		}
		if operator, ok := left[0].(Operator); ok && operator.Kind == kind {
			// Reduce left node.
			return append(operator.Operands, right...), true

		}
	}
	if len(right) > 1 {
		// Reduce right list.
		reduced, changed := reduce(append(left, right[0]), right[1:], kind)
		if changed {
			return reduced, true
		}
	}
	return append(left, right...), false
}

// newOperator constructs a new node of kind operatorKind with operands nodes,
// reducing nodes as needed.
func newOperator(nodes []Node, kind operatorKind) []Node {
	if len(nodes) == 0 {
		return nil
	} else if len(nodes) == 1 {
		return nodes
	}

	reduced, changed := reduce([]Node{nodes[0]}, nodes[1:], kind)
	if changed {
		return newOperator(reduced, kind)
	}
	return []Node{Operator{Kind: kind, Operands: reduced}}
}

// parseAnd parses and-expressions.
func (p *parser) parseAnd() ([]Node, error) {
	left, err := p.parseParameterList()
	if err != nil {
		return nil, err
	}
	if left == nil {
		return nil, fmt.Errorf("expected operand at %d", p.pos)
	}
	if !p.expect(AND) {
		return left, nil
	}
	right, err := p.parseAnd()
	if err != nil {
		return nil, err
	}
	return newOperator(append(left, right...), And), nil
}

// parseOr parses or-expressions. Or operators have lower precedence than And
// operators, therefore this function calls parseAnd.
func (p *parser) parseOr() ([]Node, error) {
	left, err := p.parseAnd()
	if err != nil {
		return nil, err
	}
	if left == nil {
		return nil, fmt.Errorf("expected operand at %d", p.pos)
	}
	if !p.expect(OR) {
		return left, nil
	}
	right, err := p.parseOr()
	if err != nil {
		return nil, err
	}
	return newOperator(append(left, right...), Or), nil
}

// parseAndOr a raw input string into a parse tree comprising Nodes.
func parseAndOr(in string) ([]Node, error) {
	if in == "" {
		return nil, nil
	}
	parser := &parser{buf: []byte(in), heuristic: true}
	nodes, err := parser.parseOr()
	if err != nil {
		return nil, err
	}
	if parser.balanced != 0 {
		return nil, errors.New("unbalanced expression")
	}
	return newOperator(nodes, And), nil
}

func ParseAndOr(in string) (QueryInfo, error) {
	query, err := parseAndOr(in)
	if err != nil {
		return nil, err
	}
	return &AndOrQuery{Query: query}, nil
}
