package golang

import (
	"context"

	"github.com/sourcegraph/sourcegraph/lib/codeintel/lsif_typed"
)

// Index returns an LSIF Typed index for all of the provided source files, which should use the syntax of
// the "repro_lang" programming language. Search for files with the `*.repro` file extension to see examples
// of how repro_lang programs looks like. Search for "grammar.js" to see the tree-sitter grammar of the repro_lang syntax.
func Index(
	projectRoot, packageName string,
	sources []*lsif_typed.SourceFile,
	dependencies []*Dependency,
) (*lsif_typed.Index, error) {
	index := &lsif_typed.Index{
		Metadata: &lsif_typed.Metadata{
			Version: 0,
			ToolInfo: &lsif_typed.ToolInfo{
				Name:      "repro_lang",
				Version:   "1.0.0",
				Arguments: []string{"arg1", "arg2"},
			},
			ProjectRoot:          projectRoot,
			TextDocumentEncoding: lsif_typed.TextEncoding_UTF8,
		},
		Documents:       nil,
		ExternalSymbols: nil,
	}

	ctx := &reproContext{
		globalScope: newScope(),
		pkg: &lsif_typed.Package{
			Manager: "repro-manager",
			Name:    packageName,
			Version: "1.0.0",
		},
	}

	// Phase 1: parse sources
	var reproSources []*reproSourceFile
	for _, source := range sources {
		reproSource, err := parseSourceFile(context.Background(), source)
		if err != nil {
			return nil, err
		}
		reproSources = append(reproSources, reproSource)
	}
	var reproDependencies []*reproDependency
	for _, dependency := range dependencies {
		dep := &reproDependency{Package: dependency.Package}
		reproDependencies = append(reproDependencies, dep)
		for _, source := range dependency.Sources {
			reproSource, err := parseSourceFile(context.Background(), source)
			if err != nil {
				return nil, err
			}
			dep.Sources = append(dep.Sources, reproSource)
		}
	}

	// Phase 2: resolve names for definitions
	for _, dependency := range reproDependencies {
		dependency.enterGlobalDefinitions(ctx)
	}
	for _, file := range reproSources {
		file.enterDefinitions(ctx)
	}

	// Phase 3: resolve names for references
	for _, file := range reproSources {
		file.resolveReferences(ctx)
	}

	// Phase 4: emit LSIF Typed
	for _, file := range reproSources {
		lsifDocument := &lsif_typed.Document{
			RelativePath: file.Source.RelativePath,
			Occurrences:  file.occurrences(),
			Symbols:      file.symbols(),
		}
		index.Documents = append(index.Documents, lsifDocument)
	}

	return index, nil
}

type reproContext struct {
	globalScope *reproScope
	pkg         *lsif_typed.Package
}

type reproScope struct {
	names map[string]string
}

func newScope() *reproScope {
	return &reproScope{names: map[string]string{}}
}
