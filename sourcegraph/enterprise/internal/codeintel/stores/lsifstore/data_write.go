package lsifstore

import (
	"context"

	"github.com/hashicorp/go-multierror"
	"github.com/opentracing/opentracing-go/log"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/internal/db/batch"
	"github.com/sourcegraph/sourcegraph/internal/db/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func (s *Store) WriteMeta(ctx context.Context, bundleID int, meta MetaData) (err error) {
	ctx, endObservation := s.operations.writeMeta.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("bundleID", bundleID),
	}})
	defer endObservation(1, observation.Args{})

	inserter := batch.NewBatchInserter(ctx, s.Handle().DB(), "lsif_data_metadata", "dump_id", "num_result_chunks")

	defer func() {
		if flushErr := inserter.Flush(ctx); flushErr != nil {
			err = multierror.Append(err, errors.Wrap(flushErr, "inserter.Flush"))
		}
	}()

	return inserter.Insert(ctx, bundleID, meta.NumResultChunks)
}

func (s *Store) WriteDocuments(ctx context.Context, bundleID int, documents chan KeyedDocumentData) (err error) {
	ctx, endObservation := s.operations.writeDocuments.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("bundleID", bundleID),
	}})
	defer endObservation(1, observation.Args{})

	inserter := func(inserter *batch.BatchInserter) error {
		for v := range documents {
			data, err := s.serializer.MarshalDocumentData(v.Document)
			if err != nil {
				return err
			}

			if err := inserter.Insert(ctx, bundleID, v.Path, data); err != nil {
				return err
			}
		}

		return nil
	}

	return withBatchInserter(ctx, s.Handle().DB(), "lsif_data_documents", []string{"dump_id", "path", "data"}, inserter)
}

func (s *Store) WriteResultChunks(ctx context.Context, bundleID int, resultChunks chan IndexedResultChunkData) (err error) {
	ctx, endObservation := s.operations.writeResultChunks.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("bundleID", bundleID),
	}})
	defer endObservation(1, observation.Args{})

	inserter := func(inserter *batch.BatchInserter) error {
		for v := range resultChunks {
			data, err := s.serializer.MarshalResultChunkData(v.ResultChunk)
			if err != nil {
				return err
			}

			if err := inserter.Insert(ctx, bundleID, v.Index, data); err != nil {
				return err
			}
		}

		return nil
	}

	return withBatchInserter(ctx, s.Handle().DB(), "lsif_data_result_chunks", []string{"dump_id", "idx", "data"}, inserter)
}

func (s *Store) WriteDefinitions(ctx context.Context, bundleID int, monikerLocations chan MonikerLocations) (err error) {
	ctx, endObservation := s.operations.writeDefinitions.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("bundleID", bundleID),
	}})
	defer endObservation(1, observation.Args{})

	return s.writeDefinitionReferences(ctx, bundleID, "lsif_data_definitions", monikerLocations)
}

func (s *Store) WriteReferences(ctx context.Context, bundleID int, monikerLocations chan MonikerLocations) (err error) {
	ctx, endObservation := s.operations.writeReferences.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("bundleID", bundleID),
	}})
	defer endObservation(1, observation.Args{})

	return s.writeDefinitionReferences(ctx, bundleID, "lsif_data_references", monikerLocations)
}

func (s *Store) writeDefinitionReferences(ctx context.Context, bundleID int, tableName string, monikerLocations chan MonikerLocations) error {
	inserter := func(inserter *batch.BatchInserter) error {
		for v := range monikerLocations {
			data, err := s.serializer.MarshalLocations(v.Locations)
			if err != nil {
				return err
			}

			if err := inserter.Insert(ctx, bundleID, v.Scheme, v.Identifier, data); err != nil {
				return err
			}
		}

		return nil
	}

	return withBatchInserter(ctx, s.Handle().DB(), tableName, []string{"dump_id", "scheme", "identifier", "data"}, inserter)
}

func withBatchInserter(ctx context.Context, db dbutil.DB, tableName string, columns []string, f func(inserter *batch.BatchInserter) error) (err error) {
	return goroutine.RunWorkers(goroutine.SimplePoolWorker(func() error {
		inserter := batch.NewBatchInserter(ctx, db, tableName, columns...)
		defer func() {
			if flushErr := inserter.Flush(ctx); flushErr != nil {
				err = multierror.Append(err, errors.Wrap(flushErr, "inserter.Flush"))
			}
		}()

		return f(inserter)
	}))
}
