package enqueuer

import (
	"context"

	"github.com/grafana/regexp"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/dbstore"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/repoupdater/protocol"
)

type DBStore interface {
	basestore.ShareableStore

	Handle() *basestore.TransactableHandle
	Transact(ctx context.Context) (DBStore, error)
	Done(err error) error

	GetIndexesByIDs(ctx context.Context, ids ...int) ([]dbstore.Index, error)
	DirtyRepositories(ctx context.Context) (map[int]int, error)
	IsQueued(ctx context.Context, repositoryID int, commit string) (bool, error)
	InsertIndexes(ctx context.Context, index []dbstore.Index) ([]dbstore.Index, error)
	GetIndexConfigurationByRepositoryID(ctx context.Context, repositoryID int) (dbstore.IndexConfiguration, bool, error)
}

type DBStoreShim struct {
	*dbstore.Store
}

func (db *DBStoreShim) Transact(ctx context.Context) (DBStore, error) {
	store, err := db.Store.Transact(ctx)
	if err != nil {
		return nil, err
	}
	return &DBStoreShim{store}, nil
}

var _ DBStore = &DBStoreShim{}

type RepoUpdaterClient interface {
	EnqueueRepoUpdate(ctx context.Context, repo api.RepoName) (*protocol.RepoUpdateResponse, error)
}

type GitserverClient interface {
	Head(ctx context.Context, repositoryID int) (string, bool, error)
	ListFiles(ctx context.Context, repositoryID int, commit string, pattern *regexp.Regexp) ([]string, error)
	FileExists(ctx context.Context, repositoryID int, commit, file string) (bool, error)
	RawContents(ctx context.Context, repositoryID int, commit, file string) ([]byte, error)
	ResolveRevision(ctx context.Context, repositoryID int, versionString string) (api.CommitID, error)
}

type gitClient struct {
	client       GitserverClient
	repositoryID int
	commit       string
}

func newGitClient(client GitserverClient, repositoryID int, commit string) gitClient {
	return gitClient{
		client:       client,
		repositoryID: repositoryID,
		commit:       commit,
	}
}

func (c gitClient) ListFiles(ctx context.Context, pattern *regexp.Regexp) ([]string, error) {
	return c.client.ListFiles(ctx, c.repositoryID, c.commit, pattern)
}

func (c gitClient) FileExists(ctx context.Context, file string) (bool, error) {
	return c.client.FileExists(ctx, c.repositoryID, c.commit, file)
}

func (c gitClient) RawContents(ctx context.Context, file string) ([]byte, error) {
	return c.client.RawContents(ctx, c.repositoryID, c.commit, file)
}
