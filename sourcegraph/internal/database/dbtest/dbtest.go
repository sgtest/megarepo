package dbtest

import (
	crand "crypto/rand"
	"database/sql"
	"encoding/binary"
	"hash/fnv"
	"math/rand"
	"net/url"
	"os"
	"strconv"
	"strings"
	"sync"
	"testing"

	"github.com/lib/pq"

	connections "github.com/sourcegraph/sourcegraph/internal/database/connections/test"
	"github.com/sourcegraph/sourcegraph/internal/database/migration/schemas"

	"github.com/sourcegraph/log"
)

// NewTx opens a transaction off of the given database, returning that
// transaction if an error didn't occur.
//
// After opening this transaction, it executes the query
//     SET CONSTRAINTS ALL DEFERRED
// which aids in testing.
func NewTx(t testing.TB, db *sql.DB) *sql.Tx {
	tx, err := db.Begin()
	if err != nil {
		t.Fatal(err)
	}

	_, err = tx.Exec("SET CONSTRAINTS ALL DEFERRED")
	if err != nil {
		t.Fatal(err)
	}

	t.Cleanup(func() {
		_ = tx.Rollback()
	})

	return tx
}

// Use a shared, locked RNG to avoid issues with multiple concurrent tests getting
// the same random database number (unlikely, but has been observed).
// Use crypto/rand.Read() to use an OS source of entropy, since, against all odds,
// using nanotime was causing conflicts.
var rng = rand.New(rand.NewSource(func() int64 {
	b := [8]byte{}
	if _, err := crand.Read(b[:]); err != nil {
		panic(err)
	}
	return int64(binary.LittleEndian.Uint64(b[:]))
}()))
var rngLock sync.Mutex

var dbTemplateOnce sync.Once

// NewDB returns a connection to a clean, new temporary testing database with
// the same schema as Sourcegraph's production Postgres database.
func NewDB(logger log.Logger, t testing.TB) *sql.DB {
	dbTemplateOnce.Do(func() {
		initTemplateDB(logger, t, "migrated", []*schemas.Schema{schemas.Frontend, schemas.CodeIntel})
	})

	return newFromDSN(logger, t, "migrated")
}

var insightsTemplateOnce sync.Once

// NewInsightsDB returns a connection to a clean, new temporary testing database with
// the same schema as Sourcegraph's CodeInsights production Postgres database.
func NewInsightsDB(logger log.Logger, t testing.TB) *sql.DB {
	insightsTemplateOnce.Do(func() {
		initTemplateDB(logger, t, "insights", []*schemas.Schema{schemas.CodeInsights})
	})
	return newFromDSN(logger, t, "insights")
}

var rawTemplateOnce sync.Once

// NewRawDB returns a connection to a clean, new temporary testing database.
func NewRawDB(logger log.Logger, t testing.TB) *sql.DB {
	rawTemplateOnce.Do(func() {
		initTemplateDB(logger, t, "raw", nil)
	})
	return newFromDSN(logger, t, "raw")
}

func newFromDSN(logger log.Logger, t testing.TB, templateNamespace string) *sql.DB {
	if testing.Short() {
		t.Skip("skipping DB test since -short specified")
	}

	config, err := getDSN()
	if err != nil {
		t.Fatalf("failed to parse dsn: %s", err)
	}

	rngLock.Lock()
	dbname := "sourcegraph-test-" + strconv.FormatUint(rng.Uint64(), 10)
	rngLock.Unlock()

	db := dbConn(logger, t, config)
	dbExec(t, db, `CREATE DATABASE `+pq.QuoteIdentifier(dbname)+` TEMPLATE `+pq.QuoteIdentifier(templateDBName(templateNamespace)))

	config.Path = "/" + dbname
	testDB := dbConn(logger, t, config)
	t.Logf("testdb: %s", config.String())

	// Some tests that exercise concurrency need lots of connections or they block forever.
	// e.g. TestIntegration/DBStore/Syncer/MultipleServices
	conns, err := strconv.Atoi(os.Getenv("TESTDB_MAXOPENCONNS"))
	if err != nil || conns == 0 {
		conns = 20
	}
	testDB.SetMaxOpenConns(conns)
	testDB.SetMaxIdleConns(1) // Default is 2, and within tests, it's not that important to have more than one.

	t.Cleanup(func() {
		defer db.Close()

		if t.Failed() {
			t.Logf("DATABASE %s left intact for inspection", dbname)
			return
		}

		if err := testDB.Close(); err != nil {
			t.Fatalf("failed to close test database: %s", err)
		}
		dbExec(t, db, killClientConnsQuery, dbname)
		dbExec(t, db, `DROP DATABASE `+pq.QuoteIdentifier(dbname))
	})

	return testDB
}

// initTemplateDB creates a template database with a fully migrated schema for the
// current package. New databases can then do a cheap copy of the migrated schema
// rather than running the full migration every time.
func initTemplateDB(logger log.Logger, t testing.TB, templateNamespace string, dbSchemas []*schemas.Schema) {
	config, err := getDSN()
	if err != nil {
		t.Fatalf("failed to parse dsn: %s", err)
	}

	db := dbConn(logger, t, config)
	defer db.Close()

	init := func(templateNamespace string, schemas []*schemas.Schema) {
		templateName := templateDBName(templateNamespace)
		name := pq.QuoteIdentifier(templateName)

		// We must first drop the template database because
		// migrations would not run on it if they had already ran,
		// even if the content of the migrations had changed during development.

		dbExec(t, db, `DROP DATABASE IF EXISTS `+name)
		dbExec(t, db, `CREATE DATABASE `+name+` TEMPLATE template0`)

		cfgCopy := *config
		cfgCopy.Path = "/" + templateName
		dbConn(logger, t, &cfgCopy, schemas...).Close()
	}

	init(templateNamespace, dbSchemas)
}

// templateDBName returns the name of the template database for the currently running package and namespace.
func templateDBName(templateNamespace string) string {
	parts := []string{
		"sourcegraph-test-template",
		wdHash(),
		templateNamespace,
	}

	return strings.Join(parts, "-")
}

// wdHash returns a hash of the current working directory.
// This is useful to get a stable identifier for the package running
// the tests.
func wdHash() string {
	h := fnv.New64()
	wd, _ := os.Getwd()
	h.Write([]byte(wd))
	return strconv.FormatUint(h.Sum64(), 10)
}

func dbConn(logger log.Logger, t testing.TB, cfg *url.URL, schemas ...*schemas.Schema) *sql.DB {
	t.Helper()
	db, err := connections.NewTestDB(logger, cfg.String(), schemas...)
	if err != nil {
		t.Fatalf("failed to connect to database %q: %s", cfg, err)
	}
	return db
}

func dbExec(t testing.TB, db *sql.DB, q string, args ...any) {
	t.Helper()
	_, err := db.Exec(q, args...)
	if err != nil {
		t.Errorf("failed to exec %q: %s", q, err)
	}
}

const killClientConnsQuery = `
SELECT pg_terminate_backend(pg_stat_activity.pid)
FROM pg_stat_activity WHERE datname = $1
`
