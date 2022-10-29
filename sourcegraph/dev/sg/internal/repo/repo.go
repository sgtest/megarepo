package repo

import (
	"context"
	"strings"

	"github.com/sourcegraph/go-diff/diff"
	"github.com/sourcegraph/run"

	sgrun "github.com/sourcegraph/sourcegraph/dev/sg/internal/run"
	"github.com/sourcegraph/sourcegraph/dev/sg/root"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// State represents the state of the repository.
type State struct {
	// Dirty indicates if the current working directory has uncommitted changes.
	Dirty bool
	// Ref is the currently checked out ref.
	Ref string
	// MergeBase is the common ancestor between Ref and main.
	MergeBase string

	// mockDiff can be injected for testing with NewMockState()
	mockDiff Diff
}

// GetState parses the git state of the root repository.
func GetState(ctx context.Context) (*State, error) {
	dirty, err := root.Run(run.Cmd(ctx, "git diff --name-only")).Lines()
	if err != nil {
		return nil, err
	}
	mergeBase, err := sgrun.TrimResult(sgrun.GitCmd("merge-base", "main", "HEAD"))
	if err != nil {
		return nil, err
	}
	ref, err := sgrun.TrimResult(sgrun.GitCmd("rev-parse", "HEAD"))
	if err != nil {
		return nil, err
	}

	return &State{Dirty: len(dirty) > 0, Ref: ref, MergeBase: mergeBase}, nil
}

// NewMockState returns a state that returns the given mocks.
func NewMockState(mockDiff Diff) *State {
	return &State{mockDiff: mockDiff}
}

type Diff map[string][]DiffHunk

// IterateHunks calls cb over each hunk in this diff, collects all errors encountered, and
// wraps each error with the file name and the position of each hunk.
func (d Diff) IterateHunks(cb func(file string, hunk DiffHunk) error) error {
	var mErr error
	for file, hunks := range d {
		for _, hunk := range hunks {
			if err := cb(file, hunk); err != nil {
				mErr = errors.Append(mErr, errors.Wrapf(err, "%s:%d", file, hunk.StartLine))
			}
		}
	}
	return mErr
}

type DiffHunk struct {
	// StartLine is new start line
	StartLine int
	// AddedLines are lines that got added
	AddedLines []string
}

// GetDiff retrieves a parsed diff from the workspace, filtered by the given path glob.
func (s *State) GetDiff(glob string) (Diff, error) {
	if s.mockDiff != nil {
		return s.mockDiff, nil
	}

	// Compare with common ancestor by default
	target := s.MergeBase
	if !s.Dirty && s.Ref == s.MergeBase {
		// Compare previous commit, if we are already at merge base and in a clean workdir
		target = "@^"
	}

	diffOutput, err := sgrun.TrimResult(sgrun.GitCmd("diff", target, "--", glob))
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
		if d.NewName == "" || d.NewName == "/dev/null" {
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
