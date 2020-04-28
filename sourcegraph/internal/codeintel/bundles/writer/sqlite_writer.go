package writer

import (
	"context"
	"database/sql"

	"github.com/hashicorp/go-multierror"
	"github.com/jmoiron/sqlx"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/serializer"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/types"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/writer/schema"
	"github.com/sourcegraph/sourcegraph/internal/sqliteutil"
)

type sqliteWriter struct {
	serializer          serializer.Serializer
	db                  *sqlx.DB
	tx                  *sql.Tx
	metaInserter        *sqliteutil.BatchInserter
	documentInserter    *sqliteutil.BatchInserter
	resultChunkInserter *sqliteutil.BatchInserter
	definitionInserter  *sqliteutil.BatchInserter
	referenceInserter   *sqliteutil.BatchInserter
}

var _ Writer = &sqliteWriter{}

const InternalVersion = "0.1.0"

func NewSQLiteWriter(filename string, serializer serializer.Serializer) (_ Writer, err error) {
	db, err := sqlx.Open("sqlite3_with_pcre", filename)
	if err != nil {
		return nil, err
	}
	defer func() {
		if err != nil {
			if closeErr := db.Close(); closeErr != nil {
				err = multierror.Append(err, closeErr)
			}
		}
	}()

	if _, err := db.Exec(schema.TableDefinitions); err != nil {
		return nil, err
	}

	tx, err := db.BeginTx(context.Background(), nil)
	if err != nil {
		return nil, err
	}

	metaColumns := []string{"lsifVersion", "sourcegraphVersion", "numResultChunks"}
	documentsColumns := []string{"path", "data"}
	resultChunksColumns := []string{"id", "data"}
	definitionsReferencesColumns := []string{"scheme", "identifier", "documentPath", "startLine", "startCharacter", "endLine", "endCharacter"}

	return &sqliteWriter{
		serializer:          serializer,
		db:                  db,
		tx:                  tx,
		metaInserter:        sqliteutil.NewBatchInserter(tx, "meta", metaColumns...),
		documentInserter:    sqliteutil.NewBatchInserter(tx, "documents", documentsColumns...),
		resultChunkInserter: sqliteutil.NewBatchInserter(tx, "resultChunks", resultChunksColumns...),
		definitionInserter:  sqliteutil.NewBatchInserter(tx, "definitions", definitionsReferencesColumns...),
		referenceInserter:   sqliteutil.NewBatchInserter(tx, `references`, definitionsReferencesColumns...),
	}, nil
}

func (w *sqliteWriter) WriteMeta(ctx context.Context, lsifVersion string, numResultChunks int) error {
	return w.metaInserter.Insert(ctx, lsifVersion, InternalVersion, numResultChunks)
}

func (w *sqliteWriter) WriteDocuments(ctx context.Context, documents map[string]types.DocumentData) error {
	for k, v := range documents {
		ser, err := w.serializer.MarshalDocumentData(v)
		if err != nil {
			return err
		}

		if err := w.documentInserter.Insert(ctx, k, ser); err != nil {
			return err
		}
	}
	return nil
}

func (w *sqliteWriter) WriteResultChunks(ctx context.Context, resultChunks map[int]types.ResultChunkData) error {
	for k, v := range resultChunks {
		ser, err := w.serializer.MarshalResultChunkData(v)
		if err != nil {
			return err
		}

		if err := w.resultChunkInserter.Insert(ctx, k, ser); err != nil {
			return err
		}
	}
	return nil
}

func (w *sqliteWriter) WriteDefinitions(ctx context.Context, definitions []types.DefinitionReferenceRow) error {
	for _, r := range definitions {
		if err := w.definitionInserter.Insert(ctx, r.Scheme, r.Identifier, r.URI, r.StartLine, r.StartCharacter, r.EndLine, r.EndCharacter); err != nil {
			return err
		}
	}
	return nil
}

func (w *sqliteWriter) WriteReferences(ctx context.Context, references []types.DefinitionReferenceRow) error {
	for _, r := range references {
		if err := w.referenceInserter.Insert(ctx, r.Scheme, r.Identifier, r.URI, r.StartLine, r.StartCharacter, r.EndLine, r.EndCharacter); err != nil {
			return err
		}
	}
	return nil
}

func (w *sqliteWriter) Flush(ctx context.Context) error {
	inserters := []*sqliteutil.BatchInserter{
		w.metaInserter,
		w.documentInserter,
		w.resultChunkInserter,
		w.definitionInserter,
		w.referenceInserter,
	}

	for _, inserter := range inserters {
		if err := inserter.Flush(ctx); err != nil {
			return err
		}
	}

	if err := w.tx.Commit(); err != nil {
		return err
	}

	if _, err := w.db.ExecContext(ctx, schema.IndexDefinitions); err != nil {
		return err
	}

	return nil
}

func (w *sqliteWriter) Close() (err error) {
	if closeErr := w.db.Close(); closeErr != nil {
		err = multierror.Append(err, closeErr)
	}

	return err
}
