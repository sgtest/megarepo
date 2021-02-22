package graphqlbackend

import (
	"fmt"
	"strconv"

	"github.com/graphql-go/graphql/language/ast"
	"github.com/graphql-go/graphql/language/kinds"
	"github.com/graphql-go/graphql/language/parser"
	"github.com/graphql-go/graphql/language/visitor"
	"github.com/pkg/errors"
)

// Included in tracing so that we can differentiate different costs as we tweak
// the algorithm
const costEstimateVersion = 2

type QueryCost struct {
	FieldCount int
	MaxDepth   int
	Version    int
}

// EstimateQueryCost estimates the cost of the query before it is actually
// executed. It is a worst cast estimate of the number of fields expected to be
// returned by the query and handles nested queries a well as fragments.
func EstimateQueryCost(query string, variables map[string]interface{}) (totalCost *QueryCost, err error) {
	// NOTE: When we encounter errors in our visit funcs we return
	// visitor.ActionBreak to stop walking the tree and set the top level err
	// variable so that it is returned
	totalCost = &QueryCost{
		Version: costEstimateVersion,
	}

	// TODO: Remove this. It's here as a safeguard until we've run over a large
	// number of real world queries.
	defer func() {
		if r := recover(); r != nil {
			totalCost = nil
			err = r.(error)
		}
	}()
	if variables == nil {
		variables = make(map[string]interface{})
	}

	doc, err := parser.Parse(parser.ParseParams{
		Source: query,
	})
	if err != nil {
		return nil, errors.Wrap(err, "parsing query")
	}

	// We need to separate operations from fragments
	var operations []ast.Node
	var fragments []*ast.FragmentDefinition

	for i, def := range doc.Definitions {
		switch def.GetKind() {
		case kinds.FragmentDefinition:
			frag, ok := doc.Definitions[i].(*ast.FragmentDefinition)
			if !ok {
				return nil, fmt.Errorf("expected FragmentDefinition, got %T", doc.Definitions[i])
			}
			fragments = append(fragments, frag)
		case kinds.OperationDefinition:
			operations = append(operations, doc.Definitions[i])
		}
	}

	// Calculate fragment costs first as we'll need them for the overall operation
	// cost
	fragmentCosts := make(map[string]int)
	for _, frag := range fragments {
		cost, err := calcNodeCost(frag, nil, variables)
		if err != nil {
			return nil, errors.Wrap(err, "calculating fragment cost")
		}
		fragmentCosts[frag.Name.Value] = cost.FieldCount
	}

	for _, def := range operations {
		cost, err := calcNodeCost(def, fragmentCosts, variables)
		if err != nil {
			return nil, errors.Wrap(err, "calculating operation cost")
		}
		totalCost.FieldCount += cost.FieldCount
		if totalCost.MaxDepth < cost.MaxDepth {
			totalCost.MaxDepth = cost.MaxDepth
		}
	}

	if totalCost.FieldCount < 1 {
		totalCost.FieldCount = 1
	}
	if totalCost.MaxDepth < 1 {
		totalCost.MaxDepth = 1
	}

	return totalCost, nil
}

func calcNodeCost(def ast.Node, fragmentCosts map[string]int, variables map[string]interface{}) (*QueryCost, error) {
	// NOTE: When we encounter errors in our visit funcs we return
	// visitor.ActionBreak to stop walking the tree and set the top level err
	// variable so that it is returned
	var visitErr error

	if fragmentCosts == nil {
		fragmentCosts = make(map[string]int)
	}
	inlineFragmentCosts := make(map[string]int)
	inlineFragmentDepth := 0

	// limitStack keeps track of the limit as we increase and decrease our depth in
	// the tree and encounter limit values
	limitStack := make([]int, 0)
	currentLimit := 1

	fieldCount := 0
	depth := 0
	maxDepth := 0
	multiplier := 1

	pushLimit := func() {
		multiplier = multiplier * currentLimit
		limitStack = append(limitStack, currentLimit)
		// Set limit back to 1 as we've already used it to increase our multiplier
		currentLimit = 1
	}
	popLimit := func() {
		if len(limitStack) == 0 {
			return
		}
		currentLimit = limitStack[len(limitStack)-1]
		limitStack = limitStack[:len(limitStack)-1]
		multiplier = multiplier / currentLimit
	}

	nonNullVariables := make(map[string]interface{})
	defaultValues := make(map[string]interface{})

	v := &visitor.VisitorOptions{
		Enter: func(p visitor.VisitFuncParams) (string, interface{}) {
			switch node := p.Node.(type) {
			case *ast.SelectionSet:
				depth++
				if depth > maxDepth {
					maxDepth = depth
				}
				pushLimit()
			case *ast.Field:
				switch node.Name.Value {
				// Values that won't appear in the result
				case "nodes", "__typename":
					return visitor.ActionNoChange, nil
				}
				if inlineFragmentDepth > 0 {
					// We don't count fields inside of inline fragments as we need to count all fragments
					// first to pick the largest one
					return visitor.ActionNoChange, nil
				}
				fieldCount += multiplier
			case *ast.VariableDefinition:
				// Track which variables are nonNull.
				if _, nonNull := node.Type.(*ast.NonNull); nonNull {
					nonNullVariables[node.Variable.Name.Value] = struct{}{}
				}
				if node.DefaultValue == nil {
					return visitor.ActionNoChange, nil
				}
				// Record default values
				switch v := node.DefaultValue.(type) {
				case *ast.IntValue:
					// Yes, IntValue's value is a string...
					defaultValues[node.Variable.Name.Value] = v.Value
				}
			case *ast.Variable:
				// We may have a limit variable
				if !shouldCheckParam(p) {
					return visitor.ActionNoChange, nil
				}
				limitVar, ok := variables[node.Name.Value]
				if !ok {
					if _, nonNull := nonNullVariables[node.Name.Value]; nonNull {
						visitErr = fmt.Errorf("missing nonnull variable: %q", node.Name.Value)
						return visitor.ActionBreak, nil
					}
					if v, ok := defaultValues[node.Name.Value]; ok {
						// Pick default value if it was defined
						limitVar = v
					} else {
						// Fall back to a default of 1
						currentLimit = 1
						return visitor.ActionNoChange, nil
					}
				}
				limit, err := extractInt(limitVar)
				if err != nil {
					visitErr = errors.Wrap(err, "extracting limit")
					return visitor.ActionBreak, nil
				}
				if limit <= 0 {
					return visitor.ActionNoChange, nil
				}
				currentLimit = limit
			case *ast.IntValue:
				// We may have a limit
				if !shouldCheckParam(p) {
					return visitor.ActionNoChange, nil
				}
				limit, err := strconv.Atoi(node.Value)
				if err != nil {
					visitErr = errors.Wrap(err, "parsing limit")
					return visitor.ActionBreak, nil
				}
				if limit <= 0 {
					return visitor.ActionNoChange, nil
				}
				currentLimit = limit
			case *ast.FragmentSpread:
				fragmentCost, ok := fragmentCosts[node.Name.Value]
				if !ok {
					visitErr = fmt.Errorf("unknown fragment %q", node.Name.Value)
					return visitor.ActionBreak, nil
				}
				fieldCount += fragmentCost * multiplier
			case *ast.InlineFragment:
				inlineFragmentDepth++
				// We calculate inline fragment costs and store them
				var fragCost *QueryCost
				fragCost, err := calcNodeCost(node.SelectionSet, fragmentCosts, variables)
				if err != nil {
					visitErr = errors.Wrap(err, "calculating inline fragment cost")
					return visitor.ActionBreak, nil
				}
				inlineFragmentCosts[node.TypeCondition.Name.Value] = fragCost.FieldCount * multiplier
			}
			return visitor.ActionNoChange, nil
		},
		Leave: func(p visitor.VisitFuncParams) (string, interface{}) {
			switch p.Node.(type) {
			case *ast.SelectionSet:
				depth--
				popLimit()
			case *ast.InlineFragment:
				inlineFragmentDepth--
			}
			return visitor.ActionNoChange, nil
		},
	}

	_ = visitor.Visit(def, v, nil)

	// We also need to pick the largest inline fragment in our tree
	var maxInlineFragmentCost int
	for _, v := range inlineFragmentCosts {
		if v > maxInlineFragmentCost {
			maxInlineFragmentCost = v
		}
	}

	return &QueryCost{
		FieldCount: fieldCount + maxInlineFragmentCost,
		MaxDepth:   maxDepth,
	}, visitErr
}

func extractInt(i interface{}) (int, error) {
	switch v := i.(type) {
	case int:
		return v, nil
	case float64:
		return int(v), nil
	case string:
		return strconv.Atoi(v)
	default:
		return 0, fmt.Errorf("unkown limit type: %T", i)
	}
}

var quantityParams = map[string]struct{}{
	"first": {},
	"last":  {},
}

func shouldCheckParam(p visitor.VisitFuncParams) bool {
	parent, ok := p.Parent.(*ast.Argument)
	if !ok {
		return false
	}
	if parent.Name == nil {
		return false
	}
	if _, ok := quantityParams[parent.Name.Value]; !ok {
		return false
	}
	return true
}
