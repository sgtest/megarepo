package main

import (
	"context"
	"flag"
	"fmt"
	"strings"

	"github.com/peterbourgon/ff/v3/ffcli"

	depgraph "github.com/sourcegraph/sourcegraph/dev/depgraph/internal/graph"
	"github.com/sourcegraph/sourcegraph/dev/depgraph/internal/visualization"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var traceInternalFlagSet = flag.NewFlagSet("depgraph trace-internal", flag.ExitOnError)

var traceInternalCommand = &ffcli.Command{
	Name:       "trace-internal",
	ShortUsage: "depgraph trace-internal {package}",
	ShortHelp:  "Outputs a DOT-formatted graph of the given package's internal dependencies",
	FlagSet:    traceInternalFlagSet,
	Exec:       traceInternal,
}

func traceInternal(ctx context.Context, args []string) error {
	if len(args) != 1 {
		return errors.Errorf("expected exactly one package")
	}
	pkg := args[0]

	root, err := findRoot()
	if err != nil {
		return err
	}

	graph, err := depgraph.Load(root)
	if err != nil {
		return err
	}
	if _, ok := graph.PackageNames[pkg]; !ok {
		return errors.Newf("pkg %q not found", pkg)
	}

	packages, dependencyEdges := filterExternalReferences(graph, pkg)
	fmt.Printf("%s\n", visualization.Dotify(packages, dependencyEdges, nil))
	return nil
}

func filterExternalReferences(graph *depgraph.DependencyGraph, prefix string) ([]string, map[string][]string) {
	packages := make([]string, 0, len(graph.Packages))
	for _, pkg := range graph.Packages {
		if strings.HasPrefix(pkg, prefix) {
			packages = append(packages, pkg)
		}
	}

	dependencyEdges := map[string][]string{}
	for pkg, dependencies := range graph.Dependencies {
		if strings.HasPrefix(pkg, prefix) {
			for _, dependency := range dependencies {
				if strings.HasPrefix(dependency, prefix) {
					dependencyEdges[pkg] = append(dependencyEdges[pkg], dependency)
				}
			}
		}
	}

	return packages, dependencyEdges
}
