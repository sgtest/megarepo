// Package query provides facilities for parsing and extracting
// information from search queries.
package query

import (
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/search/query/syntax"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/search/query/types"
)

// All field names.
const (
	FieldDefault   = ""
	FieldCase      = "case"
	FieldRepo      = "repo"
	FieldRepoGroup = "repogroup"
	FieldFile      = "file"
	FieldFork      = "fork"
	FieldArchived  = "archived"
	FieldLang      = "lang"
	FieldType      = "type"

	// For graph search only:
	FieldRef   = "ref"
	FieldHints = "hints"

	// For diff and commit search only:
	FieldBefore    = "before"
	FieldAfter     = "after"
	FieldAuthor    = "author"
	FieldCommitter = "committer"
	FieldMessage   = "message"

	// Temporary experimental fields:
	FieldIndex   = "index"
	FieldCount   = "count" // Searches that specify `count:` will fetch at least that number of results, or the full result set
	FieldMax     = "max"   // Deprecated alias for count
	FieldTimeout = "timeout"
)

var (
	regexpNegatableFieldType = types.FieldType{Literal: types.RegexpType, Quoted: types.RegexpType, Negatable: true}
	stringFieldType          = types.FieldType{Literal: types.StringType, Quoted: types.StringType}

	conf = types.Config{
		FieldTypes: map[string]types.FieldType{
			FieldDefault:   {Literal: types.RegexpType, Quoted: types.StringType},
			FieldCase:      {Literal: types.BoolType, Quoted: types.BoolType, Singular: true},
			FieldRepo:      regexpNegatableFieldType,
			FieldRepoGroup: types.FieldType{Literal: types.StringType, Quoted: types.StringType, Singular: true},
			FieldFile:      regexpNegatableFieldType,
			FieldFork:      {Literal: types.StringType, Quoted: types.StringType, Singular: true},
			FieldArchived:  {Literal: types.StringType, Quoted: types.StringType, Singular: true},
			FieldLang:      types.FieldType{Literal: types.StringType, Quoted: types.StringType, Negatable: true},
			FieldType:      stringFieldType,

			FieldRef:   {Literal: types.StringType, Quoted: types.StringType, Singular: true},
			FieldHints: {Literal: types.StringType, Quoted: types.StringType, Singular: true},

			FieldBefore:    stringFieldType,
			FieldAfter:     stringFieldType,
			FieldAuthor:    regexpNegatableFieldType,
			FieldCommitter: regexpNegatableFieldType,
			FieldMessage:   regexpNegatableFieldType,

			// Experimental fields:
			FieldIndex:   {Literal: types.StringType, Quoted: types.StringType, Singular: true},
			FieldCount:   {Literal: types.StringType, Quoted: types.StringType, Singular: true},
			FieldMax:     {Literal: types.StringType, Quoted: types.StringType, Singular: true},
			FieldTimeout: {Literal: types.StringType, Quoted: types.StringType, Singular: true},
		},
		FieldAliases: map[string]string{
			"r":        FieldRepo,
			"g":        FieldRepoGroup,
			"f":        FieldFile,
			"l":        FieldLang,
			"language": FieldLang,
			"since":    FieldAfter,
			"until":    FieldBefore,
			"m":        FieldMessage,
			"msg":      FieldMessage,
		},
	}
)

// A Query is the parsed representation of a search query.
type Query struct {
	conf *types.Config // the typechecker config used to produce this query

	*types.Query // the underlying query
}

// ParseAndCheck parses and typechecks a search query using the default
// query type configuration.
func ParseAndCheck(input string) (*Query, error) {
	return parseAndCheck(&conf, input)
}

func parseAndCheck(conf *types.Config, input string) (*Query, error) {
	syntaxQuery, err := syntax.Parse(input)
	if err != nil {
		return nil, err
	}
	checkedQuery, err := conf.Check(syntaxQuery)
	if err != nil {
		return nil, err
	}
	return &Query{conf: conf, Query: checkedQuery}, nil
}

// BoolValue returns the last boolean value (yes/no) for the field. For example, if the query is
// "foo:yes foo:no foo:yes", then the last boolean value for the "foo" field is true ("yes"). The
// default boolean value is false.
func (q *Query) BoolValue(field string) bool {
	for _, v := range q.Fields[field] {
		if v.Bool != nil {
			return *v.Bool
		}
	}
	return false // default
}

// IsCaseSensitive reports whether the query's expressions are matched
// case sensitively.
func (q *Query) IsCaseSensitive() bool {
	return q.BoolValue(FieldCase)
}

// Values returns the values for the given field.
func (q *Query) Values(field string) []*types.Value {
	if _, ok := q.conf.FieldTypes[field]; !ok {
		panic("no such field: " + field)
	}
	return q.Fields[field]
}

// RegexpPatterns returns the regexp pattern source strings for the given field.
// If the field is not recognized or it is not always regexp-typed, it panics.
func (q *Query) RegexpPatterns(field string) (values, negatedValues []string) {
	fieldType, ok := q.conf.FieldTypes[field]
	if !ok {
		panic("no such field: " + field)
	}
	if fieldType.Literal != types.RegexpType || fieldType.Quoted != types.RegexpType {
		panic("field is not always regexp-typed: " + field)
	}

	for _, v := range q.Fields[field] {
		s := v.Regexp.String()
		if v.Not() {
			negatedValues = append(negatedValues, s)
		} else {
			values = append(values, s)
		}
	}
	return
}

// StringValues returns the string values for the given field. If the field is
// not recognized or it is not always string-typed, it panics.
func (q *Query) StringValues(field string) (values, negatedValues []string) {
	fieldType, ok := q.conf.FieldTypes[field]
	if !ok {
		panic("no such field: " + field)
	}
	if fieldType.Literal != types.StringType || fieldType.Quoted != types.StringType {
		panic("field is not always string-typed: " + field)
	}

	for _, v := range q.Fields[field] {
		if v.Not() {
			negatedValues = append(negatedValues, *v.String)
		} else {
			values = append(values, *v.String)
		}
	}
	return
}

// StringValue returns the string value for the given field.
// It panics if the field is not recognized, it is not always string-typed, or it is not singular.
func (q *Query) StringValue(field string) (value, negatedValue string) {
	fieldType, ok := q.conf.FieldTypes[field]
	if !ok {
		panic("no such field: " + field)
	}
	if fieldType.Literal != types.StringType || fieldType.Quoted != types.StringType {
		panic("field is not always string-typed: " + field)
	}
	if !fieldType.Singular {
		panic("field is not singular: " + field)
	}
	if len(q.Fields[field]) == 0 {
		return "", ""
	}
	v := q.Fields[field][0]
	if v.Not() {
		return "", *v.String
	}
	return *v.String, ""
}
