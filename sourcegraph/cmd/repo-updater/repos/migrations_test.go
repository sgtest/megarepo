package repos_test

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/sourcegraph/sourcegraph/cmd/repo-updater/repos"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/jsonc"
)

func TestGithubReposEnabledStateDeprecationMigration(t *testing.T) {
	testGithubReposEnabledStateDeprecationMigration(new(repos.FakeStore))(t)
}

func testGithubReposEnabledStateDeprecationMigration(store repos.Store) func(*testing.T) {
	githubDotCom := repos.ExternalService{
		ID:          1,
		Kind:        "GITHUB",
		DisplayName: "github.com - test",
		Config: formatJSON(`
			{
				// Some comment
				"url": "https://github.com",
				"token": "secret"
			}
		`),
	}

	githubDotComDuplicate :=
		githubDotCom.With(repos.Opt.ExternalServiceID(2))

	repo := repos.Repo{
		Name:    "github.com/foo/bar",
		Enabled: false,
		ExternalRepo: api.ExternalRepoSpec{
			ID:          "bar",
			ServiceType: "github",
			ServiceID:   "http://github.com",
		},
		Sources: map[string]*repos.SourceInfo{},
	}

	excluded := func(t testing.TB, rs ...*repos.Repo) func(*repos.ExternalService) {
		t.Helper()
		return func(e *repos.ExternalService) {
			if err := e.ExcludeGithubRepos(rs...); err != nil {
				t.Fatal(err)
			}
		}
	}

	included := func(t testing.TB, rs ...*repos.Repo) func(*repos.ExternalService) {
		t.Helper()
		return func(e *repos.ExternalService) {
			if err := e.IncludeGithubRepos(rs...); err != nil {
				t.Fatal(err)
			}
		}
	}

	clock := repos.NewFakeClock(time.Now(), 0)
	now := clock.Now()

	return func(t *testing.T) {
		t.Helper()

		for _, tc := range []struct {
			name    string
			sourcer repos.Sourcer
			stored  repos.Repos
			assert  repos.ExternalServicesAssertion
			err     string
		}{
			{
				name:   "disabled: was deleted, got added, then excluded",
				stored: repos.Repos{repo.With(repos.Opt.RepoDeletedAt(now))},
				sourcer: repos.NewFakeSourcer(nil,
					repos.NewFakeSource(githubDotCom.Clone(), nil, repo.Clone()),
				),
				assert: repos.Assert.ExternalServicesEqual(githubDotCom.With(
					repos.Opt.ExternalServiceModifiedAt(now),
					excluded(t, &repo),
				)),
				err: "<nil>",
			},
			{
				name:   "disabled: was not deleted and was not modified, got excluded",
				stored: repos.Repos{&repo},
				sourcer: repos.NewFakeSourcer(nil,
					repos.NewFakeSource(githubDotCom.Clone(), nil, repo.Clone()),
				),
				assert: repos.Assert.ExternalServicesEqual(githubDotCom.With(
					repos.Opt.ExternalServiceModifiedAt(now),
					excluded(t, &repo),
				)),
				err: "<nil>",
			},
			{
				name:   "disabled: was not deleted, got modified, then excluded",
				stored: repos.Repos{&repo},
				sourcer: repos.NewFakeSourcer(nil, repos.NewFakeSource(githubDotCom.Clone(), nil,
					repo.With(func(r *repos.Repo) {
						r.Description = "some updated description"
					})),
				),
				assert: repos.Assert.ExternalServicesEqual(githubDotCom.With(
					repos.Opt.ExternalServiceModifiedAt(now),
					excluded(t, &repo),
				)),
				err: "<nil>",
			},
			{
				name:    "disabled: was deleted and is still deleted, got excluded",
				stored:  repos.Repos{repo.With(repos.Opt.RepoDeletedAt(now))},
				sourcer: repos.NewFakeSourcer(nil, repos.NewFakeSource(githubDotCom.Clone(), nil)),
				assert: repos.Assert.ExternalServicesEqual(githubDotCom.With(
					repos.Opt.ExternalServiceModifiedAt(now),
					excluded(t, &repo),
				)),
				err: "<nil>",
			},
			{
				name: "enabled: was not deleted and is still not deleted, not included",
				stored: repos.Repos{repo.With(func(r *repos.Repo) {
					r.Enabled = true
				})},
				sourcer: repos.NewFakeSourcer(nil, repos.NewFakeSource(githubDotCom.Clone(), nil,
					repo.With(func(r *repos.Repo) {
						r.Enabled = true
					})),
				),
				assert: repos.Assert.ExternalServicesEqual(githubDotCom.Clone()),
				err:    "<nil>",
			},
			{
				name: "enabled: was not deleted and got deleted, then included",
				stored: repos.Repos{repo.With(func(r *repos.Repo) {
					r.Enabled = true
				})},
				sourcer: repos.NewFakeSourcer(nil, repos.NewFakeSource(githubDotCom.Clone(), nil)),
				assert: repos.Assert.ExternalServicesEqual(githubDotCom.With(
					repos.Opt.ExternalServiceModifiedAt(now),
					included(t, &repo),
				)),
				err: "<nil>",
			},
			{
				name:   "enabled: got added for the first time, so not included",
				stored: repos.Repos{},
				sourcer: repos.NewFakeSourcer(nil, repos.NewFakeSource(githubDotCom.Clone(), nil,
					repo.With(func(r *repos.Repo) {
						r.Enabled = true
					})),
				),
				assert: repos.Assert.ExternalServicesEqual(githubDotCom.Clone()),
				err:    "<nil>",
			},
			{
				name: "initialRepositoryEnablement gets deleted",
				sourcer: repos.NewFakeSourcer(nil, repos.NewFakeSource(
					githubDotCom.With(func(e *repos.ExternalService) {
						e.Config = formatJSON(`
						{
							// Some comment
							"url": "https://github.com",
							"token": "secret",
							"initialRepositoryEnablement": false
						}`)
					}), nil,
				)),
				assert: repos.Assert.ExternalServicesEqual(githubDotCom.With(
					repos.Opt.ExternalServiceModifiedAt(now),
				)),
				err: "<nil>",
			},
			{
				name:   "disabled: repo is excluded in all of its sources",
				stored: repos.Repos{repo.With(repos.Opt.RepoDeletedAt(now))},
				sourcer: repos.NewFakeSourcer(nil,
					repos.NewFakeSource(githubDotCom.Clone(), nil, repo.Clone()),
					repos.NewFakeSource(githubDotComDuplicate.Clone(), nil, repo.Clone()),
				),
				assert: repos.Assert.ExternalServicesEqual(
					githubDotCom.With(
						repos.Opt.ExternalServiceModifiedAt(now),
						excluded(t, &repo),
					),
					githubDotComDuplicate.With(
						repos.Opt.ExternalServiceModifiedAt(now),
						excluded(t, &repo),
					),
				),
				err: "<nil>",
			},
			{
				name: "enabled: repo is included in all of its sources",
				stored: repos.Repos{repo.With(
					repos.Opt.RepoSources(githubDotCom.URN(), githubDotComDuplicate.URN()),
					func(r *repos.Repo) { r.Enabled = true },
				)},
				sourcer: repos.NewFakeSourcer(nil,
					repos.NewFakeSource(githubDotCom.Clone(), nil),
					repos.NewFakeSource(githubDotComDuplicate.Clone(), nil),
				),
				assert: repos.Assert.ExternalServicesEqual(
					githubDotCom.With(
						repos.Opt.ExternalServiceModifiedAt(now),
						included(t, &repo),
					),
					githubDotComDuplicate.With(
						repos.Opt.ExternalServiceModifiedAt(now),
						included(t, &repo),
					),
				),
				err: "<nil>",
			},
		} {
			tc := tc
			ctx := context.Background()

			t.Run(tc.name, transact(ctx, store, func(t testing.TB, tx repos.Store) {
				if err := tx.UpsertRepos(ctx, tc.stored.Clone()...); err != nil {
					t.Errorf("failed to prepare store: %v", err)
					return
				}

				err := repos.GithubReposEnabledStateDeprecationMigration(tc.sourcer, clock.Now).Run(ctx, tx)
				if have, want := fmt.Sprint(err), tc.err; have != want {
					t.Errorf("error:\nhave: %v\nwant: %v", have, want)
					return
				}

				svcs, err := tx.ListExternalServices(ctx)
				if err != nil {
					t.Error(err)
					return
				}

				if tc.assert != nil {
					tc.assert(t, svcs)
				}
			}))
		}

	}
}

func TestGithubSetDefaultRepositoryQueryMigration(t *testing.T) {
	t.Parallel()
	testGithubSetDefaultRepositoryQueryMigration(new(repos.FakeStore))(t)
}

func testGithubSetDefaultRepositoryQueryMigration(store repos.Store) func(*testing.T) {
	githubDotCom := repos.ExternalService{
		Kind:        "GITHUB",
		DisplayName: "Github.com - Test",
		Config: formatJSON(`
			{
				// Some comment
				"url": "https://github.com"
			}
		`),
	}

	githubNone := repos.ExternalService{
		Kind:        "GITHUB",
		DisplayName: "Github.com - Test",
		Config: formatJSON(`
			{
				// Some comment
				"url": "https://github.com",
				"repositoryQuery": ["none"]
			}
		`),
	}

	githubEnterprise := repos.ExternalService{
		Kind:        "GITHUB",
		DisplayName: "Github Enterprise - Test",
		Config: formatJSON(`
			{
				// Some comment
				"url": "https://github.mycorp.com"
			}
		`),
	}

	gitlab := repos.ExternalService{
		Kind:        "GITLAB",
		DisplayName: "Gitlab - Test",
		Config:      formatJSON(`{"url": "https://gitlab.com"}`),
	}

	clock := repos.NewFakeClock(time.Now(), 0)

	return func(t *testing.T) {
		t.Helper()

		for _, tc := range []struct {
			name   string
			stored repos.ExternalServices
			assert repos.ExternalServicesAssertion
			err    string
		}{
			{
				name:   "no external services",
				stored: repos.ExternalServices{},
				assert: repos.Assert.ExternalServicesEqual(),
				err:    "<nil>",
			},
			{
				name:   "non-github services are left unchanged",
				stored: repos.ExternalServices{&gitlab},
				assert: repos.Assert.ExternalServicesEqual(&gitlab),
				err:    "<nil>",
			},
			{
				name:   "github services with repositoryQuery set are left unchanged",
				stored: repos.ExternalServices{&githubNone},
				assert: repos.Assert.ExternalServicesEqual(&githubNone),
				err:    "<nil>",
			},
			{
				name:   "github.com services are set to affiliated",
				stored: repos.ExternalServices{&githubDotCom},
				assert: repos.Assert.ExternalServicesEqual(
					githubDotCom.With(
						repos.Opt.ExternalServiceModifiedAt(clock.Time(0)),
						func(e *repos.ExternalService) {
							e.Config = formatJSON(`
								{
									// Some comment
									"url": "https://github.com",
									"repositoryQuery": ["affiliated"]
								}
							`)
						},
					),
				),
				err: "<nil>",
			},
			{
				name:   "github enterprise services are set to public and affiliated",
				stored: repos.ExternalServices{&githubEnterprise},
				assert: repos.Assert.ExternalServicesEqual(
					githubEnterprise.With(
						repos.Opt.ExternalServiceModifiedAt(clock.Time(0)),
						func(e *repos.ExternalService) {
							e.Config = formatJSON(`
								{
									// Some comment
									"url": "https://github.mycorp.com",
									"repositoryQuery": ["affiliated", "public"]
								}
							`)
						},
					),
				),
				err: "<nil>",
			},
		} {
			tc := tc
			ctx := context.Background()

			t.Run(tc.name, transact(ctx, store, func(t testing.TB, tx repos.Store) {
				if err := tx.UpsertExternalServices(ctx, tc.stored.Clone()...); err != nil {
					t.Errorf("failed to prepare store: %v", err)
					return
				}

				err := repos.GithubSetDefaultRepositoryQueryMigration(clock.Now).Run(ctx, tx)
				if have, want := fmt.Sprint(err), tc.err; have != want {
					t.Errorf("error:\nhave: %v\nwant: %v", have, want)
				}

				es, err := tx.ListExternalServices(ctx)
				if err != nil {
					t.Error(err)
					return
				}

				if tc.assert != nil {
					tc.assert(t, es)
				}
			}))
		}

	}
}

func TestGitLabSetDefaultProjectQueryMigration(t *testing.T) {
	t.Parallel()
	testGitLabSetDefaultProjectQueryMigration(new(repos.FakeStore))(t)
}

func testGitLabSetDefaultProjectQueryMigration(store repos.Store) func(*testing.T) {
	github := repos.ExternalService{
		Kind:        "GITHUB",
		DisplayName: "Github.com - Test",
		Config: formatJSON(`
			{
				// Some comment
				"url": "https://github.com"
			}
		`),
	}

	gitlabNone := repos.ExternalService{
		Kind:        "GITLAB",
		DisplayName: "Gitlab - Test",
		Config: formatJSON(`
			{
				// Some comment
				"url": "https://gitlab.com",
				"projectQuery": ["none"]
			}
		`),
	}

	gitlab := repos.ExternalService{
		Kind:        "GITLAB",
		DisplayName: "Gitlab - Test",
		Config: formatJSON(`
			{
				// Some comment
				"url": "https://gitlab.com"
			}
		`),
	}

	clock := repos.NewFakeClock(time.Now(), 0)

	return func(t *testing.T) {
		t.Helper()

		for _, tc := range []struct {
			name   string
			stored repos.ExternalServices
			assert repos.ExternalServicesAssertion
			err    string
		}{
			{
				name:   "no external services",
				stored: repos.ExternalServices{},
				assert: repos.Assert.ExternalServicesEqual(),
				err:    "<nil>",
			},
			{
				name:   "non-gitlab services are left unchanged",
				stored: repos.ExternalServices{&github},
				assert: repos.Assert.ExternalServicesEqual(&github),
				err:    "<nil>",
			},
			{
				name:   "gitlab services with projectQuery set are left unchanged",
				stored: repos.ExternalServices{&gitlabNone},
				assert: repos.Assert.ExternalServicesEqual(&gitlabNone),
				err:    "<nil>",
			},
			{
				name:   "gitlab services are set to ?membership=true",
				stored: repos.ExternalServices{&gitlab},
				assert: repos.Assert.ExternalServicesEqual(
					gitlab.With(
						repos.Opt.ExternalServiceModifiedAt(clock.Time(0)),
						func(e *repos.ExternalService) {
							e.Config = formatJSON(`
								{
									// Some comment
									"url": "https://gitlab.com",
									"projectQuery": ["?membership=true"]
								}
							`)
						},
					),
				),
				err: "<nil>",
			},
		} {
			tc := tc
			ctx := context.Background()

			t.Run(tc.name, transact(ctx, store, func(t testing.TB, tx repos.Store) {
				if err := tx.UpsertExternalServices(ctx, tc.stored.Clone()...); err != nil {
					t.Errorf("failed to prepare store: %v", err)
					return
				}

				err := repos.GitLabSetDefaultProjectQueryMigration(clock.Now).Run(ctx, tx)
				if have, want := fmt.Sprint(err), tc.err; have != want {
					t.Errorf("error:\nhave: %v\nwant: %v", have, want)
				}

				es, err := tx.ListExternalServices(ctx)
				if err != nil {
					t.Error(err)
					return
				}

				if tc.assert != nil {
					tc.assert(t, es)
				}
			}))
		}

	}
}

func formatJSON(s string) string {
	formatted, err := jsonc.Format(s, true, 2)
	if err != nil {
		panic(err)
	}
	return formatted
}
