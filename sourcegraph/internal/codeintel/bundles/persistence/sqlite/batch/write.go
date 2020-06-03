package batch

import (
	"context"
	"runtime"

	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/persistence/serialization"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/persistence/sqlite/util"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/types"
	"github.com/sourcegraph/sourcegraph/internal/sqliteutil"
)

// NumWriterRoutines is the number of goroutines launched to write database records.
var NumWriterRoutines = runtime.NumCPU() * 2

// KeyedDocument pairs a document with its path.
type KeyedDocument struct {
	Path     string
	Document types.DocumentData
}

// IndexedResultChunk pairs a result chunk with its index.
type IndexedResultChunk struct {
	Index       int
	ResultChunk types.ResultChunkData
}

// WriteDocuments serializes the given documents and writes them in batch to the given execable.
func WriteDocuments(ctx context.Context, s sqliteutil.Execable, tableName string, serializer serialization.Serializer, documents map[string]types.DocumentData) error {
	ch := make(chan KeyedDocument, len(documents))

	go func() {
		defer close(ch)

		for k, v := range documents {
			ch <- KeyedDocument{Path: k, Document: v}
		}
	}()

	return WriteDocumentsChan(ctx, s, tableName, serializer, ch)
}

// WriteResultChunks serializes the given result chunks and writes them in batch to the given execable.
func WriteResultChunks(ctx context.Context, s sqliteutil.Execable, tableName string, serializer serialization.Serializer, resultChunks map[int]types.ResultChunkData) error {
	ch := make(chan IndexedResultChunk, len(resultChunks))

	go func() {
		defer close(ch)

		for i, v := range resultChunks {
			ch <- IndexedResultChunk{Index: i, ResultChunk: v}
		}
	}()

	return WriteResultChunksChan(ctx, s, tableName, serializer, ch)
}

// WriteMonikerLocations serializes the given moniker locations and writes them in batch to the given execable.
func WriteMonikerLocations(ctx context.Context, s sqliteutil.Execable, tableName string, serializer serialization.Serializer, monikerLocations []types.MonikerLocations) error {
	ch := make(chan types.MonikerLocations, len(monikerLocations))

	go func() {
		defer close(ch)

		for _, ml := range monikerLocations {
			ch <- ml
		}
	}()

	return WriteMonikerLocationsChan(ctx, s, tableName, serializer, ch)
}

// WriteDocumentsChan serializes and writes the document data read from the given channel.
func WriteDocumentsChan(ctx context.Context, s sqliteutil.Execable, tableName string, serializer serialization.Serializer, ch <-chan KeyedDocument) error {
	return util.InvokeN(NumWriterRoutines, func() error {
		inserter := sqliteutil.NewBatchInserter(s, tableName, "path", "data")

		for v := range ch {
			data, err := serializer.MarshalDocumentData(v.Document)
			if err != nil {
				return errors.Wrap(err, "serializer.MarshalDocumentData")
			}

			if err := inserter.Insert(ctx, v.Path, data); err != nil {
				return errors.Wrap(err, "inserter.Insert")
			}
		}

		if err := inserter.Flush(ctx); err != nil {
			return errors.Wrap(err, "inserter.Flush")
		}

		return nil
	})
}

// WriteResultChunksChan serializes and writes the result chunk data read from the given channel.
func WriteResultChunksChan(ctx context.Context, s sqliteutil.Execable, tableName string, serializer serialization.Serializer, ch <-chan IndexedResultChunk) error {
	return util.InvokeN(NumWriterRoutines, func() error {
		inserter := sqliteutil.NewBatchInserter(s, tableName, "id", "data")

		for v := range ch {
			data, err := serializer.MarshalResultChunkData(v.ResultChunk)
			if err != nil {
				return errors.Wrap(err, "serializer.MarshalResultChunkData")
			}

			if err := inserter.Insert(ctx, v.Index, data); err != nil {
				return errors.Wrap(err, "inserter.Insert")
			}
		}

		if err := inserter.Flush(ctx); err != nil {
			return errors.Wrap(err, "inserter.Flush")
		}

		return nil
	})
}

// WriteMonikerLocationsChan serializes and writes the moniker location data read from the given channel.
func WriteMonikerLocationsChan(ctx context.Context, s sqliteutil.Execable, tableName string, serializer serialization.Serializer, ch <-chan types.MonikerLocations) error {
	return util.InvokeN(NumWriterRoutines, func() error {
		inserter := sqliteutil.NewBatchInserter(s, tableName, "scheme", "identifier", "data")

		for v := range ch {
			data, err := serializer.MarshalLocations(v.Locations)
			if err != nil {
				return errors.Wrap(err, "serializer.MarshalLocations")
			}

			if err := inserter.Insert(ctx, v.Scheme, v.Identifier, data); err != nil {
				return errors.Wrap(err, "inserter.Insert")
			}
		}

		if err := inserter.Flush(ctx); err != nil {
			return errors.Wrap(err, "inserter.Flush")
		}

		return nil
	})
}
