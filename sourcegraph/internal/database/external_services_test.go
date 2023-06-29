package database

import (
	"context"
	"encoding/json"
	"fmt"
	"sort"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/google/go-cmp/cmp/cmpopts"
	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
	"github.com/tidwall/gjson"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/jsonc"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/pointers"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/globals"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/batch"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
	"github.com/sourcegraph/sourcegraph/internal/encryption"
	et "github.com/sourcegraph/sourcegraph/internal/encryption/testing"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/types/typestest"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestExternalServicesListOptions_sqlConditions(t *testing.T) {
	tests := []struct {
		name             string
		kinds            []string
		afterID          int64
		updatedAfter     time.Time
		wantQuery        string
		onlyCloudDefault bool
		includeDeleted   bool
		wantArgs         []any
	}{
		{
			name:      "no condition",
			wantQuery: "deleted_at IS NULL",
		},
		{
			name:      "only one kind: GitHub",
			kinds:     []string{extsvc.KindGitHub},
			wantQuery: "deleted_at IS NULL AND kind = ANY($1)",
			wantArgs:  []any{pq.Array([]string{extsvc.KindGitHub})},
		},
		{
			name:      "two kinds: GitHub and GitLab",
			kinds:     []string{extsvc.KindGitHub, extsvc.KindGitLab},
			wantQuery: "deleted_at IS NULL AND kind = ANY($1)",
			wantArgs:  []any{pq.Array([]string{extsvc.KindGitHub, extsvc.KindGitLab})},
		},
		{
			name:      "has after ID",
			afterID:   10,
			wantQuery: "deleted_at IS NULL AND id < $1",
			wantArgs:  []any{int64(10)},
		},
		{
			name:         "has after updated_at",
			updatedAfter: time.Date(2013, 0o4, 19, 0, 0, 0, 0, time.UTC),
			wantQuery:    "deleted_at IS NULL AND updated_at > $1",
			wantArgs:     []any{time.Date(2013, 0o4, 19, 0, 0, 0, 0, time.UTC)},
		},
		{
			name:             "has OnlyCloudDefault",
			onlyCloudDefault: true,
			wantQuery:        "deleted_at IS NULL AND cloud_default = true",
		},
		{
			name:           "includeDeleted",
			includeDeleted: true,
			wantQuery:      "TRUE",
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			opts := ExternalServicesListOptions{
				Kinds:            test.kinds,
				AfterID:          test.afterID,
				UpdatedAfter:     test.updatedAfter,
				OnlyCloudDefault: test.onlyCloudDefault,
				IncludeDeleted:   test.includeDeleted,
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
	t.Parallel()

	tests := []struct {
		name     string
		kind     string
		config   string
		listFunc func(ctx context.Context, opt ExternalServicesListOptions) ([]*types.ExternalService, error)
		wantErr  string
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
			config:  `{"repositoryQuery": ["none"], "token": "fake"}`,
			wantErr: "url is required",
		},
		{
			name:    "2 errors",
			kind:    extsvc.KindGitHub,
			config:  `{"url": "https://github.com", "repositoryQuery": ["none"], "token": ""}`,
			wantErr: "2 errors occurred:\n\t* token: String length must be greater than or equal to 1\n\t* either token or GitHub App Details must be set",
		},
		{
			name:   "no conflicting rate limit",
			kind:   extsvc.KindGitHub,
			config: `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc", "rateLimit": {"enabled": true, "requestsPerHour": 5000}}`,
			listFunc: func(ctx context.Context, opt ExternalServicesListOptions) ([]*types.ExternalService, error) {
				return nil, nil
			},
			wantErr: "<nil>",
		},
		{
			name:    "gjson handles comments",
			kind:    extsvc.KindGitHub,
			config:  `{"url": "https://github.com", "token": "abc", "repositoryQuery": ["affiliated"]} // comment`,
			wantErr: "<nil>",
		},
		{
			name:    "1 errors - GitHub.com",
			kind:    extsvc.KindGitHub,
			config:  `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "` + types.RedactedSecret + `"}`,
			wantErr: "unable to write external service config as it contains redacted fields, this is likely a bug rather than a problem with your config",
		},
		{
			name:    "1 errors - GitLab.com",
			kind:    extsvc.KindGitLab,
			config:  `{"url": "https://github.com", "projectQuery": ["none"], "token": "` + types.RedactedSecret + `"}`,
			wantErr: "unable to write external service config as it contains redacted fields, this is likely a bug rather than a problem with your config",
		},
		{
			name:    "1 errors - dev.azure.com",
			kind:    extsvc.KindAzureDevOps,
			config:  `{"url": "https://dev.azure.com", "token": "token", "username": "username"}`,
			wantErr: "either 'projects' or 'orgs' must be set",
		},
		{
			name:    "0 errors - dev.azure.com",
			kind:    extsvc.KindAzureDevOps,
			config:  `{"url": "https://dev.azure.com", "token": "token", "username": "username", "projects":[]}`,
			wantErr: "<nil>",
		},
		{
			name:    "0 errors - dev.azure.com",
			kind:    extsvc.KindAzureDevOps,
			config:  `{"url": "https://dev.azure.com", "token": "token", "username": "username", "orgs":[]}`,
			wantErr: "<nil>",
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			ess := NewMockExternalServiceStore()
			if test.listFunc != nil {
				ess.ListFunc.SetDefaultHook(test.listFunc)
			}
			_, err := ValidateExternalServiceConfig(context.Background(), ess, ValidateExternalServiceConfigOptions{
				Kind:   test.kind,
				Config: test.config,
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
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	envvar.MockSourcegraphDotComMode(true)
	defer envvar.MockSourcegraphDotComMode(false)

	confGet := func() *conf.Unified { return &conf.Unified{} }

	tests := []struct {
		name             string
		externalService  *types.ExternalService
		wantUnrestricted bool
		wantHasWebhooks  bool
		wantError        bool
	}{
		{
			name: "with webhooks",
			externalService: &types.ExternalService{
				Kind:        extsvc.KindGitHub,
				DisplayName: "GITHUB #1",
				Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc", "webhooks": [{"org": "org", "secret": "secret"}]}`),
			},
			wantUnrestricted: false,
			wantHasWebhooks:  true,
		},
		{
			name: "without authorization",
			externalService: &types.ExternalService{
				Kind:        extsvc.KindGitHub,
				DisplayName: "GITHUB #1",
				Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
			},
			wantUnrestricted: false,
			wantHasWebhooks:  false,
		},
		{
			name: "with authorization",
			externalService: &types.ExternalService{
				Kind:        extsvc.KindGitHub,
				DisplayName: "GITHUB #2",
				Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc", "authorization": {}}`),
			},
			wantUnrestricted: false,
			wantHasWebhooks:  false,
		},
		{
			name: "with authorization in comments",
			externalService: &types.ExternalService{
				Kind:        extsvc.KindGitHub,
				DisplayName: "GITHUB #3",
				Config: extsvc.NewUnencryptedConfig(`
{
	"url": "https://github.com",
	"repositoryQuery": ["none"],
	"token": "abc",
	// "authorization": {}
}`),
			},
			wantUnrestricted: false,
		},
		{
			name: "dotcom: auto-add authorization to code host connections for GitHub",
			externalService: &types.ExternalService{
				Kind:        extsvc.KindGitHub,
				DisplayName: "GITHUB #4",
				Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
			},
			wantUnrestricted: false,
			wantHasWebhooks:  false,
		},
		{
			name: "dotcom: auto-add authorization to code host connections for GitLab",
			externalService: &types.ExternalService{
				Kind:        extsvc.KindGitLab,
				DisplayName: "GITLAB #1",
				Config:      extsvc.NewUnencryptedConfig(`{"url": "https://gitlab.com", "projectQuery": ["none"], "token": "abc"}`),
			},
			wantUnrestricted: false,
			wantHasWebhooks:  false,
		},
		{
			name: "Empty config not allowed",
			externalService: &types.ExternalService{
				Kind:        extsvc.KindGitLab,
				DisplayName: "GITLAB #1",
				Config:      extsvc.NewUnencryptedConfig(``),
			},
			wantUnrestricted: false,
			wantHasWebhooks:  false,
			wantError:        true,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			err := db.ExternalServices().Create(ctx, confGet, test.externalService)
			if test.wantError {
				if err == nil {
					t.Fatal("wanted an error")
				}
				// We can bail out early here
				return
			}
			if err != nil {
				t.Fatal(err)
			}

			// Should get back the same one
			got, err := db.ExternalServices().GetByID(ctx, test.externalService.ID)
			if err != nil {
				t.Fatal(err)
			}

			if diff := cmp.Diff(test.externalService, got, et.CompareEncryptable); diff != "" {
				t.Fatalf("Mismatch (-want +got):\n%s", diff)
			}

			if test.wantUnrestricted != got.Unrestricted {
				t.Fatalf("Want unrestricted = %v, but got %v", test.wantUnrestricted, got.Unrestricted)
			}

			if got.HasWebhooks == nil {
				t.Fatal("has_webhooks must not be null")
			} else if *got.HasWebhooks != test.wantHasWebhooks {
				t.Fatalf("Wanted has_webhooks = %v, but got %v", test.wantHasWebhooks, *got.HasWebhooks)
			}

			err = db.ExternalServices().Delete(ctx, test.externalService.ID)
			if err != nil {
				t.Fatal(err)
			}
		})
	}
}

func TestExternalServicesStore_CreateWithTierEnforcement(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))

	ctx := context.Background()
	confGet := func() *conf.Unified { return &conf.Unified{} }
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	store := db.ExternalServices()
	BeforeCreateExternalService = func(context.Context, ExternalServiceStore, *types.ExternalService) error {
		return errcode.NewPresentationError("test plan limit exceeded")
	}
	t.Cleanup(func() { BeforeCreateExternalService = nil })
	if err := store.Create(ctx, confGet, es); err == nil {
		t.Fatal("expected an error, got none")
	}
}

func TestExternalServicesStore_Update(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	envvar.MockSourcegraphDotComMode(true)
	defer envvar.MockSourcegraphDotComMode(false)

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc", "authorization": {}}`),
	}
	err := db.ExternalServices().Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	// NOTE: The order of tests matters
	tests := []struct {
		name               string
		update             *ExternalServiceUpdate
		wantUnrestricted   bool
		wantCloudDefault   bool
		wantHasWebhooks    bool
		wantTokenExpiresAt bool
		wantError          bool
	}{
		{
			name: "update with authorization",
			update: &ExternalServiceUpdate{
				DisplayName: strptr("GITHUB (updated) #1"),
				Config:      strptr(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def", "authorization": {}, "webhooks": [{"org": "org", "secret": "secret"}]}`),
			},
			wantUnrestricted: false,
			wantCloudDefault: false,
			wantHasWebhooks:  true,
		},
		{
			name: "update without authorization",
			update: &ExternalServiceUpdate{
				DisplayName: strptr("GITHUB (updated) #2"),
				Config:      strptr(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def"}`),
			},
			wantUnrestricted: false,
			wantCloudDefault: false,
			wantHasWebhooks:  false,
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
			wantUnrestricted: false,
			wantCloudDefault: false,
			wantHasWebhooks:  false,
		},
		{
			name: "set cloud_default true",
			update: &ExternalServiceUpdate{
				DisplayName:  strptr("GITHUB (updated) #4"),
				CloudDefault: boolptr(true),
				Config: strptr(`
{
	"url": "https://github.com",
	"repositoryQuery": ["none"],
	"token": "def",
	"authorization": {},
	"webhooks": [{"org": "org", "secret": "secret"}]
}`),
			},
			wantUnrestricted: false,
			wantCloudDefault: true,
			wantHasWebhooks:  true,
		},
		{
			name: "update token_expires_at",
			update: &ExternalServiceUpdate{
				DisplayName:    strptr("GITHUB (updated) #5"),
				Config:         strptr(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def"}`),
				TokenExpiresAt: pointers.Ptr(time.Now()),
			},
			wantCloudDefault:   true,
			wantTokenExpiresAt: true,
		},
		{
			name: "update with empty config",
			update: &ExternalServiceUpdate{
				Config: strptr(``),
			},
			wantError: true,
		},
		{
			name: "update with comment config",
			update: &ExternalServiceUpdate{
				Config: strptr(`// {}`),
			},
			wantError: true,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			err = db.ExternalServices().Update(ctx, nil, es.ID, test.update)
			if test.wantError {
				if err == nil {
					t.Fatal("Wanted an error")
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}

			// Get and verify update
			got, err := db.ExternalServices().GetByID(ctx, es.ID)
			if err != nil {
				t.Fatal(err)
			}

			if diff := cmp.Diff(*test.update.DisplayName, got.DisplayName); diff != "" {
				t.Fatalf("DisplayName mismatch (-want +got):\n%s", diff)
			} else {
				cmpJSON := func(a, b string) string {
					normalize := func(s string) string {
						values := map[string]any{}
						_ = json.Unmarshal([]byte(s), &values)
						delete(values, "authorization")
						serialized, _ := json.Marshal(values)
						return string(serialized)
					}

					return cmp.Diff(normalize(a), normalize(b))
				}

				cfg, err := got.Config.Decrypt(ctx)
				if err != nil {
					t.Fatal(err)
				}
				if diff = cmpJSON(*test.update.Config, cfg); diff != "" {
					t.Fatalf("Config mismatch (-want +got):\n%s", diff)
				} else if got.UpdatedAt.Equal(es.UpdatedAt) {
					t.Fatalf("UpdateAt: want to be updated but not")
				}
			}

			if test.wantUnrestricted != got.Unrestricted {
				t.Fatalf("Want unrestricted = %v, but got %v", test.wantUnrestricted, got.Unrestricted)
			}

			if test.wantCloudDefault != got.CloudDefault {
				t.Fatalf("Want cloud_default = %v, but got %v", test.wantCloudDefault, got.CloudDefault)
			}

			if got.HasWebhooks == nil {
				t.Fatal("has_webhooks is unexpectedly null")
			} else if test.wantHasWebhooks != *got.HasWebhooks {
				t.Fatalf("Want has_webhooks = %v, but got %v", test.wantHasWebhooks, *got.HasWebhooks)
			}

			if (got.TokenExpiresAt != nil) != test.wantTokenExpiresAt {
				t.Fatalf("Want token_expires_at = %v, but got %v", test.wantTokenExpiresAt, got.TokenExpiresAt)
			}
		})
	}
}

func TestDisablePermsSyncingForExternalService(t *testing.T) {
	tests := []struct {
		name   string
		config string
		want   string
	}{
		{
			name: "github with authorization",
			config: `
{
  // Useful comments
  "url": "https://github.com",
  "repositoryQuery": ["none"],
  "token": "def",
  "authorization": {}
}`,
			want: `
{
  // Useful comments
  "url": "https://github.com",
  "repositoryQuery": ["none"],
  "token": "def"
}`,
		},
		{
			name: "github without authorization",
			config: `
{
  // Useful comments
  "url": "https://github.com",
  "repositoryQuery": ["none"],
  "token": "def"
}`,
			want: `
{
  // Useful comments
  "url": "https://github.com",
  "repositoryQuery": ["none"],
  "token": "def"
}`,
		},
		{
			name: "azure devops with enforce permissions",
			config: `
{
  // Useful comments
  "url": "https://dev.azure.com",
  "username": "horse",
  "token": "abc",
  "enforcePermissions": true
}`,
			want: `
{
  // Useful comments
  "url": "https://dev.azure.com",
  "username": "horse",
  "token": "abc"
}`,
		},
		{
			name: "azure devops without enforce permissions",
			config: `
{
  // Useful comments
  "url": "https://dev.azure.com",
  "username": "horse",
  "token": "abc"
}`,
			want: `
{
  // Useful comments
  "url": "https://dev.azure.com",
  "username": "horse",
  "token": "abc"
}`,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			got, err := disablePermsSyncingForExternalService(test.config)
			if err != nil {
				t.Fatal(err)
			}

			if diff := cmp.Diff(test.want, got); diff != "" {
				t.Fatalf("Mismatch (-want +got):\n%s", diff)
			}
		})
	}
}

// This test ensures under Sourcegraph.com mode, every call of `Create`,
// `Upsert` and `Update` removes the "authorization" field in the external
// service config automatically.
func TestExternalServicesStore_DisablePermsSyncingForExternalService(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	envvar.MockSourcegraphDotComMode(true)
	defer envvar.MockSourcegraphDotComMode(false)

	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	externalServices := db.ExternalServices()

	// Test Create method
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc", "authorization": {}}`),
	}
	err := externalServices.Create(ctx, confGet, es)
	require.NoError(t, err)

	got, err := externalServices.GetByID(ctx, es.ID)
	require.NoError(t, err)
	cfg, err := got.Config.Decrypt(ctx)
	if err != nil {
		t.Fatal(err)
	}
	exists := gjson.Get(cfg, "authorization").Exists()
	assert.False(t, exists, `"authorization" field exists, but should not`)

	// Reset Config field and test Upsert method
	es.Config.Set(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc", "authorization": {}}`)
	err = externalServices.Upsert(ctx, es)
	require.NoError(t, err)

	got, err = externalServices.GetByID(ctx, es.ID)
	require.NoError(t, err)
	cfg, err = got.Config.Decrypt(ctx)
	if err != nil {
		t.Fatal(err)
	}
	exists = gjson.Get(cfg, "authorization").Exists()
	assert.False(t, exists, `"authorization" field exists, but should not`)

	// Reset Config field and test Update method
	es.Config.Set(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc", "authorization": {}}`)
	err = externalServices.Update(ctx,
		conf.Get().AuthProviders,
		es.ID,
		&ExternalServiceUpdate{
			Config: &cfg,
		},
	)
	require.NoError(t, err)

	got, err = externalServices.GetByID(ctx, es.ID)
	require.NoError(t, err)
	cfg, err = got.Config.Decrypt(ctx)
	if err != nil {
		t.Fatal(err)
	}
	exists = gjson.Get(cfg, "authorization").Exists()
	assert.False(t, exists, `"authorization" field exists, but should not`)
}

func TestCountRepoCount(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := actor.WithInternalActor(context.Background())

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es1 := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	err := db.ExternalServices().Create(ctx, confGet, es1)
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

	count, err := db.ExternalServices().RepoCount(ctx, es1.ID)
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
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := actor.WithInternalActor(context.Background())

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es1 := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	err := db.ExternalServices().Create(ctx, confGet, es1)
	if err != nil {
		t.Fatal(err)
	}

	es2 := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #2",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def"}`),
	}
	err = db.ExternalServices().Create(ctx, confGet, es2)
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
	err = db.ExternalServices().Delete(ctx, es1.ID)
	if err != nil {
		t.Fatal(err)
	}

	// Delete again should get externalServiceNotFoundError
	err = db.ExternalServices().Delete(ctx, es1.ID)
	gotErr := fmt.Sprintf("%v", err)
	wantErr := fmt.Sprintf("external service not found: %v", es1.ID)
	if gotErr != wantErr {
		t.Errorf("error: want %q but got %q", wantErr, gotErr)
	}

	_, err = db.ExternalServices().GetByID(ctx, es1.ID)
	if err == nil {
		t.Fatal("expected an error")
	}

	// Should only get back the repo with ID=2
	repos, err := db.Repos().GetByIDs(ctx, 1, 2)
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

func TestExternalServiceStore_Delete_WithSyncJobs(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	store := &externalServiceStore{Store: basestore.NewWithHandle(db.Handle())}
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified { return &conf.Unified{} }
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	if err := store.Create(ctx, confGet, es); err != nil {
		t.Fatal(err)
	}

	// Insert a sync job
	syncJobID, _, err := basestore.ScanFirstInt64(db.Handle().QueryContext(ctx, `
INSERT INTO external_service_sync_jobs (external_service_id, state, started_at)
VALUES ($1, $2, now())
RETURNING id
`, es.ID, "processing"))
	if err != nil {
		t.Fatal(err)
	}

	// When we now delete the external service it'll mark the sync job as
	// 'cancel = true', so in a separate goroutine we need to wait until the
	// job is marked as cancel true and then set it to canceled
	go func() {
		ctx, cancel := context.WithTimeout(ctx, 10*time.Second)
		defer cancel()

		for {
			jobCancel, _, err := basestore.ScanFirstBool(db.Handle().QueryContext(ctx, `SELECT cancel FROM external_service_sync_jobs WHERE id = $1`, syncJobID))
			if err != nil {
				logger.Error("querying 'cancel' failed", log.Error(err))
				return
			}
			if jobCancel {
				break
			}
			time.Sleep(50 * time.Millisecond)
		}

		// Job has been marked as to-be-canceled, let's cancel it
		_, err := db.Handle().ExecContext(ctx, `UPDATE external_service_sync_jobs SET state = 'canceled', finished_at = now() WHERE id = $1`, syncJobID)
		if err != nil {
			logger.Error("marking job as cancelled failed", log.Error(err))
			return
		}
	}()

	deleted := make(chan error)
	go func() {
		// This will block until the goroutine above has finished
		err = db.ExternalServices().Delete(ctx, es.ID)
		deleted <- err
	}()

	select {
	case <-time.After(10 * time.Second):
		t.Fatal("timeout waiting for external service deletion")
	case err := <-deleted:
		if err != nil {
			t.Fatalf("deleting external service failed: %s", err)
		}
	}

	_, err = db.ExternalServices().GetByID(ctx, es.ID)
	if !errcode.IsNotFound(err) {
		t.Fatal("expected an error")
	}
}

// reposNumber is a number of repos created in one batch.
// TestExternalServicesStore_DeleteExtServiceWithManyRepos does 5 such batches
const reposNumber = 1000

// TestExternalServicesStore_DeleteExtServiceWithManyRepos can be used locally
// with increased number of repos to see how fast/slow deletion of external
// services works.
func TestExternalServicesStore_DeleteExtServiceWithManyRepos(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := actor.WithInternalActor(context.Background())

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	extSvc := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	servicesStore := db.ExternalServices()
	err := servicesStore.Create(ctx, confGet, extSvc)
	if err != nil {
		t.Fatal(err)
	}

	createRepo := func(offset int, c chan<- int) {
		inserter := func(inserter *batch.Inserter) error {
			for i := 0 + offset; i < reposNumber+offset; i++ {
				if err := inserter.Insert(ctx, i, "repo"+strconv.Itoa(i)); err != nil {
					return err
				}
			}
			return nil
		}

		if err := batch.WithInserter(
			ctx,
			db,
			"repo",
			batch.MaxNumPostgresParameters,
			[]string{"id", "name"},
			inserter,
		); err != nil {
			t.Error(err)
			c <- 1
			return
		}
		c <- 0
	}

	ready := make(chan int, 5)
	defer close(ready)
	offsets := []int{0, reposNumber, reposNumber * 2, reposNumber * 3, reposNumber * 4}

	for _, offset := range offsets {
		go createRepo(offset, ready)
	}

	for i := 0; i < 5; i++ {
		if status := <-ready; status != 0 {
			t.Fatal("Error during repo creation")
		}
	}

	ready2 := make(chan int, 5)
	defer close(ready2)

	extSvcId := extSvc.ID

	createExtSvc := func(offset int, c chan<- int) {
		inserter := func(inserter *batch.Inserter) error {
			for i := 0 + offset; i < reposNumber+offset; i++ {
				if err := inserter.Insert(ctx, extSvcId, i, ""); err != nil {
					return err
				}
			}
			return nil
		}

		if err := batch.WithInserter(
			ctx,
			db,
			"external_service_repos",
			batch.MaxNumPostgresParameters,
			[]string{"external_service_id", "repo_id", "clone_url"},
			inserter,
		); err != nil {
			t.Error(err)
			c <- 1
			return
		}
		c <- 0
	}

	for _, offset := range offsets {
		go createExtSvc(offset, ready2)
	}

	for i := 0; i < 5; i++ {
		if status := <-ready2; status != 0 {
			t.Fatal("Error during external service repo creation")
		}
	}

	// Delete this external service
	start := time.Now()
	err = servicesStore.Delete(ctx, extSvcId)
	if err != nil {
		t.Fatal(err)
	}
	t.Logf("Deleting of an external service with %d repos took %s", reposNumber*5, time.Since(start))

	count, err := servicesStore.RepoCount(ctx, extSvcId)
	if err != nil {
		t.Fatal(err)
	}
	if count != 0 {
		t.Fatal("External service repos are not deleted")
	}

	// Should throw not found error
	_, err = servicesStore.GetByID(ctx, extSvcId)
	if err == nil {
		t.Fatal("External service is not deleted")
	}

	rows, err := db.Handle().QueryContext(ctx, `select * from repo where deleted_at is null`)
	if err != nil {
		t.Fatal("Error during fetching repos from the DB")
	}
	if rows.Next() {
		t.Fatal("Repos of external service are not deleted")
	}
}

func TestExternalServicesStore_GetByID(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	err := db.ExternalServices().Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	// Should be able to get back by its ID
	_, err = db.ExternalServices().GetByID(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	// Delete this external service
	err = db.ExternalServices().Delete(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	// Should now get externalServiceNotFoundError
	_, err = db.ExternalServices().GetByID(ctx, es.ID)
	gotErr := fmt.Sprintf("%v", err)
	wantErr := fmt.Sprintf("external service not found: %v", es.ID)
	if gotErr != wantErr {
		t.Errorf("error: want %q but got %q", wantErr, gotErr)
	}
}

func TestExternalServicesStore_GetByID_Encrypted(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}

	store := db.ExternalServices().WithEncryptionKey(et.TestKey{})

	err := store.Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	// values encrypted should not be readable without the encrypting key
	noopStore := store.WithEncryptionKey(&encryption.NoopKey{FailDecrypt: true})
	svc, err := noopStore.GetByID(ctx, es.ID)
	if err != nil {
		t.Fatalf("unexpected error querying service: %s", err)
	}
	if _, err := svc.Config.Decrypt(ctx); err == nil {
		t.Fatalf("expected error decrypting with a different key")
	}

	// Should be able to get back by its ID
	_, err = store.GetByID(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	// Delete this external service
	err = store.Delete(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	// Should now get externalServiceNotFoundError
	_, err = store.GetByID(ctx, es.ID)
	gotErr := fmt.Sprintf("%v", err)
	wantErr := fmt.Sprintf("external service not found: %v", es.ID)
	if gotErr != wantErr {
		t.Errorf("error: want %q but got %q", wantErr, gotErr)
	}
}

func TestGetLatestSyncErrors(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()

	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	createService := func(name string) *types.ExternalService {
		confGet := func() *conf.Unified { return &conf.Unified{} }

		svc := &types.ExternalService{
			Kind:        extsvc.KindGitHub,
			DisplayName: name,
			Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
		}

		if err := db.ExternalServices().Create(ctx, confGet, svc); err != nil {
			t.Fatal(err)
		}
		return svc
	}

	addSyncError := func(t *testing.T, extSvcID int64, failure string) {
		t.Helper()
		_, err := db.Handle().ExecContext(ctx, `
INSERT INTO external_service_sync_jobs (external_service_id, state, finished_at, failure_message)
VALUES ($1,'errored', now(), $2)
`, extSvcID, failure)
		if err != nil {
			t.Fatal(err)
		}
	}

	extSvc1 := createService("GITHUB #1")
	extSvc2 := createService("GITHUB #2")

	// Listing errors now should return an empty map as none have been added yet
	results, err := db.ExternalServices().GetLatestSyncErrors(ctx)
	if err != nil {
		t.Fatal(err)
	}

	want := []*SyncError{
		{ServiceID: extSvc1.ID, Message: ""},
		{ServiceID: extSvc2.ID, Message: ""},
	}

	if diff := cmp.Diff(want, results); diff != "" {
		t.Fatalf("wrong sync errors (-want +got):\n%s", diff)
	}

	// Add two failures for the same service
	failure1 := "oops"
	failure2 := "oops again"
	addSyncError(t, extSvc1.ID, failure1)
	addSyncError(t, extSvc1.ID, failure2)

	// We should get the latest failure
	results, err = db.ExternalServices().GetLatestSyncErrors(ctx)
	if err != nil {
		t.Fatal(err)
	}

	want = []*SyncError{
		{ServiceID: extSvc1.ID, Message: failure2},
		{ServiceID: extSvc2.ID, Message: ""},
	}
	if diff := cmp.Diff(want, results); diff != "" {
		t.Fatalf("wrong sync errors (-want +got):\n%s", diff)
	}

	// Add error for other external service
	addSyncError(t, extSvc2.ID, "oops over here")

	results, err = db.ExternalServices().GetLatestSyncErrors(ctx)
	if err != nil {
		t.Fatal(err)
	}

	want = []*SyncError{
		{ServiceID: extSvc1.ID, Message: failure2},
		{ServiceID: extSvc2.ID, Message: "oops over here"},
	}
	if diff := cmp.Diff(want, results); diff != "" {
		t.Fatalf("wrong sync errors (-want +got):\n%s", diff)
	}
}

func TestGetLastSyncError(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	err := db.ExternalServices().Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	// Should be able to get back by its ID
	_, err = db.ExternalServices().GetByID(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	lastSyncError, err := db.ExternalServices().GetLastSyncError(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}
	if lastSyncError != "" {
		t.Fatalf("Expected empty error, have %q", lastSyncError)
	}

	// Could have failure message
	_, err = db.Handle().ExecContext(ctx, `
INSERT INTO external_service_sync_jobs (external_service_id, state, finished_at)
VALUES ($1,'errored', now())
`, es.ID)

	if err != nil {
		t.Fatal(err)
	}

	lastSyncError, err = db.ExternalServices().GetLastSyncError(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}
	if lastSyncError != "" {
		t.Fatalf("Expected empty error, have %q", lastSyncError)
	}

	// Add sync error
	expectedError := "oops"
	_, err = db.Handle().ExecContext(ctx, `
INSERT INTO external_service_sync_jobs (external_service_id, failure_message, state, finished_at)
VALUES ($1,$2,'errored', now())
`, es.ID, expectedError)

	if err != nil {
		t.Fatal(err)
	}

	lastSyncError, err = db.ExternalServices().GetLastSyncError(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}
	if lastSyncError != expectedError {
		t.Fatalf("Expected %q, have %q", expectedError, lastSyncError)
	}
}

func TestExternalServiceStore_HasRunningSyncJobs(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}

	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	store := &externalServiceStore{Store: basestore.NewWithHandle(db.Handle())}
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified { return &conf.Unified{} }
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	if err := store.Create(ctx, confGet, es); err != nil {
		t.Fatal(err)
	}

	ok, err := store.hasRunningSyncJobs(ctx, es.ID)
	if err != nil {
		t.Fatalf("unexpected error: %s", err)
	}
	if ok {
		t.Fatal("unexpected running sync jobs")
	}

	_, err = db.Handle().ExecContext(ctx, `
INSERT INTO external_service_sync_jobs (external_service_id, state, started_at)
VALUES ($1, 'processing', now())
RETURNING id
`, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	ok, err = store.hasRunningSyncJobs(ctx, es.ID)
	if err != nil {
		t.Fatalf("unexpected error: %s", err)
	}
	if !ok {
		t.Fatal("unexpected running sync jobs")
	}
}

func TestExternalServiceStore_CancelSyncJob(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	store := db.ExternalServices()
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified { return &conf.Unified{} }
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	err := store.Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	// Make sure "not found" is handled
	err = store.CancelSyncJob(ctx, ExternalServicesCancelSyncJobOptions{ID: 9999})
	if !errors.HasType(err, &errSyncJobNotFound{}) {
		t.Fatalf("Expected not-found error, have %q", err)
	}
	err = store.CancelSyncJob(ctx, ExternalServicesCancelSyncJobOptions{ExternalServiceID: 9999})
	if err != nil {
		t.Fatalf("Expected no error, but have %q", err)
	}

	assertCanceled := func(t *testing.T, syncJobID int64, wantState string, wantFinished bool) {
		t.Helper()

		// Make sure it was canceled
		syncJob, err := store.GetSyncJobByID(ctx, syncJobID)
		if err != nil {
			t.Fatal(err)
		}
		if !syncJob.Cancel {
			t.Fatalf("syncjob not canceled")
		}
		if syncJob.State != wantState {
			t.Fatalf("syncjob state unexpectedly changed")
		}
		if !wantFinished && !syncJob.FinishedAt.IsZero() {
			t.Fatalf("syncjob finishedAt is set but should not be")
		}
		if wantFinished && syncJob.FinishedAt.IsZero() {
			t.Fatalf("syncjob finishedAt is not set but should be")
		}
	}

	insertSyncJob := func(t *testing.T, state string) int64 {
		t.Helper()

		syncJobID, _, err := basestore.ScanFirstInt64(db.Handle().QueryContext(ctx, `
INSERT INTO external_service_sync_jobs (external_service_id, state, started_at)
VALUES ($1, $2, now())
RETURNING id
`, es.ID, state))
		if err != nil {
			t.Fatal(err)
		}
		return syncJobID
	}

	// Insert 'processing' sync job that can be canceled and cancel by ID
	syncJobID := insertSyncJob(t, "processing")
	err = store.CancelSyncJob(ctx, ExternalServicesCancelSyncJobOptions{ID: syncJobID})
	if err != nil {
		t.Fatalf("Cancel failed: %s", err)
	}
	assertCanceled(t, syncJobID, "processing", false)

	// Insert another 'processing' sync job that can be canceled, but cancel by external_service_id
	syncJobID2 := insertSyncJob(t, "processing")
	err = store.CancelSyncJob(ctx, ExternalServicesCancelSyncJobOptions{ExternalServiceID: es.ID})
	if err != nil {
		t.Fatalf("Cancel failed: %s", err)
	}
	assertCanceled(t, syncJobID2, "processing", false)

	// Insert 'queued' sync job that can be canceled
	syncJobID3 := insertSyncJob(t, "queued")
	err = store.CancelSyncJob(ctx, ExternalServicesCancelSyncJobOptions{ID: syncJobID3})
	if err != nil {
		t.Fatalf("Cancel failed: %s", err)
	}
	assertCanceled(t, syncJobID3, "canceled", true)

	// Insert sync job in state that is not cancelable
	syncJobID4 := insertSyncJob(t, "completed")
	err = store.CancelSyncJob(ctx, ExternalServicesCancelSyncJobOptions{ID: syncJobID4})
	if !errors.HasType(err, &errSyncJobNotFound{}) {
		t.Fatalf("Expected not-found error, have %q", err)
	}
}

func TestExternalServicesStore_List(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Create new external services
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	ess := []*types.ExternalService{
		{
			Kind:         extsvc.KindGitHub,
			DisplayName:  "GITHUB #1",
			Config:       extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc", "authorization": {}}`),
			CloudDefault: true,
		},
		{
			Kind:        extsvc.KindGitHub,
			DisplayName: "GITHUB #2",
			Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def"}`),
		},
		{
			Kind:        extsvc.KindGitHub,
			DisplayName: "GITHUB #3",
			Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def", "authorization": {}}`),
		},
	}

	for _, es := range ess {
		err := db.ExternalServices().Create(ctx, confGet, es)
		if err != nil {
			t.Fatal(err)
		}
	}
	createdAt := time.Now()

	deletedES := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #4",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def"}`),
	}
	err := db.ExternalServices().Create(ctx, confGet, deletedES)
	if err != nil {
		t.Fatal(err)
	}
	if err := db.ExternalServices().Delete(ctx, deletedES.ID); err != nil {
		t.Fatal(err)
	}

	t.Run("list all external services", func(t *testing.T) {
		got, err := db.ExternalServices().List(ctx, ExternalServicesListOptions{})
		if err != nil {
			t.Fatal(err)
		}
		sort.Slice(got, func(i, j int) bool { return got[i].ID < got[j].ID })

		if diff := cmp.Diff(ess, got, et.CompareEncryptable); diff != "" {
			t.Fatalf("Mismatch (-want +got):\n%s", diff)
		}
	})

	t.Run("list all external services in ascending order", func(t *testing.T) {
		got, err := db.ExternalServices().List(ctx, ExternalServicesListOptions{OrderByDirection: "ASC"})
		if err != nil {
			t.Fatal(err)
		}
		want := []*types.ExternalService(types.ExternalServices(ess).Clone())
		sort.Slice(want, func(i, j int) bool { return want[i].ID < want[j].ID })

		if diff := cmp.Diff(want, got, et.CompareEncryptable); diff != "" {
			t.Fatalf("Mismatch (-want +got):\n%s", diff)
		}
	})

	t.Run("list all external services in descending order", func(t *testing.T) {
		got, err := db.ExternalServices().List(ctx, ExternalServicesListOptions{})
		if err != nil {
			t.Fatal(err)
		}
		want := []*types.ExternalService(types.ExternalServices(ess).Clone())
		sort.Slice(want, func(i, j int) bool { return want[i].ID > want[j].ID })

		if diff := cmp.Diff(want, got, et.CompareEncryptable); diff != "" {
			t.Fatalf("Mismatch (-want +got):\n%s", diff)
		}
	})

	t.Run("list external services with certain IDs", func(t *testing.T) {
		got, err := db.ExternalServices().List(ctx, ExternalServicesListOptions{
			IDs: []int64{ess[1].ID},
		})
		if err != nil {
			t.Fatal(err)
		}
		sort.Slice(got, func(i, j int) bool { return got[i].ID < got[j].ID })

		if diff := cmp.Diff(ess[1:2], got, et.CompareEncryptable); diff != "" {
			t.Fatalf("Mismatch (-want +got):\n%s", diff)
		}
	})

	t.Run("list services updated after a certain date, expect 0", func(t *testing.T) {
		ess, err := db.ExternalServices().List(ctx, ExternalServicesListOptions{
			UpdatedAfter: createdAt,
		})
		if err != nil {
			t.Fatal(err)
		}
		// We expect zero services to have been updated after they were created
		if len(ess) != 0 {
			t.Fatalf("Want 0 external service but got %d", len(ess))
		}
	})

	t.Run("list services updated after a certain date, expect 3", func(t *testing.T) {
		ess, err := db.ExternalServices().List(ctx, ExternalServicesListOptions{
			UpdatedAfter: createdAt.Add(-5 * time.Minute),
		})
		if err != nil {
			t.Fatal(err)
		}
		// We should find all services were updated after a time in the past
		if len(ess) != 3 {
			t.Fatalf("Want 3 external services but got %d", len(ess))
		}
	})

	t.Run("list cloud default services", func(t *testing.T) {
		ess, err := db.ExternalServices().List(ctx, ExternalServicesListOptions{
			OnlyCloudDefault: true,
		})
		if err != nil {
			t.Fatal(err)
		}
		// We should find all services were updated after a time in the past
		if len(ess) != 1 {
			t.Fatalf("Want 0 external services but got %d", len(ess))
		}
	})

	t.Run("list including deleted", func(t *testing.T) {
		ess, err := db.ExternalServices().List(ctx, ExternalServicesListOptions{
			IncludeDeleted: true,
		})
		if err != nil {
			t.Fatal(err)
		}
		// We should find all services were updated after a time in the past
		if len(ess) != 4 {
			t.Fatalf("Want 4 external services but got %d", len(ess))
		}
	})
}

func TestExternalServicesStore_DistinctKinds(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	t.Run("no external service won't blow up", func(t *testing.T) {
		kinds, err := db.ExternalServices().DistinctKinds(ctx)
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
			Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
		},
		{
			Kind:        extsvc.KindGitHub,
			DisplayName: "GITHUB #2",
			Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def"}`),
		},
		{
			Kind:        extsvc.KindGitLab,
			DisplayName: "GITLAB #1",
			Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "projectQuery": ["none"], "token": "abc"}`),
		},
		{
			Kind:        extsvc.KindOther,
			DisplayName: "OTHER #1",
			Config:      extsvc.NewUnencryptedConfig(`{"repos": []}`),
		},
	}
	for _, es := range ess {
		err := db.ExternalServices().Create(ctx, confGet, es)
		if err != nil {
			t.Fatal(err)
		}
	}

	// Delete the last external service which should be excluded from the result
	err := db.ExternalServices().Delete(ctx, ess[3].ID)
	if err != nil {
		t.Fatal(err)
	}

	kinds, err := db.ExternalServices().DistinctKinds(ctx)
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
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	err := db.ExternalServices().Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	count, err := db.ExternalServices().Count(ctx, ExternalServicesListOptions{})
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
	t.Parallel()
	logger := logtest.Scoped(t)
	ctx := context.Background()

	clock := timeutil.NewFakeClock(time.Now(), 0)

	t.Run("no external services", func(t *testing.T) {
		db := NewDB(logger, dbtest.NewDB(logger, t))
		if err := db.ExternalServices().Upsert(ctx); err != nil {
			t.Fatalf("Upsert error: %s", err)
		}
	})

	t.Run("validation", func(t *testing.T) {
		db := NewDB(logger, dbtest.NewDB(logger, t))
		store := db.ExternalServices()

		t.Run("config can't be empty", func(t *testing.T) {
			want := typestest.MakeGitLabExternalService()

			want.Config.Set("")

			if err := store.Upsert(ctx, want); err == nil {
				t.Fatalf("Wanted an error")
			}
		})

		t.Run("config can't be only comments", func(t *testing.T) {
			want := typestest.MakeGitLabExternalService()
			want.Config.Set(`// {}`)

			if err := store.Upsert(ctx, want); err == nil {
				t.Fatalf("Wanted an error")
			}
		})
	})

	t.Run("one external service", func(t *testing.T) {
		db := NewDB(logger, dbtest.NewDB(logger, t))
		store := db.ExternalServices()

		svc := typestest.MakeGitLabExternalService()
		if err := store.Upsert(ctx, svc); err != nil {
			t.Fatalf("upsert error: %v", err)
		}
		if *svc.HasWebhooks != false {
			t.Fatalf("unexpected HasWebhooks: %v", svc.HasWebhooks)
		}

		cfg, err := svc.Config.Decrypt(ctx)
		if err != nil {
			t.Fatal(err)
		}

		// Add webhooks to the config and upsert.
		svc.Config.Set(`{"webhooks":[{"secret": "secret"}],` + cfg[1:])
		if err := store.Upsert(ctx, svc); err != nil {
			t.Fatalf("upsert error: %v", err)
		}
		if *svc.HasWebhooks != true {
			t.Fatalf("unexpected HasWebhooks: %v", svc.HasWebhooks)
		}
	})

	t.Run("many external services", func(t *testing.T) {
		db := NewDB(logger, dbtest.NewDB(logger, t))
		store := db.ExternalServices()

		svcs := typestest.MakeExternalServices()
		want := typestest.GenerateExternalServices(11, svcs...)

		if err := store.Upsert(ctx, want...); err != nil {
			t.Fatalf("Upsert error: %s", err)
		}

		for _, e := range want {
			if e.Kind != strings.ToUpper(e.Kind) {
				t.Errorf("external service kind didn't get upper-cased: %q", e.Kind)
				break
			}
		}

		sort.Sort(want)

		have, err := store.List(ctx, ExternalServicesListOptions{Kinds: svcs.Kinds()})
		if err != nil {
			t.Fatalf("List error: %s", err)
		}

		sort.Sort(types.ExternalServices(have))
		if diff := cmp.Diff(have, []*types.ExternalService(want), cmpopts.EquateEmpty(), et.CompareEncryptable); diff != "" {
			t.Fatalf("List:\n%s", diff)
		}

		// We'll update the external services.
		now := clock.Now()
		suffix := "-updated"
		for _, r := range want {
			r.DisplayName += suffix
			r.UpdatedAt = now
			r.CreatedAt = now
		}

		if err = store.Upsert(ctx, want...); err != nil {
			t.Errorf("Upsert error: %s", err)
		}
		have, err = store.List(ctx, ExternalServicesListOptions{})
		if err != nil {
			t.Fatalf("List error: %s", err)
		}

		sort.Sort(types.ExternalServices(have))

		if diff := cmp.Diff(have, []*types.ExternalService(want), cmpopts.EquateEmpty(), et.CompareEncryptable); diff != "" {
			t.Errorf("List:\n%s", diff)
		}

		// Delete external services
		for _, es := range want {
			if err := store.Delete(ctx, es.ID); err != nil {
				t.Fatal(err)
			}
		}

		have, err = store.List(ctx, ExternalServicesListOptions{})
		if err != nil {
			t.Errorf("List error: %s", err)
		}

		sort.Sort(types.ExternalServices(have))

		if diff := cmp.Diff(have, []*types.ExternalService(nil), cmpopts.EquateEmpty(), et.CompareEncryptable); diff != "" {
			t.Errorf("List:\n%s", diff)
		}
	})

	t.Run("with encryption key", func(t *testing.T) {
		db := NewDB(logger, dbtest.NewDB(logger, t))
		store := db.ExternalServices().WithEncryptionKey(et.TestKey{})

		svcs := typestest.MakeExternalServices()
		want := typestest.GenerateExternalServices(7, svcs...)

		if err := store.Upsert(ctx, want...); err != nil {
			t.Fatalf("Upsert error: %s", err)
		}
		for _, e := range want {
			if e.Kind != strings.ToUpper(e.Kind) {
				t.Errorf("external service kind didn't get upper-cased: %q", e.Kind)
				break
			}
		}

		// values encrypted should not be readable without the encrypting key
		noopStore := ExternalServicesWith(logger, store).WithEncryptionKey(&encryption.NoopKey{FailDecrypt: true})

		for _, e := range want {
			svc, err := noopStore.GetByID(ctx, e.ID)
			if err != nil {
				t.Fatalf("unexpected error querying service: %s", err)
			}
			if _, err := svc.Config.Decrypt(ctx); err == nil {
				t.Fatalf("expected error decrypting with a different key")
			}
		}

		have, err := store.List(ctx, ExternalServicesListOptions{Kinds: svcs.Kinds()})
		if err != nil {
			t.Fatalf("List error: %s", err)
		}

		sort.Sort(types.ExternalServices(have))
		sort.Sort(want)

		if diff := cmp.Diff(have, []*types.ExternalService(want), cmpopts.EquateEmpty(), et.CompareEncryptable); diff != "" {
			t.Fatalf("List:\n%s", diff)
		}

		// We'll update the external services.
		now := clock.Now()
		suffix := "-updated"
		for _, r := range want {
			r.DisplayName += suffix
			r.UpdatedAt = now
			r.CreatedAt = now
		}

		if err = store.Upsert(ctx, want...); err != nil {
			t.Fatalf("Upsert error: %s", err)
		}
		have, err = store.List(ctx, ExternalServicesListOptions{})
		if err != nil {
			t.Fatalf("List error: %s", err)
		}

		sort.Sort(types.ExternalServices(have))

		if diff := cmp.Diff(have, []*types.ExternalService(want), cmpopts.EquateEmpty(), et.CompareEncryptable); diff != "" {
			t.Errorf("List:\n%s", diff)
		}

		// Delete external services
		for _, es := range want {
			if err := store.Delete(ctx, es.ID); err != nil {
				t.Fatal(err)
			}
		}

		have, err = store.List(ctx, ExternalServicesListOptions{})
		if err != nil {
			t.Errorf("List error: %s", err)
		}

		sort.Sort(types.ExternalServices(have))
		if diff := cmp.Diff(have, []*types.ExternalService(nil), cmpopts.EquateEmpty(), et.CompareEncryptable); diff != "" {
			t.Errorf("List:\n%s", diff)
		}
	})
}

func TestExternalServiceStore_GetSyncJobs(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	err := db.ExternalServices().Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	_, err = db.Handle().ExecContext(ctx, "INSERT INTO external_service_sync_jobs (external_service_id) VALUES ($1)", es.ID)
	if err != nil {
		t.Fatal(err)
	}

	have, err := db.ExternalServices().GetSyncJobs(ctx, ExternalServicesGetSyncJobsOptions{})
	if err != nil {
		t.Fatal(err)
	}
	if len(have) != 1 {
		t.Fatalf("Expected 1 job, got %d", len(have))
	}

	want := &types.ExternalServiceSyncJob{
		ID:                1,
		State:             "queued",
		ExternalServiceID: es.ID,
	}
	if diff := cmp.Diff(want, have[0], cmpopts.IgnoreFields(types.ExternalServiceSyncJob{}, "ID", "QueuedAt")); diff != "" {
		t.Fatal(diff)
	}
}

func TestExternalServiceStore_CountSyncJobs(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	err := db.ExternalServices().Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	_, err = db.Handle().ExecContext(ctx, "INSERT INTO external_service_sync_jobs (external_service_id) VALUES ($1)", es.ID)
	if err != nil {
		t.Fatal(err)
	}

	have, err := db.ExternalServices().CountSyncJobs(ctx, ExternalServicesGetSyncJobsOptions{})
	if err != nil {
		t.Fatal(err)
	}
	require.Exactly(t, int64(1), have, "total count is incorrect")

	have, err = db.ExternalServices().CountSyncJobs(ctx, ExternalServicesGetSyncJobsOptions{ExternalServiceID: es.ID + 1})
	if err != nil {
		t.Fatal(err)
	}
	require.Exactly(t, int64(0), have, "total count is incorrect")
}

func TestExternalServiceStore_GetSyncJobByID(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	err := db.ExternalServices().Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	_, err = db.Handle().ExecContext(ctx,
		`INSERT INTO external_service_sync_jobs
               (id, external_service_id, repos_synced, repo_sync_errors, repos_added, repos_modified, repos_unmodified, repos_deleted)
               VALUES (1, $1, 1, 2, 3, 4, 5, 6)`, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	have, err := db.ExternalServices().GetSyncJobByID(ctx, 1)
	if err != nil {
		t.Fatal(err)
	}

	want := &types.ExternalServiceSyncJob{
		ID:                1,
		State:             "queued",
		ExternalServiceID: es.ID,
		ReposSynced:       1,
		RepoSyncErrors:    2,
		ReposAdded:        3,
		ReposModified:     4,
		ReposUnmodified:   5,
		ReposDeleted:      6,
	}
	if diff := cmp.Diff(want, have, cmpopts.IgnoreFields(types.ExternalServiceSyncJob{}, "ID", "QueuedAt")); diff != "" {
		t.Fatal(diff)
	}

	// Test not found:
	_, err = db.ExternalServices().GetSyncJobByID(ctx, 2)
	if err == nil {
		t.Fatal("no error for not found")
	}
	if !errcode.IsNotFound(err) {
		t.Fatal("wrong err code for not found")
	}
}

func TestExternalServiceStore_UpdateSyncJobCounters(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	err := db.ExternalServices().Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	_, err = db.Handle().ExecContext(ctx,
		`INSERT INTO external_service_sync_jobs
               (id, external_service_id)
               VALUES (1, $1)`, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	// Update counters
	err = db.ExternalServices().UpdateSyncJobCounters(ctx, &types.ExternalServiceSyncJob{
		ID:              1,
		ReposSynced:     1,
		RepoSyncErrors:  2,
		ReposAdded:      3,
		ReposModified:   4,
		ReposUnmodified: 5,
		ReposDeleted:    6,
	})
	if err != nil {
		t.Fatal(err)
	}

	want := &types.ExternalServiceSyncJob{
		ID:                1,
		State:             "queued",
		ExternalServiceID: es.ID,
		ReposSynced:       1,
		RepoSyncErrors:    2,
		ReposAdded:        3,
		ReposModified:     4,
		ReposUnmodified:   5,
		ReposDeleted:      6,
	}

	have, err := db.ExternalServices().GetSyncJobByID(ctx, 1)
	if err != nil {
		t.Fatal(err)
	}

	if diff := cmp.Diff(want, have, cmpopts.IgnoreFields(types.ExternalServiceSyncJob{}, "ID", "QueuedAt")); diff != "" {
		t.Fatal(diff)
	}

	// Test updating non-existent job
	err = db.ExternalServices().UpdateSyncJobCounters(ctx, &types.ExternalServiceSyncJob{ID: 2})
	if err == nil {
		t.Fatal("no error for not found")
	}
	if !errcode.IsNotFound(err) {
		t.Fatalf("wrong err code for not found, have %v", err)
	}
}

func TestExternalServicesStore_OneCloudDefaultPerKind(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	now := time.Now()

	makeService := func(cloudDefault bool) *types.ExternalService {
		cfg := `{"url": "https://github.com", "token": "abc", "repositoryQuery": ["none"]}`
		svc := &types.ExternalService{
			Kind:         extsvc.KindGitHub,
			DisplayName:  "Github - Test",
			Config:       extsvc.NewUnencryptedConfig(cfg),
			CreatedAt:    now,
			UpdatedAt:    now,
			CloudDefault: cloudDefault,
		}
		return svc
	}

	t.Run("non default", func(t *testing.T) {
		gh := makeService(false)
		if err := db.ExternalServices().Upsert(ctx, gh); err != nil {
			t.Fatalf("Upsert error: %s", err)
		}
	})

	t.Run("first default", func(t *testing.T) {
		gh := makeService(true)
		if err := db.ExternalServices().Upsert(ctx, gh); err != nil {
			t.Fatalf("Upsert error: %s", err)
		}
	})

	t.Run("second default", func(t *testing.T) {
		gh := makeService(true)
		if err := db.ExternalServices().Upsert(ctx, gh); err == nil {
			t.Fatal("Expected an error")
		}
	})
}

func TestExternalServiceStore_SyncDue(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	now := time.Now()

	makeService := func() *types.ExternalService {
		cfg := `{"url": "https://github.com", "token": "abc", "repositoryQuery": ["none"]}`
		svc := &types.ExternalService{
			Kind:        extsvc.KindGitHub,
			DisplayName: "Github - Test",
			Config:      extsvc.NewUnencryptedConfig(cfg),
			CreatedAt:   now,
			UpdatedAt:   now,
		}
		return svc
	}
	svc1 := makeService()
	svc2 := makeService()
	err := db.ExternalServices().Upsert(ctx, svc1, svc2)
	if err != nil {
		t.Fatal(err)
	}

	assertDue := func(d time.Duration, want bool) {
		t.Helper()
		ids := []int64{svc1.ID, svc2.ID}
		due, err := db.ExternalServices().SyncDue(ctx, ids, d)
		if err != nil {
			t.Error(err)
		}
		if due != want {
			t.Errorf("Want %v, got %v", want, due)
		}
	}

	makeSyncJob := func(svcID int64, state string) {
		_, err = db.Handle().ExecContext(ctx, `
INSERT INTO external_service_sync_jobs (external_service_id, state)
VALUES ($1,$2)
`, svcID, state)
		if err != nil {
			t.Fatal(err)
		}
	}

	// next_sync_at is null, so we expect a sync soon
	assertDue(1*time.Second, true)

	// next_sync_at in the future
	_, err = db.Handle().ExecContext(ctx, "UPDATE external_services SET next_sync_at = $1", now.Add(10*time.Minute))
	if err != nil {
		t.Fatal(err)
	}
	assertDue(1*time.Second, false)
	assertDue(11*time.Minute, true)

	// With sync jobs
	makeSyncJob(svc1.ID, "queued")
	makeSyncJob(svc2.ID, "completed")
	assertDue(1*time.Minute, true)

	// Sync jobs not running
	_, err = db.Handle().ExecContext(ctx, "UPDATE external_service_sync_jobs SET state = 'completed'")
	if err != nil {
		t.Fatal(err)
	}
	assertDue(1*time.Minute, false)
}

func TestConfigurationHasWebhooks(t *testing.T) {
	t.Run("supported kinds with webhooks", func(t *testing.T) {
		for _, cfg := range []any{
			&schema.GitHubConnection{
				Webhooks: []*schema.GitHubWebhook{
					{Org: "org", Secret: "super secret"},
				},
			},
			&schema.GitLabConnection{
				Webhooks: []*schema.GitLabWebhook{
					{Secret: "super secret"},
				},
			},
			&schema.BitbucketServerConnection{
				Plugin: &schema.BitbucketServerPlugin{
					Webhooks: &schema.BitbucketServerPluginWebhooks{
						Secret: "super secret",
					},
				},
			},
		} {
			t.Run(fmt.Sprintf("%T", cfg), func(t *testing.T) {
				assert.True(t, configurationHasWebhooks(cfg))
			})
		}
	})

	t.Run("supported kinds without webhooks", func(t *testing.T) {
		for _, cfg := range []any{
			&schema.GitHubConnection{},
			&schema.GitLabConnection{},
			&schema.BitbucketServerConnection{},
		} {
			t.Run(fmt.Sprintf("%T", cfg), func(t *testing.T) {
				assert.False(t, configurationHasWebhooks(cfg))
			})
		}
	})

	t.Run("unsupported kinds", func(t *testing.T) {
		for _, cfg := range []any{
			&schema.AWSCodeCommitConnection{},
			&schema.BitbucketCloudConnection{},
			&schema.GitoliteConnection{},
			&schema.PerforceConnection{},
			&schema.PhabricatorConnection{},
			&schema.JVMPackagesConnection{},
			&schema.OtherExternalServiceConnection{},
			nil,
		} {
			t.Run(fmt.Sprintf("%T", cfg), func(t *testing.T) {
				assert.False(t, configurationHasWebhooks(cfg))
			})
		}
	})
}

func TestExternalServiceStore_recalculateFields(t *testing.T) {
	tests := map[string]struct {
		explicitPermsEnabled bool
		authorizationSet     bool
		expectUnrestricted   bool
	}{
		"default state": {
			expectUnrestricted: true,
		},
		"explicit perms set": {
			explicitPermsEnabled: true,
			expectUnrestricted:   false,
		},
		"authorization set": {
			authorizationSet:   true,
			expectUnrestricted: false,
		},
		"authorization and explicit perms set": {
			explicitPermsEnabled: true,
			authorizationSet:     true,
			expectUnrestricted:   false,
		},
	}

	e := &externalServiceStore{logger: logtest.NoOp(t)}

	for name, tc := range tests {
		t.Run(name, func(t *testing.T) {
			pmu := globals.PermissionsUserMapping()
			t.Cleanup(func() {
				globals.SetPermissionsUserMapping(pmu)
			})

			es := &types.ExternalService{}

			if tc.explicitPermsEnabled {
				globals.SetPermissionsUserMapping(&schema.PermissionsUserMapping{
					BindID:  "email",
					Enabled: true,
				})
			}
			rawConfig := "{}"
			var err error
			if tc.authorizationSet {
				rawConfig, err = jsonc.Edit(rawConfig, struct{}{}, "authorization")
				require.NoError(t, err)
			}

			require.NoError(t, e.recalculateFields(es, rawConfig))

			require.Equal(t, es.Unrestricted, tc.expectUnrestricted)
		})
	}
}

func TestExternalServiceStore_ListRepos(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	t.Parallel()
	logger := logtest.Scoped(t)
	db := NewDB(logger, dbtest.NewDB(logger, t))
	ctx := context.Background()

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      extsvc.NewUnencryptedConfig(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`),
	}
	err := db.ExternalServices().Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	// create new user
	user, err := db.Users().Create(ctx,
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

	// create new org
	displayName := "Acme org"
	org, err := db.Orgs().Create(ctx, "acme", &displayName)
	if err != nil {
		t.Fatal(err)
	}

	const repoId = 1
	err = db.Repos().Create(ctx, &types.Repo{ID: repoId, Name: "test1"})
	if err != nil {
		t.Fatal(err)
	}

	_, err = db.Handle().ExecContext(ctx, "INSERT INTO external_service_repos (external_service_id, repo_id, clone_url, user_id, org_id) VALUES ($1, $2, $3, $4, $5)",
		es.ID,
		repoId,
		"cloneUrl",
		user.ID,
		org.ID,
	)
	if err != nil {
		t.Fatal(err)
	}

	// check that repos are found with empty ExternalServiceReposListOptions
	haveRepos, err := db.ExternalServices().ListRepos(ctx, ExternalServiceReposListOptions{})
	if err != nil {
		t.Fatal(err)
	}

	if len(haveRepos) != 1 {
		t.Fatalf("Expected 1 external service repo, got %d", len(haveRepos))
	}

	have := haveRepos[0]

	require.Exactly(t, es.ID, have.ExternalServiceID, "externalServiceID is incorrect")
	require.Exactly(t, api.RepoID(repoId), have.RepoID, "repoID is incorrect")
	require.Exactly(t, "cloneUrl", have.CloneURL, "cloneURL is incorrect")
	require.Exactly(t, user.ID, have.UserID, "userID is incorrect")
	require.Exactly(t, org.ID, have.OrgID, "orgID is incorrect")

	// check that repos are found with given externalServiceID
	haveRepos, err = db.ExternalServices().ListRepos(ctx, ExternalServiceReposListOptions{ExternalServiceID: 1, LimitOffset: &LimitOffset{Limit: 1}})
	if err != nil {
		t.Fatal(err)
	}

	if len(haveRepos) != 1 {
		t.Fatalf("Expected 1 external service repo, got %d", len(haveRepos))
	}

	// check that repos are limited
	haveRepos, err = db.ExternalServices().ListRepos(ctx, ExternalServiceReposListOptions{ExternalServiceID: 1, LimitOffset: &LimitOffset{Limit: 0}})
	if err != nil {
		t.Fatal(err)
	}

	if len(haveRepos) != 0 {
		t.Fatalf("Expected 0 external service repos, got %d", len(haveRepos))
	}
}
