package runtype

import (
	"strings"

	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// RunType indicates the type of this run. Each CI pipeline can only be a single run type.
type RunType int

const (
	// RunTypes should be defined by order of precedence.

	PullRequest    RunType = iota // pull request build
	WolfiExpBranch                // branch that only builds wolfi images

	// Nightly builds - must be first because they take precedence

	ReleaseNightly // release branch nightly healthcheck builds
	BextNightly    // browser extension nightly build
	VsceNightly    // vs code extension nightly build
	CodyNightly    // cody vs code extension nightly build
	AppRelease     // app release build
	AppInsiders    // app insiders build

	// Release branches

	TaggedRelease     // semver-tagged release
	ReleaseBranch     // release branch build
	BextReleaseBranch // browser extension release build
	VsceReleaseBranch // vs code extension release build
	CodyReleaseBranch // cody extension release build

	// Main branches

	MainBranch // main branch build
	MainDryRun // run everything main does, except for deploy-related steps

	// Build branches (NOT releases)

	ImagePatch          // build a patched image after testing
	ImagePatchNoTest    // build a patched image without testing
	CandidatesNoTest    // build one or all candidate images without testing
	ExecutorPatchNoTest // build executor image without testing

	// Special test branches

	BackendIntegrationTests // run backend tests that are used on main

	// None is a no-op, add all run types above this type.
	None
)

// Compute determines what RunType matches the given parameters.
func Compute(tag, branch string, env map[string]string) RunType {
	for runType := PullRequest + 1; runType < None; runType += 1 {
		if runType.Matcher().Matches(tag, branch, env) {
			return runType
		}
	}
	// RunType is PullRequest by default
	return PullRequest
}

// RunTypes returns all runtypes.
func RunTypes() []RunType {
	var results []RunType
	for runType := PullRequest + 1; runType < None; runType += 1 {
		results = append(results, runType)
	}
	return results
}

// Is returns true if this run type is one of the given RunTypes
func (t RunType) Is(oneOfTypes ...RunType) bool {
	for _, rt := range oneOfTypes {
		if t == rt {
			return true
		}
	}
	return false
}

// Matcher returns the requirements for a build to be considered of this RunType.
func (t RunType) Matcher() *RunTypeMatcher {
	switch t {
	case ReleaseNightly:
		return &RunTypeMatcher{
			EnvIncludes: map[string]string{
				"RELEASE_NIGHTLY": "true",
			},
		}
	case BextNightly:
		return &RunTypeMatcher{
			EnvIncludes: map[string]string{
				"BEXT_NIGHTLY": "true",
			},
		}
	case CodyNightly:
		return &RunTypeMatcher{
			EnvIncludes: map[string]string{
				"CODY_NIGHTLY": "true",
			},
		}
	case VsceNightly:
		return &RunTypeMatcher{
			EnvIncludes: map[string]string{
				"VSCE_NIGHTLY": "true",
			},
		}
	case VsceReleaseBranch:
		return &RunTypeMatcher{
			Branch:      "vsce/release",
			BranchExact: true,
		}
	case CodyReleaseBranch:
		return &RunTypeMatcher{
			Branch:      "cody/release",
			BranchExact: true,
		}

	case AppRelease:
		return &RunTypeMatcher{
			Branch:      "app/release",
			BranchExact: true,
		}
	case AppInsiders:
		return &RunTypeMatcher{
			Branch:      "app/insiders",
			BranchExact: true,
		}

	case TaggedRelease:
		return &RunTypeMatcher{
			TagPrefix: "v",
		}
	case ReleaseBranch:
		return &RunTypeMatcher{
			Branch:       `^[0-9]+\.[0-9]+$`,
			BranchRegexp: true,
		}
	case BextReleaseBranch:
		return &RunTypeMatcher{
			Branch:      "bext/release",
			BranchExact: true,
		}

	case MainBranch:
		return &RunTypeMatcher{
			Branch:      "main",
			BranchExact: true,
		}
	case MainDryRun:
		return &RunTypeMatcher{
			Branch: "main-dry-run/",
		}
	case WolfiExpBranch:
		return &RunTypeMatcher{
			Branch: "wolfi/",
		}
	case ImagePatch:
		return &RunTypeMatcher{
			Branch:                 "docker-images-patch/",
			BranchArgumentRequired: true,
		}
	case ImagePatchNoTest:
		return &RunTypeMatcher{
			Branch:                 "docker-images-patch-notest/",
			BranchArgumentRequired: true,
		}
	case CandidatesNoTest:
		return &RunTypeMatcher{
			Branch: "docker-images-candidates-notest/",
		}
	case ExecutorPatchNoTest:
		return &RunTypeMatcher{
			Branch: "executor-patch-notest/",
		}

	case BackendIntegrationTests:
		return &RunTypeMatcher{
			Branch: "backend-integration/",
		}
	}

	return nil
}

func (t RunType) String() string {
	switch t {
	case PullRequest:
		return "Pull request"
	case WolfiExpBranch:
		return "Wolfi Exp Branch"
	case ReleaseNightly:
		return "Release branch nightly healthcheck build"
	case BextNightly:
		return "Browser extension nightly release build"
	case CodyNightly:
		return "Cody VS Code extension nightly release build"
	case VsceNightly:
		return "VS Code extension nightly release build"
	case AppRelease:
		return "App release build"
	case AppInsiders:
		return "App insiders build"
	case TaggedRelease:
		return "Tagged release"
	case ReleaseBranch:
		return "Release branch"
	case BextReleaseBranch:
		return "Browser extension release build"
	case VsceReleaseBranch:
		return "VS Code extension release build"
	case CodyReleaseBranch:
		return "Cody VS Code extension release build"

	case MainBranch:
		return "Main branch"
	case MainDryRun:
		return "Main dry run"

	case ImagePatch:
		return "Patch image"
	case ImagePatchNoTest:
		return "Patch image without testing"
	case CandidatesNoTest:
		return "Build all candidates without testing"
	case ExecutorPatchNoTest:
		return "Build executor without testing"

	case BackendIntegrationTests:
		return "Backend integration tests"
	}
	return ""
}

// RunTypeMatcher defines the requirements for any given build to be considered a build of
// this RunType.
type RunTypeMatcher struct {
	// Branch loosely matches branches that begin with this value, unless a different type
	// of match is indicated (e.g. BranchExact, BranchRegexp)
	Branch       string
	BranchExact  bool
	BranchRegexp bool
	// BranchArgumentRequired indicates the path segment following the branch prefix match is
	// expected to be an argument (does not work in conjunction with BranchExact)
	BranchArgumentRequired bool

	// TagPrefix matches tags that begin with this value.
	TagPrefix string

	// EnvIncludes validates if these key-value pairs are configured in environment.
	EnvIncludes map[string]string
}

// Matches returns true if the given properties and environment match this RunType.
func (m *RunTypeMatcher) Matches(tag, branch string, env map[string]string) bool {
	if m.Branch != "" {
		switch {
		case m.BranchExact:
			return m.Branch == branch
		case m.BranchRegexp:
			return lazyregexp.New(m.Branch).MatchString(branch)
		default:
			return strings.HasPrefix(branch, m.Branch)
		}
	}

	if m.TagPrefix != "" {
		return strings.HasPrefix(tag, m.TagPrefix)
	}

	if len(m.EnvIncludes) > 0 && len(env) > 0 {
		for wantK, wantV := range m.EnvIncludes {
			gotV, exists := env[wantK]
			if !exists || (wantV != gotV) {
				return false
			}
		}
		return true
	}

	return false
}

// IsBranchPrefixMatcher indicates that this matcher matches on branch prefixes.
func (m *RunTypeMatcher) IsBranchPrefixMatcher() bool {
	return m.Branch != "" && !m.BranchExact && !m.BranchRegexp
}

// ExtractBranchArgument extracts the second segment, delimited by '/', of the branch as
// an argument, for example:
//
//	prefix/{argument}
//	prefix/{argument}/something-else
//
// If BranchArgumentRequired, an error is returned if no argument is found.
//
// Only works with Branch matches, and does not work with BranchExact.
func (m *RunTypeMatcher) ExtractBranchArgument(branch string) (string, error) {
	if m.BranchExact || m.Branch == "" {
		return "", errors.New("unsupported matcher type")
	}

	parts := strings.Split(branch, "/")
	if len(parts) < 2 || len(parts[1]) == 0 {
		if m.BranchArgumentRequired {
			return "", errors.New("branch argument expected, but none found")
		}
		return "", nil
	}
	return parts[1], nil
}
