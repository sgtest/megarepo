package repos_test

import (
	"context"
	"fmt"
	"testing"
	"time"

	"github.com/sourcegraph/jsonx"
	"github.com/sourcegraph/sourcegraph/cmd/repo-updater/repos"
)

func TestGithubSetDefaultRepositoryQueryMigration(t *testing.T) {
	t.Parallel()
	testGithubSetDefaultRepositoryQueryMigration(new(repos.FakeStore))(t)
}

func testGithubSetDefaultRepositoryQueryMigration(store repos.Store) func(*testing.T) {
	githubDotCom := repos.ExternalService{
		Kind:        "GITHUB",
		DisplayName: "Github.com - Test",
		Config: jsonFormat(`
			{
				// Some comment
				"url": "https://github.com"
			}
		`),
	}

	githubNone := repos.ExternalService{
		Kind:        "GITHUB",
		DisplayName: "Github.com - Test",
		Config: jsonFormat(`
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
		Config: jsonFormat(`
			{
				// Some comment
				"url": "https://github.mycorp.com"
			}
		`),
	}

	gitlab := repos.ExternalService{
		Kind:        "GITLAB",
		DisplayName: "Gitlab - Test",
		Config:      jsonFormat(`{"url": "https://gitlab.com"}`),
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
							e.Config = jsonFormat(`
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
							e.Config = jsonFormat(`
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

func jsonFormat(s string) string {
	opts := jsonx.FormatOptions{
		InsertSpaces: true,
		TabSize:      2,
	}

	formatted, err := jsonx.ApplyEdits(s, jsonx.Format(s, opts)...)
	if err != nil {
		panic(err)
	}

	return formatted
}
