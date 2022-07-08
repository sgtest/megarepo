package printer

import (
	"testing"

	"github.com/hexops/autogold"

	"github.com/sourcegraph/sourcegraph/internal/search/job"
)

func TestPrettyMermaid(t *testing.T) {
	t.Run("verbose", func(t *testing.T) {
		t.Run("simpleJob", func(t *testing.T) {
			autogold.Equal(t, autogold.Raw(MermaidVerbose(simpleJob, job.VerbosityBasic)))
		})

		t.Run("bigJob", func(t *testing.T) {
			autogold.Equal(t, autogold.Raw(MermaidVerbose(bigJob, job.VerbosityBasic)))
		})
	})

	t.Run("nonverbose", func(t *testing.T) {
		t.Run("simpleJob", func(t *testing.T) {
			autogold.Equal(t, autogold.Raw(Mermaid(simpleJob)))
		})

		t.Run("bigJob", func(t *testing.T) {
			autogold.Equal(t, autogold.Raw(Mermaid(bigJob)))
		})
	})
}
