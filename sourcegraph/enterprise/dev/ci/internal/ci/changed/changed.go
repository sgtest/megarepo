package changed

import (
	"strings"
)

// Files is the list of changed files to operate over in a pipeline.
//
// Helper functions on Files should all be in the format `AffectsXYZ`.
type Files []string

// AffectsDocs returns whether the changes affects documentation.
func (f Files) AffectsDocs() bool {
	for _, p := range f {
		if strings.HasPrefix(p, "doc/") && p != "CHANGELOG.md" {
			return true
		}
	}
	return false
}

// affectsSg returns whether the changes affects the ./dev/sg folder.
func (c Files) AffectsSg() bool {
	for _, p := range c {
		if strings.HasPrefix(p, "dev/sg/") {
			return true
		}
	}
	return false
}

// AffectsGo returns whether the changes affects go files.
func (c Files) AffectsGo() bool {
	for _, p := range c {
		if strings.HasSuffix(p, ".go") || p == "go.sum" || p == "go.mod" {
			return true
		}
	}
	return false
}

// AffectsDockerfiles returns whether the changes affects Dockerfiles.
func (f Files) AffectsDockerfiles() bool {
	for _, p := range f {
		if strings.HasPrefix(p, "Dockerfile") || strings.HasSuffix(p, "Dockerfile") {
			return true
		}
	}
	return false
}

// AffectsGraphQL returns whether the changes affects GraphQL files
func (f Files) AffectsGraphQL() bool {
	for _, p := range f {
		if strings.HasSuffix(p, ".graphql") {
			return true
		}
	}
	return false
}

// AffectsClient returns whether files that affect client code were changed.
// Used to detect if we need to run Puppeteer or Chromatic tests.
func (f Files) AffectsClient() bool {
	for _, p := range f {
		if !strings.HasSuffix(p, ".md") && (strings.HasPrefix(p, "client/") || isAllowedRootFile(p)) {
			return true
		}
	}
	return false
}
