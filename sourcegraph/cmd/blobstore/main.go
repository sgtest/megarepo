// blobstore is the blobstore server.
package main // import "github.com/sourcegraph/sourcegraph/cmd/blobstore"

import (
	"github.com/sourcegraph/sourcegraph/cmd/blobstore/shared"
	"github.com/sourcegraph/sourcegraph/internal/sanitycheck"
	"github.com/sourcegraph/sourcegraph/internal/service/svcmain"
)

func main() {
	sanitycheck.Pass()
	svcmain.SingleServiceMain(shared.Service)
}
