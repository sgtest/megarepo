package rockskip

import (
	"context"
	"database/sql"
	"sync"

	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type Symbol struct {
	Name   string `json:"name"`
	Parent string `json:"parent"`
	Kind   string `json:"kind"`
	Line   int    `json:"line"`
}

type ParseSymbolsFunc func(path string, bytes []byte) (symbols []Symbol, err error)

const NULL CommitId = 0

type Service struct {
	db                   *sql.DB
	git                  Git
	createParser         func() ParseSymbolsFunc
	status               *ServiceStatus
	repoUpdates          chan struct{}
	maxRepos             int
	logQueries           bool
	repoCommitToDone     map[string]chan struct{}
	repoCommitToDoneMu   sync.Mutex
	indexRequestQueues   []chan indexRequest
	symbolsCacheSize     int
	pathSymbolsCacheSize int
}

func NewService(
	db *sql.DB,
	git Git,
	createParser func() ParseSymbolsFunc,
	maxConcurrentlyIndexing int,
	maxRepos int,
	logQueries bool,
	indexRequestsQueueSize int,
	symbolsCacheSize int,
	pathSymbolsCacheSize int,
) (*Service, error) {
	indexRequestQueues := make([]chan indexRequest, maxConcurrentlyIndexing)
	for i := 0; i < maxConcurrentlyIndexing; i++ {
		indexRequestQueues[i] = make(chan indexRequest, indexRequestsQueueSize)
	}

	service := &Service{
		db:                   db,
		git:                  git,
		createParser:         createParser,
		status:               NewStatus(),
		repoUpdates:          make(chan struct{}, 1),
		maxRepos:             maxRepos,
		logQueries:           logQueries,
		repoCommitToDone:     map[string]chan struct{}{},
		repoCommitToDoneMu:   sync.Mutex{},
		indexRequestQueues:   indexRequestQueues,
		symbolsCacheSize:     symbolsCacheSize,
		pathSymbolsCacheSize: pathSymbolsCacheSize,
	}

	go service.startCleanupLoop()

	for i := 0; i < maxConcurrentlyIndexing; i++ {
		go service.startIndexingLoop(database.NewDB(service.db), service.indexRequestQueues[i])
	}

	return service, nil
}

func (s *Service) startIndexingLoop(db database.DB, indexRequestQueue chan indexRequest) {
	for indexRequest := range indexRequestQueue {
		err := s.Index(context.Background(), db, indexRequest.repo, indexRequest.commit)
		close(indexRequest.done)
		if err != nil {
			log15.Error("indexing error", "repo", indexRequest.repo, "commit", indexRequest.commit, "err", err)
		}
	}
}

func (s *Service) startCleanupLoop() {
	for range s.repoUpdates {
		threadStatus := s.status.NewThreadStatus("cleanup")
		err := DeleteOldRepos(context.Background(), s.db, s.maxRepos, threadStatus)
		threadStatus.End()
		if err != nil {
			log15.Error("Failed to delete old repos", "error", err)
		}
	}
}

func getHops(ctx context.Context, tx dbutil.DB, commit int, tasklog *TaskLog) ([]int, error) {
	tasklog.Start("get hops")

	current := commit
	spine := []int{current}

	for {
		_, ancestor, _, present, err := GetCommitById(ctx, tx, current)
		if err != nil {
			return nil, errors.Wrap(err, "GetCommitById")
		} else if !present {
			break
		} else {
			if current == NULL {
				break
			}
			current = ancestor
			spine = append(spine, current)
		}
	}

	return spine, nil
}

func DeleteOldRepos(ctx context.Context, db *sql.DB, maxRepos int, threadStatus *ThreadStatus) error {
	// Get a fresh connection from the DB pool to get deterministic "lock stacking" behavior.
	// See doc/dev/background-information/sql/locking_behavior.md for more details.
	conn, err := db.Conn(context.Background())
	if err != nil {
		return errors.Wrap(err, "failed to get connection for deleting old repos")
	}
	defer conn.Close()

	// Keep deleting repos until we're back to at most maxRepos.
	for {
		more, err := tryDeleteOldestRepo(ctx, conn, maxRepos, threadStatus)
		if err != nil {
			return err
		}
		if !more {
			return nil
		}
	}
}

// Ruler sequence
//
// input : 0, 1, 2, 3, 4, 5, 6, 7, 8, ...
// output: 0, 0, 1, 0, 2, 0, 1, 0, 3, ...
//
// https://oeis.org/A007814
func ruler(n int) int {
	if n == 0 {
		return 0
	}
	if n%2 != 0 {
		return 0
	}
	return 1 + ruler(n/2)
}
