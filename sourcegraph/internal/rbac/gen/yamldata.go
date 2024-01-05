package main

import (
	"flag"
	"fmt"
	"io"
	"os"
	"strings"

	"gopkg.in/yaml.v3"
)

const (
	GoGeneratedByTarget = "// Generated code - DO NOT EDIT. Regenerate by running 'bazel run //internal/rbac:write_generated'"
	TSGeneratedByTarget = "// Generated code - DO NOT EDIT. Regenerate by running 'bazel run //client/web/src/rbac:write_generated'"
)

var (
	inputFile  = flag.String("i", "schema.yaml", "input schema")
	outputFile = flag.String("o", "", "output file")
	lang       = flag.String("lang", "go", "language to generate output for")
	kind       = flag.String("kind", "constants", "the kind of output to be generated")
)

type namespace struct {
	Name    string   `yaml:"name"`
	Actions []string `yaml:"actions"`
}

type schema struct {
	Namespaces          []namespace `yaml:"namespaces"`
	ExcludeFromUserRole []string    `yaml:"excludeFromUserRole"`
}

type namespaceAction struct {
	varName string
	action  string
}

type permissionNamespace struct {
	Namespace string
	Action    string
}

func (pn *permissionNamespace) zanziBarFormat() string {
	// check that this conforms to types.Permission.DisplayName()
	return fmt.Sprintf("%s#%s", pn.Namespace, pn.Action)
}

// This generates the permission constants used on the frontend and backend for access control checks.
// The source of truth for RBAC is the `schema.yaml`, and this parses the YAML file, constructs the permission
// display names and saves the display names as constants.
func main() {
	flag.Parse()

	schema, err := loadSchema(*inputFile)
	if err != nil {
		fmt.Fprintf(os.Stderr, "failed to load schema from %q: %v\n", *inputFile, err)
		os.Exit(1)
	}

	if *outputFile == "" {
		flag.Usage()
		os.Exit(1)
	}

	output, err := os.Create(*outputFile)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	defer output.Close()

	var permissions = []permissionNamespace{}
	var namespaces = make([]string, len(schema.Namespaces))
	var actions = []namespaceAction{}
	for index, ns := range schema.Namespaces {
		for _, action := range ns.Actions {
			namespaces[index] = ns.Name

			actionVarName := fmt.Sprintf("%s%sAction", sentencizeNamespace(ns.Name), toTitleCase(action))
			actions = append(actions, namespaceAction{
				varName: actionVarName,
				action:  action,
			})

			permissions = append(permissions, permissionNamespace{
				Namespace: ns.Name,
				Action:    action,
			})
		}
	}

	switch strings.ToLower(*lang) {
	case "go":
		if *kind == "constants" {
			generateGoConstants(output, permissions)
		} else if *kind == "namespace" {
			generateNamespaces(output, namespaces)
		} else if *kind == "action" {
			generateActions(output, actions)
		} else {
			fmt.Fprintf(os.Stderr, "unknown kind %q\nm", *kind)
			os.Exit(1)
		}
	case "ts":
		generateTSConstants(output, permissions)
	default:
		fmt.Fprintf(os.Stderr, "unknown lang %q\n", *lang)
		os.Exit(1)

	}
}

func loadSchema(filename string) (*schema, error) {
	fd, err := os.Open(filename)
	if err != nil {
		return nil, err
	}
	defer fd.Close()

	var parsed schema
	err = yaml.NewDecoder(fd).Decode(&parsed)
	return &parsed, err
}

func generateTSConstants(output io.Writer, permissions []permissionNamespace) {
	fmt.Fprintln(output, TSGeneratedByTarget)
	permissionNames := make([]string, len(permissions))
	for index, permission := range permissions {
		fmt.Fprintln(output)
		name := permission.zanziBarFormat()
		permissionNames[index] = fmt.Sprintf("'%s'", name)
		fmt.Fprintf(output, "export const %sPermission: RbacPermission = '%s'\n", sentencizeNamespace(name), name)
	}
	fmt.Fprintln(output)
	fmt.Fprintf(output, "export type RbacPermission = %s\n", strings.Join(permissionNames, " | "))
}

func generateGoConstants(output io.Writer, permissions []permissionNamespace) {
	fmt.Fprintln(output, GoGeneratedByTarget)
	fmt.Fprintln(output, "package rbac")
	for _, permission := range permissions {
		fmt.Fprintln(output)
		name := permission.zanziBarFormat()
		fmt.Fprintf(output, "const %sPermission string = \"%s\"\n", sentencizeNamespace(name), name)
	}
}

func generateNamespaces(output io.Writer, namespaces []string) {
	fmt.Fprintln(output, GoGeneratedByTarget)
	fmt.Fprintln(output, "package types")
	fmt.Fprintln(output)

	namespacesConstants := make([]string, len(namespaces))
	namespaceVariableNames := make([]string, len(namespaces))
	for index, namespace := range namespaces {
		namespaceVarName := fmt.Sprintf("%sNamespace", sentencizeNamespace(namespace))
		namespacesConstants[index] = fmt.Sprintf("const %s PermissionNamespace = \"%s\"", namespaceVarName, namespace)
		namespaceVariableNames[index] = namespaceVarName
	}

	fmt.Fprintf(output, rbacNamespaceTemplate, strings.Join(namespacesConstants, "\n"), strings.Join(namespaceVariableNames, ", "))
}

func generateActions(output io.Writer, namespaceActions []namespaceAction) {
	fmt.Fprintln(output, GoGeneratedByTarget)
	fmt.Fprintln(output, "package types")
	fmt.Fprintln(output)

	namespaceActionConstants := make([]string, len(namespaceActions))
	for index, namespaceAction := range namespaceActions {
		namespaceActionConstants[index] = fmt.Sprintf("const %s NamespaceAction = \"%s\"", namespaceAction.varName, namespaceAction.action)
	}

	fmt.Fprintf(output, rbacActionTemplate, strings.Join(namespaceActionConstants, "\n"))
}

func sentencizeNamespace(permission string) string {
	separators := [2]string{"#", "_"}
	// Replace all separators with white spaces
	for _, sep := range separators {
		permission = strings.ReplaceAll(permission, sep, " ")
	}

	return toTitleCase(permission)
}

func toTitleCase(input string) string {
	words := strings.Fields(input)

	formattedWords := make([]string, len(words))

	for i, word := range words {
		formattedWords[i] = strings.Title(strings.ToLower(word))
	}

	return strings.Join(formattedWords, "")
}

const rbacNamespaceTemplate = `
// A PermissionNamespace represents a distinct context within which permission policies
// are defined and enforced.
type PermissionNamespace string

func (n PermissionNamespace) String() string {
	return string(n)
}

%s

// Valid checks if a namespace is valid and supported by Sourcegraph's RBAC system.
func (n PermissionNamespace) Valid() bool {
	switch n {
	case %s:
		return true
	default:
		return false
	}
}
`

const rbacActionTemplate = `
// NamespaceAction represents the action permitted in a namespace.
type NamespaceAction string

func (a NamespaceAction) String() string {
	return string(a)
}

%s
`
