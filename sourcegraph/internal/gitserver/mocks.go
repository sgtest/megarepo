package gitserver

import (
	"io"
)

// Mocks is used to mock behavior in tests. Tests must call ResetMocks() when finished to ensure its
// mocks are not (inadvertently) used by subsequent tests.
//
// (The emptyMocks is used by ResetMocks to zero out Mocks without needing to use a named type.)
//
// NOTE: These are temporary and are being copied over from the vcs/git package
// while we move most of that functionality onto our gitserver client. Once
// that's done, we should take advantage of the generated mock client in this
// package instead.
var Mocks, emptyMocks struct {
	ExecReader func(args []string) (reader io.ReadCloser, err error)
}

// ResetMocks clears the mock functions set on Mocks (so that subsequent tests don't inadvertently
// use them).
func ResetMocks() {
	Mocks = emptyMocks
}
