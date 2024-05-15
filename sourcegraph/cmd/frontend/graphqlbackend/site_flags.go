package graphqlbackend

import (
	"context"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/envvar"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/pkg/conf"
)

func (r *siteResolver) NeedsRepositoryConfiguration(ctx context.Context) (bool, error) {
	if envvar.SourcegraphDotComMode() {
		return false, nil
	}

	// 🚨 SECURITY: The site alerts may contain sensitive data, so only site
	// admins may view them.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return false, err
	}

	return needsRepositoryConfiguration(), nil
}

func needsRepositoryConfiguration() bool {
	cfg := conf.Get()
	return len(cfg.Github) == 0 && len(cfg.Gitlab) == 0 && len(cfg.ReposList) == 0 && len(cfg.AwsCodeCommit) == 0 && len(cfg.Gitolite) == 0 && len(cfg.BitbucketServer) == 0
}

func (r *siteResolver) NoRepositoriesEnabled(ctx context.Context) (bool, error) {
	if envvar.SourcegraphDotComMode() {
		return false, nil
	}

	// 🚨 SECURITY: The site alerts may contain sensitive data, so only site
	// admins may view them.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return false, err
	}

	return noRepositoriesEnabled(ctx)
}

func noRepositoriesEnabled(ctx context.Context) (bool, error) {
	// Fastest way to see if even a single enabled repository exists.
	repos, err := db.Repos.List(ctx, db.ReposListOptions{
		Enabled:     true,
		Disabled:    false,
		LimitOffset: &db.LimitOffset{Limit: 1},
	})
	if err != nil {
		return false, err
	}
	return len(repos) == 0, nil
}

func (*siteResolver) ExternalAuthEnabled() bool {
	for _, p := range conf.AuthProviders() {
		if p.Builtin == nil {
			return true // has a non-builtin auth provider
		}
	}
	return false
}

func (*siteResolver) ConfigurationNotice(ctx context.Context) bool {
	// 🚨 SECURITY: Only the site admin cares about this. Leaking a boolean wouldn't be a security
	// vulnerability, but just in case this method is changed to return more information, let's lock
	// it down.
	if err := backend.CheckCurrentUserIsSiteAdmin(ctx); err != nil {
		return false
	}

	messages, err := conf.Validate(conf.Raw())
	return len(messages) > 0 || err != nil
}

func (*siteResolver) DisableBuiltInSearches() bool {
	return conf.Get().DisableBuiltInSearches
}

func (*siteResolver) SendsEmailVerificationEmails() bool { return conf.EmailVerificationRequired() }
