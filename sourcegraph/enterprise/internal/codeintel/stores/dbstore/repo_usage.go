package dbstore

import (
	"context"
	"database/sql"

	"github.com/keegancsmith/sqlf"
	"github.com/opentracing/opentracing-go/log"
	"github.com/sourcegraph/sourcegraph/internal/db/basestore"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

// RepoUsageStatistics pairs a repository identifier with a count of code intelligence events.
type RepoUsageStatistics struct {
	RepositoryID int
	SearchCount  int
	PreciseCount int
}

// scanRepoUsageStatisticsSlice scans a slice of repo usage statistics from the return value of `*Store.query`.
func scanRepoUsageStatisticsSlice(rows *sql.Rows, queryErr error) (_ []RepoUsageStatistics, err error) {
	if queryErr != nil {
		return nil, queryErr
	}
	defer func() { err = basestore.CloseRows(rows, err) }()

	var stats []RepoUsageStatistics
	for rows.Next() {
		var s RepoUsageStatistics
		if err := rows.Scan(
			&s.RepositoryID,
			&s.SearchCount,
			&s.PreciseCount,
		); err != nil {
			return nil, err
		}

		stats = append(stats, s)
	}

	return stats, nil
}

// RepoUsageStatistics reads recent event log records and returns the number of search-based and precise
// code intelligence activity within the last week grouped by repository. The resulting slice is ordered
// by search then precise event counts.
func (s *Store) RepoUsageStatistics(ctx context.Context) (_ []RepoUsageStatistics, err error) {
	ctx, endObservation := s.operations.repoUsageStatistics.With(ctx, &err, observation.Args{LogFields: []log.Field{}})
	defer endObservation(1, observation.Args{})

	return scanRepoUsageStatisticsSlice(s.Store.Query(ctx, sqlf.Sprintf(`
		SELECT
			r.id,
			counts.search_count,
			counts.precise_count
		FROM (
			SELECT
				-- Cut out repo portion of event url
				-- e.g. https://{github.com/owner/repo}/-/rest-of-path
				substring(url from '//[^/]+/(.+)/-/') AS repo_name,
				COUNT(*) FILTER (WHERE name LIKE 'codeintel.search%%%%') AS search_count,
				COUNT(*) FILTER (WHERE name LIKE 'codeintel.lsif%%%%') AS precise_count
			FROM event_logs
			WHERE timestamp >= NOW() - INTERVAL '1 week'
			GROUP BY repo_name
		) counts
		-- Cast allows use of the uri btree index
		JOIN repo r ON r.uri = counts.repo_name::citext
		WHERE r.deleted_at IS NULL
		ORDER BY search_count DESC, precise_count DESC
	`)))
}
