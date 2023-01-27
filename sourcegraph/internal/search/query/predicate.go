package query

import (
	"fmt"
	"strings"

	"github.com/grafana/regexp"
	"github.com/grafana/regexp/syntax"

	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type Predicate interface {
	// Field is the name of the field that the predicate applies to.
	// For example, with `repo:contains.file`, Field returns "repo".
	Field() string

	// Name is the name of the predicate.
	// For example, with `repo:contains.file`, Name returns "contains.file".
	Name() string

	// Unmarshal parses the contents of the predicate arguments
	// into the predicate object.
	Unmarshal(params string, negated bool) error
}

var DefaultPredicateRegistry = PredicateRegistry{
	FieldRepo: {
		"contains.file":         func() Predicate { return &RepoContainsFilePredicate{} },
		"has.file":              func() Predicate { return &RepoContainsFilePredicate{} },
		"contains.path":         func() Predicate { return &RepoContainsPathPredicate{} },
		"has.path":              func() Predicate { return &RepoContainsPathPredicate{} },
		"contains.content":      func() Predicate { return &RepoContainsContentPredicate{} },
		"has.content":           func() Predicate { return &RepoContainsContentPredicate{} },
		"contains.commit.after": func() Predicate { return &RepoContainsCommitAfterPredicate{} },
		"has.commit.after":      func() Predicate { return &RepoContainsCommitAfterPredicate{} },
		"has.description":       func() Predicate { return &RepoHasDescriptionPredicate{} },
		"has.tag":               func() Predicate { return &RepoHasTagPredicate{} },
		"has":                   func() Predicate { return &RepoHasKVPPredicate{} },
		"has.key":               func() Predicate { return &RepoHasKeyPredicate{} },
	},
	FieldFile: {
		"contains.content": func() Predicate { return &FileContainsContentPredicate{} },
		"has.content":      func() Predicate { return &FileContainsContentPredicate{} },
		"has.owner":        func() Predicate { return &FileHasOwnerPredicate{} },
	},
}

type NegatedPredicateError struct {
	name string
}

func (e *NegatedPredicateError) Error() string {
	return fmt.Sprintf("search predicate %q does not support negation", e.name)
}

// PredicateTable is a lookup map of one or more predicate names that resolve to the Predicate type.
type PredicateTable map[string]func() Predicate

// PredicateRegistry is a lookup map of predicate tables associated with all fields.
type PredicateRegistry map[string]PredicateTable

// Get returns a predicate for the given field with the given name. It assumes
// it exists, and panics otherwise.
func (pr PredicateRegistry) Get(field, name string) Predicate {
	fieldPredicates, ok := pr[field]
	if !ok {
		panic("predicate lookup for " + field + " is invalid")
	}
	newPredicateFunc, ok := fieldPredicates[name]
	if !ok {
		panic("predicate lookup for " + name + " on " + field + " is invalid")
	}
	return newPredicateFunc()
}

var (
	predicateRegexp = regexp.MustCompile(`^(?P<name>[a-z\.]+)\((?s:(?P<params>.*))\)$`)
	nameIndex       = predicateRegexp.SubexpIndex("name")
	paramsIndex     = predicateRegexp.SubexpIndex("params")
)

// ParsePredicate returns the name and value of syntax conforming to
// name(value). It assumes this syntax is already validated prior. If not, it
// panics.
func ParseAsPredicate(value string) (name, params string) {
	match := predicateRegexp.FindStringSubmatch(value)
	if match == nil {
		panic("Invariant broken: attempt to parse a predicate value " + value + " which appears to have not been properly validated")
	}
	name = match[nameIndex]
	params = match[paramsIndex]
	return name, params
}

// EmptyPredicate is a noop value that satisfies the Predicate interface.
type EmptyPredicate struct{}

func (EmptyPredicate) Field() string { return "" }
func (EmptyPredicate) Name() string  { return "" }
func (EmptyPredicate) Unmarshal(_ string, negated bool) error {
	if negated {
		return &NegatedPredicateError{"empty"}
	}

	return nil
}

// RepoContainsFilePredicate represents the `repo:contains.file()` predicate,
// which filters to repos that contain a path and/or content
type RepoContainsFilePredicate struct {
	Path    string
	Content string
	Negated bool
}

func (f *RepoContainsFilePredicate) Unmarshal(params string, negated bool) error {
	nodes, err := Parse(params, SearchTypeRegex)
	if err != nil {
		return err
	}

	for _, node := range nodes {
		if err := f.parseNode(node); err != nil {
			return err
		}
	}

	if f.Path == "" && f.Content == "" {
		return errors.New("one of path or content must be set")
	}
	f.Negated = negated
	return nil
}

func (f *RepoContainsFilePredicate) parseNode(n Node) error {
	switch v := n.(type) {
	case Parameter:
		if v.Negated {
			return errors.New("predicates do not currently support negated values")
		}
		switch strings.ToLower(v.Field) {
		case "path":
			if f.Path != "" {
				return errors.New("cannot specify path multiple times")
			}
			if _, err := syntax.Parse(v.Value, syntax.Perl); err != nil {
				return errors.Errorf("`contains.file` predicate has invalid `path` argument: %w", err)
			}
			f.Path = v.Value
		case "content":
			if f.Content != "" {
				return errors.New("cannot specify content multiple times")
			}
			if _, err := syntax.Parse(v.Value, syntax.Perl); err != nil {
				return errors.Errorf("`contains.file` predicate has invalid `content` argument: %w", err)
			}
			f.Content = v.Value
		default:
			return errors.Errorf("unsupported option %q", v.Field)
		}
	case Pattern:
		return errors.Errorf(`prepend 'path:' or 'content:' to "%s" to search repositories containing path or content respectively.`, v.Value)
	case Operator:
		if v.Kind == Or {
			return errors.New("predicates do not currently support 'or' queries")
		}
		for _, operand := range v.Operands {
			if err := f.parseNode(operand); err != nil {
				return err
			}
		}
	default:
		return errors.Errorf("unsupported node type %T", n)
	}
	return nil
}

func (f *RepoContainsFilePredicate) Field() string { return FieldRepo }
func (f *RepoContainsFilePredicate) Name() string  { return "contains.file" }

/* repo:contains.content(pattern) */

type RepoContainsContentPredicate struct {
	Pattern string
	Negated bool
}

func (f *RepoContainsContentPredicate) Unmarshal(params string, negated bool) error {
	if _, err := syntax.Parse(params, syntax.Perl); err != nil {
		return errors.Errorf("contains.content argument: %w", err)
	}
	if params == "" {
		return errors.Errorf("contains.content argument should not be empty")
	}
	f.Pattern = params
	f.Negated = negated
	return nil
}

func (f *RepoContainsContentPredicate) Field() string { return FieldRepo }
func (f *RepoContainsContentPredicate) Name() string  { return "contains.content" }

/* repo:contains.path(pattern) */

type RepoContainsPathPredicate struct {
	Pattern string
	Negated bool
}

func (f *RepoContainsPathPredicate) Unmarshal(params string, negated bool) error {
	if _, err := syntax.Parse(params, syntax.Perl); err != nil {
		return errors.Errorf("contains.path argument: %w", err)
	}
	if params == "" {
		return errors.Errorf("contains.path argument should not be empty")
	}
	f.Pattern = params
	f.Negated = negated
	return nil
}

func (f *RepoContainsPathPredicate) Field() string { return FieldRepo }
func (f *RepoContainsPathPredicate) Name() string  { return "contains.path" }

/* repo:contains.commit.after(...) */

type RepoContainsCommitAfterPredicate struct {
	TimeRef string
	Negated bool
}

func (f *RepoContainsCommitAfterPredicate) Unmarshal(params string, negated bool) error {
	f.TimeRef = params
	f.Negated = negated
	return nil
}

func (f RepoContainsCommitAfterPredicate) Field() string { return FieldRepo }
func (f RepoContainsCommitAfterPredicate) Name() string {
	return "contains.commit.after"
}

/* repo:has.description(...) */

type RepoHasDescriptionPredicate struct {
	Pattern string
}

func (f *RepoHasDescriptionPredicate) Unmarshal(params string, negated bool) (err error) {
	if negated {
		return &NegatedPredicateError{f.Field() + ":" + f.Name()}
	}

	if _, err := syntax.Parse(params, syntax.Perl); err != nil {
		return errors.Errorf("invalid repo:has.description() argument: %w", err)
	}
	if len(params) == 0 {
		return errors.New("empty repo:has.description() predicate parameter")
	}
	f.Pattern = params
	return nil
}

func (f *RepoHasDescriptionPredicate) Field() string { return FieldRepo }
func (f *RepoHasDescriptionPredicate) Name() string  { return "has.description" }

type RepoHasTagPredicate struct {
	Key     string
	Negated bool
}

func (f *RepoHasTagPredicate) Unmarshal(params string, negated bool) (err error) {
	if len(params) == 0 {
		return errors.New("tag must be non-empty")
	}
	f.Key = params
	f.Negated = negated
	return nil
}

func (f *RepoHasTagPredicate) Field() string { return FieldRepo }
func (f *RepoHasTagPredicate) Name() string  { return "has.tag" }

type RepoHasKVPPredicate struct {
	Key     string
	Value   string
	Negated bool
}

func (p *RepoHasKVPPredicate) Unmarshal(params string, negated bool) (err error) {
	split := strings.Split(params, ":")
	if len(split) != 2 || len(split[0]) == 0 {
		return errors.New("expected params in the form of key:value")
	}
	p.Key = split[0]
	p.Value = split[1]
	p.Negated = negated
	return nil
}

func (p *RepoHasKVPPredicate) Field() string { return FieldRepo }
func (p *RepoHasKVPPredicate) Name() string  { return "has" }

type RepoHasKeyPredicate struct {
	Key     string
	Negated bool
}

func (p *RepoHasKeyPredicate) Unmarshal(params string, negated bool) (err error) {
	if len(params) == 0 {
		return errors.New("key must be non-empty")
	}
	p.Key = params
	p.Negated = negated
	return nil
}

func (p *RepoHasKeyPredicate) Field() string { return FieldRepo }
func (p *RepoHasKeyPredicate) Name() string  { return "has.key" }

/* file:contains.content(pattern) */

type FileContainsContentPredicate struct {
	Pattern string
}

func (f *FileContainsContentPredicate) Unmarshal(params string, negated bool) error {
	if negated {
		return &NegatedPredicateError{f.Field() + ":" + f.Name()}
	}

	if _, err := syntax.Parse(params, syntax.Perl); err != nil {
		return errors.Errorf("file:contains.content argument: %w", err)
	}
	if params == "" {
		return errors.Errorf("file:contains.content argument should not be empty")
	}
	f.Pattern = params
	return nil
}

func (f FileContainsContentPredicate) Field() string { return FieldFile }
func (f FileContainsContentPredicate) Name() string  { return "contains.content" }

/* file:has.owner(pattern) */

type FileHasOwnerPredicate struct {
	Owner   string
	Negated bool
}

func (f *FileHasOwnerPredicate) Unmarshal(params string, negated bool) error {
	f.Owner = params
	f.Negated = negated
	return nil
}

func (f FileHasOwnerPredicate) Field() string { return FieldFile }
func (f FileHasOwnerPredicate) Name() string  { return "has.owner" }
