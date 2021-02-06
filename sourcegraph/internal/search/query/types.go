package query

import (
	"fmt"
	"regexp"
	"strings"

	"github.com/sourcegraph/sourcegraph/internal/search/query/syntax"
)

type ExpectedOperand struct {
	Msg string
}

func (e *ExpectedOperand) Error() string {
	return e.Msg
}

type UnsupportedError struct {
	Msg string
}

func (e *UnsupportedError) Error() string {
	return e.Msg
}

type SearchType int

const (
	SearchTypeRegex SearchType = iota
	SearchTypeLiteral
	SearchTypeStructural
)

// QueryInfo is an interface for accessing query values that drive our search logic.
// It will be removed in favor of a cleaner query API to access values.
type QueryInfo interface {
	RegexpPatterns(field string) (values, negatedValues []string)
	StringValues(field string) (values, negatedValues []string)
	StringValue(field string) (value, negatedValue string)
	Values(field string) []*Value
	Fields() map[string][]*Value
	BoolValue(field string) bool
	IsCaseSensitive() bool
}

// A query is a tree of Nodes.
type Query []Node

func (q Query) String() string {
	var v []string
	for _, node := range q {
		v = append(v, node.String())
	}
	return strings.Join(v, " ")
}

// Query satisfies the interface for QueryInfo close to that of OrdinaryQuery.
func (q Query) RegexpPatterns(field string) (values, negatedValues []string) {
	VisitField(q, field, func(visitedValue string, negated bool, _ Annotation) {
		if negated {
			negatedValues = append(negatedValues, visitedValue)
		} else {
			values = append(values, visitedValue)
		}
	})
	return values, negatedValues
}

func (q Query) StringValues(field string) (values, negatedValues []string) {
	VisitField(q, field, func(visitedValue string, negated bool, _ Annotation) {
		if negated {
			negatedValues = append(negatedValues, visitedValue)
		} else {
			values = append(values, visitedValue)
		}
	})
	return values, negatedValues
}

func (q Query) StringValue(field string) (value, negatedValue string) {
	VisitField(q, field, func(visitedValue string, negated bool, _ Annotation) {
		if negated {
			negatedValue = visitedValue
		} else {
			value = visitedValue
		}
	})
	return value, negatedValue
}

func (q Query) Values(field string) []*Value {
	var values []*Value
	if field == "" {
		VisitPattern(q, func(value string, _ bool, annotation Annotation) {
			values = append(values, q.valueToTypedValue(field, value, annotation.Labels)...)
		})
	} else {
		VisitField(q, field, func(value string, _ bool, _ Annotation) {
			values = append(values, q.valueToTypedValue(field, value, None)...)
		})
	}
	return values
}

func (q Query) Fields() map[string][]*Value {
	fields := make(map[string][]*Value)
	VisitPattern(q, func(value string, _ bool, _ Annotation) {
		fields[""] = q.Values("")
	})
	VisitParameter(q, func(field, _ string, _ bool, _ Annotation) {
		fields[field] = q.Values(field)
	})
	return fields
}

// ParseTree returns a flat, mock-like parse tree of an and/or query. The parse
// tree values are currently only significant in alerts. Whether it is empty or
// not is significant for surfacing suggestions.
func (q Query) ParseTree() syntax.ParseTree {
	var tree syntax.ParseTree
	VisitPattern(q, func(value string, negated bool, _ Annotation) {
		expr := &syntax.Expr{
			Field: "",
			Value: value,
			Not:   negated,
		}
		tree = append(tree, expr)
	})
	VisitParameter(q, func(field, value string, negated bool, _ Annotation) {
		expr := &syntax.Expr{
			Field: field,
			Value: value,
			Not:   negated,
		}
		tree = append(tree, expr)
	})
	return tree
}

func (q Query) BoolValue(field string) bool {
	result := false
	VisitField(q, field, func(value string, _ bool, _ Annotation) {
		result, _ = parseBool(value) // err was checked during parsing and validation.
	})
	return result
}

func (q Query) IsCaseSensitive() bool {
	return q.BoolValue("case")
}

func parseRegexpOrPanic(field, value string) *regexp.Regexp {
	r, err := regexp.Compile(value)
	if err != nil {
		panic(fmt.Sprintf("Value %s for field %s invalid regex: %s", field, value, err.Error()))
	}
	return r
}

// valueToTypedValue approximately preserves the field validation for
// OrdinaryQuery processing. It does not check the validity of field negation or
// if the same field is specified more than once.
func (q Query) valueToTypedValue(field, value string, label labels) []*Value {
	switch field {
	case
		FieldDefault:
		if label.isSet(Literal) {
			return []*Value{{String: &value}}
		}
		if label.isSet(Regexp) {
			regexp, err := regexp.Compile(value)
			if err != nil {
				panic(fmt.Sprintf("Invariant broken: value must have been checked to be valid regexp. Error: %s", err))
			}
			return []*Value{{Regexp: regexp}}
		}
		// All patterns should have a label after parsing, but if not, treat the pattern as a string literal.
		return []*Value{{String: &value}}

	case
		FieldCase:
		b, _ := parseBool(value)
		return []*Value{{Bool: &b}}

	case
		FieldRepo, "r":
		return []*Value{{Regexp: parseRegexpOrPanic(field, value)}}

	case
		FieldRepoGroup, "g",
		FieldContext:
		return []*Value{{String: &value}}

	case
		FieldFile, "f":
		return []*Value{{Regexp: parseRegexpOrPanic(field, value)}}

	case
		FieldFork,
		FieldArchived,
		FieldLang, "l", "language",
		FieldType,
		FieldPatternType,
		FieldContent:
		return []*Value{{String: &value}}

	case FieldRepoHasFile:
		return []*Value{{Regexp: parseRegexpOrPanic(field, value)}}

	case
		FieldRepoHasCommitAfter,
		FieldBefore, "until",
		FieldAfter, "since":
		return []*Value{{String: &value}}

	case
		FieldAuthor,
		FieldCommitter,
		FieldMessage, "m", "msg":
		return []*Value{{Regexp: parseRegexpOrPanic(field, value)}}

	case
		FieldIndex,
		FieldCount,
		FieldMax,
		FieldTimeout,
		FieldCombyRule:
		return []*Value{{String: &value}}
	}
	return []*Value{{String: &value}}
}
