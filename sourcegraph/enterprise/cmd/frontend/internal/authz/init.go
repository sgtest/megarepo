package authz

import (
	"fmt"
	"strings"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/authz"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/frontend/internal/licensing"
	"github.com/sourcegraph/sourcegraph/pkg/conf"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

func init() {
	// Warn about usage of auth providers that are not enabled by the license.
	graphqlbackend.AlertFuncs = append(graphqlbackend.AlertFuncs, func(args graphqlbackend.AlertFuncArgs) []*graphqlbackend.Alert {
		// Only site admins can act on this alert, so only show it to site admins.
		if !args.IsSiteAdmin {
			return nil
		}

		if licensing.IsFeatureEnabledLenient(licensing.FeatureACLs) {
			return nil
		}

		var authzTypes []string
		for _, g := range conf.Get().Github {
			if g.Authorization != nil {
				authzTypes = append(authzTypes, "GitHub")
				break
			}
		}
		for _, g := range conf.Get().Gitlab {
			if g.Authorization != nil {
				authzTypes = append(authzTypes, "GitLab")
				break
			}
		}
		if len(authzTypes) > 0 {
			return []*graphqlbackend.Alert{{
				TypeValue:    graphqlbackend.AlertTypeError,
				MessageValue: fmt.Sprintf("A Sourcegraph license is required to enable repository permissions for the following code hosts: %s. [**Get a license.**](/site-admin/license)", strings.Join(authzTypes, ", ")),
			}}
		}
		return nil
	})
}

func init() {
	conf.ContributeValidator(func(cfg conf.Unified) []string {
		_, _, seriousProblems, warnings := providersFromConfig(&cfg)
		return append(seriousProblems, warnings...)
	})
	go conf.Watch(func() {
		allowAccessByDefault, authzProviders, _, _ := providersFromConfig(conf.Get())
		authz.SetProviders(allowAccessByDefault, authzProviders)
	})
}

// providersFromConfig returns the set of permission-related providers derived from the site config.
// It also returns any validation problems with the config, separating these into "serious problems"
// and "warnings".  "Serious problems" are those that should make Sourcegraph set
// authz.allowAccessByDefault to false. "Warnings" are all other validation problems.
func providersFromConfig(cfg *conf.Unified) (
	allowAccessByDefault bool,
	authzProviders []authz.Provider,
	seriousProblems []string,
	warnings []string,
) {
	allowAccessByDefault = true
	defer func() {
		if len(seriousProblems) > 0 {
			log15.Error("Repository authz config was invalid (errors are visible in the UI as an admin user, you should fix ASAP). Restricting access to repositories by default for now to be safe.")
			allowAccessByDefault = false
		}
	}()

	glp, glproblems, glwarnings := gitlabProvidersFromConfig(cfg)
	authzProviders = append(authzProviders, glp...)
	seriousProblems = append(seriousProblems, glproblems...)
	warnings = append(warnings, glwarnings...)

	ghp, ghproblems, ghwarnings := githubProvidersFromConfig(cfg)
	authzProviders = append(authzProviders, ghp...)
	seriousProblems = append(seriousProblems, ghproblems...)
	warnings = append(warnings, ghwarnings...)

	return allowAccessByDefault, authzProviders, seriousProblems, warnings
}
