package repoupdater

import (
	"fmt"

	"github.com/sourcegraph/sourcegraph/internal/api"
)

// ErrNotFound is an error that occurs when a Repo doesn't exist.
type ErrNotFound struct {
	Repo       api.RepoName
	IsNotFound bool
}

// ErrNotFound returns true if the repo does Not exist.
func (e *ErrNotFound) NotFound() bool {
	return e.IsNotFound
}

func (e *ErrNotFound) Error() string {
	return fmt.Sprintf("repository not found (name=%s notfound=%v)", e.Repo, e.IsNotFound)
}

// ErrUnauthorized is an error that occurs when repo access is
// unauthorized.
type ErrUnauthorized struct {
	Repo    api.RepoName
	NoAuthz bool
}

// Unauthorized returns true if repo access is unauthorized.
func (e *ErrUnauthorized) Unauthorized() bool {
	return e.NoAuthz
}

func (e *ErrUnauthorized) Error() string {
	return fmt.Sprintf("not authorized (name=%s noauthz=%v)", e.Repo, e.NoAuthz)
}

// ErrTemporary is an error that can be retried
type ErrTemporary struct {
	Repo        api.RepoName
	IsTemporary bool
}

// ErrTemporary is when the repository was reported as being temporarily
// unavailable.
func (e *ErrTemporary) Temporary() bool {
	return e.IsTemporary
}

func (e *ErrTemporary) Error() string {
	return fmt.Sprintf("repository temporarily unavailable (name=%s istemporary=%v)", e.Repo, e.IsTemporary)
}
