package store

import (
	"context"
	"encoding/json"
	"time"

	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func (s *store) GetStarRank(ctx context.Context, repoName api.RepoName) (_ float64, err error) {
	ctx, _, endObservation := s.operations.getStarRank.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	rank, _, err := basestore.ScanFirstFloat(s.db.Query(ctx, sqlf.Sprintf(getStarRankQuery, repoName)))
	return rank, err
}

const getStarRankQuery = `
SELECT
	s.rank
FROM (
	SELECT
		name,
		percent_rank() OVER (ORDER BY stars) AS rank
	FROM repo
) s
WHERE s.name = %s
`

func (s *store) GetDocumentRanks(ctx context.Context, repoName api.RepoName) (_ map[string]float64, _ bool, err error) {
	ctx, _, endObservation := s.operations.getDocumentRanks.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	pathRanksWithPrecision := map[string]float64{}
	scanner := func(s dbutil.Scanner) (bool, error) {
		var serialized string
		if err := s.Scan(&serialized); err != nil {
			return false, err
		}

		pathRanks := map[string]float64{}
		if err := json.Unmarshal([]byte(serialized), &pathRanks); err != nil {
			return false, err
		}

		for path, newRank := range pathRanks {
			pathRanksWithPrecision[path] = newRank
		}

		return true, nil
	}

	if err := basestore.NewCallbackScanner(scanner)(s.db.Query(ctx, sqlf.Sprintf(getDocumentRanksQuery, repoName))); err != nil {
		return nil, false, err
	}
	return pathRanksWithPrecision, true, nil
}

const getDocumentRanksQuery = `
WITH
last_completed_progress AS (
	SELECT crp.graph_key
	FROM codeintel_ranking_progress crp
	WHERE crp.reducer_completed_at IS NOT NULL
	ORDER BY crp.reducer_completed_at DESC
	LIMIT 1
)
SELECT payload
FROM codeintel_path_ranks pr
JOIN repo r ON r.id = pr.repository_id
WHERE
	pr.graph_key IN (SELECT graph_key FROM last_completed_progress) AND
	r.name = %s AND
	r.deleted_at IS NULL AND
	r.blocked IS NULL
`

func (s *store) GetReferenceCountStatistics(ctx context.Context) (logmean float64, err error) {
	ctx, _, endObservation := s.operations.getReferenceCountStatistics.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	rows, err := s.db.Query(ctx, sqlf.Sprintf(getReferenceCountStatisticsQuery))
	if err != nil {
		return 0, err
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	if rows.Next() {
		if err := rows.Scan(&logmean); err != nil {
			return 0, err
		}
	}

	return logmean, nil
}

const getReferenceCountStatisticsQuery = `
WITH
last_completed_progress AS (
	SELECT crp.graph_key
	FROM codeintel_ranking_progress crp
	WHERE crp.reducer_completed_at IS NOT NULL
	ORDER BY crp.reducer_completed_at DESC
	LIMIT 1
)
SELECT
	CASE WHEN COALESCE(SUM(pr.num_paths), 0) = 0
		THEN 0.0
		ELSE SUM(pr.refcount_logsum) / SUM(pr.num_paths)::float
	END AS logmean
FROM codeintel_path_ranks pr
WHERE pr.graph_key IN (SELECT graph_key FROM last_completed_progress)
`

func (s *store) LastUpdatedAt(ctx context.Context, repoIDs []api.RepoID) (_ map[api.RepoID]time.Time, err error) {
	ctx, _, endObservation := s.operations.lastUpdatedAt.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	pairs, err := scanLastUpdatedAtPairs(s.db.Query(ctx, sqlf.Sprintf(lastUpdatedAtQuery, pq.Array(repoIDs))))
	if err != nil {
		return nil, err
	}

	return pairs, nil
}

const lastUpdatedAtQuery = `
WITH
progress AS (
	SELECT pl.id
	FROM codeintel_ranking_progress pl
	WHERE pl.reducer_completed_at IS NOT NULL
	ORDER BY pl.reducer_completed_at DESC
	LIMIT 1
)
SELECT
	pr.repository_id,
	crp.reducer_completed_at
FROM codeintel_path_ranks pr
JOIN codeintel_ranking_progress crp ON crp.graph_key = pr.graph_key
WHERE
	pr.repository_id = ANY(%s) AND
	crp.id = (SELECT id FROM progress)
`

var scanLastUpdatedAtPairs = basestore.NewMapScanner(func(s dbutil.Scanner) (repoID api.RepoID, t time.Time, _ error) {
	err := s.Scan(&repoID, &t)
	return repoID, t, err
})
