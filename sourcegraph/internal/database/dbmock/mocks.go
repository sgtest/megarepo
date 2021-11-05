package dbmock

//go:generate ../../../dev/mockgen.sh github.com/sourcegraph/sourcegraph/internal/database -d ./ -i DB -i AccessTokenStore -i AuthzStore -i EventLogStore -i ExternalServiceStore -i FeatureFlagStore -i NamespaceStore -i OrgInvitationStore -i OrgMemberStore -i OrgStore -i PhabricatorStore -i RepoStore -i SavedSearchStore -i SearchContextsStore -i SettingsStore -i SubRepoPermsStore -i TemporarySettingsStore -i UserCredentialsStore -i UserEmailsStore -i UserExternalAccountsStore -i UserPublicRepoStore -i UserStore -i WebhookLogStore
