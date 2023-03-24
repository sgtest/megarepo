package store

import (
	"context"
	"time"

	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"
	otlog "github.com/opentracing/opentracing-go/log"

	rankingshared "github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/ranking/internal/shared"
	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/shared"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/batch"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func (s *store) InsertDefinitionsForRanking(
	ctx context.Context,
	rankingGraphKey string,
	rankingBatchNumber int,
	definitions []shared.RankingDefinitions,
) (err error) {
	ctx, _, endObservation := s.operations.insertDefinitionsForRanking.With(
		ctx,
		&err,
		observation.Args{},
	)
	defer endObservation(1, observation.Args{})

	tx, err := s.db.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Done(err) }()

	inserter := func(inserter *batch.Inserter) error {
		batchDefinitions := make([]shared.RankingDefinitions, 0, rankingBatchNumber)
		for _, def := range definitions {
			batchDefinitions = append(batchDefinitions, def)

			if len(batchDefinitions) == rankingBatchNumber {
				if err := insertDefinitions(ctx, inserter, rankingGraphKey, batchDefinitions); err != nil {
					return err
				}
				batchDefinitions = make([]shared.RankingDefinitions, 0, rankingBatchNumber)
			}
		}

		if len(batchDefinitions) > 0 {
			if err := insertDefinitions(ctx, inserter, rankingGraphKey, batchDefinitions); err != nil {
				return err
			}
		}

		return nil
	}

	if err := batch.WithInserter(
		ctx,
		tx.Handle(),
		"codeintel_ranking_definitions",
		batch.MaxNumPostgresParameters,
		[]string{
			"upload_id",
			"symbol_name",
			"document_path",
			"graph_key",
		},
		inserter,
	); err != nil {
		return err
	}

	return nil
}

func insertDefinitions(
	ctx context.Context,
	inserter *batch.Inserter,
	rankingGraphKey string,
	definitions []shared.RankingDefinitions,
) error {
	for _, def := range definitions {
		if err := inserter.Insert(
			ctx,
			def.UploadID,
			def.SymbolName,
			def.DocumentPath,
			rankingGraphKey,
		); err != nil {
			return err
		}
	}
	return nil
}

func (s *store) InsertReferencesForRanking(
	ctx context.Context,
	rankingGraphKey string,
	rankingBatchNumber int,
	references shared.RankingReferences,
) (err error) {
	ctx, _, endObservation := s.operations.insertReferencesForRanking.With(
		ctx,
		&err,
		observation.Args{},
	)
	defer endObservation(1, observation.Args{})

	tx, err := s.db.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Done(err) }()

	inserter := func(inserter *batch.Inserter) error {
		batchSymbolNames := make([]string, 0, rankingBatchNumber)
		for _, ref := range references.SymbolNames {
			batchSymbolNames = append(batchSymbolNames, ref)

			if len(batchSymbolNames) == rankingBatchNumber {
				if err := inserter.Insert(ctx, references.UploadID, pq.Array(batchSymbolNames), rankingGraphKey); err != nil {
					return err
				}
				batchSymbolNames = make([]string, 0, rankingBatchNumber)
			}
		}

		if len(batchSymbolNames) > 0 {
			if err := inserter.Insert(ctx, references.UploadID, pq.Array(batchSymbolNames), rankingGraphKey); err != nil {
				return err
			}
		}

		return nil
	}

	if err := batch.WithInserter(
		ctx,
		tx.Handle(),
		"codeintel_ranking_references",
		batch.MaxNumPostgresParameters,
		[]string{"upload_id", "symbol_names", "graph_key"},
		inserter,
	); err != nil {
		return err
	}

	return nil
}

func (s *store) InsertPathCountInputs(
	ctx context.Context,
	derivativeGraphKey string,
	batchSize int,
) (
	numReferenceRecordsProcessed int,
	numInputsInserted int,
	err error,
) {
	ctx, _, endObservation := s.operations.insertPathCountInputs.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	graphKey, ok := rankingshared.GraphKeyFromDerivativeGraphKey(derivativeGraphKey)
	if !ok {
		return 0, 0, errors.Newf("unexpected derivative graph key %q", derivativeGraphKey)
	}

	rows, err := s.db.Query(ctx, sqlf.Sprintf(
		insertPathCountInputsQuery,
		graphKey,
		derivativeGraphKey,
		batchSize,
		derivativeGraphKey,
		derivativeGraphKey,
		graphKey,
		derivativeGraphKey,
	))
	if err != nil {
		return 0, 0, err
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	for rows.Next() {
		if err := rows.Scan(
			&numReferenceRecordsProcessed,
			&numInputsInserted,
		); err != nil {
			return 0, 0, err
		}
	}

	return numReferenceRecordsProcessed, numInputsInserted, nil
}

const insertPathCountInputsQuery = `
WITH
refs AS (
	SELECT
		rr.id,
		rr.upload_id,
		rr.symbol_names
	FROM codeintel_ranking_references rr
	WHERE
		rr.graph_key = %s AND
		NOT EXISTS (
			SELECT 1
			FROM codeintel_ranking_references_processed rrp
			WHERE
				rrp.graph_key = %s AND
				rrp.codeintel_ranking_reference_id = rr.id
		)
	ORDER BY rr.id
	LIMIT %s
),
locked_refs AS (
	INSERT INTO codeintel_ranking_references_processed (graph_key, codeintel_ranking_reference_id)
	SELECT %s, r.id FROM refs r
	ON CONFLICT DO NOTHING
	RETURNING codeintel_ranking_reference_id
),
processable_symbols AS (
	SELECT r.symbol_names
	FROM locked_refs lr
	JOIN refs r ON r.id = lr.codeintel_ranking_reference_id
	JOIN lsif_uploads u ON u.id = r.upload_id
	WHERE
		-- Do not re-process references for repository/root/indexers that have already been
		-- processed. We'll still insert a processed reference so that we know we've done the
		-- "work", but we'll simply no-op the counts for this input.
		NOT EXISTS (
			SELECT 1
			FROM lsif_uploads u2
			JOIN codeintel_ranking_references rr ON rr.upload_id = u2.id
			JOIN codeintel_ranking_references_processed rrp ON rrp.codeintel_ranking_reference_id = rr.id
			WHERE
				rrp.graph_key = %s AND
				u.repository_id = u2.repository_id AND
				u.root = u2.root AND
				u.indexer = u2.indexer AND
				u.id != u2.id
		) AND
		-- For multiple references for the same repository/root/indexer in THIS batch, we want to
		-- process the one associated with the most recently processed upload record. This should
		-- maximize fresh results.
		NOT EXISTS (
			SELECT 1
			FROM locked_refs lr2
			JOIN refs r2 ON r2.id = lr2.codeintel_ranking_reference_id
			JOIN lsif_uploads u2 ON u2.id = r2.upload_id
			WHERE
				u.repository_id = u2.repository_id AND
				u.root = u2.root AND
				u.indexer = u2.indexer AND
				u.finished_at < u2.finished_at
		)
),
referenced_symbols AS (
	SELECT unnest(r.symbol_names) AS symbol_name
	FROM processable_symbols r
),
referenced_definitions AS (
	SELECT
		u.repository_id,
		rd.document_path,
		rd.graph_key,
		COUNT(*) AS count
	FROM codeintel_ranking_definitions rd
	JOIN referenced_symbols rs ON rs.symbol_name = rd.symbol_name
	JOIN lsif_uploads u ON u.id = rd.upload_id
	WHERE rd.graph_key = %s
	GROUP BY u.repository_id, rd.document_path, rd.graph_key
),
ins AS (
	INSERT INTO codeintel_ranking_path_counts_inputs (repository_id, document_path, count, graph_key)
	SELECT
		rx.repository_id,
		rx.document_path,
		SUM(rx.count),
		%s
	FROM referenced_definitions rx
	GROUP BY rx.repository_id, rx.document_path
	RETURNING 1
)
SELECT
	(SELECT COUNT(*) FROM locked_refs),
	(SELECT COUNT(*) FROM ins)
`

func (s *store) InsertInitialPathCounts(
	ctx context.Context,
	derivativeGraphKey string,
	batchSize int,
) (
	numInitialPathsProcessed int,
	numInitialPathRanksInserted int,
	err error,
) {
	ctx, _, endObservation := s.operations.insertInitialPathCounts.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	graphKey, ok := rankingshared.GraphKeyFromDerivativeGraphKey(derivativeGraphKey)
	if !ok {
		return 0, 0, errors.Newf("unexpected derivative graph key %q", derivativeGraphKey)
	}

	rows, err := s.db.Query(ctx, sqlf.Sprintf(
		insertInitialPathCountsInputsQuery,
		graphKey,
		derivativeGraphKey,
		batchSize,
		derivativeGraphKey,
		derivativeGraphKey,
	))
	if err != nil {
		return 0, 0, err
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	for rows.Next() {
		if err := rows.Scan(
			&numInitialPathsProcessed,
			&numInitialPathRanksInserted,
		); err != nil {
			return 0, 0, err
		}
	}

	return numInitialPathsProcessed, numInitialPathRanksInserted, nil
}

const insertInitialPathCountsInputsQuery = `
WITH
unprocessed_path_counts AS (
	SELECT
		ipr.id,
		ipr.upload_id,
		ipr.document_path,
		ipr.graph_key
	FROM codeintel_initial_path_ranks ipr
	WHERE
		ipr.graph_key = %s AND
		NOT EXISTS (
			SELECT 1
			FROM codeintel_initial_path_ranks_processed prp
			WHERE
				prp.graph_key = %s AND
				prp.codeintel_initial_path_ranks_id = ipr.id
		)
	ORDER BY ipr.id
	LIMIT %s
),
locked_path_counts AS (
	INSERT INTO codeintel_initial_path_ranks_processed (graph_key, codeintel_initial_path_ranks_id)
	SELECT
		%s,
		upc.id
	FROM unprocessed_path_counts upc
	ON CONFLICT DO NOTHING
	RETURNING codeintel_initial_path_ranks_id
),
ins AS (
	INSERT INTO codeintel_ranking_path_counts_inputs (repository_id, document_path, count, graph_key)
	SELECT
		u.repository_id,
		upc.document_path,
		0,
		%s
	FROM locked_path_counts lpc
	JOIN unprocessed_path_counts upc on upc.id = lpc.codeintel_initial_path_ranks_id
	JOIN lsif_uploads u ON u.id = upc.upload_id
	RETURNING 1
)
SELECT
	(SELECT COUNT(*) FROM locked_path_counts),
	(SELECT COUNT(*) FROM ins)
`

func (s *store) InsertInitialPathRanks(ctx context.Context, uploadID int, documentPath []string, graphKey string) (err error) {
	ctx, _, endObservation := s.operations.insertInitialPathRanks.With(
		ctx,
		&err,
		observation.Args{LogFields: []otlog.Field{
			otlog.String("graphKey", graphKey),
		}},
	)
	defer endObservation(1, observation.Args{})

	tx, err := s.db.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Done(err) }()

	inserter := func(inserter *batch.Inserter) error {
		for _, path := range documentPath {
			if err := inserter.Insert(ctx, path); err != nil {
				return err
			}
		}

		return nil
	}

	if err := tx.Exec(ctx, sqlf.Sprintf(createInitialPathTemporaryTableQuery)); err != nil {
		return err
	}

	if err := batch.WithInserter(
		ctx,
		tx.Handle(),
		"t_codeintel_initial_path_ranks",
		batch.MaxNumPostgresParameters,
		[]string{"document_path"},
		inserter,
	); err != nil {
		return err
	}

	if err = tx.Exec(ctx, sqlf.Sprintf(insertInitialPathRankCountsQuery, uploadID, graphKey)); err != nil {
		return err
	}

	return nil
}

const createInitialPathTemporaryTableQuery = `
CREATE TEMPORARY TABLE IF NOT EXISTS t_codeintel_initial_path_ranks (
	document_path text NOT NULL
)
ON COMMIT DROP
`

const insertInitialPathRankCountsQuery = `
INSERT INTO codeintel_initial_path_ranks (upload_id, document_path, graph_key)
	SELECT
		%s,
		document_path,
		%s
	FROM t_codeintel_initial_path_ranks
`

func (s *store) InsertPathRanks(
	ctx context.Context,
	derivativeGraphKey string,
	batchSize int,
) (numPathRanksInserted int, numInputsProcessed int, err error) {
	ctx, _, endObservation := s.operations.insertPathRanks.With(
		ctx,
		&err,
		observation.Args{LogFields: []otlog.Field{
			otlog.String("derivativeGraphKey", derivativeGraphKey),
		}},
	)
	defer endObservation(1, observation.Args{})

	_, ok := rankingshared.GraphKeyFromDerivativeGraphKey(derivativeGraphKey)
	if !ok {
		return 0, 0, errors.Newf("unexpected derivative graph key %q", derivativeGraphKey)
	}

	rows, err := s.db.Query(ctx, sqlf.Sprintf(
		insertPathRanksQuery,
		derivativeGraphKey,
		batchSize,
		derivativeGraphKey,
	))
	if err != nil {
		return 0, 0, err
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	if !rows.Next() {
		return 0, 0, errors.New("no rows from count")
	}

	if err = rows.Scan(&numPathRanksInserted, &numInputsProcessed); err != nil {
		return 0, 0, err
	}

	return numPathRanksInserted, numInputsProcessed, nil
}

const insertPathRanksQuery = `
WITH
input_ranks AS (
	SELECT
		pci.id,
		pci.repository_id,
		pci.document_path AS path,
		pci.count
	FROM codeintel_ranking_path_counts_inputs pci
	WHERE
		pci.graph_key = %s AND
		NOT pci.processed AND
		EXISTS (
			SELECT 1 FROM repo r
			WHERE
				r.id = pci.repository_id AND
				r.deleted_at IS NULL AND
				r.blocked IS NULL
		)
	ORDER BY pci.graph_key, pci.repository_id, pci.id
	LIMIT %s
	FOR UPDATE SKIP LOCKED
),
processed AS (
	UPDATE codeintel_ranking_path_counts_inputs
	SET processed = true
	WHERE id IN (SELECT ir.id FROM input_ranks ir)
	RETURNING 1
),
inserted AS (
	INSERT INTO codeintel_path_ranks AS pr (repository_id, graph_key, payload)
	SELECT
		temp.repository_id,
		%s,
		jsonb_object_agg(temp.path, temp.count)
	FROM (
		SELECT
			cr.repository_id,
			cr.path,
			SUM(count) AS count
		FROM input_ranks cr
		GROUP BY cr.repository_id, cr.path
	) temp
	GROUP BY temp.repository_id
	ON CONFLICT (repository_id) DO UPDATE SET
		graph_key = EXCLUDED.graph_key,
		payload = CASE
			WHEN pr.graph_key != EXCLUDED.graph_key
				THEN EXCLUDED.payload
			ELSE
				(
					SELECT jsonb_object_agg(key, sum) FROM (
						SELECT key, SUM(value::int) AS sum
						FROM
							(
								SELECT * FROM jsonb_each(pr.payload)
								UNION
								SELECT * FROM jsonb_each(EXCLUDED.payload)
							) AS both_payloads
						GROUP BY key
					) AS combined_json
				)
			END
	RETURNING 1
)
SELECT
	(SELECT COUNT(*) FROM processed) AS num_processed,
	(SELECT COUNT(*) FROM inserted) AS num_inserted
`

// TODO - configure via envvar
const vacuumBatchSize = 100

// TODO - configure via envvar
var threshold = time.Duration(1) * time.Hour

func (s *store) VacuumStaleDefinitions(ctx context.Context, graphKey string) (
	numDefinitionRecordsScanned int,
	numStaleDefinitionRecordsDeleted int,
	err error,
) {
	ctx, _, endObservation := s.operations.vacuumStaleDefinitions.With(ctx, &err, observation.Args{LogFields: []otlog.Field{}})
	defer endObservation(1, observation.Args{})

	rows, err := s.db.Query(ctx, sqlf.Sprintf(
		vacuumStaleDefinitionsQuery,
		graphKey, int(threshold/time.Hour), vacuumBatchSize,
	))
	if err != nil {
		return 0, 0, err
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	for rows.Next() {
		if err := rows.Scan(
			&numDefinitionRecordsScanned,
			&numStaleDefinitionRecordsDeleted,
		); err != nil {
			return 0, 0, err
		}
	}

	return numDefinitionRecordsScanned, numStaleDefinitionRecordsDeleted, nil
}

const vacuumStaleDefinitionsQuery = `
WITH
locked_definitions AS (
	SELECT
		rd.id,
		u.repository_id,
		rd.upload_id
	FROM codeintel_ranking_definitions rd
	JOIN lsif_uploads u ON u.id = rd.upload_id
	WHERE
		rd.graph_key = %s AND
		(rd.last_scanned_at IS NULL OR NOW() - rd.last_scanned_at >= %s * '1 hour'::interval)
	ORDER BY rd.last_scanned_at ASC NULLS FIRST, rd.id
	FOR UPDATE SKIP LOCKED
	LIMIT %s
),
candidates AS (
	SELECT
		ld.id,
		uvt.is_default_branch IS TRUE AS safe
	FROM locked_definitions ld
	LEFT JOIN lsif_uploads_visible_at_tip uvt ON uvt.repository_id = ld.repository_id AND uvt.upload_id = ld.upload_id
),
updated_definitions AS (
	UPDATE codeintel_ranking_definitions
	SET last_scanned_at = NOW()
	WHERE id IN (SELECT c.id FROM candidates c WHERE c.safe)
),
deleted_definitions AS (
	DELETE FROM codeintel_ranking_definitions
	WHERE id IN (SELECT c.id FROM candidates c WHERE NOT c.safe)
	RETURNING 1
)
SELECT
	(SELECT COUNT(*) FROM candidates),
	(SELECT COUNT(*) FROM deleted_definitions)
`

func (s *store) VacuumStaleReferences(ctx context.Context, graphKey string) (
	numReferenceRecordsScanned int,
	numStaleReferenceRecordsDeleted int,
	err error,
) {
	ctx, _, endObservation := s.operations.vacuumStaleReferences.With(ctx, &err, observation.Args{LogFields: []otlog.Field{}})
	defer endObservation(1, observation.Args{})

	rows, err := s.db.Query(ctx, sqlf.Sprintf(
		vacuumStaleReferencesQuery,
		graphKey, int(threshold/time.Hour), vacuumBatchSize,
	))
	if err != nil {
		return 0, 0, err
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	for rows.Next() {
		if err := rows.Scan(
			&numReferenceRecordsScanned,
			&numStaleReferenceRecordsDeleted,
		); err != nil {
			return 0, 0, err
		}
	}

	return numReferenceRecordsScanned, numStaleReferenceRecordsDeleted, nil
}

const vacuumStaleReferencesQuery = `
WITH
locked_references AS (
	SELECT
		rr.id,
		u.repository_id,
		rr.upload_id
	FROM codeintel_ranking_references rr
	JOIN lsif_uploads u ON u.id = rr.upload_id
	WHERE
		rr.graph_key = %s AND
		(rr.last_scanned_at IS NULL OR NOW() - rr.last_scanned_at >= %s * '1 hour'::interval)
	ORDER BY rr.last_scanned_at ASC NULLS FIRST, rr.id
	FOR UPDATE SKIP LOCKED
	LIMIT %s
),
candidates AS (
	SELECT
		lr.id,
		uvt.is_default_branch IS TRUE AS safe
	FROM locked_references lr
	LEFT JOIN lsif_uploads_visible_at_tip uvt ON uvt.repository_id = lr.repository_id AND uvt.upload_id = lr.upload_id
),
updated_references AS (
	UPDATE codeintel_ranking_references
	SET last_scanned_at = NOW()
	WHERE id IN (SELECT c.id FROM candidates c WHERE c.safe)
),
deleted_references AS (
	DELETE FROM codeintel_ranking_references
	WHERE id IN (SELECT c.id FROM candidates c WHERE NOT c.safe)
	RETURNING 1
)
SELECT
	(SELECT COUNT(*) FROM candidates),
	(SELECT COUNT(*) FROM deleted_references)
`

func (s *store) VacuumStaleInitialPaths(ctx context.Context, graphKey string) (
	numPathRecordsScanned int,
	numStalePathRecordsDeleted int,
	err error,
) {
	ctx, _, endObservation := s.operations.vacuumStalePaths.With(ctx, &err, observation.Args{LogFields: []otlog.Field{}})
	defer endObservation(1, observation.Args{})

	rows, err := s.db.Query(ctx, sqlf.Sprintf(
		vacuumStalePathsQuery,
		graphKey, int(threshold/time.Hour), vacuumBatchSize,
	))
	if err != nil {
		return 0, 0, err
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	for rows.Next() {
		if err := rows.Scan(
			&numPathRecordsScanned,
			&numStalePathRecordsDeleted,
		); err != nil {
			return 0, 0, err
		}
	}

	return numPathRecordsScanned, numStalePathRecordsDeleted, nil
}

const vacuumStalePathsQuery = `
WITH
locked_initial_path_ranks AS (
	SELECT
		ipr.id,
		u.repository_id,
		ipr.upload_id
	FROM codeintel_initial_path_ranks ipr
	JOIN lsif_uploads u ON u.id = ipr.upload_id
	WHERE
		ipr.graph_key = %s AND
		(ipr.last_scanned_at IS NULL OR NOW() - ipr.last_scanned_at >= %s * '1 hour'::interval)
	ORDER BY ipr.last_scanned_at ASC NULLS FIRST, ipr.id
	FOR UPDATE SKIP LOCKED
	LIMIT %s
),
candidates AS (
	SELECT
		lipr.id,
		uvt.is_default_branch IS TRUE AS safe
	FROM locked_initial_path_ranks lipr
	LEFT JOIN lsif_uploads_visible_at_tip uvt ON uvt.repository_id = lipr.repository_id AND uvt.upload_id = lipr.upload_id
),
updated_initial_path_ranks AS (
	UPDATE codeintel_initial_path_ranks
	SET last_scanned_at = NOW()
	WHERE id IN (SELECT c.id FROM candidates c WHERE c.safe)
),
deleted_initial_path_ranks AS (
	DELETE FROM codeintel_initial_path_ranks
	WHERE id IN (SELECT c.id FROM candidates c WHERE NOT c.safe)
	RETURNING 1
)
SELECT
	(SELECT COUNT(*) FROM candidates),
	(SELECT COUNT(*) FROM deleted_initial_path_ranks)
`

func (s *store) VacuumStaleGraphs(ctx context.Context, derivativeGraphKey string) (_ int, err error) {
	ctx, _, endObservation := s.operations.vacuumStaleGraphs.With(ctx, &err, observation.Args{LogFields: []otlog.Field{}})
	defer endObservation(1, observation.Args{})

	count, _, err := basestore.ScanFirstInt(s.db.Query(ctx, sqlf.Sprintf(vacuumStaleGraphsQuery, derivativeGraphKey)))
	return count, err
}

const vacuumStaleGraphsQuery = `
WITH
locked_path_counts_inputs AS (
	SELECT id
	FROM codeintel_ranking_path_counts_inputs
	WHERE graph_key != %s
	ORDER BY id
	FOR UPDATE
),
deleted_path_counts_inputs AS (
	DELETE FROM codeintel_ranking_path_counts_inputs
	WHERE id IN (SELECT id FROM locked_path_counts_inputs)
	RETURNING 1
)
SELECT COUNT(*) FROM deleted_path_counts_inputs
`

func (s *store) VacuumStaleRanks(ctx context.Context, derivativeGraphKey string) (rankRecordsDeleted, rankRecordsScanned int, err error) {
	ctx, _, endObservation := s.operations.vacuumStaleRanks.With(ctx, &err, observation.Args{LogFields: []otlog.Field{}})
	defer endObservation(1, observation.Args{})

	graphKey, ok := rankingshared.GraphKeyFromDerivativeGraphKey(derivativeGraphKey)
	if !ok {
		return 0, 0, errors.Newf("unexpected derivative graph key %q", derivativeGraphKey)
	}

	rows, err := s.db.Query(ctx, sqlf.Sprintf(
		vacuumStaleRanksQuery,
		derivativeGraphKey,
		graphKey,
		derivativeGraphKey,
	))
	defer func() { err = basestore.CloseRows(rows, err) }()

	for rows.Next() {
		if err := rows.Scan(&rankRecordsScanned, &rankRecordsDeleted); err != nil {
			return 0, 0, err
		}
	}

	return rankRecordsScanned, rankRecordsDeleted, nil
}

const vacuumStaleRanksQuery = `
WITH
matching_graph_keys AS (
	SELECT DISTINCT graph_key
	FROM codeintel_path_ranks
	-- Implicit delete anything with a different graph key root
	WHERE graph_key != %s AND graph_key LIKE %s || '.%%'
),
valid_graph_keys AS (
	-- Select the current graph key as well as the highest graph key that
	-- shares the same parent graph key. Returning both will help bridge
	-- the gap that happens if we were to flush the entire table at the
	-- start of a new graph reduction.
	--
	-- This may have the effect of returning stale ranking data for a repo
	-- for which we no longer have SCIP data, but only from the previous
	-- graph reduction (and changing the parent graph key will flush all
	-- previous data (see the CTE definition above) if the need arises.
	SELECT %s AS graph_key
	UNION (
		SELECT graph_key
		FROM matching_graph_keys
		ORDER BY reverse(split_part(reverse(graph_key), '-', 1))::int DESC
		LIMIT 1
	)
),
locked_records AS (
	-- Lock all path rank records that don't have a recent graph key
	SELECT repository_id
	FROM codeintel_path_ranks
	WHERE graph_key NOT IN (SELECT graph_key FROM valid_graph_keys)
	ORDER BY repository_id
	FOR UPDATE
),
del AS (
	DELETE FROM codeintel_path_ranks
	WHERE repository_id IN (SELECT repository_id FROM locked_records)
	RETURNING 1
)
SELECT
	(SELECT COUNT(*) FROM locked_records),
	(SELECT COUNT(*) FROM del)
`
