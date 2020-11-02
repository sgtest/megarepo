package db

import (
	"context"
	"fmt"
	"sort"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/google/go-cmp/cmp/cmpopts"
	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/db/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/db/dbtesting"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
)

func TestExternalServicesListOptions_sqlConditions(t *testing.T) {
	tests := []struct {
		name            string
		noNamespace     bool
		namespaceUserID int32
		kinds           []string
		afterID         int64
		wantQuery       string
		wantArgs        []interface{}
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
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			opts := ExternalServicesListOptions{
				NoNamespace:     test.noNamespace,
				NamespaceUserID: test.namespaceUserID,
				Kinds:           test.kinds,
				AfterID:         test.afterID,
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
	tests := []struct {
		name         string
		kind         string
		config       string
		hasNamespace bool
		setup        func(t *testing.T)
		wantErr      string
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
			name:         "prevent code hosts that are not allowed",
			kind:         extsvc.KindGitHub,
			config:       `{"url": "https://github.example.com", "repositoryQuery": ["none"], "token": "abc"}`,
			hasNamespace: true,
			wantErr:      `users are only allowed to add external service for https://github.com/, https://gitlab.com/ and https://bitbucket.org/`,
		},
		{
			name:         "prevent disallowed fields",
			kind:         extsvc.KindGitHub,
			config:       `{"url": "https://github.com", "repositoryPathPattern": "github/{nameWithOwner}" // comments}`,
			hasNamespace: true,
			wantErr:      `field "repositoryPathPattern" is not allowed in a user-added external service`,
		},
	}
	for _, test := range tests {
		t.Run(test.name, func(t *testing.T) {
			if test.setup != nil {
				test.setup(t)
			}

			err := ExternalServices.ValidateConfig(context.Background(), ValidateExternalServiceConfigOptions{
				Kind:         test.kind,
				Config:       test.config,
				HasNamespace: test.hasNamespace,
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
	dbtesting.SetupGlobalTestDB(t)
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
	err := (&ExternalServiceStore{}).Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	// Should get back the same one
	got, err := (&ExternalServiceStore{}).GetByID(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	if diff := cmp.Diff(es, got); diff != "" {
		t.Fatalf("(-want +got):\n%s", diff)
	}
}

func TestExternalServicesStore_CreateWithTierEnforcement(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)

	ctx := context.Background()
	confGet := func() *conf.Unified { return &conf.Unified{} }
	es := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GITHUB #1",
		Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
	}
	store := &ExternalServiceStore{
		PreCreateExternalService: func(ctx context.Context) error {
			return errcode.NewPresentationError("test plan limit exceeded")
		},
	}
	if err := store.Create(ctx, confGet, es); err == nil {
		t.Fatal("expected an error, got none")
	}
}

func TestExternalServicesStore_Update(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
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
	err := (&ExternalServiceStore{}).Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	// Update its name and config
	esUpdate := &ExternalServiceUpdate{
		DisplayName: strptr("GITHUB (updated) #1"),
		Config:      strptr(`{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def"}`),
	}
	err = (&ExternalServiceStore{}).Update(ctx, nil, es.ID, esUpdate)
	if err != nil {
		t.Fatal(err)
	}

	// Get and verify update
	got, err := (&ExternalServiceStore{}).GetByID(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	if diff := cmp.Diff(*esUpdate.DisplayName, got.DisplayName); diff != "" {
		t.Fatalf("DisplayName mismatch (-want +got):\n%s", diff)
	} else if diff = cmp.Diff(*esUpdate.Config, got.Config); diff != "" {
		t.Fatalf("Config mismatch (-want +got):\n%s", diff)
	} else if got.UpdatedAt.Equal(es.UpdatedAt) {
		t.Fatalf("UpdateAt: want to be updated but not")
	}
}

func TestExternalServicesStore_Delete(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
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
	err := ExternalServices.Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	// Create two repositories to test trigger of soft-deleting external service:
	//  - ID=1 is expected to be deleted along with deletion of the external service.
	//  - ID=2 remains untouched because it is not associated with the external service.
	_, err = dbconn.Global.ExecContext(ctx, `
INSERT INTO repo (id, name, description, fork)
VALUES (1, 'github.com/user/repo', '', FALSE);
INSERT INTO repo (id, name, description, fork)
VALUES (2, 'github.com/user/repo2', '', FALSE);
`)
	if err != nil {
		t.Fatal(err)
	}

	// Insert a row to `external_service_repos` table to test the trigger.
	q := sqlf.Sprintf(`
INSERT INTO external_service_repos (external_service_id, repo_id, clone_url)
VALUES (%d, 1, '')
`, es.ID)
	_, err = dbconn.Global.ExecContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
	if err != nil {
		t.Fatal(err)
	}

	// Delete this external service
	err = ExternalServices.Delete(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	// Delete again should get externalServiceNotFoundError
	err = ExternalServices.Delete(ctx, es.ID)
	gotErr := fmt.Sprintf("%v", err)
	wantErr := fmt.Sprintf("external service not found: %v", es.ID)
	if gotErr != wantErr {
		t.Errorf("error: want %q but got %q", wantErr, gotErr)
	}

	// Should only get back the repo with ID=2
	repos, err := Repos.GetByIDs(ctx, 1, 2)
	if err != nil {
		t.Fatal(err)
	}

	want := []*types.Repo{
		{ID: 2, Name: "github.com/user/repo2"},
	}
	if diff := cmp.Diff(want, repos); diff != "" {
		t.Fatalf("Repos mismatch (-want +got):\n%s", diff)
	}
}

func TestExternalServicesStore_GetByID(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
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
	err := (&ExternalServiceStore{}).Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	// Should be able to get back by its ID
	_, err = (&ExternalServiceStore{}).GetByID(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	// Delete this external service
	err = (&ExternalServiceStore{}).Delete(ctx, es.ID)
	if err != nil {
		t.Fatal(err)
	}

	// Should now get externalServiceNotFoundError
	_, err = (&ExternalServiceStore{}).GetByID(ctx, es.ID)
	gotErr := fmt.Sprintf("%v", err)
	wantErr := fmt.Sprintf("external service not found: %v", es.ID)
	if gotErr != wantErr {
		t.Errorf("error: want %q but got %q", wantErr, gotErr)
	}
}

func TestExternalServicesStore_List(t *testing.T) {
	if testing.Short() {
		t.Skip()
	}
	dbtesting.SetupGlobalTestDB(t)
	ctx := context.Background()

	// Create test user
	user, err := Users.Create(ctx, NewUser{
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
			Config:          `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "abc"}`,
			NamespaceUserID: &user.ID,
		},
		{
			Kind:        extsvc.KindGitHub,
			DisplayName: "GITHUB #2",
			Config:      `{"url": "https://github.com", "repositoryQuery": ["none"], "token": "def"}`,
		},
	}
	for _, es := range ess {
		err := ExternalServices.Create(ctx, confGet, es)
		if err != nil {
			t.Fatal(err)
		}
	}

	t.Run("list all external services", func(t *testing.T) {
		got, err := (&ExternalServiceStore{}).List(ctx, ExternalServicesListOptions{})
		if err != nil {
			t.Fatal(err)
		}
		sort.Slice(got, func(i, j int) bool { return got[i].ID < got[j].ID })

		if diff := cmp.Diff(ess, got); diff != "" {
			t.Fatalf("Mismatch (-want +got):\n%s", diff)
		}
	})

	t.Run("list external services with certain IDs", func(t *testing.T) {
		got, err := (&ExternalServiceStore{}).List(ctx, ExternalServicesListOptions{
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
		got, err := (&ExternalServiceStore{}).List(ctx, ExternalServicesListOptions{
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
		got, err := (&ExternalServiceStore{}).List(ctx, ExternalServicesListOptions{
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
		ess, err := (&ExternalServiceStore{}).List(ctx, ExternalServicesListOptions{
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
	dbtesting.SetupGlobalTestDB(t)
	ctx := context.Background()

	t.Run("no external service won't blow up", func(t *testing.T) {
		kinds, err := ExternalServices.DistinctKinds(ctx)
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
		err := ExternalServices.Create(ctx, confGet, es)
		if err != nil {
			t.Fatal(err)
		}
	}

	// Delete the last external service which should be excluded from the result
	err := ExternalServices.Delete(ctx, ess[3].ID)
	if err != nil {
		t.Fatal(err)
	}

	kinds, err := ExternalServices.DistinctKinds(ctx)
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
	dbtesting.SetupGlobalTestDB(t)
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
	err := (&ExternalServiceStore{}).Create(ctx, confGet, es)
	if err != nil {
		t.Fatal(err)
	}

	count, err := (&ExternalServiceStore{}).Count(ctx, ExternalServicesListOptions{})
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
	dbtesting.SetupGlobalTestDB(t)
	ctx := context.Background()

	clock := NewFakeClock(time.Now(), 0)

	var svcs types.ExternalServices
	for _, svc := range createExternalServices(t) {
		svcs = append(svcs, svc)
	}
	sort.Sort(svcs)

	t.Run("no external services", func(t *testing.T) {
		if err := ExternalServices.Upsert(ctx); err != nil {
			t.Fatalf("Upsert error: %s", err)
		}
	})

	t.Run("many external services", func(t *testing.T) {
		tx, err := ExternalServices.Transact(ctx)
		if err != nil {
			t.Fatalf("Transact error: %s", err)
		}
		defer func() {
			err = tx.Done(err)
			if err != nil {
				t.Fatalf("Done error: %s", err)
			}
		}()

		want := generateExternalServices(7, svcs...)
		sort.Sort(want)

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
			e.DeletedAt = &now
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

func createExternalServices(t *testing.T) map[string]*types.ExternalService {
	clock := NewFakeClock(time.Now(), 0)
	now := clock.Now()

	svcs := mkExternalServices(now)

	// Create a new external service
	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}

	// create a few external services
	for _, svc := range svcs {
		if err := ExternalServices.Create(context.Background(), confGet, svc); err != nil {
			t.Fatalf("failed to insert external service %v: %v", svc.DisplayName, err)
		}
	}

	services, err := ExternalServices.List(context.Background(), ExternalServicesListOptions{})
	if err != nil {
		t.Fatal("failed to list external services")
	}

	servicesPerKind := make(map[string]*types.ExternalService)
	for _, svc := range services {
		servicesPerKind[svc.Kind] = svc
	}

	return servicesPerKind
}

func generateExternalServices(n int, base ...*types.ExternalService) types.ExternalServices {
	if len(base) == 0 {
		return nil
	}
	es := make(types.ExternalServices, 0, n)
	for i := 0; i < n; i++ {
		id := strconv.Itoa(i)
		r := base[i%len(base)].Clone()
		r.DisplayName += id
		es = append(es, r)
	}
	return es
}

func mkExternalServices(now time.Time) []*types.ExternalService {
	githubSvc := types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "Github - Test",
		Config:      `{"url": "https://github.com", "token": "abc", "repositoryQuery": ["none"]}`,
		CreatedAt:   now,
		UpdatedAt:   now,
	}

	gitlabSvc := types.ExternalService{
		Kind:        extsvc.KindGitLab,
		DisplayName: "GitLab - Test",
		Config:      `{"url": "https://gitlab.com", "token": "abc", "projectQuery": ["projects?membership=true&archived=no"]}`,
		CreatedAt:   now,
		UpdatedAt:   now,
	}

	bitbucketServerSvc := types.ExternalService{
		Kind:        extsvc.KindBitbucketServer,
		DisplayName: "Bitbucket Server - Test",
		Config:      `{"url": "https://bitbucket.com", "username": "foo", "token": "abc", "repositoryQuery": ["none"]}`,
		CreatedAt:   now,
		UpdatedAt:   now,
	}

	bitbucketCloudSvc := types.ExternalService{
		Kind:        extsvc.KindBitbucketCloud,
		DisplayName: "Bitbucket Cloud - Test",
		Config:      `{"url": "https://bitbucket.com", "username": "foo", "appPassword": "abc"}`,
		CreatedAt:   now,
		UpdatedAt:   now,
	}

	awsSvc := types.ExternalService{
		Kind:        extsvc.KindAWSCodeCommit,
		DisplayName: "AWS Code - Test",
		Config:      `{"region": "eu-west-1", "accessKeyID": "key", "secretAccessKey": "secret", "gitCredentials": {"username": "foo", "password": "bar"}}`,
		CreatedAt:   now,
		UpdatedAt:   now,
	}

	otherSvc := types.ExternalService{
		Kind:        extsvc.KindOther,
		DisplayName: "Other - Test",
		Config:      `{"url": "https://other.com", "repos": ["none"]}`,
		CreatedAt:   now,
		UpdatedAt:   now,
	}

	gitoliteSvc := types.ExternalService{
		Kind:        extsvc.KindGitolite,
		DisplayName: "Gitolite - Test",
		Config:      `{"prefix": "foo", "host": "bar"}`,
		CreatedAt:   now,
		UpdatedAt:   now,
	}

	return []*types.ExternalService{
		&githubSvc,
		&gitlabSvc,
		&bitbucketServerSvc,
		&bitbucketCloudSvc,
		&awsSvc,
		&otherSvc,
		&gitoliteSvc,
	}
}
