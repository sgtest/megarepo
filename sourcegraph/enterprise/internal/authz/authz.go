package authz

import (
	"context"
	"fmt"
	"strconv"
	"strings"

	"github.com/cockroachdb/errors"
	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/authz/bitbucketserver"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/authz/github"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/authz/gitlab"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/authz/perforce"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/schema"
)

type ExternalServicesStore interface {
	List(context.Context, database.ExternalServicesListOptions) ([]*types.ExternalService, error)
}

// ProvidersFromConfig returns the set of permission-related providers derived from the site config.
// It also returns any validation problems with the config, separating these into "serious problems"
// and "warnings". "Serious problems" are those that should make Sourcegraph set authz.allowAccessByDefault
// to false. "Warnings" are all other validation problems.
func ProvidersFromConfig(
	ctx context.Context,
	cfg conftypes.SiteConfigQuerier,
	store ExternalServicesStore,
) (
	allowAccessByDefault bool,
	providers []authz.Provider,
	seriousProblems []string,
	warnings []string,
) {
	allowAccessByDefault = true
	defer func() {
		if len(seriousProblems) > 0 {
			log15.Error("Repository authz config was invalid (errors are visible in the UI as an admin user, you should fix ASAP). Restricting access to repositories by default for now to be safe.", "seriousProblems", seriousProblems)
			allowAccessByDefault = false
		}
	}()

	opt := database.ExternalServicesListOptions{
		ExcludeNamespaceUser: true,
		Kinds: []string{
			extsvc.KindGitHub,
			extsvc.KindGitLab,
			extsvc.KindBitbucketServer,
			extsvc.KindPerforce,
		},
		LimitOffset: &database.LimitOffset{
			Limit: 500, // The number is randomly chosen
		},
	}

	var (
		gitHubConns          []*types.GitHubConnection
		gitLabConns          []*types.GitLabConnection
		bitbucketServerConns []*types.BitbucketServerConnection
		perforceConns        []*types.PerforceConnection
	)
	for {
		svcs, err := store.List(ctx, opt)
		if err != nil {
			seriousProblems = append(seriousProblems, fmt.Sprintf("Could not list external services: %v", err))
			break
		}
		if len(svcs) == 0 {
			break // No more results, exiting
		}
		opt.AfterID = svcs[len(svcs)-1].ID // Advance the cursor

		for _, svc := range svcs {
			if svc.CloudDefault { // Only public repos in CloudDefault services
				continue
			}

			cfg, err := extsvc.ParseConfig(svc.Kind, svc.Config)
			if err != nil {
				seriousProblems = append(seriousProblems, fmt.Sprintf("Could not parse config of external service %d: %v", svc.ID, err))
				continue
			}

			switch c := cfg.(type) {
			case *schema.GitHubConnection:
				gitHubConns = append(gitHubConns, &types.GitHubConnection{
					URN:              svc.URN(),
					GitHubConnection: c,
				})
			case *schema.GitLabConnection:
				gitLabConns = append(gitLabConns, &types.GitLabConnection{
					URN:              svc.URN(),
					GitLabConnection: c,
				})
			case *schema.BitbucketServerConnection:
				bitbucketServerConns = append(bitbucketServerConns, &types.BitbucketServerConnection{
					URN:                       svc.URN(),
					BitbucketServerConnection: c,
				})
			case *schema.PerforceConnection:
				perforceConns = append(perforceConns, &types.PerforceConnection{
					URN:                svc.URN(),
					PerforceConnection: c,
				})
			default:
				log15.Error("ProvidersFromConfig", "error", errors.Errorf("unexpected connection type: %T", cfg))
				continue
			}
		}

		if len(svcs) < opt.Limit {
			break // Less results than limit means we've reached end
		}
	}

	if len(gitHubConns) > 0 {
		enableGithubInternalRepoVisibility := false
		ef := cfg.SiteConfig().ExperimentalFeatures
		if ef != nil {
			enableGithubInternalRepoVisibility = ef.EnableGithubInternalRepoVisibility
		}

		ghProviders, ghProblems, ghWarnings := github.NewAuthzProviders(gitHubConns, cfg.SiteConfig().AuthProviders, enableGithubInternalRepoVisibility)
		providers = append(providers, ghProviders...)
		seriousProblems = append(seriousProblems, ghProblems...)
		warnings = append(warnings, ghWarnings...)
	}

	if len(gitLabConns) > 0 {
		glProviders, glProblems, glWarnings := gitlab.NewAuthzProviders(cfg.SiteConfig(), gitLabConns)
		providers = append(providers, glProviders...)
		seriousProblems = append(seriousProblems, glProblems...)
		warnings = append(warnings, glWarnings...)
	}

	if len(bitbucketServerConns) > 0 {
		bbsProviders, bbsProblems, bbsWarnings := bitbucketserver.NewAuthzProviders(bitbucketServerConns)
		providers = append(providers, bbsProviders...)
		seriousProblems = append(seriousProblems, bbsProblems...)
		warnings = append(warnings, bbsWarnings...)
	}

	if len(perforceConns) > 0 {
		pfProviders, pfProblems, pfWarnings := perforce.NewAuthzProviders(perforceConns)
		providers = append(providers, pfProviders...)
		seriousProblems = append(seriousProblems, pfProblems...)
		warnings = append(warnings, pfWarnings...)
	}

	// 🚨 SECURITY: Warn the admin when both code host authz provider and the permissions user mapping are configured.
	if cfg.SiteConfig().PermissionsUserMapping != nil &&
		cfg.SiteConfig().PermissionsUserMapping.Enabled {
		allowAccessByDefault = false
		if len(providers) > 0 {
			serviceTypes := make([]string, len(providers))
			for i := range providers {
				serviceTypes[i] = strconv.Quote(providers[i].ServiceType())
			}
			msg := fmt.Sprintf(
				"The permissions user mapping (site configuration `permissions.userMapping`) cannot be enabled when %s authorization providers are in use. Blocking access to all repositories until the conflict is resolved.",
				strings.Join(serviceTypes, ", "))
			seriousProblems = append(seriousProblems, msg)
		}
	}

	return allowAccessByDefault, providers, seriousProblems, warnings
}

var MockProviderFromExternalService func(siteConfig schema.SiteConfiguration, svc *types.ExternalService) (authz.Provider, error)

// ProviderFromExternalService returns the parsed authz.Provider derived from
// the site config and the given external service.
//
// It returns `(nil, nil)` if no authz.Provider can be derived and no error had
// occurred.
func ProviderFromExternalService(siteConfig schema.SiteConfiguration, svc *types.ExternalService) (authz.Provider, error) {
	if MockProviderFromExternalService != nil {
		return MockProviderFromExternalService(siteConfig, svc)
	}

	cfg, err := extsvc.ParseConfig(svc.Kind, svc.Config)
	if err != nil {
		return nil, errors.Wrap(err, "parse config")
	}

	var providers []authz.Provider
	var problems []string

	enableGithubInternalRepoVisibility := false
	ex := siteConfig.ExperimentalFeatures
	if ex != nil {
		enableGithubInternalRepoVisibility = ex.EnableGithubInternalRepoVisibility
	}

	switch c := cfg.(type) {
	case *schema.GitHubConnection:
		providers, problems, _ = github.NewAuthzProviders(
			[]*types.GitHubConnection{
				{
					URN:              svc.URN(),
					GitHubConnection: c,
				},
			},
			siteConfig.AuthProviders,
			enableGithubInternalRepoVisibility,
		)
	case *schema.GitLabConnection:
		providers, problems, _ = gitlab.NewAuthzProviders(
			siteConfig,
			[]*types.GitLabConnection{
				{
					URN:              svc.URN(),
					GitLabConnection: c,
				},
			},
		)
	case *schema.BitbucketServerConnection:
		providers, problems, _ = bitbucketserver.NewAuthzProviders(
			[]*types.BitbucketServerConnection{
				{
					URN:                       svc.URN(),
					BitbucketServerConnection: c,
				},
			},
		)
	case *schema.PerforceConnection:
		providers, problems, _ = perforce.NewAuthzProviders(
			[]*types.PerforceConnection{
				{
					URN:                svc.URN(),
					PerforceConnection: c,
				},
			},
		)
	default:
		return nil, errors.Errorf("unsupported connection type %T", cfg)
	}

	if len(problems) > 0 {
		return nil, errors.New(problems[0])
	}

	if len(providers) == 0 {
		return nil, nil
	}
	return providers[0], nil
}
