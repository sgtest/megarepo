package gitserver

import (
	"fmt"

	"github.com/sourcegraph/sourcegraph/internal/api"
)

// RevisionNotFoundError is an error that reports a revision doesn't exist.
type RevisionNotFoundError struct {
	Repo api.RepoName
	Spec string
}

func (e *RevisionNotFoundError) Error() string {
	return fmt.Sprintf("revision not found: %s@%s", e.Repo, e.Spec)
}

func (e *RevisionNotFoundError) HTTPStatusCode() int {
	return 404
}

// IsRevisionNotFound reports if err is a RevisionNotFoundError.
func IsRevisionNotFound(err error) bool {
	_, ok := err.(*RevisionNotFoundError)
	return ok
}
