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
	"github.com/sourcegraph/sourcegraph/pkg/extsvc/phabricator"
	"github.com/sourcegraph/sourcegraph/pkg/httpcli"
	"github.com/sourcegraph/sourcegraph/pkg/httptestutil"
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

	githubDotCom := ExternalService{Kind: "GITHUB"}

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
		{
			name: "github.com is added when not existent",
			svcs: ExternalServices{},
			srcs: sources(&githubDotCom),
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
		}

		testCases = append(testCases, testCase{
			name: "repositoryPathPattern determines the repo name",
			svcs: svcs,
			assert: func(s *ExternalService) ReposAssertion {
				return func(t testing.TB, rs Repos) {
					t.Helper()

					have := rs.Names()

					var want []string
					switch s.Kind {
					case "GITHUB":
						want = []string{
							"github.com/a/b/c/tsenart/vegeta",
						}
					case "GITLAB":
						want = []string{
							"gitlab.com/a/b/c/gnachman/iterm2",
						}
					case "BITBUCKETSERVER":
						want = []string{
							"127.0.0.1/a/b/c/ORG/baz",
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
