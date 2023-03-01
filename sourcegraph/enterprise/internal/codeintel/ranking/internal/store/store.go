package store

import (
	"context"
	"encoding/json"
	"time"

	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"
	logger "github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/uploads/shared"
	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

// Store provides the interface for ranking storage.
type Store interface {
	// Transactions
	Transact(ctx context.Context) (Store, error)
	Done(err error) error

	GetStarRank(ctx context.Context, repoName api.RepoName) (float64, error)
	GetDocumentRanks(ctx context.Context, repoName api.RepoName) (map[string][2]float64, bool, error)
	LastUpdatedAt(ctx context.Context, repoIDs []api.RepoID) (map[api.RepoID]time.Time, error)
	UpdatedAfter(ctx context.Context, t time.Time) ([]api.RepoName, error)

	InsertDefinitionsForRanking(ctx context.Context, rankingGraphKey string, rankingBatchSize int, definitions []shared.RankingDefinitions) (err error)
	InsertReferencesForRanking(ctx context.Context, rankingGraphKey string, rankingBatchSize int, references shared.RankingReferences) (err error)
	InsertPathCountInputs(ctx context.Context, rankingGraphKey string, batchSize int) (numReferenceRecordsProcessed int, numInputsInserted int, err error)
	InsertPathRanks(ctx context.Context, graphKey string, batchSize int) (numPathRanksInserted float64, numInputsProcessed float64, err error)

	VacuumStaleGraphs(ctx context.Context, derivativeGraphKey string) (
		metadataRecordsDeleted int,
		inputRecordsDeleted int,
		err error,
	)

	VacuumStaleDefinitionsAndReferences(ctx context.Context, graphKey string) (
		numStaleDefinitionRecordsDeleted int,
		numStaleReferenceRecordsDeleted int,
		err error,
	)

	GetUploadsForRanking(ctx context.Context, graphKey, objectPrefix string, batchSize int) ([]shared.ExportedUpload, error)

	ProcessStaleExportedUploads(
		ctx context.Context,
		graphKey string,
		batchSize int,
		deleter func(ctx context.Context, objectPrefix string) error,
	) (totalDeleted int, err error)
}

// store manages the ranking store.
type store struct {
	db         *basestore.Store
	logger     logger.Logger
	operations *operations
}

// New returns a new ranking store.
func New(observationCtx *observation.Context, db database.DB) Store {
	return newInternal(observationCtx, db)
}

func newInternal(observationCtx *observation.Context, db database.DB) *store {
	return &store{
		db:         basestore.NewWithHandle(db.Handle()),
		logger:     logger.Scoped("ranking.store", ""),
		operations: newOperations(observationCtx),
	}
}

func (s *store) Transact(ctx context.Context) (Store, error) {
	return s.transact(ctx)
}

func (s *store) transact(ctx context.Context) (*store, error) {
	tx, err := s.db.Transact(ctx)
	if err != nil {
		return nil, err
	}

	return &store{
		logger:     s.logger,
		db:         tx,
		operations: s.operations,
	}, nil
}

func (s *store) Done(err error) error {
	return s.db.Done(err)
}

func (s *store) GetStarRank(ctx context.Context, repoName api.RepoName) (float64, error) {
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

func (s *store) GetDocumentRanks(ctx context.Context, repoName api.RepoName) (map[string][2]float64, bool, error) {
	pathRanksWithPrecision := map[string][2]float64{}
	scanner := func(s dbutil.Scanner) (bool, error) {
		var (
			precision  float64
			serialized string
		)
		if err := s.Scan(&precision, &serialized); err != nil {
			return false, err
		}

		pathRanks := map[string]float64{}
		if err := json.Unmarshal([]byte(serialized), &pathRanks); err != nil {
			return false, err
		}

		for path, newRank := range pathRanks {
			if oldRank, ok := pathRanksWithPrecision[path]; ok && oldRank[0] <= precision {
				continue
			}

			pathRanksWithPrecision[path] = [2]float64{precision, newRank}
		}

		return true, nil
	}

	if err := basestore.NewCallbackScanner(scanner)(s.db.Query(ctx, sqlf.Sprintf(getDocumentRanksQuery, repoName))); err != nil {
		return nil, false, err
	}
	return pathRanksWithPrecision, true, nil
}

const getDocumentRanksQuery = `
SELECT
	precision,
	payload
FROM codeintel_path_ranks pr
JOIN repo r ON r.id = pr.repository_id
WHERE
	r.name = %s AND
	r.deleted_at IS NULL AND
	r.blocked IS NULL
`

func (s *store) SetDocumentRanks(ctx context.Context, repoName api.RepoName, precision float64, ranks map[string]float64) error {
	serialized, err := json.Marshal(ranks)
	if err != nil {
		return err
	}

	return s.db.Exec(ctx, sqlf.Sprintf(setDocumentRanksQuery, repoName, precision, serialized))
}

const setDocumentRanksQuery = `
INSERT INTO codeintel_path_ranks AS pr (repository_id, precision, payload)
VALUES (
	(SELECT id FROM repo WHERE name = %s),
	%s,
	%s
)
ON CONFLICT (repository_id, precision) DO
UPDATE
	SET payload = EXCLUDED.payload
`

func (s *store) LastUpdatedAt(ctx context.Context, repoIDs []api.RepoID) (map[api.RepoID]time.Time, error) {
	pairs, err := scanLastUpdatedAtPairs(s.db.Query(ctx, sqlf.Sprintf(lastUpdatedAtQuery, pq.Array(repoIDs))))
	if err != nil {
		return nil, err
	}

	return pairs, nil
}

const lastUpdatedAtQuery = `
SELECT
	repository_id,
	updated_at
FROM codeintel_path_ranks
WHERE repository_id = ANY(%s)
`

var scanLastUpdatedAtPairs = basestore.NewMapScanner(func(s dbutil.Scanner) (repoID api.RepoID, t time.Time, _ error) {
	err := s.Scan(&repoID, &t)
	return repoID, t, err
})

func (s *store) UpdatedAfter(ctx context.Context, t time.Time) ([]api.RepoName, error) {
	names, err := basestore.ScanStrings(s.db.Query(ctx, sqlf.Sprintf(updatedAfterQuery, t)))
	if err != nil {
		return nil, err
	}

	repoNames := make([]api.RepoName, 0, len(names))
	for _, name := range names {
		repoNames = append(repoNames, api.RepoName(name))
	}

	return repoNames, nil
}

const updatedAfterQuery = `
SELECT r.name
FROM codeintel_path_ranks pr
JOIN repo r ON r.id = pr.repository_id
WHERE pr.updated_at >= %s
ORDER BY r.name
`
