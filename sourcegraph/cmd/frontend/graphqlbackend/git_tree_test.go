package graphqlbackend

import (
	"context"
	"os"
	"testing"

	"github.com/graph-gophers/graphql-go/gqltesting"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/vcs/git"
	"github.com/sourcegraph/sourcegraph/pkg/vcs/util"
)

func TestGitTree(t *testing.T) {
	resetMocks()
	db.Mocks.ExternalServices.List = func(opt db.ExternalServicesListOptions) ([]*types.ExternalService, error) {
		return nil, nil
	}
	db.Mocks.Repos.MockGetByName(t, "github.com/gorilla/mux", 2)
	backend.Mocks.Repos.ResolveRev = func(ctx context.Context, repo types.RepoIdentifier, rev string) (api.CommitID, error) {
		if repo.RepoID() != 2 || rev != exampleCommitSHA1 {
			t.Error("wrong arguments to Repos.ResolveRev")
		}
		return exampleCommitSHA1, nil
	}
	backend.Mocks.Repos.MockGetCommit_Return_NoCheck(t, &git.Commit{ID: exampleCommitSHA1})

	git.Mocks.Stat = func(commit api.CommitID, path string) (os.FileInfo, error) {
		if string(commit) != exampleCommitSHA1 || path != "foo bar" {
			t.Error("wrong arguments to Stat")
		}
		return &util.FileInfo{Name_: "", Mode_: os.ModeDir}, nil
	}
	git.Mocks.ReadDir = func(commit api.CommitID, name string, recurse bool) ([]os.FileInfo, error) {
		if string(commit) != exampleCommitSHA1 || name != "foo bar" || recurse {
			t.Error("wrong arguments to RepoTree.Get")
		}
		return []os.FileInfo{
			&util.FileInfo{Name_: "testDirectory", Mode_: os.ModeDir},
			&util.FileInfo{Name_: "Geoffrey's random queries.32r242442bf", Mode_: os.ModeDir},
			&util.FileInfo{Name_: "testFile", Mode_: 0},
			&util.FileInfo{Name_: "% token.4288249258.sql", Mode_: 0},
		}, nil
	}
	defer git.ResetMocks()

	gqltesting.RunTests(t, []*gqltesting.Test{
		{
			Schema: GraphQLSchema,
			Query: `
				{
					repository(name: "github.com/gorilla/mux") {
						commit(rev: "` + exampleCommitSHA1 + `") {
							tree(path: "foo bar") {
								directories {
									name
									path
									url
								}
								files {
									name
									path
									url
								}
							}
						}
					}
				}
			`,
			ExpectedResult: `
{
  "repository": {
    "commit": {
      "tree": {
        "directories": [
          {
            "name": "Geoffrey's random queries.32r242442bf",
            "path": "foo bar/Geoffrey's random queries.32r242442bf",
            "url": "/github.com/gorilla/mux@1234567890123456789012345678901234567890/-/tree/foo%20bar/Geoffrey%27s%20random%20queries.32r242442bf"
          },
          {
            "name": "testDirectory",
            "path": "foo bar/testDirectory",
            "url": "/github.com/gorilla/mux@1234567890123456789012345678901234567890/-/tree/foo%20bar/testDirectory"
          }
        ],
        "files": [
          {
            "name": "% token.4288249258.sql",
            "path": "foo bar/% token.4288249258.sql",
            "url": "/github.com/gorilla/mux@1234567890123456789012345678901234567890/-/blob/foo%20bar/%25%20token.4288249258.sql"
          },
          {
            "name": "testFile",
            "path": "foo bar/testFile",
            "url": "/github.com/gorilla/mux@1234567890123456789012345678901234567890/-/blob/foo%20bar/testFile"
          }
        ]
      }
    }
  }
}
			`,
		},
	})
}
