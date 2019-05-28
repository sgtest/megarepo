package repos

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/dnaeon/go-vcr/cassette"
	"github.com/dnaeon/go-vcr/recorder"
	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/conf"
	"github.com/sourcegraph/sourcegraph/pkg/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/github"
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/phabricator"
	"github.com/sourcegraph/sourcegraph/pkg/httpcli"
	"github.com/sourcegraph/sourcegraph/pkg/httptestutil"
	"github.com/sourcegraph/sourcegraph/pkg/rcache"
	"github.com/sourcegraph/sourcegraph/schema"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

func TestOtherRepoName(t *testing.T) {
	for _, tc := range []struct {
		name string
		in   string
		out  string
	}{
		{"user and password elided", "https://user:pass@foo.bar/baz", "foo.bar/baz"},
		{"scheme elided", "https://user@foo.bar/baz", "foo.bar/baz"},
		{"raw query elided", "https://foo.bar/baz?secret_token=12345", "foo.bar/baz"},
		{"fragment elided", "https://foo.bar/baz#fragment", "foo.bar/baz"},
		{": replaced with -", "git://foo.bar/baz:bam", "foo.bar/baz-bam"},
		{"@ replaced with -", "ssh://foo.bar/baz@bam", "foo.bar/baz-bam"},
	} {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			cloneURL, err := url.Parse(tc.in)
			if err != nil {
				t.Fatal(err)
			}

			if have, want := otherRepoName(cloneURL), tc.out; have != want {
				t.Errorf("otherRepoName(%q):\nhave: %q\nwant: %q", tc.in, have, want)
			}
		})
	}
}

func TestNewSourcer(t *testing.T) {
	now := time.Now()

	github := ExternalService{
		Kind:        "GITHUB",
		DisplayName: "Github - Test",
		Config:      `{"url": "https://github.com"}`,
		CreatedAt:   now,
		UpdatedAt:   now,
	}

	gitlab := ExternalService{
		Kind:        "GITHUB",
		DisplayName: "Github - Test",
		Config:      `{"url": "https://github.com"}`,
		CreatedAt:   now,
		UpdatedAt:   now,
		DeletedAt:   now,
	}

	sources := func(es ...*ExternalService) (srcs []Source) {
		t.Helper()

		for _, e := range es {
			src, err := NewSource(e, nil)
			if err != nil {
				t.Fatal(err)
			}
			srcs = append(srcs, src)
		}

		return srcs
	}

	for _, tc := range []struct {
		name string
		svcs ExternalServices
		srcs Sources
		err  string
	}{
		{
			name: "deleted external services are excluded",
			svcs: ExternalServices{&github, &gitlab},
			srcs: sources(&github),
			err:  "<nil>",
		},
	} {
		tc := tc

		t.Run(tc.name, func(t *testing.T) {
			srcs, err := NewSourcer(nil)(tc.svcs...)
			if have, want := fmt.Sprint(err), tc.err; have != want {
				t.Errorf("error:\nhave: %q\nwant: %q", have, want)
			}

			have := srcs.ExternalServices()
			want := tc.srcs.ExternalServices()

			if !reflect.DeepEqual(have, want) {
				t.Errorf("sources:\n%s", cmp.Diff(have, want))
			}
		})
	}
}

func TestSources_ListRepos(t *testing.T) {
	// NOTE(tsenart): Updating this test's testdata with the `-update` flag requires
	// setting up a local instance of Bitbucket Server and populating it.
	// $ docker run \
	//    -v ~/.bitbucket/:/var/atlassian/application-data/bitbucket \
	//    --name="bitbucket"\
	//    -d -p 7990:7990 -p 7999:7999 \
	//    atlassian/bitbucket-server

	conf.Mock(&conf.Unified{
		ServiceConnections: conftypes.ServiceConnections{
			GitServers: []string{"127.0.0.1:3178"},
		},
	})
	defer conf.Mock(nil)

	type testCase struct {
		name   string
		ctx    context.Context
		svcs   ExternalServices
		assert func(*ExternalService) ReposAssertion
		err    string
	}

	var testCases []testCase
	{
		svcs := ExternalServices{
			{
				Kind: "GITHUB",
				Config: marshalJSON(t, &schema.GitHubConnection{
					Url:   "https://github.com",
					Token: os.Getenv("GITHUB_ACCESS_TOKEN"),
					RepositoryQuery: []string{
						"user:tsenart in:name patrol",
					},
					Repos: []string{"sourcegraph/sourcegraph"},
				}),
			},
			{
				Kind: "GITLAB",
				Config: marshalJSON(t, &schema.GitLabConnection{
					Url:   "https://gitlab.com",
					Token: os.Getenv("GITLAB_ACCESS_TOKEN"),
					ProjectQuery: []string{
						"?search=gokulkarthick",
					},
				}),
			},
			{
				Kind: "BITBUCKETSERVER",
				Config: marshalJSON(t, &schema.BitbucketServerConnection{
					Url:   "http://127.0.0.1:7990",
					Token: os.Getenv("BITBUCKET_SERVER_TOKEN"),
					RepositoryQuery: []string{
						"?visibility=private",
						"?visibility=public",
					},
				}),
			},
			{
				Kind: "GITOLITE",
				Config: marshalJSON(t, &schema.GitoliteConnection{
					Host: "ssh://git@127.0.0.1:2222",
				}),
			},
			{
				Kind: "AWSCODECOMMIT",
				Config: marshalJSON(t, &schema.AWSCodeCommitConnection{
					AccessKeyID:     getAWSEnv("AWS_ACCESS_KEY_ID"),
					SecretAccessKey: getAWSEnv("AWS_SECRET_ACCESS_KEY"),
					Region:          "us-west-1",
					GitCredentials: schema.AWSCodeCommitGitCredentials{
						Username: "git-username",
						Password: "git-password",
					},
				}),
			},
			{
				Kind: "OTHER",
				Config: marshalJSON(t, &schema.OtherExternalServiceConnection{
					Url: "https://github.com",
					Repos: []string{
						"google/go-cmp",
					},
				}),
			},
		}

		testCases = append(testCases, testCase{
			name: "yielded repos are always enabled",
			svcs: svcs,
			assert: func(e *ExternalService) ReposAssertion {
				return func(t testing.TB, rs Repos) {
					t.Helper()

					set := make(map[string]bool)
					for _, r := range rs {
						set[strings.ToUpper(r.ExternalRepo.ServiceType)] = true
						if !r.Enabled {
							t.Errorf("repo %q is not enabled", r.Name)
						}
					}

					if !set[e.Kind] {
						t.Errorf("external service of kind %q didn't yield any repos", e.Kind)
					}
				}
			},
			err: "<nil>",
		})
	}

	{
		svcs := ExternalServices{
			{
				Kind: "GITHUB",
				Config: marshalJSON(t, &schema.GitHubConnection{
					Url:   "https://github.com",
					Token: os.Getenv("GITHUB_ACCESS_TOKEN"),
					RepositoryQuery: []string{
						"user:tsenart in:name patrol",
					},
					Repos: []string{
						"sourcegraph/Sourcegraph",
						"tsenart/VEGETA",
					},
					Exclude: []*schema.ExcludedGitHubRepo{
						{Name: "tsenart/Vegeta"},
						{Id: "MDEwOlJlcG9zaXRvcnkxNTM2NTcyNDU="}, // tsenart/patrol ID
					},
				}),
			},
			{
				Kind: "GITLAB",
				Config: marshalJSON(t, &schema.GitLabConnection{
					Url:   "https://gitlab.com",
					Token: os.Getenv("GITLAB_ACCESS_TOKEN"),
					ProjectQuery: []string{
						"?search=gokulkarthick",
						"?search=dotfiles-vegetableman",
					},
					Exclude: []*schema.ExcludedGitLabProject{
						{Name: "gokulkarthick/gokulkarthick"},
						{Id: 7789240},
					},
				}),
			},
			{
				Kind: "BITBUCKETSERVER",
				Config: marshalJSON(t, &schema.BitbucketServerConnection{
					Url:   "http://127.0.0.1:7990",
					Token: os.Getenv("BITBUCKET_SERVER_TOKEN"),
					Repos: []string{
						"ORG/foo",
						"org/BAZ",
					},
					RepositoryQuery: []string{
						"?visibility=private",
					},
					Exclude: []*schema.ExcludedBitbucketServerRepo{
						{Name: "ORG/Foo"},   // test case insensitivity
						{Id: 3},             // baz id
						{Pattern: ".*/bar"}, // only matches org/bar
					},
				}),
			},
			{
				Kind: "AWSCODECOMMIT",
				Config: marshalJSON(t, &schema.AWSCodeCommitConnection{
					AccessKeyID:     getAWSEnv("AWS_ACCESS_KEY_ID"),
					SecretAccessKey: getAWSEnv("AWS_SECRET_ACCESS_KEY"),
					Region:          "us-west-1",
					GitCredentials: schema.AWSCodeCommitGitCredentials{
						Username: "git-username",
						Password: "git-password",
					},
					Exclude: []*schema.ExcludedAWSCodeCommitRepo{
						{Name: "stRIPE-gO"},
						{Id: "020a4751-0f46-4e19-82bf-07d0989b67dd"},                // ID of `test`
						{Name: "test2", Id: "2686d63d-bff4-4a3e-a94f-3e6df904238d"}, // ID of `test2`
					},
				}),
			},
			{
				Kind: "GITOLITE",
				Config: marshalJSON(t, &schema.GitoliteConnection{
					Prefix:    "gitolite.mycorp.com/",
					Host:      "ssh://git@127.0.0.1:2222",
					Blacklist: `gitolite\.mycorp\.com\/foo`,
					Exclude: []*schema.ExcludedGitoliteRepo{
						{Name: "bar"},
					},
				}),
			},
		}

		testCases = append(testCases, testCase{
			name: "excluded repos are never yielded",
			svcs: svcs,
			assert: func(s *ExternalService) ReposAssertion {
				return func(t testing.TB, rs Repos) {
					t.Helper()

					set := make(map[string]bool)
					var patterns []*regexp.Regexp

					c, err := s.Configuration()
					if err != nil {
						t.Fatal(err)
					}

					type excluded struct {
						name, id, pattern string
					}

					var ex []excluded
					switch cfg := c.(type) {
					case *schema.GitHubConnection:
						for _, e := range cfg.Exclude {
							ex = append(ex, excluded{name: e.Name, id: e.Id})
						}
					case *schema.GitLabConnection:
						for _, e := range cfg.Exclude {
							ex = append(ex, excluded{name: e.Name, id: strconv.Itoa(e.Id)})
						}
					case *schema.BitbucketServerConnection:
						for _, e := range cfg.Exclude {
							ex = append(ex, excluded{name: e.Name, id: strconv.Itoa(e.Id), pattern: e.Pattern})
						}
					case *schema.AWSCodeCommitConnection:
						for _, e := range cfg.Exclude {
							ex = append(ex, excluded{name: e.Name, id: e.Id})
						}
					case *schema.GitoliteConnection:
						ex = append(ex, excluded{pattern: cfg.Blacklist})
						for _, e := range cfg.Exclude {
							ex = append(ex, excluded{name: e.Name})
						}
					}

					if len(ex) == 0 {
						t.Fatal("exclude list must not be empty")
					}

					for _, e := range ex {
						name := e.name
						switch s.Kind {
						case "GITHUB", "BITBUCKETSERVER":
							name = strings.ToLower(name)
						}
						set[name], set[e.id] = true, true
						if e.pattern != "" {
							re, err := regexp.Compile(e.pattern)
							if err != nil {
								t.Fatal(err)
							}
							patterns = append(patterns, re)
						}
					}

					for _, r := range rs {
						if set[r.Name] || set[r.ExternalRepo.ID] {
							t.Errorf("excluded repo{name=%s, id=%s} was yielded", r.Name, r.ExternalRepo.ID)
						}
						for _, re := range patterns {
							if re.MatchString(r.Name) {
								t.Errorf("excluded repo{name=%s} matching %q was yielded", r.Name, re.String())
							}
						}
					}
				}
			},
			err: "<nil>",
		})
	}

	{
		svcs := ExternalServices{
			{
				Kind: "GITHUB",
				Config: marshalJSON(t, &schema.GitHubConnection{
					Url:   "https://github.com",
					Token: os.Getenv("GITHUB_ACCESS_TOKEN"),
					Repos: []string{
						"sourcegraph/Sourcegraph",
						"tsenart/Vegeta",
						"tsenart/vegeta-missing",
					},
				}),
			},
			{
				Kind: "GITLAB",
				Config: marshalJSON(t, &schema.GitLabConnection{
					Url:          "https://gitlab.com",
					Token:        os.Getenv("GITLAB_ACCESS_TOKEN"),
					ProjectQuery: []string{"none"},
					Projects: []*schema.GitLabProject{
						{Name: "gnachman/iterm2"},
						{Name: "gnachman/iterm2-missing"},
						{Id: 13083}, // https://gitlab.com/gitlab-org/gitlab-ce
					},
				}),
			},
			{
				Kind: "BITBUCKETSERVER",
				Config: marshalJSON(t, &schema.BitbucketServerConnection{
					Url:             "http://127.0.0.1:7990",
					Token:           os.Getenv("BITBUCKET_SERVER_TOKEN"),
					RepositoryQuery: []string{"none"},
					Repos: []string{
						"Org/foO",
						"org/baz",
						"ORG/bar",
					},
				}),
			},
			{
				Kind: "OTHER",
				Config: marshalJSON(t, &schema.OtherExternalServiceConnection{
					Url: "https://github.com",
					Repos: []string{
						"google/go-cmp",
					},
				}),
			},
			{
				Kind: "AWSCODECOMMIT",
				Config: marshalJSON(t, &schema.AWSCodeCommitConnection{
					AccessKeyID:     getAWSEnv("AWS_ACCESS_KEY_ID"),
					SecretAccessKey: getAWSEnv("AWS_SECRET_ACCESS_KEY"),
					Region:          "us-west-1",
					GitCredentials: schema.AWSCodeCommitGitCredentials{
						Username: "git-username",
						Password: "git-password",
					},
				}),
			},
		}

		testCases = append(testCases, testCase{
			name: "included repos that exist are yielded",
			svcs: svcs,
			assert: func(s *ExternalService) ReposAssertion {
				return func(t testing.TB, rs Repos) {
					t.Helper()

					have := rs.Names()
					sort.Strings(have)

					var want []string
					switch s.Kind {
					case "GITHUB":
						want = []string{
							"github.com/sourcegraph/sourcegraph",
							"github.com/tsenart/vegeta",
						}
					case "BITBUCKETSERVER":
						want = []string{
							"127.0.0.1/ORG/bar",
							"127.0.0.1/ORG/baz",
							"127.0.0.1/ORG/foo",
						}
					case "GITLAB":
						want = []string{
							"gitlab.com/gitlab-org/gitlab-ce",
							"gitlab.com/gnachman/iterm2",
						}
					case "AWSCODECOMMIT":
						want = []string{
							"__WARNING_DO_NOT_PUT_ANY_PRIVATE_CODE_IN_HERE",
							"empty-repo",
							"stripe-go",
							"test",
							"test2",
						}
					case "OTHER":
						want = []string{
							"github.com/google/go-cmp",
						}
					}

					if !reflect.DeepEqual(have, want) {
						t.Error(cmp.Diff(have, want))
					}
				}
			},
			err: "<nil>",
		})
	}

	{
		svcs := ExternalServices{
			{
				Kind: "GITHUB",
				Config: marshalJSON(t, &schema.GitHubConnection{
					Url:                   "https://github.com",
					Token:                 os.Getenv("GITHUB_ACCESS_TOKEN"),
					RepositoryPathPattern: "{host}/a/b/c/{nameWithOwner}",
					RepositoryQuery:       []string{"none"},
					Repos:                 []string{"tsenart/vegeta"},
				}),
			},
			{
				Kind: "GITLAB",
				Config: marshalJSON(t, &schema.GitLabConnection{
					Url:                   "https://gitlab.com",
					Token:                 os.Getenv("GITLAB_ACCESS_TOKEN"),
					RepositoryPathPattern: "{host}/a/b/c/{pathWithNamespace}",
					ProjectQuery:          []string{"none"},
					Projects: []*schema.GitLabProject{
						{Name: "gnachman/iterm2"},
					},
				}),
			},
			{
				Kind: "BITBUCKETSERVER",
				Config: marshalJSON(t, &schema.BitbucketServerConnection{
					Url:                   "http://127.0.0.1:7990",
					Token:                 os.Getenv("BITBUCKET_SERVER_TOKEN"),
					RepositoryPathPattern: "{host}/a/b/c/{projectKey}/{repositorySlug}",
					RepositoryQuery:       []string{"none"},
					Repos:                 []string{"org/baz"},
				}),
			},
			{
				Kind: "AWSCODECOMMIT",
				Config: marshalJSON(t, &schema.AWSCodeCommitConnection{
					AccessKeyID:     getAWSEnv("AWS_ACCESS_KEY_ID"),
					SecretAccessKey: getAWSEnv("AWS_SECRET_ACCESS_KEY"),
					Region:          "us-west-1",
					GitCredentials: schema.AWSCodeCommitGitCredentials{
						Username: "git-username",
						Password: "git-password",
					},
					RepositoryPathPattern: "a/b/c/{name}",
				}),
			},
			{
				Kind: "GITOLITE",
				Config: marshalJSON(t, &schema.GitoliteConnection{
					// Prefix serves as a sort of repositoryPathPattern for Gitolite
					Prefix: "gitolite.mycorp.com/",
					Host:   "ssh://git@127.0.0.1:2222",
				}),
			},
		}

		testCases = append(testCases, testCase{
			name: "repositoryPathPattern determines the repo name",
			svcs: svcs,
			assert: func(s *ExternalService) ReposAssertion {
				return func(t testing.TB, rs Repos) {
					t.Helper()

					haveNames := rs.Names()
					var haveURIs []string
					for _, r := range rs {
						haveURIs = append(haveURIs, r.URI)
					}

					var wantNames, wantURIs []string
					switch s.Kind {
					case "GITHUB":
						wantNames = []string{
							"github.com/a/b/c/tsenart/vegeta",
						}
						wantURIs = []string{
							"github.com/tsenart/vegeta",
						}
					case "GITLAB":
						wantNames = []string{
							"gitlab.com/a/b/c/gnachman/iterm2",
						}
						wantURIs = []string{
							"gitlab.com/gnachman/iterm2",
						}
					case "BITBUCKETSERVER":
						wantNames = []string{
							"127.0.0.1/a/b/c/ORG/baz",
						}
						wantURIs = []string{
							"127.0.0.1/ORG/baz",
						}
					case "AWSCODECOMMIT":
						wantNames = []string{
							"a/b/c/empty-repo",
							"a/b/c/stripe-go",
							"a/b/c/test2",
							"a/b/c/__WARNING_DO_NOT_PUT_ANY_PRIVATE_CODE_IN_HERE",
							"a/b/c/test",
						}
						wantURIs = []string{
							"empty-repo",
							"stripe-go",
							"test2",
							"__WARNING_DO_NOT_PUT_ANY_PRIVATE_CODE_IN_HERE",
							"test",
						}
					case "GITOLITE":
						wantNames = []string{
							"gitolite.mycorp.com/bar",
							"gitolite.mycorp.com/baz",
							"gitolite.mycorp.com/foo",
							"gitolite.mycorp.com/gitolite-admin",
							"gitolite.mycorp.com/testing",
						}
						wantURIs = wantNames
					}

					if !reflect.DeepEqual(haveNames, wantNames) {
						t.Error(cmp.Diff(haveNames, wantNames))
					}
					if !reflect.DeepEqual(haveURIs, wantURIs) {
						t.Error(cmp.Diff(haveURIs, wantURIs))
					}
				}
			},
			err: "<nil>",
		})
	}
	{
		svcs := ExternalServices{
			{
				Kind: "AWSCODECOMMIT",
				Config: marshalJSON(t, &schema.AWSCodeCommitConnection{
					AccessKeyID:     getAWSEnv("AWS_ACCESS_KEY_ID"),
					SecretAccessKey: getAWSEnv("AWS_SECRET_ACCESS_KEY"),
					Region:          "us-west-1",
					GitCredentials: schema.AWSCodeCommitGitCredentials{
						Username: "git-username",
						Password: "git-password",
					},
				}),
			},
		}

		testCases = append(testCases, testCase{
			name: "yielded repos have authenticated CloneURLs",
			svcs: svcs,
			assert: func(s *ExternalService) ReposAssertion {
				return func(t testing.TB, rs Repos) {
					t.Helper()

					urls := []string{}
					for _, r := range rs {
						urls = append(urls, r.CloneURLs()...)
					}

					switch s.Kind {
					case "AWSCODECOMMIT":
						want := []string{
							"https://git-username:git-password@git-codecommit.us-west-1.amazonaws.com/v1/repos/empty-repo",
							"https://git-username:git-password@git-codecommit.us-west-1.amazonaws.com/v1/repos/stripe-go",
							"https://git-username:git-password@git-codecommit.us-west-1.amazonaws.com/v1/repos/test2",
							"https://git-username:git-password@git-codecommit.us-west-1.amazonaws.com/v1/repos/__WARNING_DO_NOT_PUT_ANY_PRIVATE_CODE_IN_HERE",
							"https://git-username:git-password@git-codecommit.us-west-1.amazonaws.com/v1/repos/test",
						}

						if have := urls; !reflect.DeepEqual(have, want) {
							t.Error(cmp.Diff(have, want))
						}
					}
				}
			},
			err: "<nil>",
		})
	}

	{
		svcs := ExternalServices{
			{
				Kind: "PHABRICATOR",
				Config: marshalJSON(t, &schema.PhabricatorConnection{
					Url:   "https://secure.phabricator.com",
					Token: os.Getenv("PHABRICATOR_TOKEN"),
				}),
			},
		}

		testCases = append(testCases, testCase{
			name: "phabricator",
			svcs: svcs,
			assert: func(*ExternalService) ReposAssertion {
				return func(t testing.TB, rs Repos) {
					t.Helper()

					if len(rs) == 0 {
						t.Fatalf("no repos yielded")
					}

					for _, r := range rs {
						repo := r.Metadata.(*phabricator.Repo)
						if repo.VCS != "git" {
							t.Fatalf("non git repo yielded: %+v", repo)
						}

						if repo.Status == "inactive" {
							t.Fatalf("inactive repo yielded: %+v", repo)
						}

						if repo.Name == "" {
							t.Fatalf("empty repo name: %+v", repo)
						}

						if !r.Enabled {
							t.Fatalf("repo disabled: %+v", repo)
						}

						ext := api.ExternalRepoSpec{
							ID:          repo.PHID,
							ServiceType: "phabricator",
							ServiceID:   "https://secure.phabricator.com",
						}

						if have, want := r.ExternalRepo, ext; have != want {
							t.Fatal(cmp.Diff(have, want))
						}
					}
				}
			},
			err: "<nil>",
		})
	}

	for _, tc := range testCases {
		tc := tc
		for _, svc := range tc.svcs {
			name := svc.Kind + "/" + tc.name
			t.Run(name, func(t *testing.T) {
				cf, save := newClientFactory(t, name)
				defer save(t)

				lg := log15.New()
				lg.SetHandler(log15.DiscardHandler())

				obs := ObservedSource(lg, NewSourceMetrics())
				srcs, err := NewSourcer(cf, obs)(svc)
				if err != nil {
					t.Fatal(err)
				}

				ctx := tc.ctx
				if ctx == nil {
					ctx = context.Background()
				}

				repos, err := srcs.ListRepos(ctx)
				if have, want := fmt.Sprint(err), tc.err; have != want {
					t.Errorf("error:\nhave: %q\nwant: %q", have, want)
				}

				if tc.assert != nil {
					tc.assert(svc)(t, repos)
				}
			})
		}
	}
}

func TestGithubSource_GetRepo(t *testing.T) {
	testCases := []struct {
		name          string
		nameWithOwner string
		assert        func(*testing.T, *Repo)
		err           string
	}{
		{
			name:          "invalid name",
			nameWithOwner: "thisIsNotANameWithOwner",
			err:           `Invalid GitHub repository: nameWithOwner=thisIsNotANameWithOwner: invalid GitHub repository "owner/name" string: "thisIsNotANameWithOwner"`,
		},
		{
			name:          "not found",
			nameWithOwner: "foobarfoobarfoobar/please-let-this-not-exist",
			err:           `GitHub repository not found`,
		},
		{
			name:          "found",
			nameWithOwner: "sourcegraph/sourcegraph",
			assert: func(t *testing.T, have *Repo) {
				t.Helper()

				want := &Repo{
					Name:        "github.com/sourcegraph/sourcegraph",
					Description: "Code search and navigation tool (self-hosted)",
					Enabled:     true,
					URI:         "github.com/sourcegraph/sourcegraph",
					ExternalRepo: api.ExternalRepoSpec{
						ID:          "MDEwOlJlcG9zaXRvcnk0MTI4ODcwOA==",
						ServiceType: "github",
						ServiceID:   "https://github.com/",
					},
					Sources: map[string]*SourceInfo{
						"extsvc:github:0": {
							ID:       "extsvc:github:0",
							CloneURL: "https://github.com/sourcegraph/sourcegraph",
						},
					},
					Metadata: &github.Repository{
						ID:            "MDEwOlJlcG9zaXRvcnk0MTI4ODcwOA==",
						DatabaseID:    41288708,
						NameWithOwner: "sourcegraph/sourcegraph",
						Description:   "Code search and navigation tool (self-hosted)",
						URL:           "https://github.com/sourcegraph/sourcegraph",
					},
				}

				if !reflect.DeepEqual(have, want) {
					t.Errorf("response: %s", cmp.Diff(have, want))
				}
			},
			err: "<nil>",
		},
	}

	for _, tc := range testCases {
		tc := tc
		tc.name = "GITHUB-DOT-COM/" + tc.name

		t.Run(tc.name, func(t *testing.T) {
			// The GithubSource uses the github.Client under the hood, which
			// uses rcache, a caching layer that uses Redis.
			// We need to clear the cache before we run the tests
			rcache.SetupForTest(t)

			cf, save := newClientFactory(t, tc.name)
			defer save(t)

			lg := log15.New()
			lg.SetHandler(log15.DiscardHandler())

			svc := &ExternalService{
				Kind: "GITHUB",
				Config: marshalJSON(t, &schema.GitHubConnection{
					Url: "https://github.com",
				}),
			}

			githubSrc, err := NewGithubSource(svc, cf)
			if err != nil {
				t.Fatal(err)
			}

			repo, err := githubSrc.GetRepo(context.Background(), tc.nameWithOwner)
			if have, want := fmt.Sprint(err), tc.err; have != want {
				t.Errorf("error:\nhave: %q\nwant: %q", have, want)
			}

			if tc.assert != nil {
				tc.assert(t, repo)
			}
		})
	}
}

func newClientFactory(t testing.TB, name string) (*httpcli.Factory, func(testing.TB)) {
	cassete := filepath.Join("testdata", "sources", strings.Replace(name, " ", "-", -1))
	rec := newRecorder(t, cassete, update(name))
	mw := httpcli.NewMiddleware(
		githubProxyRedirectMiddleware,
		gitserverRedirectMiddleware,
	)
	return httpcli.NewFactory(mw, httptestutil.NewRecorderOpt(rec)),
		func(t testing.TB) { save(t, rec) }
}

func githubProxyRedirectMiddleware(cli httpcli.Doer) httpcli.Doer {
	return httpcli.DoerFunc(func(req *http.Request) (*http.Response, error) {
		if req.URL.Hostname() == "github-proxy" {
			req.URL.Host = "api.github.com"
			req.URL.Scheme = "https"
		}
		return cli.Do(req)
	})
}

func gitserverRedirectMiddleware(cli httpcli.Doer) httpcli.Doer {
	return httpcli.DoerFunc(func(req *http.Request) (*http.Response, error) {
		if req.URL.Hostname() == "gitserver" {
			// Start local git server first
			req.URL.Host = "127.0.0.1:3178"
			req.URL.Scheme = "http"
		}
		return cli.Do(req)
	})
}

func newRecorder(t testing.TB, file string, record bool) *recorder.Recorder {
	rec, err := httptestutil.NewRecorder(file, record, func(i *cassette.Interaction) error {
		// The ratelimit.Monitor type resets its internal timestamp if it's
		// updated with a timestamp in the past. This makes tests ran with
		// recorded interations just wait for a very long time. Removing
		// these headers from the casseste effectively disables rate-limiting
		// in tests which replay HTTP interactions, which is desired behaviour.
		for _, name := range [...]string{
			"RateLimit-Limit",
			"RateLimit-Observed",
			"RateLimit-Remaining",
			"RateLimit-Reset",
			"RateLimit-Resettime",
			"X-RateLimit-Limit",
			"X-RateLimit-Remaining",
			"X-RateLimit-Reset",
		} {
			i.Response.Headers.Del(name)
		}

		// Phabricator requests include a token in the form and body.
		ua := i.Request.Headers.Get("User-Agent")
		if strings.Contains(strings.ToLower(ua), "phabricator") {
			i.Request.Body = ""
			i.Request.Form = nil
		}

		return nil
	})

	if err != nil {
		t.Fatal(err)
	}

	return rec
}

func save(t testing.TB, rec *recorder.Recorder) {
	if err := rec.Stop(); err != nil {
		t.Errorf("failed to update test data: %s", err)
	}
}

func marshalJSON(t testing.TB, v interface{}) string {
	t.Helper()

	bs, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}

	return string(bs)
}

func getAWSEnv(envVar string) string {
	s := os.Getenv(envVar)
	if s == "" {
		s = fmt.Sprintf("BOGUS-%s", envVar)
	}
	return s
}
