package database

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/google/go-cmp/cmp/cmpopts"
	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtesting"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestExternalServicesListOptions_sqlConditions(t *testing.T) {
	tests := []struct {
		name             string
		noNamespace      bool
		namespaceUserID  int32
		kinds            []string
		afterID          int64
		wantQuery        string
		onlyCloudDefault bool
		wantArgs         []interface{}
	}{
		{
			name:      "no condition",
			wantQuery: "deleted_at IS NULL",
		},
		{
			name:      "only one kind: GitHub",
			kinds:     []string{extsvc.KindGitHub},
			wantQuery: "deleted_at IS NULL AND kind IN ($1)",
			wantArgs:  []interface{}{extsvc.KindGitHub},
		},
		{
			name:      "two kinds: GitHub and GitLab",
			kinds:     []string{extsvc.KindGitHub, extsvc.KindGitLab},
			wantQuery: "deleted_at IS NULL AND kind IN ($1 , $2)",
			wantArgs:  []interface{}{extsvc.KindGitHub, extsvc.KindGitLab},
		},
		{
			name:            "has namespace user ID",
			namespaceUserID: 1,
			wantQuery:       "deleted_at IS NULL AND namespace_user_id = $1",
			wantArgs:        []interface{}{int32(1)},
		},
		{
			name:            "want no namespace",
			noNamespace:     true,
			namespaceUserID: 1,
			wantQuery:       "deleted_at IS NULL AND namespace_user_id IS NULL",
		},
		{
			name:      "has after ID",
			afterID:   10,
			wantQuery: "deleted_at IS NULL AND id < $1",
			wantArgs:  []interface{}{int64(10)},
		},
		{
			name:             "has OnlyCloudDefault",
			onlyCloudDefault: true,
			wantQuery:        "deleted_at IS NULL AND cloud_default = true",
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			opts := ExternalServicesListOptions{
				NoNamespace:      test.noNamespace,
				NamespaceUserID:  test.namespaceUserID,
				Kinds:            test.kinds,
				AfterID:          test.afterID,
				OnlyCloudDefault: test.onlyCloudDefault,
			}
			q := sqlf.Join(opts.sqlConditions(), "AND")
			if diff := cmp.Diff(test.wantQuery, q.Query(sqlf.PostgresBindVar)); diff != "" {
				t.Fatalf("query mismatch (-want +got):\n%s", diff)
			} else if diff = cmp.Diff(test.wantArgs, q.Args()); diff != "" {
				t.Fatalf("args mismatch (-want +got):\n%s", diff)
			}
		})
	}
}

func TestExternalServicesStore_ValidateConfig(t *testing.T) {
	db := dbtesting.GetDB(t)

	tests := []struct {
		name            string
		kind            string
		config          string
		namespaceUserID int32
		setup           func(t *testing.T)
		wantErr         string
	}{
		{
			name:    "0 errors - GitHub.com",
			kind:    extsvc.KindGitHub,
			config:  `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
			wantErr: "<nil>",
		},
		{
			name:    "0 errors - GitLab.com",
			kind:    extsvc.KindGitLab,
			config:  `{"url": "https://github.com", "projectQuery": ["none"], "token": "abc"}`,
			wantErr: "<nil>",
		},
		{
			name:    "0 errors - Bitbucket.org",
			kind:    extsvc.KindBitbucketCloud,
			config:  `{"url": "https://bitbucket.org", "username": "ceo", "appPassword": "abc"}`,
			wantErr: "<nil>",
		},
		{
			name:    "1 error",
			kind:    extsvc.KindGitHub,
			config:  `{"url": "https://github.com", "repositoryQuery": ["none"], "token": ""}`,
			wantErr: "1 error occurred:\n\t* token: String length must be greater than or equal to 1\n\n",
		},
		{
			name:    "2 errors",
			kind:    extsvc.KindGitHub,
			config:  `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "", "x": 123}`,
			wantErr: "2 errors occurred:\n\t* Additional property x is not allowed\n\t* token: String length must be greater than or equal to 1\n\n",
		},
		{
			name:   "no conflicting rate limit",
			kind:   extsvc.KindGitHub,
			config: `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc", "rateLimit": {"enabled": true, "requestsPerHour": 5000}}`,
			setup: func(t *testing.T) {
				t.Cleanup(func() {
					Mocks.ExternalServices.List = nil
				})
				Mocks.ExternalServices.List = func(opt ExternalServicesListOptions) ([]*types.ExternalService, error) {
					return nil, nil
				}
			},
			wantErr: "<nil>",
		},
		{
			name:   "conflicting rate limit",
			kind:   extsvc.KindGitHub,
			config: `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc", "rateLimit": {"enabled": true, "requestsPerHour": 5000}}`,
			setup: func(t *testing.T) {
				t.Cleanup(func() {
					Mocks.ExternalServices.List = nil
				})
				Mocks.ExternalServices.List = func(opt ExternalServicesListOptions) ([]*types.ExternalService, error) {
					return []*types.ExternalService{
						{
							ID:          1,
							Kind:        extsvc.KindGitHub,
							DisplayName: "GITHUB 1",
							Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc", "rateLimit": {"enabled": true, "requestsPerHour": 5000}}`,
						},
					}, nil
				}
			},
			wantErr: "1 error occurred:\n\t* existing external service, \"GITHUB 1\", already has a rate limit set\n\n",
		},
		{
			name:            "prevent code hosts that are not allowed",
			kind:            extsvc.KindGitHub,
			config:          `{"url": "https://github.example.com", "repositoryQuery": ["none"], "token": "abc"}`,
			namespaceUserID: 1,
			wantErr:         `users are only allowed to add external service for https://github.com/ and https://gitlab.com/`,
		},
		{
			name:            "gjson handles comments",
			kind:            extsvc.KindGitHub,
			config:          `{"url": "https://github.com", "token": "abc", "repositoryQuery": ["affiliated"]} // comment`,
			namespaceUserID: 1,
			wantErr:         "<nil>",
		},
		{
			name:            "prevent disallowed repositoryPathPattern field",
			kind:            extsvc.KindGitHub,
			config:          `{"url": "https://github.com", "repositoryPathPattern": "github/{nameWithOwner}"}`,
			namespaceUserID: 1,
			wantErr:         `field "repositoryPathPattern" is not allowed in a user-added external service`,
		},
		{
			name:            "prevent disallowed nameTransformations field",
			kind:            extsvc.KindGitHub,
			config:          `{"url": "https://github.com", "nameTransformations": [{"regex": "\\.d/","replacement": "/"},{"regex": "-git$","replacement": ""}]}`,
			namespaceUserID: 1,
			wantErr:         `field "nameTransformations" is not allowed in a user-added external service`,
		},
		{
			name:            "prevent disallowed rateLimit field",
			kind:            extsvc.KindGitHub,
			config:          `{"url": "https://github.com", "rateLimit": {}}`,
			namespaceUserID: 1,
			wantErr:         `field "rateLimit" is not allowed in a user-added external service`,
		},
		{
			name:            "duplicate kinds not allowed for user owned services",
			kind:            extsvc.KindGitHub,
			config:          `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
			namespaceUserID: 1,
			setup: func(t *testing.T) {
				t.Cleanup(func() {
					Mocks.ExternalServices.List = nil
				})
				Mocks.ExternalServices.List = func(opt ExternalServicesListOptions) ([]*types.ExternalService, error) {
					return []*types.ExternalService{
						{
							ID:          1,
							Kind:        extsvc.KindGitHub,
							DisplayName: "GITHUB 1",
							Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
						},
					}, nil
				}
			},
			wantErr: `existing external service, "GITHUB 1", of same kind already added`,
		},
		{
			name:    "1 errors - GitHub.com",
			kind:    extsvc.KindGitHub,
			config:  `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "` + types.RedactedSecret + `"}`,
			wantErr: "found unexpected Redacted string: \"" + types.RedactedSecret + "\", has ExternalService.UnredactConfig been called before attempting to write?",
		},
		{
			name:    "1 errors - GitLab.com",
			kind:    extsvc.KindGitLab,
			config:  `{"url": "https://github.com", "projectQuery": ["none"], "token": "` + types.RedactedSecret + `"}`,
			wantErr: "found unexpected Redacted string: \"" + types.RedactedSecret + "\", has ExternalService.UnredactConfig been called before attempting to write?",
		},
		{
			name:    "0 errors - potentially redacted string",
			kind:    extsvc.KindGitHub,
			config:  `{"url": "https://github.com", "repositoryQuery": ["github.com/srcgraph_redacted"], "token": "abc"}`,
			wantErr: "<nil>",
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if test.setup != nil {
				test.setup(t)
			}

			_, err := ExternalServices(db).ValidateConfig(context.Background(), ValidateExternalServiceConfigOptions{
				Kind:            test.kind,
				Config:          test.config,
				NamespaceUserID: test.namespaceUserID,
			})
			gotErr := fmt.Sprintf("%v", err)
			if gotErr != test.wantErr {
				t.Errorf("error: want %q but got %q", test.wantErr, gotErr)
			}
		})
	}
}

func TestExternalServicesStore_Create(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	ctx := context.Background()

	envvar.MockSourcegraphDotComMode(true)
	defer envvar.MockSourcegraphDotComMode(false)

	user, err := Users(db).Create(ctx,
		NewUser{
			Email:           "alice@example.com",
			Username:        "alice",
			Password:        "password",
			EmailIsVerified: true,
		},
	)
	if err != nil {
		t.Fatal(err)
	}

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}

	tests := []struct {
		name             string
		externalService  *types.ExternalService
		wantUnrestricted bool
	}{
		{
			name: "without authorization",
			externalService: &types.ExternalService{
				Kind:        extsvc.KindGitHub,
				DisplayName: "GITHUB #1",
				Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
			},
			wantUnrestricted: true,
		},
		{
			name: "with authorization",
			externalService: &types.ExternalService{
				Kind:        extsvc.KindGitHub,
				DisplayName: "GITHUB #2",
				Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc", "authorization": {}}`,
			},
			wantUnrestricted: false,
		},
		{
			name: "with authorization in comments",
			externalService: &types.ExternalService{
				Kind:        extsvc.KindGitHub,
				DisplayName: "GITHUB #3",
				Config: `
{
	"url": "https://github.com",
	"repositoryQuery": ["none"],
	"token": "abc",
	// "authorization": {}
}`,
			},
			wantUnrestricted: true,
		},

		{
			name: "Cloud: auto-add authorization to user code host connections for GitHub",
			externalService: &types.ExternalService{
				Kind:            extsvc.KindGitHub,
				DisplayName:     "GITHUB #4",
				Config:          `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
				NamespaceUserID: user.ID,
			},
			wantUnrestricted: false,
		},
		{
			name: "Cloud: auto-add authorization to user code host connections for GitLab",
			externalService: &types.ExternalService{
				Kind:            extsvc.KindGitLab,
				DisplayName:     "GITLAB #1",
				Config:          `{"url": "https://gitlab.com", "projectQuery": ["none"], "token": "abc"}`,
				NamespaceUserID: user.ID,
			},
			wantUnrestricted: false,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			err := ExternalServices(db).Create(ctx, confGet, test.externalService)
			if err != nil {
				t.Fatal(err)
			}

			// Should get back the same one
			got, err := ExternalServices(db).GetByID(ctx, test.externalService.ID)
			if err != nil {
				t.Fatal(err)
			}

			if diff := cmp.Diff(test.externalService, got); diff != "" {
				t.Fatalf("Mismatch (-want +got):\n%s", diff)
			}

			if test.wantUnrestricted != got.Unrestricted {
				t.Fatalf("Want unrestricted = %v, but got %v", test.wantUnrestricted, got.Unrestricted)
			}
		})
	}
}

func TestExternalServicesStore_CreateWithTierEnforcement(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)

	ctx := context.Background()
	confGet := func() *conf.Unified { return &conf.Unified{} }
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
	}
	store := ExternalServices(db)
	store.PreCreateExternalService = func(ctx context.Context) error {
		return errcode.NewPresentationError("test plan limit exceeded")
	}
	if err := store.Create(ctx, confGet, es); err == nil {
		t.Fatal("expected an error, got none")
	}
}

func TestExternalServicesStore_Update(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
	}
	err := ExternalServices(db).Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	// NOTE: The order of tests matters
	tests := []struct {
		name             string
		update           *ExternalServiceUpdate
		wantUnrestricted bool
	}{
		{
			name: "update with authorization",
			update: &ExternalServiceUpdate{
				DisplayName: strptr("GITHUB (updated) #1"),
				Config:      strptr(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def", "authorization": {}}`),
			},
			wantUnrestricted: false,
		},
		{
			name: "update without authorization",
			update: &ExternalServiceUpdate{
				DisplayName: strptr("GITHUB (updated) #2"),
				Config:      strptr(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def"}`),
			},
			wantUnrestricted: true,
		},
		{
			name: "update with authorization in comments",
			update: &ExternalServiceUpdate{
				DisplayName: strptr("GITHUB (updated) #3"),
				Config: strptr(`
{
	"url": "https://github.com",
	"repositoryQuery": ["none"],
	"token": "def",
	// "authorization": {}
}`),
			},
			wantUnrestricted: true,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			err = ExternalServices(db).Update(ctx, nil, es.ID, test.update)
			if err != nil {
				t.Fatal(err)
			}

			// Get and verify update
			got, err := ExternalServices(db).GetByID(ctx, es.ID)
			if err != nil {
				t.Fatal(err)
			}

			if diff := cmp.Diff(*test.update.DisplayName, got.DisplayName); diff != "" {
				t.Fatalf("DisplayName mismatch (-want +got):\n%s", diff)
			} else if diff = cmp.Diff(*test.update.Config, got.Config); diff != "" {
				t.Fatalf("Config mismatch (-want +got):\n%s", diff)
			} else if got.UpdatedAt.Equal(es.UpdatedAt) {
				t.Fatalf("UpdateAt: want to be updated but not")
			}

			if test.wantUnrestricted != got.Unrestricted {
				t.Fatalf("Want unrestricted = %v, but got %v", test.wantUnrestricted, got.Unrestricted)
			}
		})
	}
}

func TestCountRepoCount(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	ctx := actor.WithInternalActor(context.Background())

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es1 := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
	}
	err := ExternalServices(db).Create(ctx, confGet, es1)
	if err != nil {
		t.Fatal(err)
	}

	_, err = db.ExecContext(ctx, `
INSERT INTO repo (id, name, description, fork)
VALUES (1, 'github.com/user/repo', '', FALSE);
`)
	if err != nil {
		t.Fatal(err)
	}

	// Insert rows to `external_service_repos` table to test the trigger.
	q := sqlf.Sprintf(`
INSERT INTO external_service_repos (external_service_id, repo_id, clone_url)
VALUES (%d, 1, '')
`, es1.ID)
	_, err = db.ExecContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
	if err != nil {
		t.Fatal(err)
	}

	count, err := ExternalServices(db).RepoCount(ctx, es1.ID)
	if err != nil {
		t.Fatal(err)
	}

	if count != 1 {
		t.Fatalf("Expected 1, got %d", count)
	}
}

func TestExternalServicesStore_Delete(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	ctx := actor.WithInternalActor(context.Background())

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es1 := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
	}
	err := ExternalServices(db).Create(ctx, confGet, es1)
	if err != nil {
		t.Fatal(err)
	}

	es2 := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #2",
		Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def"}`,
	}
	err = ExternalServices(db).Create(ctx, confGet, es2)
	if err != nil {
		t.Fatal(err)
	}

	// Create two repositories to test trigger of soft-deleting external service:
	//  - ID=1 is expected to be deleted along with deletion of the external service.
	//  - ID=2 remains untouched because it is not associated with the external service.
	_, err = db.ExecContext(ctx, `
INSERT INTO repo (id, name, description, fork)
VALUES (1, 'github.com/user/repo', '', FALSE);
INSERT INTO repo (id, name, description, fork)
VALUES (2, 'github.com/user/repo2', '', FALSE);
`)
	if err != nil {
		t.Fatal(err)
	}

	// Insert rows to `external_service_repos` table to test the trigger.
	q := sqlf.Sprintf(`
INSERT INTO external_service_repos (external_service_id, repo_id, clone_url)
VALUES (%d, 1, ''), (%d, 2, '')
`, es1.ID, es2.ID)
	_, err = db.ExecContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
	if err != nil {
		t.Fatal(err)
	}

	// Delete this external service
	err = ExternalServices(db).Delete(ctx, es1.ID)
	if err != nil {
		t.Fatal(err)
	}

	// Delete again should get externalServiceNotFoundError
	err = ExternalServices(db).Delete(ctx, es1.ID)
	gotErr := fmt.Sprintf("%v", err)
	wantErr := fmt.Sprintf("external service not found: %v", es1.ID)
	if gotErr != wantErr {
		t.Errorf("error: want %q but got %q", wantErr, gotErr)
	}

	// Should only get back the repo with ID=2
	repos, err := Repos(db).GetByIDs(ctx, 1, 2)
	if err != nil {
		t.Fatal(err)
	}

	want := []*types.Repo{
		{ID: 2, Name: "github.com/user/repo2"},
	}

	repos = types.Repos(repos).With(func(r *types.Repo) {
		r.CreatedAt = time.Time{}
		r.Sources = nil
	})
	if diff := cmp.Diff(want, repos); diff != "" {
		t.Fatalf("Repos mismatch (-want +got):\n%s", diff)
	}
}

func TestExternalServicesStore_GetByID(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
	}
	err := ExternalServices(db).Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	// Should be able to get back by its ID
	_, err = ExternalServices(db).GetByID(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	// Delete this external service
	err = ExternalServices(db).Delete(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	// Should now get externalServiceNotFoundError
	_, err = ExternalServices(db).GetByID(ctx, es.ID)
	gotErr := fmt.Sprintf("%v", err)
	wantErr := fmt.Sprintf("external service not found: %v", es.ID)
	if gotErr != wantErr {
		t.Errorf("error: want %q but got %q", wantErr, gotErr)
	}
}

func TestGetLastSyncError(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
	}
	err := ExternalServices(db).Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	// Should be able to get back by its ID
	_, err = ExternalServices(db).GetByID(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	lastSyncError, err := ExternalServices(db).GetLastSyncError(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}
	if lastSyncError != "" {
		t.Fatalf("Expected empty error, have %q", lastSyncError)
	}

	// Could have failure message
	_, err = db.Exec(`
INSERT INTO external_service_sync_jobs (external_service_id, state, finished_at)
VALUES ($1,'errored', now())
`, es.ID)

	if err != nil {
		t.Fatal(err)
	}

	lastSyncError, err = ExternalServices(db).GetLastSyncError(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}
	if lastSyncError != "" {
		t.Fatalf("Expected empty error, have %q", lastSyncError)
	}

	// Add sync error
	expectedError := "oops"
	_, err = db.Exec(`
INSERT INTO external_service_sync_jobs (external_service_id, failure_message, state, finished_at)
VALUES ($1,$2,'errored', now())
`, es.ID, expectedError)

	if err != nil {
		t.Fatal(err)
	}

	lastSyncError, err = ExternalServices(db).GetLastSyncError(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}
	if lastSyncError != expectedError {
		t.Fatalf("Expected %q, have %q", expectedError, lastSyncError)
	}
}

func TestExternalServicesStore_List(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	ctx := context.Background()

	// Create test user
	user, err := Users(db).Create(ctx, NewUser{
		Email:           "alice@example.com",
		Username:        "alice",
		Password:        "password",
		EmailIsVerified: true,
	})
	if err != nil {
		t.Fatal(err)
	}

	// Create new external services
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	ess := []*types.ExternalService{
		{
			Kind:            extsvc.KindGitHub,
			DisplayName:     "GITHUB #1",
			Config:          `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc", "authorization": {}}`,
			NamespaceUserID: user.ID,
		},
		{
			Kind:        extsvc.KindGitHub,
			DisplayName: "GITHUB #2",
			Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def"}`,
		},
	}
	for _, es := range ess {
		err := ExternalServices(db).Create(ctx, confGet, es)
		if err != nil {
			t.Fatal(err)
		}
	}

	t.Run("list all external services", func(t *testing.T) {
		got, err := ExternalServices(db).List(ctx, ExternalServicesListOptions{})
		if err != nil {
			t.Fatal(err)
		}
		sort.Slice(got, func(i, j int) bool { return got[i].ID < got[j].ID })

		if diff := cmp.Diff(ess, got); diff != "" {
			t.Fatalf("Mismatch (-want +got):\n%s", diff)
		}
	})

	t.Run("list all external services in ascending order", func(t *testing.T) {
		got, err := ExternalServices(db).List(ctx, ExternalServicesListOptions{OrderByDirection: "ASC"})
		if err != nil {
			t.Fatal(err)
		}
		want := []*types.ExternalService(types.ExternalServices(ess).Clone())
		sort.Slice(want, func(i, j int) bool { return want[i].ID < want[j].ID })

		if diff := cmp.Diff(want, got); diff != "" {
			t.Fatalf("Mismatch (-want +got):\n%s", diff)
		}
	})

	t.Run("list all external services in descending order", func(t *testing.T) {
		got, err := ExternalServices(db).List(ctx, ExternalServicesListOptions{})
		if err != nil {
			t.Fatal(err)
		}
		want := []*types.ExternalService(types.ExternalServices(ess).Clone())
		sort.Slice(want, func(i, j int) bool { return want[i].ID > want[j].ID })

		if diff := cmp.Diff(want, got); diff != "" {
			t.Fatalf("Mismatch (-want +got):\n%s", diff)
		}
	})

	t.Run("list external services with certain IDs", func(t *testing.T) {
		got, err := ExternalServices(db).List(ctx, ExternalServicesListOptions{
			IDs: []int64{ess[1].ID},
		})
		if err != nil {
			t.Fatal(err)
		}
		sort.Slice(got, func(i, j int) bool { return got[i].ID < got[j].ID })

		if diff := cmp.Diff(ess[1:], got); diff != "" {
			t.Fatalf("Mismatch (-want +got):\n%s", diff)
		}
	})

	t.Run("list external services with no namespace", func(t *testing.T) {
		got, err := ExternalServices(db).List(ctx, ExternalServicesListOptions{
			NoNamespace: true,
		})
		if err != nil {
			t.Fatal(err)
		}

		if len(got) != 1 {
			t.Fatalf("Want 1 external service but got %d", len(ess))
		} else if diff := cmp.Diff(ess[1], got[0]); diff != "" {
			t.Fatalf("Mismatch (-want +got):\n%s", diff)
		}
	})

	t.Run("list only test user's external services", func(t *testing.T) {
		got, err := ExternalServices(db).List(ctx, ExternalServicesListOptions{
			NamespaceUserID: user.ID,
		})
		if err != nil {
			t.Fatal(err)
		}

		if len(got) != 1 {
			t.Fatalf("Want 1 external service but got %d", len(ess))
		} else if diff := cmp.Diff(ess[0], got[0]); diff != "" {
			t.Fatalf("Mismatch (-want +got):\n%s", diff)
		}
	})

	t.Run("list non-exist user's external services", func(t *testing.T) {
		ess, err := ExternalServices(db).List(ctx, ExternalServicesListOptions{
			NamespaceUserID: 404,
		})
		if err != nil {
			t.Fatal(err)
		}

		if len(ess) != 0 {
			t.Fatalf("Want 0 external service but got %d", len(ess))
		}
	})
}

func TestExternalServicesStore_DistinctKinds(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	ctx := context.Background()

	t.Run("no external service won't blow up", func(t *testing.T) {
		kinds, err := ExternalServices(db).DistinctKinds(ctx)
		if err != nil {
			t.Fatal(err)
		}
		if len(kinds) != 0 {
			t.Fatalf("Kinds: want 0 but got %d", len(kinds))
		}
	})

	// Create new external services in different kinds
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	ess := []*types.ExternalService{
		{
			Kind:        extsvc.KindGitHub,
			DisplayName: "GITHUB #1",
			Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
		},
		{
			Kind:        extsvc.KindGitHub,
			DisplayName: "GITHUB #2",
			Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def"}`,
		},
		{
			Kind:        extsvc.KindGitLab,
			DisplayName: "GITLAB #1",
			Config:      `{"url": "https://github.com", "projectQuery": ["none"], "token": "abc"}`,
		},
		{
			Kind:        extsvc.KindOther,
			DisplayName: "OTHER #1",
			Config:      `{"repos": []}`,
		},
	}
	for _, es := range ess {
		err := ExternalServices(db).Create(ctx, confGet, es)
		if err != nil {
			t.Fatal(err)
		}
	}

	// Delete the last external service which should be excluded from the result
	err := ExternalServices(db).Delete(ctx, ess[3].ID)
	if err != nil {
		t.Fatal(err)
	}

	kinds, err := ExternalServices(db).DistinctKinds(ctx)
	if err != nil {
		t.Fatal(err)
	}
	sort.Strings(kinds)
	wantKinds := []string{extsvc.KindGitHub, extsvc.KindGitLab}
	if diff := cmp.Diff(wantKinds, kinds); diff != "" {
		t.Fatalf("Kinds mismatch (-want +got):\n%s", diff)
	}
}

func TestExternalServicesStore_Count(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
	}
	err := ExternalServices(db).Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	count, err := ExternalServices(db).Count(ctx, ExternalServicesListOptions{})
	if err != nil {
		t.Fatal(err)
	}

	if count != 1 {
		t.Fatalf("Want 1 external service but got %d", count)
	}
}

func TestExternalServicesStore_Upsert(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	ctx := context.Background()

	clock := timeutil.NewFakeClock(time.Now(), 0)

	svcs := types.MakeExternalServices()

	t.Run("no external services", func(t *testing.T) {
		if err := ExternalServices(db).Upsert(ctx); err != nil {
			t.Fatalf("Upsert error: %s", err)
		}
	})

	t.Run("many external services", func(t *testing.T) {
		tx, err := ExternalServices(db).Transact(ctx)
		if err != nil {
			t.Fatalf("Transact error: %s", err)
		}
		defer func() {
			err = tx.Done(err)
			if err != nil {
				t.Fatalf("Done error: %s", err)
			}
		}()

		want := types.GenerateExternalServices(7, svcs...)

		if err := tx.Upsert(ctx, want...); err != nil {
			t.Fatalf("Upsert error: %s", err)
		}

		for _, e := range want {
			if e.Kind != strings.ToUpper(e.Kind) {
				t.Errorf("external service kind didn't get upper-cased: %q", e.Kind)
				break
			}
		}

		sort.Sort(want)

		have, err := tx.List(ctx, ExternalServicesListOptions{
			Kinds: svcs.Kinds(),
		})
		if err != nil {
			t.Fatalf("List error: %s", err)
		}

		sort.Sort(types.ExternalServices(have))

		if diff := cmp.Diff(have, []*types.ExternalService(want), cmpopts.EquateEmpty()); diff != "" {
			t.Fatalf("List:\n%s", diff)
		}

		now := clock.Now()
		suffix := "-updated"
		for _, r := range want {
			r.DisplayName += suffix
			r.Kind += suffix
			r.Config += suffix
			r.UpdatedAt = now
			r.CreatedAt = now
		}

		if err = tx.Upsert(ctx, want...); err != nil {
			t.Errorf("Upsert error: %s", err)
		}
		have, err = tx.List(ctx, ExternalServicesListOptions{})
		if err != nil {
			t.Fatalf("List error: %s", err)
		}

		sort.Sort(types.ExternalServices(have))

		if diff := cmp.Diff(have, []*types.ExternalService(want), cmpopts.EquateEmpty()); diff != "" {
			t.Errorf("List:\n%s", diff)
		}

		want.Apply(func(e *types.ExternalService) {
			e.UpdatedAt = now
			e.DeletedAt = now
		})

		if err = tx.Upsert(ctx, want.Clone()...); err != nil {
			t.Errorf("Upsert error: %s", err)
		}
		have, err = tx.List(ctx, ExternalServicesListOptions{})
		if err != nil {
			t.Errorf("List error: %s", err)
		}

		sort.Sort(types.ExternalServices(have))

		if diff := cmp.Diff(have, []*types.ExternalService(nil), cmpopts.EquateEmpty()); diff != "" {
			t.Errorf("List:\n%s", diff)
		}
	})
}

func TestExternalServicesStore_OneCloudDefaultPerKind(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	db := dbtesting.GetDB(t)
	ctx := context.Background()

	now := time.Now()

	makeService := func(cloudDefault bool) *types.ExternalService {
		cfg := `{"url": "https://github.com", "token": "abc", "repositoryQuery": ["none"]}`
		svc := &types.ExternalService{
			Kind:         extsvc.KindGitHub,
			DisplayName:  "Github - Test",
			Config:       cfg,
			CreatedAt:    now,
			UpdatedAt:    now,
			CloudDefault: cloudDefault,
		}
		return svc
	}

	t.Run("non default", func(t *testing.T) {
		gh := makeService(false)
		if err := ExternalServices(db).Upsert(ctx, gh); err != nil {
			t.Fatalf("Upsert error: %s", err)
		}
	})

	t.Run("first default", func(t *testing.T) {
		gh := makeService(true)
		if err := ExternalServices(db).Upsert(ctx, gh); err != nil {
			t.Fatalf("Upsert error: %s", err)
		}
	})

	t.Run("second default", func(t *testing.T) {
		gh := makeService(true)
		if err := ExternalServices(db).Upsert(ctx, gh); err == nil {
			t.Fatal("Expected an error")
		}
	})
}
