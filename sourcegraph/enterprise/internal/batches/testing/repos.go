package testing

import (
	"context"
	"database/sql"
	"fmt"
	"strings"
	"testing"
	"time"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/bitbucketserver"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitlab"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestRepo(t *testing.T, store *database.ExternalServiceStore, serviceKind string) *types.Repo {
	t.Helper()

	clock := timeutil.NewFakeClock(time.Now(), 0)
	now := clock.Now()

	svc := types.ExternalService{
		Kind:        serviceKind,
		DisplayName: serviceKind + " - Test",
		Config:      `{"url": "https://github.com", "authorization": {}}`,
		CreatedAt:   now,
		UpdatedAt:   now,
	}

	if err := store.Upsert(context.Background(), &svc); err != nil {
		t.Fatalf("failed to insert external services: %v", err)
	}

	repo := TestRepoWithService(t, store, fmt.Sprintf("repo-%d", svc.ID), &svc)

	repo.Sources[svc.URN()].CloneURL = "https://secrettoken@github.com/sourcegraph/sourcegraph"
	return repo
}

func TestRepoWithService(t *testing.T, store *database.ExternalServiceStore, name string, svc *types.ExternalService) *types.Repo {
	t.Helper()

	return &types.Repo{
		Name:    api.RepoName(name),
		URI:     name,
		Private: true,
		ExternalRepo: api.ExternalRepoSpec{
			ID:          fmt.Sprintf("external-id-%s", name),
			ServiceType: extsvc.KindToType(svc.Kind),
			ServiceID:   fmt.Sprintf("https://%s.com/", strings.ToLower(svc.Kind)),
		},
		Sources: map[string]*types.SourceInfo{
			svc.URN(): {
				ID: svc.URN(),
			},
		},
	}
}

func CreateTestRepos(t *testing.T, ctx context.Context, db dbutil.DB, count int) ([]*types.Repo, *types.ExternalService) {
	t.Helper()

	repoStore := database.Repos(db)
	esStore := database.ExternalServices(db)

	ext := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GitHub",
		Config: MarshalJSON(t, &schema.GitHubConnection{
			Url:             "https://github.com",
			Token:           "SECRETTOKEN",
			RepositoryQuery: []string{"none"},
			// This field is needed to enforce permissions
			Authorization: &schema.GitHubAuthorization{},
		}),
	}

	confGet := func() *conf.Unified {
		return &conf.Unified{}
	}

	if err := esStore.Create(ctx, confGet, ext); err != nil {
		t.Fatal(err)
	}

	var rs []*types.Repo
	for i := 0; i < count; i++ {
		r := TestRepoWithService(t, esStore, fmt.Sprintf("repo-%d-%d", ext.ID, i+1), ext)
		r.Metadata = &github.Repository{
			NameWithOwner: string(r.Name),
			URL:           fmt.Sprintf("https://github.com/sourcegraph/%s", string(r.Name)),
		}

		rs = append(rs, r)
	}

	err := repoStore.Create(ctx, rs...)
	if err != nil {
		t.Fatal(err)
	}

	return rs, ext
}

func CreateGitlabTestRepos(t *testing.T, ctx context.Context, db *sql.DB, count int) ([]*types.Repo, *types.ExternalService) {
	t.Helper()

	repoStore := database.Repos(db)
	esStore := database.ExternalServices(db)

	ext := &types.ExternalService{
		Kind:        extsvc.KindGitLab,
		DisplayName: "GitLab",
		Config: MarshalJSON(t, &schema.GitLabConnection{
			Url:   "https://gitlab.com",
			Token: "SECRETTOKEN",
		}),
	}
	if err := esStore.Upsert(ctx, ext); err != nil {
		t.Fatal(err)
	}

	var rs []*types.Repo
	for i := 0; i < count; i++ {
		r := TestRepoWithService(t, esStore, fmt.Sprintf("repo-%d-%d", ext.ID, i+1), ext)
		r.Metadata = &gitlab.Project{
			ProjectCommon: gitlab.ProjectCommon{
				HTTPURLToRepo: fmt.Sprintf("https://gitlab.com/sourcegraph/%s", string(r.Name)),
			},
		}

		rs = append(rs, r)
	}

	err := repoStore.Create(ctx, rs...)
	if err != nil {
		t.Fatal(err)
	}

	return rs, ext
}

func CreateBbsTestRepos(t *testing.T, ctx context.Context, db *sql.DB, count int) ([]*types.Repo, *types.ExternalService) {
	t.Helper()

	ext := &types.ExternalService{
		Kind:        extsvc.KindBitbucketServer,
		DisplayName: "Bitbucket Server",
		Config: MarshalJSON(t, &schema.BitbucketServerConnection{
			Url:   "https://bitbucket.sourcegraph.com",
			Token: "SECRETTOKEN",
		}),
	}

	return createBbsRepos(t, ctx, db, ext, count, "https://bbs-user:bbs-token@bitbucket.sourcegraph.com/scm")
}

func CreateGitHubSSHTestRepos(t *testing.T, ctx context.Context, db dbutil.DB, count int) ([]*types.Repo, *types.ExternalService) {
	t.Helper()

	ext := &types.ExternalService{
		Kind:        extsvc.KindGitHub,
		DisplayName: "GitHub SSH",
		Config: MarshalJSON(t, &schema.GitHubConnection{
			Url:        "https://github.com",
			Token:      "SECRETTOKEN",
			GitURLType: "ssh",
		}),
	}
	esStore := database.ExternalServices(db)
	if err := esStore.Upsert(ctx, ext); err != nil {
		t.Fatal(err)
	}

	var rs []*types.Repo
	for i := 0; i < count; i++ {
		r := TestRepo(t, esStore, extsvc.KindGitHub)
		r.Sources = map[string]*types.SourceInfo{ext.URN(): {
			ID:       ext.URN(),
			CloneURL: "git@github.com:" + string(r.Name) + ".git",
		}}

		rs = append(rs, r)
	}

	err := database.Repos(db).Create(ctx, rs...)
	if err != nil {
		t.Fatal(err)
	}
	return rs, nil
}

func CreateBbsSSHTestRepos(t *testing.T, ctx context.Context, db dbutil.DB, count int) ([]*types.Repo, *types.ExternalService) {
	t.Helper()

	ext := &types.ExternalService{
		Kind:        extsvc.KindBitbucketServer,
		DisplayName: "Bitbucket Server SSH",
		Config: MarshalJSON(t, &schema.BitbucketServerConnection{
			Url:        "https://bitbucket.sgdev.org",
			Token:      "SECRETTOKEN",
			GitURLType: "ssh",
		}),
	}

	return createBbsRepos(t, ctx, db, ext, count, "ssh://git@bitbucket.sgdev.org:7999")
}

func createBbsRepos(t *testing.T, ctx context.Context, db dbutil.DB, ext *types.ExternalService, count int, cloneBaseURL string) ([]*types.Repo, *types.ExternalService) {
	t.Helper()

	repoStore := database.Repos(db)
	esStore := database.ExternalServices(db)

	if err := esStore.Upsert(ctx, ext); err != nil {
		t.Fatal(err)
	}

	var rs []*types.Repo
	for i := 0; i < count; i++ {
		r := TestRepoWithService(t, esStore, fmt.Sprintf("repo-%d-%d", ext.ID, i+1), ext)
		var metadata bitbucketserver.Repo
		urlType := "http"
		if strings.HasPrefix(cloneBaseURL, "ssh") {
			urlType = "ssh"
		}
		metadata.Links.Clone = append(metadata.Links.Clone, struct {
			Href string "json:\"href\""
			Name string "json:\"name\""
		}{
			Name: urlType,
			Href: cloneBaseURL + "/" + string(r.Name),
		})
		r.Metadata = &metadata
		rs = append(rs, r)
	}

	err := repoStore.Create(ctx, rs...)
	if err != nil {
		t.Fatal(err)
	}

	return rs, ext
}
