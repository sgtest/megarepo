package models

import (
	"fmt"

	"github.com/prometheus/prometheus/model/labels"
	"github.com/prometheus/prometheus/promql/parser"
)

func ApplyQueryScope(rawExpr string, scope ScopeSpec) (string, error) {
	expr, err := parser.ParseExpr(rawExpr)
	if err != nil {
		return "", err
	}

	matchers, err := scopeFiltersToMatchers(scope.Filters)
	if err != nil {
		return "", err
	}

	matcherNamesToIdx := make(map[string]int, len(matchers))
	for i, matcher := range matchers {
		if matcher == nil {
			continue
		}
		matcherNamesToIdx[matcher.Name] = i
	}

	parser.Inspect(expr, func(node parser.Node, nodes []parser.Node) error {
		switch v := node.(type) {
		case *parser.VectorSelector:
			found := make([]bool, len(matchers))
			for _, matcher := range v.LabelMatchers {
				if matcher == nil || matcher.Name == "__name__" { // const prob
					continue
				}
				if _, ok := matcherNamesToIdx[matcher.Name]; ok {
					found[matcherNamesToIdx[matcher.Name]] = true
					newM := matchers[matcherNamesToIdx[matcher.Name]]
					matcher.Name = newM.Name
					matcher.Type = newM.Type
					matcher.Value = newM.Value
				}
			}
			for i, f := range found {
				if f {
					continue
				}
				v.LabelMatchers = append(v.LabelMatchers, matchers[i])
			}

			return nil

		default:
			return nil
		}
	})
	return expr.String(), nil
}

func scopeFiltersToMatchers(filters []ScopeFilter) ([]*labels.Matcher, error) {
	matchers := make([]*labels.Matcher, 0, len(filters))
	for _, f := range filters {
		var mt labels.MatchType
		switch f.Operator {
		case FilterOperatorEquals:
			mt = labels.MatchEqual
		case FilterOperatorNotEquals:
			mt = labels.MatchNotEqual
		case FilterOperatorRegexMatch:
			mt = labels.MatchRegexp
		case FilterOperatorRegexNotMatch:
			mt = labels.MatchNotRegexp
		default:
			return nil, fmt.Errorf("unknown operator %q", f.Operator)
		}
		m, err := labels.NewMatcher(mt, f.Key, f.Value)
		if err != nil {
			return nil, err
		}
		matchers = append(matchers, m)
	}
	return matchers, nil
}
