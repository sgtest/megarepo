package database

import (
	"context"
	"database/sql"
	"time"

	"github.com/sourcegraph/log"

	gha "github.com/sourcegraph/sourcegraph/enterprise/internal/github_apps/store"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
)

type EnterpriseDB interface {
	database.DB
	CodeMonitors() CodeMonitorStore
	Perms() PermsStore
	SubRepoPerms() SubRepoPermsStore
	Codeowners() CodeownersStore
	GitHubApps() gha.GitHubAppsStore
}

func NewEnterpriseDB(db database.DB) EnterpriseDB {
	// If the underlying type already implements EnterpriseDB,
	// return that rather than wrapping it. This enables us to
	// pass a mock EnterpriseDB through as a database.DB, and
	// avoid overwriting its mocked methods by wrapping it.
	if edb, ok := db.(EnterpriseDB); ok {
		return edb
	}
	return &enterpriseDB{db}
}

type enterpriseDB struct {
	database.DB
}

func (edb *enterpriseDB) CodeMonitors() CodeMonitorStore {
	return &codeMonitorStore{Store: basestore.NewWithHandle(edb.Handle()), now: time.Now}
}

func (edb *enterpriseDB) Perms() PermsStore {
	return &permsStore{Store: basestore.NewWithHandle(edb.Handle()), clock: time.Now, ossDB: edb.DB}
}

func (edb *enterpriseDB) SubRepoPerms() SubRepoPermsStore {
	return SubRepoPermsWith(basestore.NewWithHandle(edb.Handle()))
}

func (edb *enterpriseDB) Codeowners() CodeownersStore {
	return CodeownersWith(basestore.NewWithHandle(edb.Handle()))
}

func (edb *enterpriseDB) GitHubApps() gha.GitHubAppsStore {
	return gha.GitHubAppsWith(basestore.NewWithHandle(edb.Handle()))
}

type InsightsDB interface {
	dbutil.DB
	basestore.ShareableStore

	Transact(context.Context) (InsightsDB, error)
	Done(error) error
}

func NewInsightsDB(inner *sql.DB, logger log.Logger) InsightsDB {
	return &insightsDB{basestore.NewWithHandle(basestore.NewHandleWithDB(logger, inner, sql.TxOptions{}))}
}

func NewInsightsDBWith(other basestore.ShareableStore) InsightsDB {
	return &insightsDB{basestore.NewWithHandle(other.Handle())}
}

type insightsDB struct {
	*basestore.Store
}

func (d *insightsDB) Transact(ctx context.Context) (InsightsDB, error) {
	tx, err := d.Store.Transact(ctx)
	if err != nil {
		return nil, err
	}
	return &insightsDB{tx}, nil
}

func (d *insightsDB) Done(err error) error {
	return d.Store.Done(err)
}

func (d *insightsDB) QueryContext(ctx context.Context, q string, args ...any) (*sql.Rows, error) {
	return d.Handle().QueryContext(dbconn.SkipFrameForQuerySource(ctx), q, args...)
}

func (d *insightsDB) ExecContext(ctx context.Context, q string, args ...any) (sql.Result, error) {
	return d.Handle().ExecContext(dbconn.SkipFrameForQuerySource(ctx), q, args...)
}

func (d *insightsDB) QueryRowContext(ctx context.Context, q string, args ...any) *sql.Row {
	return d.Handle().QueryRowContext(dbconn.SkipFrameForQuerySource(ctx), q, args...)
}
