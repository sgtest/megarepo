package repos

import (
	"context"
	"encoding/json"
	"fmt"
	"io/ioutil"
	"path/filepath"
	"reflect"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/inconshreveable/log15"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitlab"
	"github.com/sourcegraph/sourcegraph/internal/rcache"
	"github.com/sourcegraph/sourcegraph/internal/testutil"
	"github.com/sourcegraph/sourcegraph/schema"
)

func TestProjectQueryToURL(t *testing.T) {
	tests := []struct {
		projectQuery string
		perPage      int
		expURL       string
		expErr       error
	}{{
		projectQuery: "?membership=true",
		perPage:      100,
		expURL:       "projects?membership=true&per_page=100",
	}, {
		projectQuery: "projects?membership=true",
		perPage:      100,
		expURL:       "projects?membership=true&per_page=100",
	}, {
		projectQuery: "groups/groupID/projects",
		perPage:      100,
		expURL:       "groups/groupID/projects?per_page=100",
	}, {
		projectQuery: "groups/groupID/projects?foo=bar",
		perPage:      100,
		expURL:       "groups/groupID/projects?foo=bar&per_page=100",
	}, {
		projectQuery: "",
		perPage:      100,
		expURL:       "projects?per_page=100",
	}, {
		projectQuery: "https://somethingelse.com/foo/bar",
		perPage:      100,
		expErr:       schemeOrHostNotEmptyErr,
	}}

	for _, test := range tests {
		t.Logf("Test case %+v", test)
		url, err := projectQueryToURL(test.projectQuery, test.perPage)
		if url != test.expURL {
			t.Errorf("expected %v, got %v", test.expURL, url)
		}
		if !reflect.DeepEqual(test.expErr, err) {
			t.Errorf("expected err %v, got %v", test.expErr, err)
		}
	}
}

func TestGitLabSource_GetRepo(t *testing.T) {
	testCases := []struct {
		name                 string
		projectWithNamespace string
		assert               func(*testing.T, *Repo)
		err                  string
	}{
		{
			name:                 "not found",
			projectWithNamespace: "foobarfoobarfoobar/please-let-this-not-exist",
			err:                  `unexpected response from GitLab API (https://gitlab.com/api/v4/projects/foobarfoobarfoobar%2Fplease-let-this-not-exist): HTTP error status 404`,
		},
		{
			name:                 "found",
			projectWithNamespace: "gitlab-org/gitaly",
			assert: func(t *testing.T, have *Repo) {
				t.Helper()

				want := &Repo{
					Name:        "gitlab.com/gitlab-org/gitaly",
					Description: "Gitaly is a Git RPC service for handling all the git calls made by GitLab",
					URI:         "gitlab.com/gitlab-org/gitaly",
					ExternalRepo: api.ExternalRepoSpec{
						ID:          "2009901",
						ServiceType: "gitlab",
						ServiceID:   "https://gitlab.com/",
					},
					Sources: map[string]*SourceInfo{
						"extsvc:gitlab:0": {
							ID:       "extsvc:gitlab:0",
							CloneURL: "https://gitlab.com/gitlab-org/gitaly.git",
						},
					},
					Metadata: &gitlab.Project{
						ProjectCommon: gitlab.ProjectCommon{
							ID:                2009901,
							PathWithNamespace: "gitlab-org/gitaly",
							Description:       "Gitaly is a Git RPC service for handling all the git calls made by GitLab",
							WebURL:            "https://gitlab.com/gitlab-org/gitaly",
							HTTPURLToRepo:     "https://gitlab.com/gitlab-org/gitaly.git",
							SSHURLToRepo:      "git@gitlab.com:gitlab-org/gitaly.git",
						},
						Visibility: "",
						Archived:   false,
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
		tc.name = "GITLAB-DOT-COM/" + tc.name

		t.Run(tc.name, func(t *testing.T) {
			// The GitLabSource uses the gitlab.Client under the hood, which
			// uses rcache, a caching layer that uses Redis.
			// We need to clear the cache before we run the tests
			rcache.SetupForTest(t)

			cf, save := newClientFactory(t, tc.name)
			defer save(t)

			lg := log15.New()
			lg.SetHandler(log15.DiscardHandler())

			svc := &ExternalService{
				Kind: extsvc.KindGitLab,
				Config: marshalJSON(t, &schema.GitLabConnection{
					Url: "https://gitlab.com",
				}),
			}

			gitlabSrc, err := NewGitLabSource(svc, cf)
			if err != nil {
				t.Fatal(err)
			}

			repo, err := gitlabSrc.GetRepo(context.Background(), tc.projectWithNamespace)
			if have, want := fmt.Sprint(err), tc.err; have != want {
				t.Errorf("error:\nhave: %q\nwant: %q", have, want)
			}

			if tc.assert != nil {
				tc.assert(t, repo)
			}
		})
	}
}

func TestGitLabSource_makeRepo(t *testing.T) {
	b, err := ioutil.ReadFile(filepath.Join("testdata", "gitlab-repos.json"))
	if err != nil {
		t.Fatal(err)
	}
	var repos []*gitlab.Project
	if err := json.Unmarshal(b, &repos); err != nil {
		t.Fatal(err)
	}

	svc := ExternalService{ID: 1, Kind: extsvc.KindGitLab}

	tests := []struct {
		name   string
		schmea *schema.GitLabConnection
	}{
		{
			name: "simple",
			schmea: &schema.GitLabConnection{
				Url: "https://gitlab.com",
			},
		}, {
			name: "ssh",
			schmea: &schema.GitLabConnection{
				Url:        "https://gitlab.com",
				GitURLType: "ssh",
			},
		}, {
			name: "path-pattern",
			schmea: &schema.GitLabConnection{
				Url:                   "https://gitlab.com",
				RepositoryPathPattern: "gl/{pathWithNamespace}",
			},
		},
	}
	for _, test := range tests {
		test.name = "GitLabSource_makeRepo_" + test.name
		t.Run(test.name, func(t *testing.T) {
			lg := log15.New()
			lg.SetHandler(log15.DiscardHandler())

			s, err := newGitLabSource(&svc, test.schmea, nil)
			if err != nil {
				t.Fatal(err)
			}

			var got []*Repo
			for _, r := range repos {
				got = append(got, s.makeRepo(r))
			}

			testutil.AssertGolden(t, "testdata/golden/"+test.name, update(test.name), got)
		})
	}
}
