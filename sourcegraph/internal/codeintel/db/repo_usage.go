package db

import (
	"context"

	"github.com/keegancsmith/sqlf"
)

// RepoUsageStatistics pairs a repository identifier with a count of code intelligence events.
type RepoUsageStatistics struct {
	RepositoryID int
	SearchCount  int
	PreciseCount int
}

// RepoUsageStatistics reads recent event log records and returns the number of search-based and precise
// code intelligence activity within the last week grouped by repository. The resulting slice is ordered
// by search then precise event counts.
func (db *dbImpl) RepoUsageStatistics(ctx context.Context) ([]RepoUsageStatistics, error) {
	return scanRepoUsageStatisticsSlice(db.query(ctx, sqlf.Sprintf(`
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
		ORDER BY search_count DESC, precise_count DESC
	`)))
}
