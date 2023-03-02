package azuredevops

import (
	"context"
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/sourcegraph/log"
	authztypes "github.com/sourcegraph/sourcegraph/enterprise/internal/authz/types"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/licensing"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/auth"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/azuredevops"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"golang.org/x/exp/maps"
)

var mockServerURL string

func NewAuthzProviders(db database.DB, conns []*types.AzureDevOpsConnection) *authztypes.ProviderInitResult {
	orgs, projects := map[string]struct{}{}, map[string]struct{}{}

	initResults := &authztypes.ProviderInitResult{}

	if len(conns) == 0 {
		return initResults
	}

	// Iterate over all Azure Dev Ops code host connections to make sure we sync permissions for all
	// orgs and projects in every permissions sync iteration.
	for _, c := range conns {
		// The list of orgs and projects may have duplicates if there are multiple Azure DevOps code
		// host connections that have the same project in their config.
		//
		// Add them to a map so that we may filter out duplicates before passing them over to the
		// provider.
		for _, name := range c.Orgs {
			orgs[name] = struct{}{}
		}

		for _, name := range c.Projects {
			projects[name] = struct{}{}
		}
	}

	// Convert the map back to a slice now that we have a unique list of orgs and projects.
	uniqueOrgs := maps.Keys(orgs)
	uniqueProjects := maps.Keys(projects)

	p, err := newAuthzProvider(db, conns, uniqueOrgs, uniqueProjects)
	if err != nil {
		initResults.InvalidConnections = append(initResults.InvalidConnections, extsvc.TypeAzureDevOps)
		initResults.Problems = append(initResults.Problems, err.Error())
	} else if p != nil {
		initResults.Providers = append(initResults.Providers, p)
	}

	return initResults
}

func newAuthzProvider(db database.DB, conns []*types.AzureDevOpsConnection, orgs, projects []string) (*Provider, error) {
	if err := licensing.Check(licensing.FeatureACLs); err != nil {
		return nil, err
	}

	u, err := url.Parse(azuredevops.AzureDevOpsAPIURL)
	if err != nil {
		return nil, errors.Wrapf(err, "failed to parse url: %q, this is likely a misconfigured URL in the constant azuredevops.AzureDevOpsAPIURL", azuredevops.AzureDevOpsAPIURL)
	}

	return &Provider{
		db:       db,
		urn:      "azuredevops:authzprovider",
		conns:    conns,
		codeHost: extsvc.NewCodeHost(u, extsvc.TypeAzureDevOps),
		orgs:     orgs,
		projects: projects,
	}, nil
}

type Provider struct {
	db database.DB

	urn      string
	codeHost *extsvc.CodeHost

	conns []*types.AzureDevOpsConnection

	// orgs is the list of orgs as configured in the code host connection.
	orgs []string
	// projects is the list of projects as configured in the code host connection.
	projects []string
}

func (p *Provider) FetchAccount(_ context.Context, _ *types.User, _ []*extsvc.Account, _ []string) (*extsvc.Account, error) {
	return nil, nil
}

func (p *Provider) FetchUserPerms(ctx context.Context, account *extsvc.Account, _ authz.FetchPermsOptions) (*authz.ExternalUserPermissions, error) {
	logger := log.Scoped("azuredevops.FetchuserPerms", "logger for azuredevops provider")

	logger.Debug("starting FetchUserPerms", log.String("user ID", fmt.Sprintf("%#v", account.UserID)))

	_, token, err := azuredevops.GetExternalAccountData(ctx, &account.AccountData)
	if err != nil {
		return nil, errors.Wrapf(
			err,
			"failed to load external account data from database with external account with ID: %d",
			account.ID,
		)
	}

	oauthToken := &auth.OAuthBearerToken{
		Token:              token.AccessToken,
		RefreshToken:       token.RefreshToken,
		Expiry:             token.Expiry,
		NeedsRefreshBuffer: 5,
	}

	oauthContext, err := azuredevops.GetOAuthContext(token.RefreshToken)
	if err != nil {
		return nil, errors.Wrapf(err, "failed to generate oauth context, this is likely a misconfiguration with the Azure OAuth provider (bad URL?), please check the auth.providers configuration in your site config")
	}

	oauthToken.RefreshFunc = database.GetAccountRefreshAndStoreOAuthTokenFunc(p.db, account.ID, oauthContext)

	var apiURL string
	if mockServerURL != "" {
		apiURL = mockServerURL
	} else {
		apiURL = azuredevops.AzureDevOpsAPIURL
	}

	client, err := azuredevops.NewClient(
		p.ServiceID(),
		apiURL,
		oauthToken,
		nil,
	)
	if err != nil {
		return nil, errors.Wrapf(
			err,
			"failed to create client for service ID: %q, account ID: %q", p.ServiceID(), account.AccountID,
		)
	}

	var repos []azuredevops.Repository

	for _, org := range p.orgs {
		logger.Debug("listing repos", log.String("org", fmt.Sprintf("%#v", org)))

		foundRepos, err := client.ListRepositoriesByProjectOrOrg(ctx, azuredevops.ListRepositoriesByProjectOrOrgArgs{
			ProjectOrOrgName: org,
		})
		if err != nil {
			if httpErr, ok := err.(*azuredevops.HTTPError); ok {
				// If the HTTPError is 401 / 403 / 404, this user does not have access to this org.
				// Skip and continue to the next.
				//
				// For orgs that don't exist, the API returns 404. For orgs that the user does not
				// have access to the API returns 401. We're not sure if the API might return 403
				// for some use case but we don't want to hard fail on that either.
				if httpErr.StatusCode == http.StatusUnauthorized || httpErr.StatusCode == http.StatusForbidden || httpErr.StatusCode == http.StatusNotFound {

					logger.Debug("user does not have access to this org",
						log.String("org", org),
						log.Int("http status code", httpErr.StatusCode),
					)

					continue
				}
			}

			// For any other errors, we want to hard fail so that the issue can be identified.
			return nil, errors.Newf("failed to list repositories for org: %q with error: %q", org, err.Error())
		}

		logger.Debug("adding repos", log.Int("count", len(foundRepos)))
		repos = append(repos, foundRepos...)
	}

	for _, project := range p.projects {
		foundRepos, err := client.ListRepositoriesByProjectOrOrg(ctx, azuredevops.ListRepositoriesByProjectOrOrgArgs{
			ProjectOrOrgName: project,
		})
		if err != nil {
			if httpErr, ok := err.(*azuredevops.HTTPError); ok {
				// If the HTTPError is 401 / 403 / 404, this user does not have access to this org.
				// Skip and continue to the next.
				//
				// For orgs/projects that don't exist, or the user does not have access to the API
				// returns 404. We're not sure if the API might return 401 or 403 for some use case
				// but we don't want to hard fail on that either.
				if httpErr.StatusCode == http.StatusUnauthorized || httpErr.StatusCode == http.StatusForbidden || httpErr.StatusCode == http.StatusNotFound {

					logger.Debug("user does not have access to this project",
						log.String("project", project),
						log.Int("http status code", httpErr.StatusCode),
					)

					continue
				}
			}

			// For any other errors, we want to hard fail so that the issue can be identified.
			return nil, errors.Newf("failed to list repositories for project: %q with error: %q", project, err.Error())
		}

		repos = append(repos, foundRepos...)
	}

	extIDs := make([]extsvc.RepoID, 0, len(repos))
	for _, repo := range repos {
		extIDs = append(extIDs, extsvc.RepoID(repo.ID))
	}

	return &authz.ExternalUserPermissions{
		Exacts: extIDs,
	}, err
}

// FetchRepoPerms remains unimplemented for Azure DevOps.
//
// Repo permissions syncing is a three step process with the Azure DevOps API:
// 1. Trigger a permissions report for a specific repo
// 2. Check the status of the permissions report (and maybe backoff and check again until the report is generated)
// 3. Download the report and parse it to generate the permissions table
//
// This makes the entire process per repo fragile and cumbersome. Repo syncing could be unreliable and may not scale very well in terms of rate limits if we have to make at least three API requests per repo.
//
// As a result, we prefer incremental user permissions sync instead.
func (p *Provider) FetchRepoPerms(_ context.Context, _ *extsvc.Repository, _ authz.FetchPermsOptions) ([]extsvc.AccountID, error) {
	return nil, authz.ErrUnimplemented{Feature: "azuredevops.FetchRepoPerms"}
}

func (p *Provider) ServiceType() string {
	return p.codeHost.ServiceType
}

func (p *Provider) ServiceID() string {
	return p.codeHost.ServiceID
}

func (p *Provider) URN() string {
	return p.urn
}

func (p *Provider) ValidateConnection(ctx context.Context) error {
	ctx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()

	allErrors := []string{}
	for _, conn := range p.conns {
		client, err := azuredevops.NewClient(
			p.ServiceID(),
			azuredevops.AzureDevOpsAPIURL,
			&auth.BasicAuth{
				Username: conn.Username,
				Password: conn.Token,
			},
			nil,
		)

		if err != nil {
			allErrors = append(allErrors, fmt.Sprintf("%s:%s", conn.URN, err.Error()))
			continue
		}

		_, err = client.GetAuthorizedProfile(ctx)
		if err != nil {
			allErrors = append(allErrors, err.Error())
		}
	}

	if len(allErrors) > 0 {
		msg := strings.Join(allErrors, "\n")
		return errors.Newf("ValidateConnection failed for Azure DevOps with the following errors: %s", msg)
	}

	return nil
}
