package repo

import (
	"strings"

	"github.com/sourcegraph/go-diff/diff"

	"github.com/sourcegraph/sourcegraph/dev/sg/internal/run"
)

// State represents the state of the repository.
type State struct {
	// Branch is the currently checked out branch.
	Branch string
}

type DiffHunk struct {
	// StartLine is new start line
	StartLine int
	// AddedLines are lines that got added
	AddedLines []string
}

func (s *State) GetDiff(paths string) (map[string][]DiffHunk, error) {
	if paths == "" {
		paths = "**/*"
	}

	target := "origin/main..." // compare from common ancestor
	if s.Branch == "main" {
		target = "@^" // previous commit
	}

	diffOutput, err := run.TrimResult(run.GitCmd("diff", target, "--", paths))
	if err != nil {
		return nil, err
	}
	return parseDiff(diffOutput)
}

func parseDiff(diffOutput string) (map[string][]DiffHunk, error) {
	fullDiffs, err := diff.ParseMultiFileDiff([]byte(diffOutput))
	if err != nil {
		return nil, err
	}

	diffs := make(map[string][]DiffHunk)
	for _, d := range fullDiffs {
		if d.NewName == "" {
			continue
		}

		// b/dev/sg/lints.go -> dev/sg/lints.go
		fileName := strings.SplitN(d.NewName, "/", 2)[1]

		// Summarize hunks
		for _, h := range d.Hunks {
			lines := strings.Split(string(h.Body), "\n")

			var addedLines []string
			for _, l := range lines {
				// +$LINE -> $LINE
				if strings.HasPrefix(l, "+") {
					addedLines = append(addedLines, strings.TrimPrefix(l, "+"))
				}
			}

			diffs[fileName] = append(diffs[fileName], DiffHunk{
				StartLine:  int(h.NewStartLine),
				AddedLines: addedLines,
			})
		}
	}
	return diffs, nil
}
