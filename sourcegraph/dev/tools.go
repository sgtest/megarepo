// +build tools

package main

import (
	// zoekt-* used in sourcegraph/server docker image build
	_ "github.com/google/zoekt/cmd/zoekt-archive-index"
	_ "github.com/google/zoekt/cmd/zoekt-git-index"
	_ "github.com/google/zoekt/cmd/zoekt-sourcegraph-indexserver"
	_ "github.com/google/zoekt/cmd/zoekt-webserver"

	// go-mockgen is used to codegen mockable interfaces, used in precise code intel tests
	_ "github.com/efritz/go-mockgen"

	// used in schema pkg
	_ "github.com/sourcegraph/go-jsonschema/cmd/go-jsonschema-compiler"

	// used in many places
	_ "golang.org/x/tools/cmd/stringer"
)
