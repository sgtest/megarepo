package query

import (
	"fmt"
	"regexp"
	"strconv"
	"strings"

	"github.com/src-d/enry/v2"
)

type UnsupportedError struct {
	UnsupportedMsg string
}

func (e *UnsupportedError) Error() string {
	return e.UnsupportedMsg
}

// isPatternExpression returns true if every leaf node in a tree root at node is
// a search pattern.
func isPatternExpression(nodes []Node) bool {
	result := true
	VisitParameter(nodes, func(field, _ string, _, _ bool) {
		if field != "" && field != "content" {
			result = false
		}
	})
	return result
}

// ContainsAndOrKeyword returns true if this query contains or- or and-
// keywords. It is a temporary signal to determine whether we can fallback to
// the older existing search functionality.
func ContainsAndOrKeyword(input string) bool {
	lower := strings.ToLower(input)
	return strings.Contains(lower, " and ") || strings.Contains(lower, " or ")
}

// processTopLevel processes the top level of a query. It validates that we can
// process the query with respect to and/or expressions on file content, but not
// otherwise for nested parameters.
func processTopLevel(nodes []Node) ([]Node, error) {
	if term, ok := nodes[0].(Operator); ok {
		if term.Kind == And && isPatternExpression([]Node{term}) {
			return nodes, nil
		} else if term.Kind == Or && isPatternExpression([]Node{term}) {
			return nodes, nil
		} else if term.Kind == And {
			return term.Operands, nil
		} else if term.Kind == Concat {
			return nodes, nil
		} else {
			return nil, &UnsupportedError{UnsupportedMsg: "cannot evaluate: unable to partition pure search pattern"}
		}
	}
	return nodes, nil
}

// PartitionSearchPattern partitions an and/or query into (1) a single search
// pattern expression and (2) other parameters that scope the evaluation of
// search patterns (e.g., to repos, files, etc.). It validates that a query
// contains at most one search pattern expression and that scope parameters do
// not contain nested expressions.
func PartitionSearchPattern(nodes []Node) (parameters []Node, pattern Node, err error) {
	if len(nodes) == 1 {
		nodes, err = processTopLevel(nodes)
		if err != nil {
			return nil, nil, err
		}
	}

	var patterns []Node
	for _, node := range nodes {
		if isPatternExpression([]Node{node}) {
			patterns = append(patterns, node)
		} else if term, ok := node.(Parameter); ok {
			parameters = append(parameters, term)
		} else {
			return nil, nil, &UnsupportedError{UnsupportedMsg: "cannot evaluate: unable to partition pure search pattern"}
		}
	}
	if len(patterns) > 1 {
		pattern = Operator{Kind: And, Operands: patterns}
	} else if len(patterns) == 1 {
		pattern = patterns[0]
	}

	return parameters, pattern, nil
}

// isPureSearchPattern implements a heuristic that returns true if buf, possibly
// containing whitespace or balanced parentheses, can be treated as a search
// pattern in the and/or grammar.
func isPureSearchPattern(buf []byte) bool {
	// Check if the balanced string we scanned is perhaps an and/or expression by parsing without the heuristic.
	try := &parser{
		buf:       buf,
		heuristic: heuristic{parensAsPatterns: false},
	}
	result, err := try.parseOr()
	if err != nil {
		// This is not an and/or expression, but it is balanced. It
		// could be, e.g., (foo or). Reject this sort of pattern for now.
		return false
	}
	if try.balanced != 0 {
		return false
	}
	if containsAndOrExpression(result) {
		// The balanced string is an and/or expression in our grammar,
		// so it cannot be interpreted as a search pattern.
		return false
	}
	if !isPatternExpression(newOperator(result, Concat)) {
		// The balanced string contains other parameters, like
		// "repo:foo", which are not search patterns.
		return false
	}
	return true
}

// parseBool is like strconv.ParseBool except that it also accepts y, Y, yes,
// YES, Yes, n, N, no, NO, No.
func parseBool(s string) (bool, error) {
	switch strings.ToLower(s) {
	case "y", "yes":
		return true, nil
	case "n", "no":
		return false, nil
	default:
		b, err := strconv.ParseBool(s)
		if err != nil {
			err = fmt.Errorf("invalid boolean %q", s)
		}
		return b, err
	}
}

func validateField(field, value string, negated bool, seen map[string]struct{}) error {
	isNotNegated := func() error {
		if negated {
			return fmt.Errorf("field %q does not support negation", field)
		}
		return nil
	}

	isSingular := func() error {
		if _, notSingular := seen[field]; notSingular {
			return fmt.Errorf("field %q may not be used more than once", field)
		}
		return nil
	}

	isValidRegexp := func() error {
		if _, err := regexp.Compile(value); err != nil {
			return err
		}
		return nil
	}

	isBoolean := func() error {
		if _, err := parseBool(value); err != nil {
			return err
		}
		return nil
	}

	isNumber := func() error {
		count, err := strconv.ParseInt(value, 10, 32)
		if err != nil {
			if err.(*strconv.NumError).Err == strconv.ErrRange {
				return fmt.Errorf("field %s has a value that is out of range, try making it smaller", field)
			}
			return fmt.Errorf("field %s has value %[2]s, %[2]s is not a number", field, value)
		}
		if count <= 0 {
			return fmt.Errorf("field %s requires a positive number", field)
		}
		return nil
	}

	isLanguage := func() error {
		_, ok := enry.GetLanguageByAlias(value)
		if !ok {
			return fmt.Errorf("unknown language: %q", value)
		}
		return nil
	}

	isUnrecognizedField := func() error {
		return fmt.Errorf("unrecognized field %q", field)
	}

	satisfies := func(fns ...func() error) error {
		for _, fn := range fns {
			if err := fn(); err != nil {
				return err
			}
		}
		return nil
	}

	switch field {
	case
		FieldDefault:
		// Search patterns are not validated here, as it depends on the search type.
	case
		FieldCase:
		return satisfies(isSingular, isBoolean, isNotNegated)
	case
		FieldRepo, "r":
		return satisfies(isValidRegexp)
	case
		FieldRepoGroup, "g":
		return satisfies(isSingular, isNotNegated)
	case
		FieldFile, "f":
		return satisfies(isValidRegexp)
	case
		FieldFork,
		FieldArchived:
		return satisfies(isSingular, isNotNegated)
	case
		FieldLang, "l", "language":
		return satisfies(isLanguage)
	case
		FieldType:
		return satisfies(isNotNegated)
	case
		FieldPatternType,
		FieldContent:
		return satisfies(isSingular, isNotNegated)
	case
		FieldRepoHasFile:
		return satisfies(isValidRegexp)
	case
		FieldRepoHasCommitAfter:
		return satisfies(isSingular, isNotNegated)
	case
		FieldBefore, "until",
		FieldAfter, "since":
		return satisfies(isNotNegated)
	case
		FieldAuthor,
		FieldCommitter,
		FieldMessage, "m", "msg":
		return satisfies(isValidRegexp)
	case
		FieldIndex:
		return satisfies(isSingular, isNotNegated)
	case
		FieldCount:
		return satisfies(isSingular, isNumber, isNotNegated)
	case
		FieldStable:
		return satisfies(isSingular, isBoolean, isNotNegated)
	case
		FieldMax,
		FieldTimeout,
		FieldReplace,
		FieldCombyRule:
		return satisfies(isSingular, isNotNegated)
	default:
		return isUnrecognizedField()
	}
	return nil
}

func validate(nodes []Node) error {
	var err error
	seen := map[string]struct{}{}
	VisitParameter(nodes, func(field, value string, negated, _ bool) {
		if err != nil {
			return
		}
		err = validateField(field, value, negated, seen)
		seen[field] = struct{}{}
	})
	return err
}
