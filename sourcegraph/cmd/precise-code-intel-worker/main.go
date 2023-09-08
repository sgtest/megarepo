package main

import (
	"github.com/sourcegraph/sourcegraph/cmd/precise-code-intel-worker/shared"
	"github.com/sourcegraph/sourcegraph/internal/sanitycheck"
	"github.com/sourcegraph/sourcegraph/internal/service/svcmain"
)

var config = svcmain.Config{}

func main() {
	sanitycheck.Pass()
	svcmain.SingleServiceMain(shared.Service, config)
}
