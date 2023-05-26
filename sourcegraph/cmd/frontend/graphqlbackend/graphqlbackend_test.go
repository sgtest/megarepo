package graphqlbackend

import (
	"encoding/base64"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"log"
	"net/http"
	"net/http/httptest"
	"net/url"
	"os"
	"reflect"
	"strings"
	"sync/atomic"
	"testing"

	"github.com/inconshreveable/log15"
	sglog "github.com/sourcegraph/log"
	"github.com/sourcegraph/log/logtest"
	"github.com/stretchr/testify/assert"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/conftypes"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/gitserver/protocol"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

func TestMain(m *testing.M) {
	flag.Parse()
	if !testing.Verbose() {
		log15.Root().SetHandler(log15.DiscardHandler())
		log.SetOutput(io.Discard)
		logtest.InitWithLevel(m, sglog.LevelNone)
	} else {
		logtest.Init(m)
	}
	os.Exit(m.Run())
}

func BenchmarkPrometheusFieldName(b *testing.B) {
	tests := [][3]string{
		{"Query", "settingsSubject", "settingsSubject"},
		{"SearchResultMatch", "highlights", "highlights"},
		{"TreeEntry", "isSingleChild", "isSingleChild"},
		{"NoMatch", "NotMatch", "other"},
	}
	for i, t := range tests {
		typeName, fieldName, want := t[0], t[1], t[2]
		b.Run(fmt.Sprintf("test-%v", i), func(b *testing.B) {
			for i := 0; i < b.N; i++ {
				got := prometheusFieldName(typeName, fieldName)
				if got != want {
					b.Fatalf("got %q want %q", got, want)
				}
			}
		})
	}
}

func TestRepository(t *testing.T) {
	db := database.NewMockDB()
	repos := database.NewMockRepoStore()
	repos.GetByNameFunc.SetDefaultReturn(&types.Repo{ID: 2, Name: "github.com/gorilla/mux"}, nil)
	db.ReposFunc.SetDefaultReturn(repos)
	RunTests(t, []*Test{
		{
			Schema: mustParseGraphQLSchema(t, db),
			Query: `
				{
					repository(name: "github.com/gorilla/mux") {
						name
					}
				}
			`,
			ExpectedResult: `
				{
					"repository": {
						"name": "github.com/gorilla/mux"
					}
				}
			`,
		},
	})
}

func TestRecloneRepository(t *testing.T) {
	resetMocks()

	var gitserverCalled atomic.Bool
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusOK)
		resp := protocol.RepoUpdateResponse{}
		gitserverCalled.Store(true)
		json.NewEncoder(w).Encode(&resp)
	}))
	defer srv.Close()

	serverURL, err := url.Parse(srv.URL)
	assert.Nil(t, err)
	conf.Mock(&conf.Unified{
		ServiceConnectionConfig: conftypes.ServiceConnections{
			GitServers: []string{serverURL.Host},
		},
	})
	defer conf.Mock(nil)

	repos := database.NewMockRepoStore()
	repos.GetFunc.SetDefaultReturn(&types.Repo{ID: 1, Name: "github.com/gorilla/mux"}, nil)

	users := database.NewMockUserStore()
	users.GetByCurrentAuthUserFunc.SetDefaultReturn(&types.User{ID: 1, SiteAdmin: true}, nil)

	gitserverRepos := database.NewMockGitserverRepoStore()
	gitserverRepos.GetByIDFunc.SetDefaultReturn(&types.GitserverRepo{RepoID: 1, CloneStatus: "cloned"}, nil)

	db := database.NewMockDB()
	db.ReposFunc.SetDefaultReturn(repos)
	db.UsersFunc.SetDefaultReturn(users)
	db.GitserverReposFunc.SetDefaultReturn(gitserverRepos)

	called := backend.Mocks.Repos.MockDeleteRepositoryFromDisk(t, 1)

	repoID := base64.StdEncoding.EncodeToString([]byte("Repository:1"))

	RunTests(t, []*Test{
		{
			Schema: mustParseGraphQLSchema(t, db),
			Query: fmt.Sprintf(`
                mutation {
                    recloneRepository(repo: "%s") {
                        alwaysNil
                    }
                }
            `, repoID),
			ExpectedResult: `
                {
                    "recloneRepository": {
                        "alwaysNil": null
                    }
                }
            `,
		},
	})

	assert.True(t, *called)
	assert.True(t, gitserverCalled.Load())
}

func TestDeleteRepositoryFromDisk(t *testing.T) {
	resetMocks()

	repos := database.NewMockRepoStore()

	users := database.NewMockUserStore()
	users.GetByCurrentAuthUserFunc.SetDefaultReturn(&types.User{ID: 1, SiteAdmin: true}, nil)
	called := backend.Mocks.Repos.MockDeleteRepositoryFromDisk(t, 1)

	gitserverRepos := database.NewMockGitserverRepoStore()
	gitserverRepos.GetByIDFunc.SetDefaultReturn(&types.GitserverRepo{RepoID: 1, CloneStatus: "cloned"}, nil)

	db := database.NewMockDB()
	db.ReposFunc.SetDefaultReturn(repos)
	db.UsersFunc.SetDefaultReturn(users)
	db.GitserverReposFunc.SetDefaultReturn(gitserverRepos)
	repoID := base64.StdEncoding.EncodeToString([]byte("Repository:1"))

	RunTests(t, []*Test{
		{
			Schema: mustParseGraphQLSchema(t, db),
			Query: fmt.Sprintf(`
                mutation {
                    deleteRepositoryFromDisk(repo: "%s") {
                        alwaysNil
                    }
                }
            `, repoID),
			ExpectedResult: `
                {
                    "deleteRepositoryFromDisk": {
                        "alwaysNil": null
                    }
                }
            `,
		},
	})

	assert.True(t, *called)
}

func TestResolverTo(t *testing.T) {
	db := database.NewMockDB()
	// This test exists purely to remove some non determinism in our tests
	// run. The To* resolvers are stored in a map in our graphql
	// implementation => the order we call them is non deterministic =>
	// codecov coverage reports are noisy.
	resolvers := []any{
		&FileMatchResolver{db: db},
		&NamespaceResolver{},
		&NodeResolver{},
		&RepositoryResolver{db: db, logger: logtest.Scoped(t)},
		&CommitSearchResultResolver{},
		&gitRevSpec{},
		&settingsSubjectResolver{},
		&statusMessageResolver{db: db},
	}
	for _, r := range resolvers {
		typ := reflect.TypeOf(r)
		t.Run(typ.Name(), func(t *testing.T) {
			for i := 0; i < typ.NumMethod(); i++ {
				if name := typ.Method(i).Name; strings.HasPrefix(name, "To") {
					reflect.ValueOf(r).MethodByName(name).Call(nil)
				}
			}
		})
	}

	t.Run("GitTreeEntryResolver", func(t *testing.T) {
		blobStat, err := os.Stat("graphqlbackend_test.go")
		if err != nil {
			t.Fatalf("unexpected error opening file: %s", err)
		}
		blobEntry := &GitTreeEntryResolver{db: db, stat: blobStat}
		if _, isBlob := blobEntry.ToGitBlob(); !isBlob {
			t.Errorf("expected blobEntry to be blob")
		}
		if _, isTree := blobEntry.ToGitTree(); isTree {
			t.Errorf("expected blobEntry to be blob, but is tree")
		}

		treeStat, err := os.Stat(".")
		if err != nil {
			t.Fatalf("unexpected error opening directory: %s", err)
		}
		treeEntry := &GitTreeEntryResolver{db: db, stat: treeStat}
		if _, isBlob := treeEntry.ToGitBlob(); isBlob {
			t.Errorf("expected treeEntry to be tree, but is blob")
		}
		if _, isTree := treeEntry.ToGitTree(); !isTree {
			t.Errorf("expected treeEntry to be tree")
		}
	})
}

type roundTripFunc func(*http.Request) (*http.Response, error)

func (f roundTripFunc) RoundTrip(r *http.Request) (*http.Response, error) {
	return f(r)
}

// copied from the github client, just the fields we need
type githubRepository struct {
	FullName string `json:"full_name"`
	Private  bool   `json:"private"`
}

type gitlabRepository struct {
	Visibility        string `json:"visibility"`
	ID                int    `json:"id"`
	PathWithNamespace string `json:"path_with_namespace"`
}
