package inference

import (
	"testing"

	"github.com/sourcegraph/sourcegraph/lib/codeintel/autoindex/config"
)

func TestRecognizersRust(t *testing.T) {
	testRecognizers(t,
		recognizerTestCase{
			description: "rust-analyzer",
			repositoryContents: map[string]string{
				"foo/bar/Cargo.toml": "",
				"foo/baz/Cargo.toml": "",
			},
			expected: []config.IndexJob{
				{
					Steps:       nil,
					LocalSteps:  nil,
					Root:        "",
					Indexer:     "sourcegraph/lsif-rust",
					IndexerArgs: []string{"lsif-rust", "index"},
					Outfile:     "dump.lsif",
				},
			},
		},
	)
}
