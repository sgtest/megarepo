package writer

import (
	"context"
	"path/filepath"

	"golang.org/x/sync/semaphore"

	"github.com/sourcegraph/sourcegraph/cmd/symbols/gitserver"
	"github.com/sourcegraph/sourcegraph/cmd/symbols/internal/api/observability"
	"github.com/sourcegraph/sourcegraph/cmd/symbols/internal/database/store"
	"github.com/sourcegraph/sourcegraph/cmd/symbols/parser"
	"github.com/sourcegraph/sourcegraph/cmd/symbols/types"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/diskcache"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type DatabaseWriter interface {
	WriteDBFile(ctx context.Context, args types.SearchArgs, tempDBFile string) error
}

type databaseWriter struct {
	path            string
	gitserverClient gitserver.GitserverClient
	parser          parser.Parser
	sem             *semaphore.Weighted
}

func NewDatabaseWriter(
	path string,
	gitserverClient gitserver.GitserverClient,
	parser parser.Parser,
	sem *semaphore.Weighted,
) DatabaseWriter {
	return &databaseWriter{
		path:            path,
		gitserverClient: gitserverClient,
		parser:          parser,
		sem:             sem,
	}
}

func (w *databaseWriter) WriteDBFile(ctx context.Context, args types.SearchArgs, dbFile string) error {
	w.sem.Acquire(ctx, 1)
	defer w.sem.Release(1)

	if newestDBFile, oldCommit, ok, err := w.getNewestCommit(ctx, args); err != nil {
		return err
	} else if ok {
		if ok, err := w.writeFileIncrementally(ctx, args, dbFile, newestDBFile, oldCommit); err != nil || ok {
			return err
		}
	}

	return w.writeDBFile(ctx, args, dbFile)
}

func (w *databaseWriter) getNewestCommit(ctx context.Context, args types.SearchArgs) (dbFile string, commit string, ok bool, err error) {
	components := []string{}
	components = append(components, w.path)
	components = append(components, diskcache.EncodeKeyComponents(repoKey(args.Repo))...)

	newest, err := findNewestFile(filepath.Join(components...))
	if err != nil || newest == "" {
		return "", "", false, err
	}

	err = store.WithSQLiteStore(newest, func(db store.Store) (err error) {
		if commit, ok, err = db.GetCommit(ctx); err != nil {
			return errors.Wrap(err, "store.GetCommit")
		}

		return nil
	})

	return newest, commit, ok, err
}

func (w *databaseWriter) writeDBFile(ctx context.Context, args types.SearchArgs, dbFile string) error {
	observability.SetParseAmount(ctx, observability.FullParse)

	return w.parseAndWriteInTransaction(ctx, args, nil, dbFile, func(tx store.Store, symbolOrErrors <-chan parser.SymbolOrError) error {
		if err := tx.CreateMetaTable(ctx); err != nil {
			return errors.Wrap(err, "store.CreateMetaTable")
		}
		if err := tx.CreateSymbolsTable(ctx); err != nil {
			return errors.Wrap(err, "store.CreateSymbolsTable")
		}
		if err := tx.InsertMeta(ctx, string(args.CommitID)); err != nil {
			return errors.Wrap(err, "store.InsertMeta")
		}
		if err := tx.WriteSymbols(ctx, symbolOrErrors); err != nil {
			return errors.Wrap(err, "store.WriteSymbols")
		}
		if err := tx.CreateSymbolIndexes(ctx); err != nil {
			return errors.Wrap(err, "store.CreateSymbolIndexes")
		}

		return nil
	})
}

func (w *databaseWriter) writeFileIncrementally(ctx context.Context, args types.SearchArgs, dbFile, newestDBFile, oldCommit string) (bool, error) {
	observability.SetParseAmount(ctx, observability.PartialParse)

	changes, err := w.gitserverClient.GitDiff(ctx, args.Repo, api.CommitID(oldCommit), args.CommitID)
	if err != nil {
		return false, errors.Wrap(err, "gitserverClient.GitDiff")
	}

	// Paths to re-parse
	addedOrModifiedPaths := append(changes.Added, changes.Modified...)

	// Paths to modify in the database
	addedModifiedOrDeletedPaths := append(addedOrModifiedPaths, changes.Deleted...)

	if err := copyFile(newestDBFile, dbFile); err != nil {
		return false, err
	}

	return true, w.parseAndWriteInTransaction(ctx, args, addedOrModifiedPaths, dbFile, func(tx store.Store, symbolOrErrors <-chan parser.SymbolOrError) error {
		if err := tx.UpdateMeta(ctx, string(args.CommitID)); err != nil {
			return errors.Wrap(err, "store.UpdateMeta")
		}
		if err := tx.DeletePaths(ctx, addedModifiedOrDeletedPaths); err != nil {
			return errors.Wrap(err, "store.DeletePaths")
		}
		if err := tx.WriteSymbols(ctx, symbolOrErrors); err != nil {
			return errors.Wrap(err, "store.WriteSymbols")
		}

		return nil
	})
}

func (w *databaseWriter) parseAndWriteInTransaction(ctx context.Context, args types.SearchArgs, paths []string, dbFile string, callback func(tx store.Store, symbolOrErrors <-chan parser.SymbolOrError) error) (err error) {
	symbolOrErrors, err := w.parser.Parse(ctx, args, paths)
	if err != nil {
		return errors.Wrap(err, "parser.Parse")
	}
	defer func() {
		if err != nil {
			go func() {
				// Drain channel on early exit
				for range symbolOrErrors {
				}
			}()
		}
	}()

	return store.WithSQLiteStoreTransaction(ctx, dbFile, func(tx store.Store) error {
		return callback(tx, symbolOrErrors)
	})
}
