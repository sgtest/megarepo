package graphqlbackend

import (
	"context"
	"reflect"
	"testing"
	"time"

	"github.com/davecgh/go-spew/spew"
	"github.com/stretchr/testify/assert"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database/dbmock"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git"
	"github.com/sourcegraph/sourcegraph/internal/vcs/git/gitapi"
)

func TestGitCommitResolver(t *testing.T) {
	ctx := context.Background()
	db := dbmock.NewMockDB()

	commit := &gitapi.Commit{
		ID:      "c1",
		Message: "subject: Changes things\nBody of changes",
		Parents: []api.CommitID{"p1", "p2"},
		Author: gitapi.Signature{
			Name:  "Bob",
			Email: "bob@alice.com",
			Date:  time.Now(),
		},
		Committer: &gitapi.Signature{
			Name:  "Alice",
			Email: "alice@bob.com",
			Date:  time.Now(),
		},
	}

	t.Run("Lazy loading", func(t *testing.T) {
		git.Mocks.GetCommit = func(api.CommitID) (*gitapi.Commit, error) {
			return commit, nil
		}
		t.Cleanup(func() {
			git.Mocks.GetCommit = nil
		})

		for _, tc := range []struct {
			name string
			want interface{}
			have func(*GitCommitResolver) (interface{}, error)
		}{{
			name: "author",
			want: toSignatureResolver(db, &commit.Author, true),
			have: func(r *GitCommitResolver) (interface{}, error) {
				return r.Author(ctx)
			},
		}, {
			name: "committer",
			want: toSignatureResolver(db, commit.Committer, true),
			have: func(r *GitCommitResolver) (interface{}, error) {
				return r.Committer(ctx)
			},
		}, {
			name: "message",
			want: string(commit.Message),
			have: func(r *GitCommitResolver) (interface{}, error) {
				return r.Message(ctx)
			},
		}, {
			name: "subject",
			want: "subject: Changes things",
			have: func(r *GitCommitResolver) (interface{}, error) {
				return r.Subject(ctx)
			},
		}, {
			name: "body",
			want: "Body of changes",
			have: func(r *GitCommitResolver) (interface{}, error) {
				s, err := r.Body(ctx)
				return *s, err
			},
		}, {
			name: "url",
			want: "/bob-repo/-/commit/c1",
			have: func(r *GitCommitResolver) (interface{}, error) {
				return r.URL(), nil
			},
		}, {
			name: "canonical-url",
			want: "/bob-repo/-/commit/c1",
			have: func(r *GitCommitResolver) (interface{}, error) {
				return r.CanonicalURL(), nil
			},
		}} {
			t.Run(tc.name, func(t *testing.T) {
				repo := NewRepositoryResolver(db, &types.Repo{Name: "bob-repo"})
				// We pass no commit here to test that it gets lazy loaded via
				// the git.GetCommit mock above.
				r := NewGitCommitResolver(db, repo, "c1", nil)

				have, err := tc.have(r)
				if err != nil {
					t.Fatal(err)
				}

				if !reflect.DeepEqual(have, tc.want) {
					t.Errorf("\nhave: %s\nwant: %s", spew.Sprint(have), spew.Sprint(tc.want))
				}
			})
		}
	})
}

func TestGitCommitFileNames(t *testing.T) {
	externalServices := dbmock.NewMockExternalServiceStore()
	externalServices.ListFunc.SetDefaultReturn(nil, nil)

	repos := dbmock.NewMockRepoStore()
	repos.GetFunc.SetDefaultReturn(&types.Repo{ID: 2, Name: "github.com/gorilla/mux"}, nil)

	db := dbmock.NewMockDB()
	db.ExternalServicesFunc.SetDefaultReturn(externalServices)
	db.ReposFunc.SetDefaultReturn(repos)

	backend.Mocks.Repos.ResolveRev = func(ctx context.Context, repo *types.Repo, rev string) (api.CommitID, error) {
		assert.Equal(t, api.RepoID(2), repo.ID)
		assert.Equal(t, exampleCommitSHA1, rev)
		return exampleCommitSHA1, nil
	}
	backend.Mocks.Repos.MockGetCommit_Return_NoCheck(t, &gitapi.Commit{ID: exampleCommitSHA1})
	git.Mocks.LsFiles = func(repo api.RepoName, commit api.CommitID) ([]string, error) {
		return []string{"a", "b"}, nil
	}
	defer func() {
		backend.Mocks = backend.MockServices{}
		git.ResetMocks()
	}()

	RunTests(t, []*Test{
		{
			Schema: mustParseGraphQLSchema(t, db),
			Query: `
				{
					repository(name: "github.com/gorilla/mux") {
						commit(rev: "` + exampleCommitSHA1 + `") {
							fileNames
						}
					}
				}
			`,
			ExpectedResult: `
{
  "repository": {
    "commit": {
		"fileNames": ["a", "b"]
    }
  }
}
			`,
		},
	})
}
