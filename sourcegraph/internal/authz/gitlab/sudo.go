package gitlab

import (
	"context"
	"fmt"
	"net/url"
	"strconv"
	"time"

	"github.com/pkg/errors"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/auth/providers"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/authz"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitlab"
	"github.com/sourcegraph/sourcegraph/internal/httpcli"
)

// SudoProvider is an implementation of AuthzProvider that provides repository permissions as
// determined from a GitLab instance API. For documentation of specific fields, see the docstrings
// of SudoProviderOp.
type SudoProvider struct {
	// sudoToken is the sudo-scoped access token. This is different from the Sudo parameter, which
	// is set per client and defines which user to impersonate.
	sudoToken string

	urn               string
	clientProvider    *gitlab.ClientProvider
	clientURL         *url.URL
	codeHost          *extsvc.CodeHost
	gitlabProvider    string
	authnConfigID     providers.ConfigID
	useNativeUsername bool
}

var _ authz.Provider = (*SudoProvider)(nil)

type SudoProviderOp struct {
	// The unique resource identifier of the external service where the provider is defined.
	URN string

	// BaseURL is the URL of the GitLab instance.
	BaseURL *url.URL

	// AuthnConfigID identifies the authn provider to use to lookup users on the GitLab instance.
	// This should be the authn provider that's used to sign into the GitLab instance.
	AuthnConfigID providers.ConfigID

	// GitLabProvider is the id of the authn provider to GitLab. It will be used in the
	// `users?extern_uid=$uid&provider=$provider` API query.
	GitLabProvider string

	// SudoToken is an access token with sudo *and* api scope.
	//
	// 🚨 SECURITY: This value contains secret information that must not be shown to non-site-admins.
	SudoToken string

	// UseNativeUsername, if true, maps Sourcegraph users to GitLab users using username equivalency
	// instead of the authn provider user ID. This is *very* insecure (Sourcegraph usernames can be
	// changed at the user's will) and should only be used in development environments.
	UseNativeUsername bool
}

func newSudoProvider(op SudoProviderOp, cli httpcli.Doer) *SudoProvider {
	return &SudoProvider{
		sudoToken: op.SudoToken,

		urn:               op.URN,
		clientProvider:    gitlab.NewClientProvider(op.BaseURL, cli),
		clientURL:         op.BaseURL,
		codeHost:          extsvc.NewCodeHost(op.BaseURL, extsvc.TypeGitLab),
		authnConfigID:     op.AuthnConfigID,
		gitlabProvider:    op.GitLabProvider,
		useNativeUsername: op.UseNativeUsername,
	}
}

func (p *SudoProvider) Validate() (problems []string) {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if _, _, err := p.clientProvider.GetPATClient(p.sudoToken, "1").ListProjects(ctx, "projects"); err != nil {
		if err == ctx.Err() {
			problems = append(problems, fmt.Sprintf("GitLab API did not respond within 5s (%s)", err.Error()))
		} else if !gitlab.IsNotFound(err) {
			problems = append(problems, "access token did not have sufficient privileges, requires scopes \"sudo\" and \"api\"")
		}
	}
	return problems
}

func (p *SudoProvider) URN() string {
	return p.urn
}

func (p *SudoProvider) ServiceID() string {
	return p.codeHost.ServiceID
}

func (p *SudoProvider) ServiceType() string {
	return p.codeHost.ServiceType
}

// FetchAccount satisfies the authz.Provider interface. It iterates through the current list of
// linked external accounts, find the one (if it exists) that matches the authn provider specified
// in the SudoProvider struct, and fetches the user account from the GitLab API using that identity.
func (p *SudoProvider) FetchAccount(ctx context.Context, user *types.User, current []*extsvc.Account) (mine *extsvc.Account, err error) {
	if user == nil {
		return nil, nil
	}

	var glUser *gitlab.User
	if p.useNativeUsername {
		glUser, err = p.fetchAccountByUsername(ctx, user.Username)
	} else {
		// resolve the GitLab account using the authn provider (specified by p.AuthnConfigID)
		authnProvider := providers.GetProviderByConfigID(p.authnConfigID)
		if authnProvider == nil {
			return nil, nil
		}
		var authnAcct *extsvc.Account
		for _, acct := range current {
			if acct.ServiceID == authnProvider.CachedInfo().ServiceID && acct.ServiceType == authnProvider.ConfigID().Type {
				authnAcct = acct
				break
			}
		}
		if authnAcct == nil {
			return nil, nil
		}
		glUser, err = p.fetchAccountByExternalUID(ctx, authnAcct.AccountID)
	}
	if err != nil {
		return nil, err
	}
	if glUser == nil {
		return nil, nil
	}

	var accountData extsvc.AccountData
	gitlab.SetExternalAccountData(&accountData, glUser, nil)

	glExternalAccount := extsvc.Account{
		UserID: user.ID,
		AccountSpec: extsvc.AccountSpec{
			ServiceType: p.codeHost.ServiceType,
			ServiceID:   p.codeHost.ServiceID,
			AccountID:   strconv.Itoa(int(glUser.ID)),
		},
		AccountData: accountData,
	}
	return &glExternalAccount, nil
}

func (p *SudoProvider) fetchAccountByExternalUID(ctx context.Context, uid string) (*gitlab.User, error) {
	q := make(url.Values)
	q.Add("extern_uid", uid)
	q.Add("provider", p.gitlabProvider)
	q.Add("per_page", "2")
	glUsers, _, err := p.clientProvider.GetPATClient(p.sudoToken, "").ListUsers(ctx, "users?"+q.Encode())
	if err != nil {
		return nil, err
	}
	if len(glUsers) >= 2 {
		return nil, fmt.Errorf("failed to determine unique GitLab user for query %q", q.Encode())
	}
	if len(glUsers) == 0 {
		return nil, nil
	}
	return glUsers[0], nil
}

func (p *SudoProvider) fetchAccountByUsername(ctx context.Context, username string) (*gitlab.User, error) {
	q := make(url.Values)
	q.Add("username", username)
	q.Add("per_page", "2")
	glUsers, _, err := p.clientProvider.GetPATClient(p.sudoToken, "").ListUsers(ctx, "users?"+q.Encode())
	if err != nil {
		return nil, err
	}
	if len(glUsers) >= 2 {
		return nil, fmt.Errorf("failed to determine unique GitLab user for query %q", q.Encode())
	}
	if len(glUsers) == 0 {
		return nil, nil
	}
	return glUsers[0], nil
}

// FetchUserPerms returns a list of project IDs (on code host) that the given account
// has read access on the code host. The project ID has the same value as it would be
// used as api.ExternalRepoSpec.ID. The returned list only includes private project IDs.
//
// This method may return partial but valid results in case of error, and it is up to
// callers to decide whether to discard.
//
// API docs: https://docs.gitlab.com/ee/api/projects.html#list-all-projects
func (p *SudoProvider) FetchUserPerms(ctx context.Context, account *extsvc.Account) ([]extsvc.RepoID, error) {
	if account == nil {
		return nil, errors.New("no account provided")
	} else if !extsvc.IsHostOfAccount(p.codeHost, account) {
		return nil, fmt.Errorf("not a code host of the account: want %q but have %q",
			account.AccountSpec.ServiceID, p.codeHost.ServiceID)
	}

	user, _, err := gitlab.GetExternalAccountData(&account.AccountData)
	if err != nil {
		return nil, errors.Wrap(err, "get external account data")
	}

	client := p.clientProvider.GetPATClient(p.sudoToken, strconv.Itoa(int(user.ID)))
	return listProjects(ctx, client)
}

// listProjects is a helper function to request for all private projects that are accessible
// (access level: 20 => Reporter access) by the authenticated or impersonated user in the client.
// It may return partial but valid results in case of error, and it is up to callers to decide
// whether to discard.
func listProjects(ctx context.Context, client *gitlab.Client) ([]extsvc.RepoID, error) {
	q := make(url.Values)
	q.Add("visibility", "private")  // This method is meant to return only private projects
	q.Add("min_access_level", "20") // 20 => Reporter access (i.e. have access to project code)
	q.Add("per_page", "100")        // 100 is the maximum page size

	// The next URL to request for projects, and it is reused in the succeeding for loop.
	nextURL := "projects?" + q.Encode()

	// 100 matches the maximum page size, thus a good default to avoid multiple allocations
	// when appending the first 100 results to the slice.
	projectIDs := make([]extsvc.RepoID, 0, 100)
	for {
		projects, next, err := client.ListProjects(ctx, nextURL)
		if err != nil {
			return projectIDs, err
		}

		for _, p := range projects {
			projectIDs = append(projectIDs, extsvc.RepoID(strconv.Itoa(p.ID)))
		}

		if next == nil {
			break
		}
		nextURL = *next
	}

	return projectIDs, nil
}

// FetchRepoPerms returns a list of user IDs (on code host) who have read access to
// the given project on the code host. The user ID has the same value as it would
// be used as extsvc.Account.AccountID. The returned list includes both direct access
// and inherited from the group membership.
//
// This method may return partial but valid results in case of error, and it is up to
// callers to decide whether to discard.
//
// API docs: https://docs.gitlab.com/ee/api/members.html#list-all-members-of-a-group-or-project-including-inherited-members
func (p *SudoProvider) FetchRepoPerms(ctx context.Context, repo *extsvc.Repository) ([]extsvc.AccountID, error) {
	if repo == nil {
		return nil, errors.New("no repository provided")
	} else if !extsvc.IsHostOfRepo(p.codeHost, &repo.ExternalRepoSpec) {
		return nil, fmt.Errorf("not a code host of the repository: want %q but have %q",
			repo.ServiceID, p.codeHost.ServiceID)
	}

	client := p.clientProvider.GetPATClient(p.sudoToken, "")
	return listMembers(ctx, client, repo.ID)
}

// listMembers is a helper function to request for all users who has read access
// (access level: 20 => Reporter access) to given project on the code host, including
// both direct access and inherited from the group membership. It may return partial
// but valid results in case of error, and it is up to callers to decide whether to
// discard.
func listMembers(ctx context.Context, client *gitlab.Client, repoID string) ([]extsvc.AccountID, error) {
	q := make(url.Values)
	q.Add("per_page", "100") // 100 is the maximum page size

	// The next URL to request for members, and it is reused in the succeeding for loop.
	nextURL := fmt.Sprintf("projects/%s/members/all?%s", repoID, q.Encode())

	// 100 matches the maximum page size, thus a good default to avoid multiple allocations
	// when appending the first 100 results to the slice.
	userIDs := make([]extsvc.AccountID, 0, 100)

	for {
		members, next, err := client.ListMembers(ctx, nextURL)
		if err != nil {
			return userIDs, err
		}

		for _, m := range members {
			// Members with access level 20 (i.e. Reporter) has access to project code.
			if m.AccessLevel < 20 {
				continue
			}

			userIDs = append(userIDs, extsvc.AccountID(strconv.Itoa(int(m.ID))))
		}

		if next == nil {
			break
		}
		nextURL = *next
	}

	return userIDs, nil
}
