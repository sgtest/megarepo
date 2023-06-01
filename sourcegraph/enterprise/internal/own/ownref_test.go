package own

import (
	"context"
	"encoding/json"
	"fmt"
	"testing"
	"time"

	"github.com/sourcegraph/log/logtest"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/sourcegraph/sourcegraph/internal/auth/providers"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/types"

	edb "github.com/sourcegraph/sourcegraph/enterprise/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
)

const (
	username        = "jdoe"
	verifiedEmail   = "john.doe@example.com"
	unverifiedEmail = "john-the-unverified@example.com"
	gitHubLogin     = "jdoegh"
	gitLabLogin     = "jdoegl"
	gerritLogin     = "no"
)

func TestSearchFilteringExample(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := edb.NewEnterpriseDB(database.NewDB(logger, dbtest.NewDB(logger, t)))
	ctx := context.Background()
	user, err := initUser(ctx, t, db)
	require.NoError(t, err)

	// Now we add 2 verified emails.
	testTime := time.Now().Round(time.Second).UTC()
	verificationCode := "ok"
	_, err = db.ExecContext(ctx,
		`INSERT INTO user_emails(user_id, email, verification_code, verified_at) VALUES($1, $2, $3, $4)`,
		user.ID, "john-the-BIG-dough@example.com", verificationCode, testTime)
	require.NoError(t, err)
	_, err = db.ExecContext(ctx,
		`INSERT INTO user_emails(user_id, email, verification_code, verified_at) VALUES($1, $2, $3, $4)`,
		user.ID, "john-aka-im-rich@didyouget.it", verificationCode, testTime)
	require.NoError(t, err)

	// Then for given file we have owner matches (translated to references here):
	ownerReferences := map[string]Reference{
		// Some possible matching entries:
		// email entry in CODEOWNERS
		"email entry in CODEOWNERS": {
			Email: verifiedEmail,
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
		"email entry in CODEOWNERS for second verified email": {
			Email: "john-the-BIG-dough@example.com",
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
		"email entry in CODEOWNERS for third verified email": {
			Email: "john-aka-im-rich@didyouget.it",
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
		"jdoe entry in CODEOWNERS": {
			Handle: username,
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
		"@jdoe entry in CODEOWNERS": {
			Handle: "@jdoe",
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
		"@jdoegh (github handle) entry in CODEOWNERS": {
			Handle: gitHubLogin,
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
		"@jdoegl (gitlab handle) entry in CODEOWNERS": {
			Handle: gitLabLogin,
			RepoContext: &RepoContext{
				Name:         "gitlab.com/sourcegraph/sourcegraph",
				CodeHostKind: "gitlab",
			},
		},
		"user ID from assigned ownership": {
			UserID: user.ID,
		},
	}

	// Imagine these are searches with filters `file:has.owner(jdoe)` and
	// `file:has.owner(john-aka-im-rich@didyouget.it)` respectively.
	tests := map[string]struct{ searchTerm string }{
		"Search by handle":         {searchTerm: username},
		"Search by verified email": {searchTerm: "john-aka-im-rich@didyouget.it"},
	}
	for testName, testCase := range tests {
		t.Run(testName, func(t *testing.T) {
			// Do this at first during search and hold references to all the known entities
			// that can be referred to by given search term.
			bag, err := ByTextReference(ctx, db, testCase.searchTerm)
			require.NoError(t, err)
			for name, r := range ownerReferences {
				t.Run(name, func(t *testing.T) {
					assert.True(t, bag.Contains(r), fmt.Sprintf("%s.Contains(%s), want true, got false", bag, r))
				})
			}
		})
	}
}

func TestBagNoUser(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := edb.NewEnterpriseDB(database.NewDB(logger, dbtest.NewDB(logger, t)))
	ctx := context.Background()
	bag, err := ByTextReference(ctx, db, "userdoesnotexist")
	require.NoError(t, err)
	for name, r := range map[string]Reference{
		"same handle matches even when there is no user": {
			Handle: "userdoesnotexist",
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
		"same handle with @ matches even when there is no user": {
			Handle: "@userdoesnotexist",
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
	} {
		t.Run(name, func(t *testing.T) {
			assert.True(t, bag.Contains(r), fmt.Sprintf("bag.Contains(%s), want true, got false", r))
		})
	}
	for name, r := range map[string]Reference{
		"email entry in CODEOWNERS": {
			Email: verifiedEmail,
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
		"different handle entry in CODEOWNERS": {
			Handle: "anotherhandle",
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
		"user ID from assigned ownership": {
			UserID: 42,
		},
	} {
		t.Run(name, func(t *testing.T) {
			assert.False(t, bag.Contains(r), fmt.Sprintf("bag.Contains(%s), want false, got true", r))
		})
	}
}

func TestBagUserFoundNoMatches(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := edb.NewEnterpriseDB(database.NewDB(logger, dbtest.NewDB(logger, t)))
	ctx := context.Background()
	user, err := initUser(ctx, t, db)
	require.NoError(t, err)
	// Make user email verified.
	err = db.UserEmails().SetVerified(ctx, user.ID, verifiedEmail, true)
	require.NoError(t, err)
	// Now we add 1 unverified email.
	verificationCode := "ok"
	require.NoError(t, db.UserEmails().Add(ctx, user.ID, unverifiedEmail, &verificationCode))

	// Then for given file we have owner matches (translated to references here):
	ownerReferences := map[string]Reference{
		"email entry in CODEOWNERS": {
			Email: "jdoe@example.com",
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
		"email entry in CODEOWNERS, but the email is unverified": {
			Email: unverifiedEmail,
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
		"different handle entry in CODEOWNERS": {
			Handle: "anotherhandle",
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
		"different code host handle entry in CODEOWNERS": {
			Handle: gerritLogin,
			RepoContext: &RepoContext{
				Name:         "gerrithub.io/sourcegraph/sourcegraph",
				CodeHostKind: "gerrit",
			},
		},
		"user ID from assigned ownership": {
			UserID: user.ID + 1, // different user ID
		},
	}

	// Imagine these are searches with filters `file:has.owner(jdoe)` and
	// `file:has.owner(john-aka-im-rich@didyouget.it)` respectively.
	tests := map[string]struct {
		searchTerm    string
		validationRef Reference
	}{
		"Search by handle":         {searchTerm: username, validationRef: Reference{Handle: username}},
		"Search by verified email": {searchTerm: verifiedEmail, validationRef: Reference{Email: verifiedEmail}},
	}
	for testName, testCase := range tests {
		t.Run(testName, func(t *testing.T) {
			bag, err := ByTextReference(ctx, db, testCase.searchTerm)
			require.NoError(t, err)
			// Check test is valid by verifying user can be found by handle/email.
			require.True(t, bag.Contains(testCase.validationRef), fmt.Sprintf("validation: Contains(%s), want true, got false", testCase.validationRef))
			for name, r := range ownerReferences {
				t.Run(name, func(t *testing.T) {
					assert.False(t, bag.Contains(r), fmt.Sprintf("bag.Contains(%s), want false, got true", r))
				})
			}
		})
	}
}

func TestBagUnverifiedEmailOnlyMatchesWithItself(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := edb.NewEnterpriseDB(database.NewDB(logger, dbtest.NewDB(logger, t)))
	ctx := context.Background()
	user, err := initUser(ctx, t, db)
	require.NoError(t, err)
	// Now we add 1 unverified email.
	verificationCode := "ok"
	require.NoError(t, db.UserEmails().Add(ctx, user.ID, unverifiedEmail, &verificationCode))

	// Then for given file we have owner matches (translated to references here):
	ownerReferences := map[string]Reference{
		"email entry in CODEOWNERS, the email is unverified but matches with search term": {
			Email: unverifiedEmail,
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
		"email entry in CODEOWNERS, although the email is verified, but the search term is an unverified email": {
			Email: verifiedEmail,
			RepoContext: &RepoContext{
				Name:         "github.com/sourcegraph/sourcegraph",
				CodeHostKind: "github",
			},
		},
	}

	// Imagine this is the search with filter
	// `file:has.owner(john-the-unverified@example.com)`.
	bag, err := ByTextReference(ctx, db, unverifiedEmail)
	require.NoError(t, err)
	for name, r := range ownerReferences {
		t.Run(name, func(t *testing.T) {
			if r.Email == unverifiedEmail {
				assert.True(t, bag.Contains(r), fmt.Sprintf("bag.Contains(%s), want true, got false", r))
			} else {
				assert.False(t, bag.Contains(r), fmt.Sprintf("bag.Contains(%s), want false, got true", r))
			}
		})
	}
}

func TestBagRetrievesTeamsByName(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := edb.NewEnterpriseDB(database.NewDB(logger, dbtest.NewDB(logger, t)))
	ctx := context.Background()
	err := db.Teams().CreateTeam(ctx, &types.Team{Name: "team-name"})
	require.NoError(t, err)
	team, err := db.Teams().GetTeamByName(ctx, "team-name")
	require.NoError(t, err)
	bag, err := ByTextReference(ctx, db, "team-name")
	require.NoError(t, err)
	ref := Reference{TeamID: team.ID}
	assert.True(t, bag.Contains(ref), "%s contains %s", bag, ref)
}

func TestBagManyUsers(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := edb.NewEnterpriseDB(database.NewDB(logger, dbtest.NewDB(logger, t)))
	ctx := context.Background()
	_, err := db.Users().Create(ctx, database.NewUser{
		Email:           "john.doe@example.com",
		Username:        "jdoe",
		EmailIsVerified: true,
	})
	require.NoError(t, err)
	_, err = db.Users().Create(ctx, database.NewUser{
		Email:           "suzy.smith@example.com",
		Username:        "ssmith",
		EmailIsVerified: true,
	})
	require.NoError(t, err)
	bag, err := ByTextReference(ctx, db, "jdoe", "ssmith")
	require.NoError(t, err)
	assert.True(t, bag.Contains(Reference{Handle: "ssmith"}))
	assert.True(t, bag.Contains(Reference{Handle: "jdoe"}))
}

func initUser(ctx context.Context, t *testing.T, db edb.EnterpriseDB) (*types.User, error) {
	t.Helper()
	user, err := db.Users().Create(ctx, database.NewUser{
		Email:           verifiedEmail,
		Username:        username,
		EmailIsVerified: true,
	})
	require.NoError(t, err)
	// Adding user external accounts.
	// 1) GitHub.
	spec := extsvc.AccountSpec{
		ServiceType: extsvc.TypeGitHub,
		ServiceID:   "https://github.com/",
		AccountID:   "1337",
	}
	data := json.RawMessage(fmt.Sprintf(`{"login": "%s"}`, gitHubLogin))
	accountData := extsvc.AccountData{
		Data: extsvc.NewUnencryptedData(data),
	}
	require.NoError(t, db.UserExternalAccounts().Insert(ctx, user.ID, spec, accountData))
	mockGitHubProvider := providers.MockAuthProvider{
		MockConfigID:          providers.ConfigID{Type: extsvc.TypeGitHub},
		MockPublicAccountData: &extsvc.PublicAccountData{Login: stringPointer(gitHubLogin)},
	}
	// 2) GitLab.
	gitLabSpec := extsvc.AccountSpec{
		ServiceType: extsvc.TypeGitLab,
		ServiceID:   "https://gitlab.com/",
		AccountID:   "42",
	}
	gitLabData := json.RawMessage(fmt.Sprintf(`{"username": "%s"}`, gitLabLogin))
	gitLabAccountData := extsvc.AccountData{
		Data: extsvc.NewUnencryptedData(gitLabData),
	}
	require.NoError(t, db.UserExternalAccounts().Insert(ctx, user.ID, gitLabSpec, gitLabAccountData))
	gitLabMockGitHubProvider := providers.MockAuthProvider{
		MockConfigID:          providers.ConfigID{Type: extsvc.TypeGitLab},
		MockPublicAccountData: &extsvc.PublicAccountData{Login: stringPointer(gitLabLogin)},
	}
	// Adding providers to the mock.
	providers.MockProviders = []providers.Provider{mockGitHubProvider, gitLabMockGitHubProvider}
	return user, err
}

func stringPointer(s string) *string {
	return &s
}
