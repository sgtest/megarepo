package main

import (
	"github.com/sourcegraph/sourcegraph/cmd/server/shared"
	"github.com/sourcegraph/sourcegraph/internal/sanitycheck"

	_ "github.com/sourcegraph/sourcegraph/ui/assets/oss" // Select oss assets
)

func main() {
	sanitycheck.Pass()
	shared.Main()
}
