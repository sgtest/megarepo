package lsifstore

import (
	"context"
	"time"

	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"
	otlog "github.com/opentracing/opentracing-go/log"

	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func (s *store) IDsWithMeta(ctx context.Context, ids []int) (_ []int, err error) {
	ctx, _, endObservation := s.operations.idsWithMeta.With(ctx, &err, observation.Args{LogFields: []otlog.Field{
		otlog.Int("numIDs", len(ids)),
		otlog.String("ids", intsToString(ids)),
	}})
	defer endObservation(1, observation.Args{})

	return basestore.ScanInts(s.db.Query(ctx, sqlf.Sprintf(
		idsWithMetaQuery,
		pq.Array(ids),
	)))
}

const idsWithMetaQuery = `
SELECT m.upload_id FROM codeintel_scip_metadata m WHERE m.upload_id = ANY(%s)
`

func (s *store) ReconcileCandidates(ctx context.Context, batchSize int) (_ []int, err error) {
	ctx, _, endObservation := s.operations.reconcileCandidates.With(ctx, &err, observation.Args{LogFields: []otlog.Field{
		otlog.Int("batchSize", batchSize),
	}})
	defer endObservation(1, observation.Args{})

	return s.reconcileCandidates(ctx, batchSize, time.Now().UTC())
}

func (s *store) reconcileCandidates(ctx context.Context, batchSize int, now time.Time) (_ []int, err error) {
	return basestore.ScanInts(s.db.Query(ctx, sqlf.Sprintf(reconcileQuery, batchSize, now, now)))
}

const reconcileQuery = `
WITH scip_candidates AS (
	SELECT m.upload_id
	FROM codeintel_scip_metadata m
	LEFT JOIN codeintel_last_reconcile lr ON lr.dump_id = m.upload_id
	ORDER BY lr.last_reconcile_at NULLS FIRST, m.upload_id
	LIMIT %s
)
INSERT INTO codeintel_last_reconcile
SELECT upload_id, %s FROM scip_candidates
ON CONFLICT (dump_id) DO UPDATE
SET last_reconcile_at = %s
RETURNING dump_id
`
