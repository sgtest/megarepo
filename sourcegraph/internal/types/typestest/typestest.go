package typestest

import (
	"sort"
	"strconv"
	"testing"
	"time"

	"github.com/google/go-cmp/cmp"
	"github.com/google/go-cmp/cmp/cmpopts"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/awscodecommit"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/bitbucketserver"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitlab"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitolite"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func MakeRepo(name, serviceID, serviceType string, services ...*types.ExternalService) *types.Repo {
	clock := timeutil.NewFakeClock(time.Now(), 0)
	now := clock.Now()

	repo := types.Repo{
		ExternalRepo: api.ExternalRepoSpec{
			ID:          "1234",
			ServiceType: serviceType,
			ServiceID:   serviceID,
		},
		Name:        api.RepoName(name),
		URI:         name,
		Description: "The description",
		CreatedAt:   now,
		Sources:     make(map[string]*types.SourceInfo),
	}

	for _, svc := range services {
		repo.Sources[svc.URN()] = &types.SourceInfo{
			ID: svc.URN(),
		}
	}

	return &repo
}

// MakeGithubRepo returns a configured Github repository.
func MakeGithubRepo(services ...*types.ExternalService) *types.Repo {
	repo := MakeRepo("github.com/foo/bar", "http://github.com", extsvc.TypeGitHub, services...)
	repo.Metadata = new(github.Repository)
	return repo
}

// MakeGitlabRepo returns a configured Gitlab repository.
func MakeGitlabRepo(services ...*types.ExternalService) *types.Repo {
	repo := MakeRepo("gitlab.com/foo/bar", "http://gitlab.com", extsvc.TypeGitLab, services...)
	repo.Metadata = new(gitlab.Project)
	return repo
}

// MakeBitbucketServerRepo returns a configured Bitbucket Server repository.
func MakeBitbucketServerRepo(services ...*types.ExternalService) *types.Repo {
	repo := MakeRepo("bitbucketserver.mycorp.com/foo/bar", "http://bitbucketserver.mycorp.com", extsvc.TypeBitbucketServer, services...)
	repo.Metadata = new(bitbucketserver.Repo)
	return repo
}

// MakeAWSCodeCommitRepo returns a configured AWS Code Commit repository.
func MakeAWSCodeCommitRepo(services ...*types.ExternalService) *types.Repo {
	repo := MakeRepo("git-codecommit.us-west-1.amazonaws.com/stripe-go", "arn:aws:codecommit:us-west-1:999999999999:", extsvc.KindAWSCodeCommit, services...)
	repo.Metadata = new(awscodecommit.Repository)
	return repo
}

// MakeOtherRepo returns a configured repository from a custom host.
func MakeOtherRepo(services ...*types.ExternalService) *types.Repo {
	repo := MakeRepo("git-host.com/org/foo", "https://git-host.com/", extsvc.KindOther, services...)
	repo.Metadata = new(extsvc.OtherRepoMetadata)
	return repo
}

// MakeGitoliteRepo returns a configured Gitolite repository.
func MakeGitoliteRepo(services ...*types.ExternalService) *types.Repo {
	repo := MakeRepo("gitolite.mycorp.com/bar", "git@gitolite.mycorp.com", extsvc.KindGitolite, services...)
	repo.Metadata = new(gitolite.Repo)
	return repo
}

// GenerateRepos takes a list of base repos and generates n ones with different names.
func GenerateRepos(n int, base ...*types.Repo) types.Repos {
	if len(base) == 0 {
		return nil
	}

	rs := make(types.Repos, 0, n)
	for i := 0; i < n; i++ {
		id := strconv.Itoa(i)
		r := base[i%len(base)].Clone()
		r.Name += api.RepoName(id)
		r.ExternalRepo.ID += id
		rs = append(rs, r)
	}
	return rs
}

// Maketypes.ExternalServices creates one configured external service per kind and returns the list.
func MakeExternalServices() types.ExternalServices {
	clock := timeutil.NewFakeClock(time.Now(), 0)
	now := clock.Now()

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

// MakeNamespacedExternalServices creates one configured external service per kind, per user or org.
func MakeNamespacedExternalServices(userID int32, orgID int32) types.ExternalServices {
	clock := timeutil.NewFakeClock(time.Now(), 0)
	now := clock.Now()

	services := []*types.ExternalService{}

	if userID > 0 {
		services = append(services, &types.ExternalService{
			Kind:            extsvc.KindGitHub,
			DisplayName:     "Github - User",
			Config:          `{"url": "https://github.com", "token": "abc", "repositoryQuery": ["none"]}`,
			CreatedAt:       now,
			UpdatedAt:       now,
			NamespaceUserID: userID,
		}, &types.ExternalService{
			Kind:            extsvc.KindGitLab,
			DisplayName:     "GitLab - User",
			Config:          `{"url": "https://gitlab.com", "token": "abc", "projectQuery": ["projects?membership=true&archived=no"]}`,
			CreatedAt:       now,
			UpdatedAt:       now,
			NamespaceUserID: userID,
		})
	}
	if orgID > 0 {
		services = append(services, &types.ExternalService{
			Kind:           extsvc.KindGitHub,
			DisplayName:    "Github - Org",
			Config:         `{"url": "https://github.com", "token": "abc", "repositoryQuery": ["none"]}`,
			CreatedAt:      now,
			UpdatedAt:      now,
			NamespaceOrgID: orgID,
		}, &types.ExternalService{
			Kind:           extsvc.KindGitLab,
			DisplayName:    "GitLab - Org",
			Config:         `{"url": "https://gitlab.com", "token": "abc", "projectQuery": ["projects?membership=true&archived=no"]}`,
			CreatedAt:      now,
			UpdatedAt:      now,
			NamespaceOrgID: orgID,
		})
	}

	return services
}

// Generatetypes.ExternalServices takes a list of base external services and generates n ones with different names.
func GenerateExternalServices(n int, base ...*types.ExternalService) types.ExternalServices {
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

// ExternalServicesToMap is a helper function that returns a map whose key is the external service kind.
// If two external services have the same kind, only the last one will be stored in the map.
func ExternalServicesToMap(es types.ExternalServices) map[string]*types.ExternalService {
	m := make(map[string]*types.ExternalService)

	for _, svc := range es {
		m[svc.Kind] = svc
	}

	return m
}

//
// Functional options
//

// Opt contains functional options to be used in tests.
var Opt = struct {
	ExternalServiceID         func(int64) func(*types.ExternalService)
	ExternalServiceModifiedAt func(time.Time) func(*types.ExternalService)
	ExternalServiceDeletedAt  func(time.Time) func(*types.ExternalService)
	RepoID                    func(api.RepoID) func(*types.Repo)
	RepoName                  func(api.RepoName) func(*types.Repo)
	RepoCreatedAt             func(time.Time) func(*types.Repo)
	RepoModifiedAt            func(time.Time) func(*types.Repo)
	RepoDeletedAt             func(time.Time) func(*types.Repo)
	RepoSources               func(...string) func(*types.Repo)
	RepoMetadata              func(any) func(*types.Repo)
	RepoExternalID            func(string) func(*types.Repo)
}{
	ExternalServiceID: func(n int64) func(*types.ExternalService) {
		return func(e *types.ExternalService) {
			e.ID = n
		}
	},
	ExternalServiceModifiedAt: func(ts time.Time) func(*types.ExternalService) {
		return func(e *types.ExternalService) {
			e.UpdatedAt = ts
			e.DeletedAt = time.Time{}
		}
	},
	ExternalServiceDeletedAt: func(ts time.Time) func(*types.ExternalService) {
		return func(e *types.ExternalService) {
			e.UpdatedAt = ts
			e.DeletedAt = ts
		}
	},
	RepoID: func(n api.RepoID) func(*types.Repo) {
		return func(r *types.Repo) {
			r.ID = n
		}
	},
	RepoName: func(name api.RepoName) func(*types.Repo) {
		return func(r *types.Repo) {
			r.Name = name
		}
	},
	RepoCreatedAt: func(ts time.Time) func(*types.Repo) {
		return func(r *types.Repo) {
			r.CreatedAt = ts
			r.UpdatedAt = ts
			r.DeletedAt = time.Time{}
		}
	},
	RepoModifiedAt: func(ts time.Time) func(*types.Repo) {
		return func(r *types.Repo) {
			r.UpdatedAt = ts
			r.DeletedAt = time.Time{}
		}
	},
	RepoDeletedAt: func(ts time.Time) func(*types.Repo) {
		return func(r *types.Repo) {
			r.UpdatedAt = ts
			r.DeletedAt = ts
			r.Sources = map[string]*types.SourceInfo{}
		}
	},
	RepoSources: func(srcs ...string) func(*types.Repo) {
		return func(r *types.Repo) {
			r.Sources = map[string]*types.SourceInfo{}
			for _, src := range srcs {
				r.Sources[src] = &types.SourceInfo{ID: src, CloneURL: "clone-url"}
			}
		}
	},
	RepoMetadata: func(md any) func(*types.Repo) {
		return func(r *types.Repo) {
			r.Metadata = md
		}
	},
	RepoExternalID: func(id string) func(*types.Repo) {
		return func(r *types.Repo) {
			r.ExternalRepo.ID = id
		}
	},
}

//
// Assertions
//

// A ReposAssertion performs an assertion on the given Repos.
type ReposAssertion func(testing.TB, types.Repos)

// An ExternalServicesAssertion performs an assertion on the given
// types.ExternalServices.
type ExternalServicesAssertion func(testing.TB, types.ExternalServices)

// Assert contains assertion functions to be used in tests.
var Assert = struct {
	ReposEqual                func(...*types.Repo) ReposAssertion
	ReposOrderedBy            func(func(a, b *types.Repo) bool) ReposAssertion
	ExternalServicesEqual     func(...*types.ExternalService) ExternalServicesAssertion
	ExternalServicesOrderedBy func(func(a, b *types.ExternalService) bool) ExternalServicesAssertion
}{
	ReposEqual: func(rs ...*types.Repo) ReposAssertion {
		want := types.Repos(rs)
		return func(t testing.TB, have types.Repos) {
			t.Helper()
			// Exclude auto-generated IDs from equality tests
			opts := cmpopts.IgnoreFields(types.Repo{}, "ID", "CreatedAt", "UpdatedAt")
			if diff := cmp.Diff(want, have, opts); diff != "" {
				t.Errorf("repos (-want +got): %s", diff)
			}
		}
	},
	ReposOrderedBy: func(ord func(a, b *types.Repo) bool) ReposAssertion {
		return func(t testing.TB, have types.Repos) {
			t.Helper()
			want := have.Clone()
			sort.Slice(want, func(i, j int) bool {
				return ord(want[i], want[j])
			})
			if diff := cmp.Diff(want, have); diff != "" {
				t.Errorf("repos (-want +got): %s", cmp.Diff(want, have))
			}
		}
	},
	ExternalServicesEqual: func(es ...*types.ExternalService) ExternalServicesAssertion {
		want := types.ExternalServices(es)
		return func(t testing.TB, have types.ExternalServices) {
			t.Helper()
			opts := cmpopts.IgnoreFields(types.ExternalService{}, "ID", "CreatedAt", "UpdatedAt")
			if diff := cmp.Diff(want, have, opts); diff != "" {
				t.Errorf("external services (-want +got): %s", diff)
			}
		}
	},
	ExternalServicesOrderedBy: func(ord func(a, b *types.ExternalService) bool) ExternalServicesAssertion {
		return func(t testing.TB, have types.ExternalServices) {
			t.Helper()
			want := have.Clone()
			sort.Slice(want, func(i, j int) bool {
				return ord(want[i], want[j])
			})
			if diff := cmp.Diff(want, have); diff != "" {
				t.Errorf("external services (-want +got): %s", cmp.Diff(want, have))
			}
		}
	},
}
