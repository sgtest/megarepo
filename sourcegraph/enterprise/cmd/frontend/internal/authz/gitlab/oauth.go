package gitlab

import (
	"context"
	"fmt"

	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitlab"
)

// FetchUserPerms returns a list of private project IDs (on code host) that the given account
// has read access to. The project ID has the same value as it would be
// used as api.ExternalRepoSpec.ID. The returned list only includes private project IDs.
//
// This method may return partial but valid results in case of error, and it is up to
// callers to decide whether to discard.
//
// API docs: https://docs.gitlab.com/ee/api/projects.html#list-all-projects
func (p *OAuthAuthzProvider) FetchUserPerms(ctx context.Context, account *extsvc.ExternalAccount) ([]extsvc.ExternalRepoID, error) {
	if account == nil {
		return nil, errors.New("no account provided")
	} else if !extsvc.IsHostOfAccount(p.codeHost, account) {
		return nil, fmt.Errorf("not a code host of the account: want %+v but have %+v", account, p.codeHost)
	}

	_, tok, err := gitlab.GetExternalAccountData(&account.ExternalAccountData)
	if err != nil {
		return nil, errors.Wrap(err, "get external account data")
	}

	client := p.clientProvider.GetOAuthClient(tok.AccessToken)
	return listProjects(ctx, client)
}

// FetchRepoPerms returns a list of user IDs (on code host) who have read ccess to
// the given project on the code host. The user ID has the same value as it would
// be used as extsvc.ExternalAccount.AccountID. The returned list includes both
// direct access and inherited from the group membership.
//
// This method may return partial but valid results in case of error, and it is up to
// callers to decide whether to discard.
//
// API docs: https://docs.gitlab.com/ee/api/members.html#list-all-members-of-a-group-or-project-including-inherited-members
func (p *OAuthAuthzProvider) FetchRepoPerms(ctx context.Context, repo *api.ExternalRepoSpec) ([]extsvc.ExternalAccountID, error) {
	if repo == nil {
		return nil, errors.New("no repository provided")
	} else if !extsvc.IsHostOfRepo(p.codeHost, repo) {
		return nil, fmt.Errorf("not a code host of the repository: want %+v but have %+v", repo, p.codeHost)
	}

	client := p.clientProvider.GetPATClient(p.token, "")
	return listMembers(ctx, client, repo.ID)
}
