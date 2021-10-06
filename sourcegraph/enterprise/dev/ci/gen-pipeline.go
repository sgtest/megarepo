// gen-pipeline.go generates a Buildkite YAML file that tests the entire
// Sourcegraph application and writes it to stdout.
package main

import (
	"flag"
	"fmt"
	"io"
	"os"
	"strings"
	"time"

	"github.com/sourcegraph/sourcegraph/enterprise/dev/ci/internal/buildkite"
	"github.com/sourcegraph/sourcegraph/enterprise/dev/ci/internal/ci"
)

var preview bool

func init() {
	flag.BoolVar(&preview, "preview", false, "Preview the pipeline steps")
}

func main() {
	flag.Parse()

	config := ci.NewConfig(time.Now())

	pipeline, err := ci.GeneratePipeline(config)
	if err != nil {
		panic(err)
	}

	if preview {
		previewPipeline(os.Stdout, config, pipeline)
		return
	}

	_, err = pipeline.WriteTo(os.Stdout)
	if err != nil {
		panic(err)
	}
}

func previewPipeline(w io.Writer, c ci.Config, bk *buildkite.Pipeline) {
	fmt.Fprintf(w, "Detected run type: %s\n", c.RunType.String())
	fmt.Fprintf(w, "Detected Changed files (%d):\n", len(c.ChangedFiles))
	for _, f := range c.ChangedFiles {
		fmt.Fprintf(w, "\t%s\n", f)
	}
	fmt.Fprintln(w, "Detected changes:")
	fmt.Fprintf(w, "\tAffects Client: %t\n", c.ChangedFiles.AffectsClient())
	fmt.Fprintf(w, "\tAffects Go: %t\n", c.ChangedFiles.AffectsGo())
	fmt.Fprintf(w, "\tAffects Dockerfiles: %t\n", c.ChangedFiles.AffectsDockerfiles())
	fmt.Fprintf(w, "\tAffects GraphQL: %t\n", c.ChangedFiles.AffectsGraphQL())
	fmt.Fprintf(w, "\tAffects SG: %t\n", c.ChangedFiles.AffectsSg())
	fmt.Fprintf(w, "Computed Build Steps:\n")
	for _, raw := range bk.Steps {
		if step, ok := raw.(*buildkite.Step); ok {
			fmt.Fprintf(w, "\t%s\n", step.Label)
			if len(step.DependsOn) > 0 {
				fmt.Fprintf(w, "\t→ depends on %s\n", strings.Join(step.DependsOn, " "))
			}
		}
	}
}
