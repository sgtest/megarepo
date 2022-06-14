package squirrel

import (
	"context"
	"fmt"
	"path/filepath"
	"sort"
	"strings"

	sitter "github.com/smacker/go-tree-sitter"

	"github.com/sourcegraph/sourcegraph/internal/types"
)

func (squirrel *SquirrelService) getDefJava(ctx context.Context, node Node) (ret *Node, err error) {
	defer squirrel.onCall(node, String(node.Type()), lazyNodeStringer(&ret))()

	switch node.Type() {
	case "identifier":
		ident := node.Content(node.Contents)

		cur := node.Node

	outer:
		for {
			prev := cur
			cur = cur.Parent()
			if cur == nil {
				squirrel.breadcrumb(node, "getDefJava: ran out of parents")
				return nil, nil
			}

			switch cur.Type() {

			case "program":
				return squirrel.getDefInImportsOrCurrentPackageJava(ctx, swapNode(node, cur), ident)

			case "import_declaration":
				program := cur.Parent()
				if program == nil {
					squirrel.breadcrumb(node, "getDefJava: expected parent for import_declaration")
					return nil, nil
				}
				if program.Type() != "program" {
					squirrel.breadcrumb(node, "getDefJava: expected parent of import_declaration to be program")
				}
				root, err := getProjectRoot(swapNode(node, program))
				if err != nil {
					return nil, err
				}
				allComponents, err := getPath(swapNode(node, cur))
				if err != nil {
					return nil, err
				}
				components, err := getPathUpTo(swapNode(node, cur), node.Node)
				if err != nil {
					return nil, err
				}
				if len(components) == len(allComponents) {
					return squirrel.symbolSearchOne(
						ctx,
						node.RepoCommitPath.Repo,
						node.RepoCommitPath.Commit,
						[]string{fmt.Sprintf("^%s/%s", filepath.Join(root...), filepath.Join(components[:len(components)-1]...))},
						ident,
					)
				}
				dir := filepath.Join(append(root, components...)...)
				return &Node{
					RepoCommitPath: types.RepoCommitPath{
						Repo:   node.RepoCommitPath.Repo,
						Commit: node.RepoCommitPath.Commit,
						Path:   dir,
					},
					Node:     nil,
					Contents: node.Contents,
					LangSpec: node.LangSpec,
				}, nil

			// Check for field access
			case "field_access":
				object := cur.ChildByFieldName("object")
				if object != nil && nodeId(prev) == nodeId(object) {
					continue
				}
				field := cur.ChildByFieldName("field")
				if field != nil {
					found, err := squirrel.getFieldJava(ctx, swapNode(node, object), field.Content(node.Contents))
					if err != nil {
						return nil, err
					}
					if found != nil {
						return found, nil
					}
				}
				continue

			case "method_invocation":
				object := cur.ChildByFieldName("object")
				if object == nil {
					continue
				}
				if nodeId(prev) == nodeId(object) {
					continue
				}
				args := cur.ChildByFieldName("arguments")
				if args == nil {
					continue
				}
				if nodeId(prev) == nodeId(args) {
					continue
				}
				name := cur.ChildByFieldName("name")
				if name != nil {
					found, err := squirrel.getFieldJava(ctx, swapNode(node, object), name.Content(node.Contents))
					if err != nil {
						return nil, err
					}
					if found != nil {
						return found, nil
					}
				}
				continue

			// Check nodes that might have bindings:
			case "constructor_body":
				fallthrough
			case "block":
				blockChild := prev
				for {
					blockChild = blockChild.PrevNamedSibling()
					if blockChild == nil {
						continue outer
					}
					query := "(local_variable_declaration declarator: (variable_declarator name: (identifier) @ident))"
					captures, err := allCaptures(query, swapNode(node, blockChild))
					if err != nil {
						return nil, err
					}
					for _, capture := range captures {
						if capture.Content(capture.Contents) == ident {
							return swapNodePtr(node, capture.Node), nil
						}
					}
				}

			case "constructor_declaration":
				query := `[
					(constructor_declaration parameters: (formal_parameters (formal_parameter name: (identifier) @ident)))
					(constructor_declaration parameters: (formal_parameters (spread_parameter (variable_declarator name: (identifier) @ident))))
				]`
				captures, err := allCaptures(query, swapNode(node, cur))
				if err != nil {
					return nil, err
				}
				for _, capture := range captures {
					if capture.Content(capture.Contents) == ident {
						return swapNodePtr(node, capture.Node), nil
					}
				}
				continue

			case "method_declaration":
				query := `[
					(method_declaration name: (identifier) @ident)
					(method_declaration parameters: (formal_parameters (formal_parameter name: (identifier) @ident)))
					(method_declaration parameters: (formal_parameters (spread_parameter (variable_declarator name: (identifier) @ident))))
				]`
				captures, err := allCaptures(query, swapNode(node, cur))
				if err != nil {
					return nil, err
				}
				for _, capture := range captures {
					if capture.Content(capture.Contents) == ident {
						return swapNodePtr(node, capture.Node), nil
					}
				}
				continue

			case "class_declaration":
				name := cur.ChildByFieldName("name")
				if name != nil {
					if name.Content(node.Contents) == ident {
						return swapNodePtr(node, name), nil
					}
				}
				found, err := squirrel.lookupFieldJava(ctx, ClassType{def: swapNode(node, cur)}, ident)
				if err != nil {
					return nil, err
				}
				if found != nil {
					return found, nil
				}
				super := getSuperclassJava(swapNode(node, cur))
				if super != nil {
					found, err := squirrel.getFieldJava(ctx, *super, ident)
					if err != nil {
						return nil, err
					}
					if found != nil {
						return found, nil
					}
				}
				continue

			case "lambda_expression":
				query := `[
					(lambda_expression parameters: (identifier) @ident)
					(lambda_expression parameters: (formal_parameters (formal_parameter name: (identifier) @ident)))
					(lambda_expression parameters: (formal_parameters (spread_parameter (variable_declarator name: (identifier) @ident))))
					(lambda_expression parameters: (inferred_parameters (identifier) @ident))
				]`
				captures, err := allCaptures(query, swapNode(node, cur))
				if err != nil {
					return nil, err
				}
				for _, capture := range captures {
					if capture.Content(capture.Contents) == ident {
						return swapNodePtr(node, capture.Node), nil
					}
				}
				continue

			case "catch_clause":
				query := `(catch_clause (catch_formal_parameter name: (identifier) @ident))`
				captures, err := allCaptures(query, swapNode(node, cur))
				if err != nil {
					return nil, err
				}
				for _, capture := range captures {
					if capture.Content(capture.Contents) == ident {
						return swapNodePtr(node, capture.Node), nil
					}
				}
				continue

			case "for_statement":
				query := `(for_statement init: (local_variable_declaration declarator: (variable_declarator name: (identifier) @ident)))`
				captures, err := allCaptures(query, swapNode(node, cur))
				if err != nil {
					return nil, err
				}
				for _, capture := range captures {
					if capture.Content(capture.Contents) == ident {
						return swapNodePtr(node, capture.Node), nil
					}
				}
				continue

			case "enhanced_for_statement":
				query := `(enhanced_for_statement name: (identifier) @ident)`
				captures, err := allCaptures(query, swapNode(node, cur))
				if err != nil {
					return nil, err
				}
				for _, capture := range captures {
					if capture.Content(capture.Contents) == ident {
						return swapNodePtr(node, capture.Node), nil
					}
				}
				continue

			case "method_reference":
				if cur.NamedChildCount() == 0 {
					return nil, nil
				}
				object := cur.NamedChild(0)
				if nodeId(object) == nodeId(prev) {
					continue
				}
				if ident == "new" {
					return squirrel.getDefJava(ctx, swapNode(node, object))
				}
				return squirrel.getFieldJava(ctx, swapNode(node, object), ident)

			// Skip all other nodes
			default:
				continue
			}
		}

	case "type_identifier":
		ident := node.Content(node.Contents)

		cur := node.Node

		for {
			prev := cur
			cur = cur.Parent()
			if cur == nil {
				squirrel.breadcrumb(node, "getDefJava: ran out of parents")
				return nil, nil
			}

			switch cur.Type() {
			case "program":
				query := `[
					(program (class_declaration name: (identifier) @ident))
					(program (enum_declaration name: (identifier) @ident))
					(program (interface_declaration name: (identifier) @ident))
				]`
				captures, err := allCaptures(query, swapNode(node, cur))
				if err != nil {
					return nil, err
				}
				for _, capture := range captures {
					if capture.Content(capture.Contents) == ident {
						return swapNodePtr(node, capture.Node), nil
					}
				}
				return squirrel.getDefInImportsOrCurrentPackageJava(ctx, swapNode(node, cur), ident)
			case "class_declaration":
				query := `[
					(class_declaration name: (identifier) @ident)
					(class_declaration body: (class_body (class_declaration name: (identifier) @ident)))
					(class_declaration body: (class_body (enum_declaration name: (identifier) @ident)))
					(class_declaration body: (class_body (interface_declaration name: (identifier) @ident)))
				]`
				captures, err := allCaptures(query, swapNode(node, cur))
				if err != nil {
					return nil, err
				}
				for _, capture := range captures {
					if capture.Content(capture.Contents) == ident {
						return swapNodePtr(node, capture.Node), nil
					}
				}
				continue
			case "scoped_type_identifier":
				object := cur.NamedChild(0)
				if object != nil && nodeId(prev) == nodeId(object) {
					continue
				}
				field := cur.NamedChild(int(cur.NamedChildCount()) - 1)
				if field != nil {
					found, err := squirrel.getFieldJava(ctx, swapNode(node, object), field.Content(node.Contents))
					if err != nil {
						return nil, err
					}
					if found != nil {
						return found, nil
					}
				}
				continue
			default:
				continue
			}
		}

	case "this":
		cur := node.Node
		for cur != nil {
			switch cur.Type() {
			case "class_declaration":
				fallthrough
			case "interface_declaration":
				name := cur.ChildByFieldName("name")
				if name == nil {
					return nil, nil
				}
				return swapNodePtr(node, name), nil
			}
			cur = cur.Parent()
		}
		return nil, nil

	case "super":
		cur := node.Node
		for cur != nil {
			switch cur.Type() {
			case "class_declaration":
				fallthrough
			case "interface_declaration":
				super := getSuperclassJava(swapNode(node, cur))
				if super == nil {
					return nil, nil
				}
				return squirrel.getDefJava(ctx, *super)
			}
			cur = cur.Parent()
		}
		return nil, nil

	// No other nodes have a definition
	default:
		return nil, nil
	}
}

func (squirrel *SquirrelService) getFieldJava(ctx context.Context, object Node, field string) (ret *Node, err error) {
	defer squirrel.onCall(object, &Tuple{String(object.Type()), String(field)}, lazyNodeStringer(&ret))()

	ty, err := squirrel.getTypeDefJava(ctx, object)
	if err != nil {
		return nil, err
	}
	if ty == nil {
		return nil, nil
	}
	return squirrel.lookupFieldJava(ctx, ty, field)
}

func (squirrel *SquirrelService) lookupFieldJava(ctx context.Context, ty Type, field string) (ret *Node, err error) {
	defer squirrel.onCall(ty.node(), &Tuple{String(ty.variant()), String(field)}, lazyNodeStringer(&ret))()

	switch ty2 := ty.(type) {
	case ClassType:
		body := ty2.def.ChildByFieldName("body")
		if body == nil {
			return nil, nil
		}
		for _, child := range children(body) {
			switch child.Type() {
			case "method_declaration":
				name := child.ChildByFieldName("name")
				if name == nil {
					continue
				}
				if name.Content(ty2.def.Contents) == field {
					return swapNodePtr(ty2.def, name), nil
				}
			case "class_declaration":
				name := child.ChildByFieldName("name")
				if name == nil {
					continue
				}
				if name.Content(ty2.def.Contents) == field {
					return swapNodePtr(ty2.def, name), nil
				}
			case "field_declaration":
				query := "(field_declaration declarator: (variable_declarator name: (identifier) @ident))"
				captures, err := allCaptures(query, swapNode(ty2.def, child))
				if err != nil {
					return nil, err
				}
				for _, capture := range captures {
					if capture.Content(capture.Contents) == field {
						return swapNodePtr(ty2.def, capture.Node), nil
					}
				}
			}
		}
		super := getSuperclassJava(ty2.def)
		if super != nil {
			found, err := squirrel.getFieldJava(ctx, *super, field)
			if err != nil {
				return nil, err
			}
			if found != nil {
				return found, nil
			}
		}
		return nil, nil
	case FnType:
		squirrel.breadcrumb(ty.node(), fmt.Sprintf("lookupFieldJava: unexpected object type %s", ty.variant()))
		return nil, nil
	case PrimType:
		squirrel.breadcrumb(ty.node(), fmt.Sprintf("lookupFieldJava: unexpected object type %s", ty.variant()))
		return nil, nil
	default:
		squirrel.breadcrumb(ty.node(), fmt.Sprintf("lookupFieldJava: unrecognized type variant %q", ty.variant()))
		return nil, nil
	}
}

func (squirrel *SquirrelService) getTypeDefJava(ctx context.Context, node Node) (ret Type, err error) {
	defer squirrel.onCall(node, String(node.Type()), lazyTypeStringer(&ret))()

	onIdent := func() (Type, error) {
		found, err := squirrel.getDefJava(ctx, node)
		if err != nil {
			return nil, err
		}
		if found == nil {
			return nil, nil
		}
		return squirrel.defToType(ctx, *found)
	}

	switch node.Type() {
	case "type_identifier":
		if node.Content(node.Contents) == "var" {
			localVariableDeclaration := node.Parent()
			if localVariableDeclaration == nil {
				return nil, nil
			}
			captures, err := allCaptures("(local_variable_declaration declarator: (variable_declarator value: (_) @value))", swapNode(node, localVariableDeclaration))
			if err != nil {
				return nil, err
			}
			for _, capture := range captures {
				return squirrel.getTypeDefJava(ctx, capture)
			}
			return nil, nil
		} else {
			return onIdent()
		}
	case "this":
		fallthrough
	case "super":
		fallthrough
	case "identifier":
		return onIdent()
	case "field_access":
		object := node.ChildByFieldName("object")
		if object == nil {
			return nil, nil
		}
		field := node.ChildByFieldName("field")
		if field == nil {
			return nil, nil
		}
		objectType, err := squirrel.getTypeDefJava(ctx, swapNode(node, object))
		if err != nil {
			return nil, err
		}
		if objectType == nil {
			return nil, nil
		}
		found, err := squirrel.lookupFieldJava(ctx, objectType, field.Content(node.Contents))
		if err != nil {
			return nil, err
		}
		if found == nil {
			return nil, nil
		}
		return squirrel.defToType(ctx, *found)
	case "method_invocation":
		name := node.ChildByFieldName("name")
		if name == nil {
			return nil, nil
		}
		ty, err := squirrel.getTypeDefJava(ctx, swapNode(node, name))
		if err != nil {
			return nil, err
		}
		if ty == nil {
			return nil, nil
		}
		switch ty2 := ty.(type) {
		case FnType:
			return ty2.ret, nil
		default:
			squirrel.breadcrumb(ty.node(), fmt.Sprintf("getTypeDefJava: expected method, got %q", ty.variant()))
			return nil, nil
		}
	case "generic_type":
		for _, child := range children(node.Node) {
			if child.Type() == "type_identifier" || child.Type() == "scoped_type_identifier" {
				return squirrel.getTypeDefJava(ctx, swapNode(node, child))
			}
		}
		squirrel.breadcrumb(node, "getTypeDefJava: expected an identifier")
		return nil, nil
	case "scoped_type_identifier":
		for i := int(node.NamedChildCount()) - 1; i >= 0; i-- {
			child := node.NamedChild(i)
			if child.Type() == "type_identifier" {
				return squirrel.getTypeDefJava(ctx, swapNode(node, child))
			}
		}
		return nil, nil
	case "object_creation_expression":
		ty := node.ChildByFieldName("type")
		if ty == nil {
			return nil, nil
		}
		return squirrel.getTypeDefJava(ctx, swapNode(node, ty))
	case "void_type":
		return PrimType{noad: node, varient: "void"}, nil
	case "integral_type":
		return PrimType{noad: node, varient: "integral"}, nil
	case "floating_point_type":
		return PrimType{noad: node, varient: "floating"}, nil
	case "boolean_type":
		return PrimType{noad: node, varient: "boolean"}, nil
	default:
		squirrel.breadcrumb(node, fmt.Sprintf("getTypeDefJava: unrecognized node type %q", node.Type()))
		return nil, nil
	}
}

func (squirrel *SquirrelService) getDefInImportsOrCurrentPackageJava(ctx context.Context, program Node, ident string) (ret *Node, err error) {
	defer squirrel.onCall(program, &Tuple{String(program.Type()), String(ident)}, lazyNodeStringer(&ret))()

	// Determine project root
	root, err := getProjectRoot(program)
	if err != nil {
		return nil, err
	}

	// Collect imports
	imports := [][]string{}
	for _, importNode := range children(program.Node) {
		if importNode.Type() != "import_declaration" {
			continue
		}
		path, err := getPath(swapNode(program, importNode))
		if err != nil {
			return nil, err
		}
		for _, child := range children(importNode) {
			if child.Type() == "asterisk" {
				path = append(path, "*")
				break
			}
		}
		if len(path) == 0 {
			continue
		}
		imports = append(imports, path)
	}

	// Check explicit imports (faster) before running symbol searches (slower)
	for _, importPath := range imports {
		last := importPath[len(importPath)-1]
		if last == "*" {
			continue
		}
		if last == ident {
			return squirrel.symbolSearchOne(
				ctx,
				program.RepoCommitPath.Repo,
				program.RepoCommitPath.Commit,
				[]string{fmt.Sprintf("^%s/%s", filepath.Join(root...), filepath.Join(importPath[:len(importPath)-1]...))},
				ident,
			)
		}
	}

	// Search in current package
	found, err := squirrel.symbolSearchOne(
		ctx,
		program.RepoCommitPath.Repo,
		program.RepoCommitPath.Commit,
		[]string{filepath.Dir(program.RepoCommitPath.Path)},
		ident,
	)
	if err != nil {
		return nil, err
	}
	if found != nil {
		return found, nil
	}

	// Search in packages imported with an asterisk
	for _, importPath := range imports {
		if importPath[len(importPath)-1] != "*" {
			continue
		}

		found, err := squirrel.symbolSearchOne(
			ctx,
			program.RepoCommitPath.Repo,
			program.RepoCommitPath.Commit,
			[]string{fmt.Sprintf("^%s/%s", filepath.Join(root...), filepath.Join(importPath[:len(importPath)-1]...))},
			ident,
		)
		if err != nil {
			return nil, err
		}
		if found != nil {
			return found, nil
		}
	}

	return nil, nil
}

func getProjectRoot(program Node) ([]string, error) {
	root := strings.Split(filepath.Dir(program.RepoCommitPath.Path), "/")
	for _, pkgNode := range children(program.Node) {
		if pkgNode.Type() != "package_declaration" {
			continue
		}
		pkg, err := getPath(swapNode(program, pkgNode))
		if err != nil {
			return nil, err
		}
		root = root[:len(root)-len(pkg)]
	}
	return root, nil
}

func getPath(node Node) ([]string, error) {
	query := `(identifier) @ident`
	captures, err := allCaptures(query, node)
	if err != nil {
		return nil, err
	}
	sort.Slice(captures, func(i, j int) bool {
		return captures[i].StartByte() < captures[j].StartByte()
	})
	components := []string{}
	for _, capture := range captures {
		components = append(components, capture.Content(capture.Contents))
	}
	return components, nil
}

func getPathUpTo(node Node, component *sitter.Node) ([]string, error) {
	query := `(identifier) @ident`
	captures, err := allCaptures(query, node)
	if err != nil {
		return nil, err
	}
	sort.Slice(captures, func(i, j int) bool {
		return captures[i].StartByte() < captures[j].StartByte()
	})
	components := []string{}
	for _, capture := range captures {
		components = append(components, capture.Content(capture.Contents))
		if nodeId(capture.Node) == nodeId(component) {
			break
		}
	}
	return components, nil
}

func getSuperclassJava(declaration Node) *Node {
	super := declaration.ChildByFieldName("superclass")
	if super == nil {
		return nil
	}
	class := super.NamedChild(0)
	if class == nil {
		return nil
	}
	return swapNodePtr(declaration, class)
}

type Type interface {
	variant() string
	node() Node
}

type FnType struct {
	ret  Type
	noad Node
}

func (t FnType) variant() string {
	return "fn"
}

func (t FnType) node() Node {
	return t.noad
}

type ClassType struct {
	def Node
}

func (t ClassType) variant() string {
	return "class"
}

func (t ClassType) node() Node {
	return t.def
}

type PrimType struct {
	noad    Node
	varient string
}

func (t PrimType) variant() string {
	return fmt.Sprintf("prim:%s", t.varient)
}

func (t PrimType) node() Node {
	return t.noad
}

func (squirrel *SquirrelService) defToType(ctx context.Context, def Node) (Type, error) {
	parent := def.Node.Parent()
	if parent == nil {
		return nil, nil
	}
	switch parent.Type() {
	case "class_declaration":
		return (Type)(ClassType{def: swapNode(def, parent)}), nil
	case "method_declaration":
		retTyNode := parent.ChildByFieldName("type")
		if retTyNode == nil {
			squirrel.breadcrumb(swapNode(def, parent), "defToType: could not find return type")
			return (Type)(FnType{
				ret:  nil,
				noad: swapNode(def, parent),
			}), nil
		}
		retTy, err := squirrel.getTypeDefJava(ctx, swapNode(def, retTyNode))
		if err != nil {
			return nil, err
		}
		return (Type)(FnType{
			ret:  retTy,
			noad: swapNode(def, parent),
		}), nil
	case "formal_parameter":
		fallthrough
	case "enhanced_for_statement":
		tyNode := parent.ChildByFieldName("type")
		if tyNode == nil {
			squirrel.breadcrumb(swapNode(def, parent), "defToType: could not find type")
			return nil, nil
		}
		return squirrel.getTypeDefJava(ctx, swapNode(def, tyNode))
	case "variable_declarator":
		grandparent := parent.Parent()
		if grandparent == nil {
			return nil, nil
		}
		tyNode := grandparent.ChildByFieldName("type")
		if tyNode == nil {
			squirrel.breadcrumb(swapNode(def, parent), "defToType: could not find type")
			return nil, nil
		}
		return squirrel.getTypeDefJava(ctx, swapNode(def, tyNode))
	default:
		squirrel.breadcrumb(swapNode(def, parent), fmt.Sprintf("unrecognized def parent %q", parent.Type()))
		return nil, nil
	}
}

func lazyTypeStringer(ty *Type) func() fmt.Stringer {
	return func() fmt.Stringer {
		if ty != nil && *ty != nil {
			return String((*ty).variant())
		} else {
			return String("<nil>")
		}
	}
}
