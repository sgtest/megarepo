package main

import (
	"os"
	"strconv"

	"github.com/sourcegraph/sourcegraph/cmd/server/shared"
	"github.com/sourcegraph/sourcegraph/internal/sanitycheck"

	_ "github.com/sourcegraph/sourcegraph/ui/assets/enterprise" // Select enterprise assets
)

func main() {
	sanitycheck.Pass()

	enableEmbeddings, _ := strconv.ParseBool(os.Getenv("SRC_ENABLE_EMBEDDINGS"))
	if enableEmbeddings {
		shared.ProcfileAdditions = append(
			shared.ProcfileAdditions,
			`embeddings: embeddings`,
		)
		shared.SrcProfServices = append(
			shared.SrcProfServices,
			map[string]string{"Name": "embeddings", "Host": "127.0.0.1:6099"},
		)
	}

	shared.Main()
}
