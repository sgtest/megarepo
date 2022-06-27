package externallink

import (
	"context"
	"reflect"
	"testing"

	mockrequire "github.com/derision-test/go-mockgen/testutil/require"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/github"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/gitlab"
	"github.com/sourcegraph/sourcegraph/internal/gitserver"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func TestRepository(t *testing.T) {
	t.Parallel()

	t.Run("repo", func(t *testing.T) {
		repo := &types.Repo{
			Name: api.RepoName("github.com/foo/bar"),
			ExternalRepo: api.ExternalRepoSpec{
				ServiceID:   extsvc.GitHubDotCom.ServiceID,
				ServiceType: extsvc.GitHubDotCom.ServiceType,
			},
			Metadata: &github.Repository{
				URL: "http://github.com/foo/bar",
			},
		}

		phabricator := database.NewMockPhabricatorStore()
		phabricator.GetByNameFunc.SetDefaultReturn(nil, errors.New("x"))
		db := database.NewMockDB()
		db.PhabricatorFunc.SetDefaultReturn(phabricator)

		links, err := Repository(context.Background(), db, repo)
		if err != nil {
			t.Fatal(err)
		}
		if want := []*Resolver{
			{
				url:         "http://github.com/foo/bar",
				serviceKind: extsvc.TypeToKind(repo.ExternalRepo.ServiceType),
				serviceType: repo.ExternalRepo.ServiceType,
			},
		}; !reflect.DeepEqual(links, want) {
			t.Errorf("got %+v, want %+v", links, want)
		}
		mockrequire.Called(t, phabricator.GetByNameFunc)
	})

	t.Run("phabricator", func(t *testing.T) {
		phabricator := database.NewMockPhabricatorStore()
		phabricator.GetByNameFunc.SetDefaultHook(func(_ context.Context, repo api.RepoName) (*types.PhabricatorRepo, error) {
			if want := api.RepoName("myrepo"); repo != want {
				t.Errorf("got %q, want %q", repo, want)
			}
			return &types.PhabricatorRepo{URL: "http://phabricator.example.com/", Callsign: "MYREPO"}, nil
		})
		db := database.NewMockDB()
		db.PhabricatorFunc.SetDefaultReturn(phabricator)

		links, err := Repository(context.Background(), db, &types.Repo{Name: "myrepo"})
		if err != nil {
			t.Fatal(err)
		}
		if want := []*Resolver{
			{
				url:         "http://phabricator.example.com/diffusion/MYREPO",
				serviceKind: extsvc.KindPhabricator,
				serviceType: extsvc.TypePhabricator,
			},
		}; !reflect.DeepEqual(links, want) {
			t.Errorf("got %+v, want %+v", links, want)
		}
		mockrequire.Called(t, phabricator.GetByNameFunc)
	})

	t.Run("errors", func(t *testing.T) {
		phabricator := database.NewMockPhabricatorStore()
		phabricator.GetByNameFunc.SetDefaultReturn(nil, errors.New("x"))
		db := database.NewMockDB()
		db.PhabricatorFunc.SetDefaultReturn(phabricator)

		links, err := Repository(context.Background(), db, &types.Repo{Name: "myrepo"})
		if err != nil {
			t.Fatal(err)
		}
		if want := []*Resolver(nil); !reflect.DeepEqual(links, want) {
			t.Errorf("got %+v, want %+v", links, want)
		}
		mockrequire.Called(t, phabricator.GetByNameFunc)
	})
}

func TestFileOrDir(t *testing.T) {
	const (
		rev  = "myrev"
		path = "mydir/myfile"
	)

	repo := &types.Repo{
		Name: api.RepoName("gitlab.com/foo/bar"),
		ExternalRepo: api.ExternalRepoSpec{
			ServiceID:   extsvc.GitLabDotCom.ServiceID,
			ServiceType: extsvc.GitLabDotCom.ServiceType,
		},
		Metadata: &gitlab.Project{
			ProjectCommon: gitlab.ProjectCommon{
				WebURL: "http://gitlab.com/foo/bar",
			},
		},
	}

	for _, which := range []string{"file", "dir"} {
		var (
			isDir   bool
			wantURL string
		)
		switch which {
		case "file":
			isDir = false
			wantURL = "http://gitlab.com/foo/bar/blob/myrev/mydir/myfile"
		case "dir":
			isDir = true
			wantURL = "http://gitlab.com/foo/bar/tree/myrev/mydir/myfile"
		}

		t.Run(which, func(t *testing.T) {
			phabricator := database.NewMockPhabricatorStore()
			phabricator.GetByNameFunc.SetDefaultReturn(nil, errors.New("x"))
			db := database.NewMockDB()
			db.PhabricatorFunc.SetDefaultReturn(phabricator)

			links, err := FileOrDir(context.Background(), db, repo, rev, path, isDir)
			if err != nil {
				t.Fatal(err)
			}
			if want := []*Resolver{
				{
					url:         wantURL,
					serviceKind: extsvc.TypeToKind(repo.ExternalRepo.ServiceType),
					serviceType: repo.ExternalRepo.ServiceType,
				},
			}; !reflect.DeepEqual(links, want) {
				t.Errorf("got %+v, want %+v", links, want)
			}
			mockrequire.Called(t, phabricator.GetByNameFunc)
		})
	}

	t.Run("phabricator", func(t *testing.T) {
		phabricator := database.NewMockPhabricatorStore()
		phabricator.GetByNameFunc.SetDefaultHook(func(_ context.Context, repo api.RepoName) (*types.PhabricatorRepo, error) {
			if want := api.RepoName("myrepo"); repo != want {
				t.Errorf("got %q, want %q", repo, want)
			}
			return &types.PhabricatorRepo{URL: "http://phabricator.example.com/", Callsign: "MYREPO"}, nil
		})
		db := database.NewMockDB()
		db.PhabricatorFunc.SetDefaultReturn(phabricator)

		gitserver.Mocks.GetDefaultBranchShort = func(repo api.RepoName) (refName string, commit api.CommitID, err error) {
			return "mybranch", "", nil
		}
		defer gitserver.ResetMocks()

		links, err := FileOrDir(context.Background(), db, &types.Repo{Name: "myrepo"}, rev, path, true)
		if err != nil {
			t.Fatal(err)
		}
		if want := []*Resolver{
			{
				url:         "http://phabricator.example.com/source/MYREPO/browse/mybranch/mydir/myfile;myrev",
				serviceKind: extsvc.KindPhabricator,
				serviceType: extsvc.TypePhabricator,
			},
		}; !reflect.DeepEqual(links, want) {
			t.Errorf("got %+v, want %+v", links, want)
		}
		mockrequire.Called(t, phabricator.GetByNameFunc)
	})

	t.Run("errors", func(t *testing.T) {
		phabricator := database.NewMockPhabricatorStore()
		phabricator.GetByNameFunc.SetDefaultReturn(nil, errors.New("x"))
		db := database.NewMockDB()
		db.PhabricatorFunc.SetDefaultReturn(phabricator)

		links, err := FileOrDir(context.Background(), db, &types.Repo{Name: "myrepo"}, rev, path, true)
		if err != nil {
			t.Fatal(err)
		}
		if want := []*Resolver(nil); !reflect.DeepEqual(links, want) {
			t.Errorf("got %+v, want %+v", links, want)
		}
		mockrequire.Called(t, phabricator.GetByNameFunc)
	})
}

func TestCommit(t *testing.T) {
	const commit = "mycommit"

	repo := &types.Repo{
		Name: api.RepoName("github.com/foo/bar"),
		ExternalRepo: api.ExternalRepoSpec{
			ServiceID:   extsvc.GitHubDotCom.ServiceID,
			ServiceType: extsvc.GitHubDotCom.ServiceType,
		},
		Metadata: &github.Repository{
			URL: "http://github.com/foo/bar",
		},
	}

	t.Run("repo", func(t *testing.T) {
		phabricator := database.NewMockPhabricatorStore()
		phabricator.GetByNameFunc.SetDefaultReturn(nil, errors.New("x"))
		db := database.NewMockDB()
		db.PhabricatorFunc.SetDefaultReturn(phabricator)

		links, err := Commit(context.Background(), db, repo, commit)
		if err != nil {
			t.Fatal(err)
		}
		if want := []*Resolver{
			{
				url:         "http://github.com/foo/bar/commit/mycommit",
				serviceKind: extsvc.TypeToKind(repo.ExternalRepo.ServiceType),
				serviceType: repo.ExternalRepo.ServiceType,
			},
		}; !reflect.DeepEqual(links, want) {
			t.Errorf("got %+v, want %+v", links, want)
		}
		mockrequire.Called(t, phabricator.GetByNameFunc)
	})

	t.Run("phabricator", func(t *testing.T) {
		phabricator := database.NewMockPhabricatorStore()
		phabricator.GetByNameFunc.SetDefaultHook(func(_ context.Context, repo api.RepoName) (*types.PhabricatorRepo, error) {
			if want := api.RepoName("myrepo"); repo != want {
				t.Errorf("got %q, want %q", repo, want)
			}
			return &types.PhabricatorRepo{URL: "http://phabricator.example.com/", Callsign: "MYREPO"}, nil
		})
		db := database.NewMockDB()
		db.PhabricatorFunc.SetDefaultReturn(phabricator)

		links, err := Commit(context.Background(), db, &types.Repo{Name: "myrepo"}, commit)
		if err != nil {
			t.Fatal(err)
		}
		if want := []*Resolver{
			{
				url:         "http://phabricator.example.com/rMYREPOmycommit",
				serviceKind: extsvc.KindPhabricator,
				serviceType: extsvc.TypePhabricator,
			},
		}; !reflect.DeepEqual(links, want) {
			t.Errorf("got %+v, want %+v", links, want)
		}
		mockrequire.Called(t, phabricator.GetByNameFunc)
	})

	t.Run("errors", func(t *testing.T) {
		phabricator := database.NewMockPhabricatorStore()
		phabricator.GetByNameFunc.SetDefaultReturn(nil, errors.New("x"))
		db := database.NewMockDB()
		db.PhabricatorFunc.SetDefaultReturn(phabricator)

		links, err := Commit(context.Background(), db, &types.Repo{Name: "myrepo"}, commit)
		if err != nil {
			t.Fatal(err)
		}
		if want := []*Resolver(nil); !reflect.DeepEqual(links, want) {
			t.Errorf("got %+v, want %+v", links, want)
		}
		mockrequire.Called(t, phabricator.GetByNameFunc)
	})
}
