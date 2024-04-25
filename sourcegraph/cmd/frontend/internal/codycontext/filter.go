package codycontext

import (
	"context"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/dotcom"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

// FileMatcher returns true if the given repo and path are allowed to be returned. It is used to filter out
// file matches that don't satisfy Cody ignore rules
type FileMatcher func(repoID api.RepoID, path string) bool
type RepoContentFilter interface {
	// GetMatcher returns a matcher to filter out file matches, and returns a filtered down list of
	// repositories containing only the ones that are allowed to be searched.
	GetMatcher(ctx context.Context, repos []types.RepoIDName) ([]types.RepoIDName, FileMatcher, error)
}

func newRepoContentFilter(logger log.Logger, client gitserver.Client) RepoContentFilter {
	if dotcom.SourcegraphDotComMode() {
		return newDotcomFilter(logger, client)
	}
	return newEnterpriseFilter(logger)
}
