package ci

import (
	"os"
	"strings"

	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"
)

// RunType indicates the type of this run. Each CI pipeline can only be a single run type.
type RunType int

const (
	PullRequest RunType = iota // pull request build
	MainBranch                 // main branch build

	// Releases

	TaggedRelease // semver-tagged release
	ReleaseBranch // release branch build

	// Browser extensions

	BextReleaseBranch // browser extension release build
	BextNightly       // browser extension nightly build

	// Patches (NOT patch releases)

	ImagePatch       // build a patched image
	ImagePatchNoTest // build a patched image without testing
	CandidatesNoTest // build all candidates without testing

	// Special run cases

	MainDryRun    // run everything main does, except for deploy-related steps
	BackendDryRun // run backend tests that are used on main
)

func computeRunType(tag, branch string) RunType {
	switch {
	case strings.HasPrefix(tag, "v"):
		return TaggedRelease
	case lazyregexp.New(`^[0-9]+\.[0-9]+$`).MatchString(branch):
		return ReleaseBranch

	case strings.HasPrefix(branch, "docker-images-patch/"):
		return ImagePatch
	case strings.HasPrefix(branch, "docker-images-patch-notest/"):
		return ImagePatch
	case branch == "docker-images-candidates-notest":
		return CandidatesNoTest

	case branch == "bext/release":
		return BextReleaseBranch
	case os.Getenv("BEXT_NIGHTLY") == "true":
		return BextNightly

	case strings.HasPrefix(branch, "main-dry-run/"):
		return MainDryRun
	case strings.HasPrefix(branch, "backend-dry-run/"):
		return BackendDryRun

	case branch == "main":
		return MainBranch

	default:
		// If no specific run type is matched, assumed to be a PR
		return PullRequest
	}
}

// Is returns true if this run type Is one of the given RunTypes
func (t RunType) Is(oneOfTypes ...RunType) bool {
	for _, rt := range oneOfTypes {
		if t == rt {
			return true
		}
	}
	return false
}
