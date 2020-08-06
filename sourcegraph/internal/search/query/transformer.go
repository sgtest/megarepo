package query

import (
	"errors"
	"fmt"
	"regexp"
	"strings"
	"unicode"

	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"
)

// SubstituteAliases substitutes field name aliases for their canonical names.
func SubstituteAliases(nodes []Node) []Node {
	aliases := map[string]string{
		"r":        FieldRepo,
		"g":        FieldRepoGroup,
		"f":        FieldFile,
		"l":        FieldLang,
		"language": FieldLang,
		"since":    FieldAfter,
		"until":    FieldBefore,
		"m":        FieldMessage,
		"msg":      FieldMessage,
	}
	return MapParameter(nodes, func(field, value string, negated bool, annotation Annotation) Node {
		if field == "content" {
			return Pattern{Value: value, Negated: negated, Annotation: annotation}
		}
		if canonical, ok := aliases[field]; ok {
			field = canonical
		}
		return Parameter{Field: field, Value: value, Negated: negated, Annotation: annotation}
	})
}

// LowercaseFieldNames performs strings.ToLower on every field name.
func LowercaseFieldNames(nodes []Node) []Node {
	return MapParameter(nodes, func(field, value string, negated bool, annotation Annotation) Node {
		return Parameter{Field: strings.ToLower(field), Value: value, Negated: negated, Annotation: annotation}
	})
}

var ErrBadGlobPattern = errors.New("syntax error in glob pattern")

// translateCharacterClass translates character classes like [a-zA-Z].
func translateCharacterClass(r []rune, startIx int) (int, string, error) {
	sb := strings.Builder{}
	i := startIx
	lenR := len(r)

	switch r[i] {
	case '!':
		if i < lenR-1 && r[i+1] == ']' {
			// the character class cannot contain just "!"
			return -1, "", ErrBadGlobPattern
		}
		sb.WriteRune('^')
		i++
	case '^':
		sb.WriteString("//^")
		i++
	}

	for i < lenR {
		if r[i] == ']' {
			if i > startIx {
				break
			}
			sb.WriteRune(r[i])
			i++
			continue
		}

		lo := r[i]
		sb.WriteRune(r[i]) // lo
		i++
		if i == lenR {
			// no closing bracket
			return -1, "", ErrBadGlobPattern
		}

		// lo = hi
		if r[i] != '-' {
			continue
		}

		sb.WriteRune(r[i]) // -
		i++
		if i == lenR {
			// no closing bracket
			return -1, "", ErrBadGlobPattern
		}

		if r[i] == ']' {
			continue
		}

		hi := r[i]
		if lo > hi {
			// range is reversed
			return -1, "", ErrBadGlobPattern
		}
		sb.WriteRune(r[i]) // hi
		i++
	}
	if i == lenR {
		return -1, "", ErrBadGlobPattern
	}
	return i - startIx, sb.String(), nil
}

var globSpecialSymbols = map[rune]struct{}{
	'\\': {},
	'*':  {},
	'?':  {},
	'[':  {},
}

// globToRegex converts a glob string to a regular expression.
// We support: *, ?, and character classes [...].
func globToRegex(value string) (string, error) {
	if value == "" {
		return value, nil
	}

	r := []rune(value)
	l := len(r)
	sb := strings.Builder{}

	// Add regex anchor "^" as prefix to all patterns
	sb.WriteRune('^')

	for i := 0; i < l; i++ {
		switch r[i] {
		case '*':
			// **
			if i < l-1 && r[i+1] == '*' {
				sb.WriteString(".*?")
			} else {
				sb.WriteString("[^/]*?")
			}
			// Skip repeated '*'.
			for i < l-1 && r[i+1] == '*' {
				i++
			}
		case '?':
			sb.WriteRune('.')
		case '\\':
			// trailing backslashes are not allowed
			if i == l-1 {
				return "", ErrBadGlobPattern
			}

			sb.WriteRune('\\')
			i++

			// we only support escaping of special characters
			if _, ok := globSpecialSymbols[r[i]]; !ok {
				return "", ErrBadGlobPattern
			}
			sb.WriteRune(r[i])
		case '[':
			if i == l-1 {
				return "", ErrBadGlobPattern
			}
			sb.WriteRune('[')
			i++

			advanced, s, err := translateCharacterClass(r, i)
			if err != nil {
				return "", err
			}

			i += advanced
			sb.WriteString(s)

			sb.WriteRune(']')
		default:
			sb.WriteString(regexp.QuoteMeta(string(r[i])))
		}
	}
	// add regex anchor '$' as suffix to all patterns
	sb.WriteRune('$')
	return sb.String(), nil
}

// globError carries the error message and the name of
// field where the error occurred.
type globError struct {
	field string
	err   error
}

func (g globError) Error() string {
	return g.err.Error()
}

// reporevToRegex is a wrapper around globToRegex that takes care of
// treating repo and rev (as in repo@rev) separately during translation
// from glob to regex.
func reporevToRegex(value string) (string, error) {
	reporev := strings.SplitN(value, "@", 2)
	repo := reporev[0]
	if ContainsNoGlobSyntax(repo) {
		repo = fuzzifyGlobPattern(repo)
	}
	repo, err := globToRegex(repo)
	if err != nil {
		return "", err
	}
	value = repo
	if len(reporev) > 1 {
		value = value + "@" + reporev[1]
	}
	return value, nil
}

var globSyntax = lazyregexp.New(`[][*?/]`)

func ContainsNoGlobSyntax(value string) bool {
	return !globSyntax.MatchString(value)
}

func fuzzifyGlobPattern(value string) string {
	if value == "" {
		return value
	}
	return "**" + value + "**"
}

// mapGlobToRegex translates glob to regexp for fields repo, file, and repohasfile.
func mapGlobToRegex(nodes []Node) ([]Node, error) {
	var globErrors []globError

	nodes = MapParameter(nodes, func(field, value string, negated bool, annotation Annotation) Node {
		var err error
		switch field {
		case FieldRepo:
			value, err = reporevToRegex(value)
		case FieldFile, FieldRepoHasFile:
			if ContainsNoGlobSyntax(value) {
				value = fuzzifyGlobPattern(value)
			}
			value, err = globToRegex(value)
		}
		if err != nil {
			globErrors = append(globErrors, globError{field: field, err: err})
		}
		return Parameter{Field: field, Value: value, Negated: negated, Annotation: annotation}
	})

	if len(globErrors) == 1 {
		return nil, fmt.Errorf("invalid glob syntax in field %s: ", globErrors[0].field)
	}

	if len(globErrors) > 1 {
		fields := globErrors[0].field + ":"

		for _, e := range globErrors[1:] {
			fields += fmt.Sprintf(", %s:", e.field)
		}
		return nil, fmt.Errorf("invalid glob syntax in fields %s", fields)
	}

	return nodes, nil
}

// Hoist is a heuristic that rewrites simple but possibly ambiguous queries. It
// changes certain expressions in a way that some consider to be more natural.
// For example, the following query without parentheses is interpreted as
// follows in the grammar:
//
// repo:foo a or b and c => (repo:foo a) or ((b) and (c))
//
// This function rewrites the above expression as follows:
//
// repo:foo a or b and c => repo:foo (a or b and c)
//
// Any number of field:value parameters may occur before and after the pattern
// expression separated by or- or and-operators, and these are hoisted out. The
// pattern expression must be contiguous. If not, we want to preserve the
// default interpretation, which corresponds more naturally to groupings with
// field parameters, i.e.,
//
// repo:foo a or b or repo:bar c => (repo:foo a) or (b) or (repo:bar c)
func Hoist(nodes []Node) ([]Node, error) {
	if len(nodes) != 1 {
		return nil, fmt.Errorf("heuristic requires one top-level expression")
	}

	expression, ok := nodes[0].(Operator)
	if !ok || expression.Kind == Concat {
		return nil, fmt.Errorf("heuristic requires top-level and- or or-expression")
	}

	n := len(expression.Operands)
	var pattern []Node
	var scopeParameters []Node
	for i, node := range expression.Operands {
		if i == 0 || i == n-1 {
			scopePart, patternPart, err := PartitionSearchPattern([]Node{node})
			if err != nil || patternPart == nil {
				return nil, errors.New("could not partition first or last expression")
			}
			pattern = append(pattern, patternPart)
			scopeParameters = append(scopeParameters, scopePart...)
			continue
		}
		if !isPatternExpression([]Node{node}) {
			return nil, fmt.Errorf("inner expression %s is not a pure pattern expression", node.String())
		}
		pattern = append(pattern, node)
	}
	pattern = MapPattern(pattern, func(value string, negated bool, annotation Annotation) Node {
		annotation.Labels |= HeuristicHoisted
		return Pattern{Value: value, Negated: negated, Annotation: annotation}
	})
	return append(scopeParameters, newOperator(pattern, expression.Kind)...), nil
}

// SearchUppercase adds case:yes to queries if any pattern is mixed-case.
func SearchUppercase(nodes []Node) []Node {
	var foundMixedCase bool
	VisitPattern(nodes, func(value string, _ bool, _ Annotation) {
		if match := containsUppercase(value); match {
			foundMixedCase = true
		}
	})
	if foundMixedCase {
		nodes = append(nodes, Parameter{Field: "case", Value: "yes"})
		return newOperator(nodes, And)
	}
	return nodes
}

func containsUppercase(s string) bool {
	for _, r := range s {
		if unicode.IsUpper(r) && unicode.IsLetter(r) {
			return true
		}
	}
	return false
}

// partition partitions nodes into left and right groups. A node is put in the
// left group if fn evaluates to true, or in the right group if fn evaluates to false.
func partition(nodes []Node, fn func(node Node) bool) (left, right []Node) {
	for _, node := range nodes {
		if fn(node) {
			left = append(left, node)
		} else {
			right = append(right, node)
		}
	}
	return left, right
}

// product appends the list of n elements in right to each of the m rows in
// left. If left is empty, it is initialized with right.
func product(left [][]Node, right []Node) [][]Node {
	result := [][]Node{}
	if len(left) == 0 {
		return append(result, right)
	}

	for _, row := range left {
		newRow := make([]Node, len(row))
		copy(newRow, row)
		result = append(result, append(newRow, right...))
	}
	return result
}

// distribute applies the distributed property to nodes. See the dnf function
// for context. Its first argument takes the current set of prefixes to prepend
// to each term in an or-expression.
func distribute(prefixes [][]Node, nodes []Node) [][]Node {
	for _, node := range nodes {
		switch v := node.(type) {
		case Operator:
			switch v.Kind {
			case Or:
				result := [][]Node{}
				for _, o := range v.Operands {
					var newPrefixes [][]Node
					newPrefixes = distribute(newPrefixes, []Node{o})
					for _, newPrefix := range newPrefixes {
						result = append(result, product(prefixes, newPrefix)...)
					}
				}
				prefixes = result
			case And, Concat:
				prefixes = distribute(prefixes, v.Operands)
			}
		case Parameter, Pattern:
			prefixes = product(prefixes, []Node{v})
		}
	}
	return prefixes
}

// dnf returns the Disjunctive Normal Form of a query (a flat sequence of
// or-expressions) by applying the distributive property on (possibly nested)
// or-expressions. For example, the query:
//
// (repo:a (file:b OR file:c))
// in DNF becomes:
// (repo:a file:b) OR (repo:a file:c)
//
// Using the DNF expression makes it easy to support general nested queries that
// imply scope, like the one above: We simply evaluate all disjuncts and union
// the results. Note that various optimizations are possible
// during evaluation, but those are separate query pre- or postprocessing steps
// separate from this general transformation.
func dnf(query []Node) [][]Node {
	return distribute([][]Node{}, query)
}

func substituteOrForRegexp(nodes []Node) []Node {
	isPattern := func(node Node) bool {
		if pattern, ok := node.(Pattern); ok && !pattern.Negated {
			return true
		}
		return false
	}
	new := []Node{}
	for _, node := range nodes {
		switch v := node.(type) {
		case Operator:
			if v.Kind == Or {
				patterns, rest := partition(v.Operands, isPattern)
				var values []string
				for _, node := range patterns {
					values = append(values, node.(Pattern).Value)
				}
				valueString := "(" + strings.Join(values, ")|(") + ")"
				new = append(new, Pattern{Value: valueString})
				if len(rest) > 0 {
					rest = substituteOrForRegexp(rest)
					new = newOperator(append(new, rest...), Or)
				}
			} else {
				new = append(new, newOperator(substituteOrForRegexp(v.Operands), v.Kind)...)
			}
		case Parameter, Pattern:
			new = append(new, node)
		}
	}
	return new
}

// substituteConcat reduces a concatenation of patterns to a separator-separated string.
func substituteConcat(nodes []Node, separator string) []Node {
	isPattern := func(node Node) bool {
		if pattern, ok := node.(Pattern); ok && !pattern.Negated {
			return true
		}
		return false
	}
	new := []Node{}
	for _, node := range nodes {
		switch v := node.(type) {
		case Parameter, Pattern:
			new = append(new, node)
		case Operator:
			if v.Kind == Concat {
				// Merge consecutive patterns.
				previous := v.Operands[0]
				merged := Pattern{}
				if p, ok := previous.(Pattern); ok {
					merged = p
				}
				for _, node := range v.Operands[1:] {
					if isPattern(node) && isPattern(previous) {
						p := node.(Pattern)
						if merged.Value != "" {
							merged.Annotation.Labels |= p.Annotation.Labels
							merged = Pattern{
								Value:      merged.Value + separator + p.Value,
								Annotation: merged.Annotation,
							}
						} else {
							// Base case.
							merged = Pattern{Value: p.Value}
						}
						previous = node
						continue
					}
					if merged.Value != "" {
						new = append(new, merged)
						merged = Pattern{}
					}
					new = append(new, substituteConcat([]Node{node}, separator)...)
				}
				if merged.Value != "" {
					new = append(new, merged)
					merged = Pattern{}
				}
			} else {
				new = append(new, newOperator(substituteConcat(v.Operands, separator), v.Kind)...)
			}
		}
	}
	return new
}

// TrailingParensToLiteral is a heuristic used in the context of regular
// expression search. It checks whether any pattern is annotated with a label
// HeusticDanglingParens. This label implies that the regular expression is not
// well-formed, for example, "foo.*bar(" or "foo(.*bar". As a special case for
// usability we escape a trailing parenthesis and treat it literally. Any other
// forms are ignored, and will likely not pass validation.
func TrailingParensToLiteral(nodes []Node) []Node {
	return MapPattern(nodes, func(value string, negated bool, annotation Annotation) Node {
		if annotation.Labels.isSet(HeuristicDanglingParens) && strings.HasSuffix(value, "(") {
			value = strings.TrimSuffix(value, "(")
			value += `\(`
		}
		return Pattern{
			Value:      value,
			Negated:    negated,
			Annotation: annotation,
		}
	})
}

// EmptyGroupsToLiteral is a heuristic used in the context of regular expression
// search. It labels any pattern containing "()" as a literal pattern since in
// regex it implies the empty string, which is meaningless as a search query and
// probably not what the user intended.
func EmptyGroupsToLiteral(nodes []Node) []Node {
	return MapPattern(nodes, func(value string, negated bool, annotation Annotation) Node {
		if ok, _ := regexp.MatchString(`\(\)`, value); ok {
			annotation.Labels.set(Literal)
			annotation.Labels.unset(Regexp)
		}
		return Pattern{
			Value:      value,
			Negated:    negated,
			Annotation: annotation,
		}
	})
}

// Map pipes query through one or more query transformer functions.
func Map(query []Node, fns ...func([]Node) []Node) []Node {
	for _, fn := range fns {
		query = fn(query)
	}
	return query
}

func FuzzifyRegexPatterns(nodes []Node) []Node {
	return MapParameter(nodes, func(field string, value string, negated bool, annotation Annotation) Node {
		if field == FieldRepo || field == FieldFile || field == FieldRepoHasFile {
			value = strings.TrimSuffix(value, "$")
		}
		return Parameter{Field: field, Value: value, Negated: negated, Annotation: annotation}
	})
}
