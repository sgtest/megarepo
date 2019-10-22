package syntax

import (
	"bytes"
	"fmt"
	"regexp"
	"strings"
)

// The parse tree for search input. It is a list of expressions.
type ParseTree []*Expr

func (p ParseTree) String() string {
	return ExprString(p)
}

// Values returns the raw string values associated with a field.
func (p ParseTree) Values(field string) []string {
	var v []string
	for _, expr := range p {
		if expr.Field == field {
			v = append(v, expr.Value)
		}
	}
	return v
}

// WithErrorsQuoted converts a search input like `f:foo b(ar` to `f:foo "b(ar"`.
func (p ParseTree) WithErrorsQuoted() ParseTree {
	p2 := []*Expr{}
	for _, e := range p {
		e2 := e.WithErrorsQuoted()
		p2 = append(p2, &e2)
	}
	return p2
}

// An Expr describes an expression in the parse tree.
type Expr struct {
	Pos       int       // the starting character position of the expression
	Not       bool      // the expression is negated (e.g., -term or -field:term)
	Field     string    // the field that this expression applies to
	Value     string    // the raw field value
	ValueType TokenType // the type of the value
}

func (e Expr) String() string {
	var buf bytes.Buffer
	if e.Not {
		buf.WriteByte('-')
	}
	if e.Field != "" {
		buf.WriteString(e.Field)
		buf.WriteByte(':')
	}
	if e.ValueType == TokenPattern {
		buf.WriteByte('/')
	}
	buf.WriteString(e.Value)
	if e.ValueType == TokenPattern {
		buf.WriteByte('/')
	}
	return buf.String()
}

// WithErrorsQuoted returns a new version of the expression,
// quoting in case of TokenError or an invalid regular expression.
func (e Expr) WithErrorsQuoted() Expr {
	e2 := e
	needsQuoting := false
	switch e.ValueType {
	case TokenError:
		needsQuoting = true
	case TokenPattern, TokenLiteral:
		_, err := regexp.Compile(e2.Value)
		if err != nil {
			needsQuoting = true
		}
	}
	if needsQuoting {
		e2.Not = false
		e2.Field = ""
		e2.Value = fmt.Sprintf("%q", e.String())
		e2.ValueType = TokenQuoted
	}
	return e2
}

// ExprString returns the string that parses to expr.
func ExprString(expr []*Expr) string {
	s := make([]string, len(expr))
	for i, e := range expr {
		s[i] = e.String()
	}
	return strings.Join(s, " ")
}
