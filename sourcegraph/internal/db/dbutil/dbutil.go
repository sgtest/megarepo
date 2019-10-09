package dbutil

import (
	"bytes"
	"context"
	"database/sql"
	"database/sql/driver"
	"encoding/json"
	"fmt"
	"net/url"
	"os"
	"strconv"
	"time"

	// Register driver
	_ "github.com/lib/pq"

	migr "github.com/golang-migrate/migrate/v4"
	"github.com/golang-migrate/migrate/v4/database/postgres"
	bindata "github.com/golang-migrate/migrate/v4/source/go_bindata"
	multierror "github.com/hashicorp/go-multierror"
	opentracing "github.com/opentracing/opentracing-go"
	"github.com/opentracing/opentracing-go/ext"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/migrations"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

// Transaction calls f within a transaction, rolling back if any error is
// returned by the function.
func Transaction(ctx context.Context, db *sql.DB, f func(tx *sql.Tx) error) (err error) {
	finish := func(tx *sql.Tx) {
		if err != nil {
			if err2 := tx.Rollback(); err2 != nil {
				err = multierror.Append(err, err2)
			}
			return
		}
		err = tx.Commit()
	}

	span, ctx := opentracing.StartSpanFromContext(ctx, "Transaction")
	defer func() {
		if err != nil {
			ext.Error.Set(span, true)
			span.SetTag("err", err.Error())
		}
		span.Finish()
	}()

	tx, err := db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer finish(tx)
	return f(tx)
}

// A DB captures the essential method of a sql.DB: QueryContext.
type DB interface {
	QueryContext(ctx context.Context, q string, args ...interface{}) (*sql.Rows, error)
}

// A Tx captures the essential methods of a sql.Tx.
type Tx interface {
	Rollback() error
	Commit() error
}

// A TxBeginner captures BeginTx method of a sql.DB
type TxBeginner interface {
	BeginTx(context.Context, *sql.TxOptions) (*sql.Tx, error)
}

// NewDB returns a new *sql.DB from the given dsn (data source name).
func NewDB(dsn, app string) (*sql.DB, error) {
	cfg, err := url.Parse(dsn)
	if err != nil {
		return nil, errors.Wrap(err, "failed to parse dsn")
	}

	qry := cfg.Query()

	// Force PostgreSQL session timezone to UTC.
	qry.Set("timezone", "UTC")

	// Force application name.
	qry.Set("application_name", app)

	// Set max open and idle connections
	maxOpen, _ := strconv.Atoi(qry.Get("max_conns"))
	if maxOpen == 0 {
		maxOpen = 30
	}
	qry.Del("max_conns")

	cfg.RawQuery = qry.Encode()
	db, err := sql.Open("postgres", cfg.String())
	if err != nil {
		return nil, errors.Wrap(err, "failed to connect to database")
	}

	if err := db.Ping(); err != nil {
		return nil, errors.Wrap(err, "failed to ping database")
	}

	db.SetMaxOpenConns(maxOpen)
	db.SetMaxIdleConns(maxOpen)
	db.SetConnMaxLifetime(time.Minute)

	return db, nil
}

func NewMigrationSourceLoader(dataSource string) (*bindata.AssetSource, error) {
	// The following constructs a map of text placeholder/replacements
	// that are run over the content of the migration files before being
	// run. This is necessary as the lsif-server migrations need to reference
	// the PGPASSWORD envvar to make a successful dblink connection in an
	// environment where there is no superuser account (such as Amazon RDS).

	pgPassword, err := pgPassword(dataSource)
	if err != nil {
		return nil, err
	}

	replacements := map[string]string{
		"$$$PGPASSWORD$$$": pgPassword,
	}

	return bindata.Resource(migrations.AssetNames(), func(name string) ([]byte, error) {
		asset, err := migrations.Asset(name)
		if err != nil {
			return nil, err
		}

		for placeholder, replacement := range replacements {
			asset = bytes.Replace(asset, []byte(placeholder), []byte(replacement), -1)
		}

		return asset, nil
	}), nil
}

func pgPassword(dataSource string) (string, error) {
	if dataSource == "" {
		return os.Getenv("PGPASSWORD"), nil
	}

	url, err := url.Parse(dataSource)
	if err != nil {
		return "", errors.Wrap(err, "dataSource is not a valid URL")
	}

	password, _ := url.User.Password()
	return password, nil
}

// MigrateDB runs all migrations from github.com/sourcegraph/sourcegraph/migrations
// against the given sql.DB
func MigrateDB(db *sql.DB, dataSource string) error {
	var cfg postgres.Config
	driver, err := postgres.WithInstance(db, &cfg)
	if err != nil {
		return err
	}

	s, err := NewMigrationSourceLoader(dataSource)
	if err != nil {
		return err
	}

	d, err := bindata.WithInstance(s)
	if err != nil {
		return err
	}

	m, err := migr.NewWithInstance("go-bindata", d, "postgres", driver)
	if err != nil {
		return err
	}

	err = m.Up()
	if err == nil || err == migr.ErrNoChange {
		return nil
	}

	if os.IsNotExist(err) {
		// This should only happen if the DB is ahead of the migrations available
		version, dirty, verr := m.Version()
		if verr != nil {
			return verr
		}
		if dirty { // this shouldn't happen, but checking anyways
			return err
		}
		log15.Warn("WARNING: Detected an old version of Sourcegraph. The database has migrated to a newer version. If you have applied a rollback, this is expected and you can ignore this warning. If not, please contact support@sourcegraph.com for further assistance.", "db_version", version)
		return nil
	}
	return err
}

// NullTime represents a time.Time that may be null. nullTime implements the
// sql.Scanner interface so it can be used as a scan destination, similar to
// sql.NullString. When the scanned value is null, Time is set to the zero value.
type NullTime struct{ *time.Time }

// Scan implements the Scanner interface.
func (nt *NullTime) Scan(value interface{}) error {
	*nt.Time, _ = value.(time.Time)
	return nil
}

// Value implements the driver Valuer interface.
func (nt NullTime) Value() (driver.Value, error) {
	if nt.Time == nil {
		return nil, nil
	}
	return *nt.Time, nil
}

// NullString represents a string that may be null. NullString implements the
// sql.Scanner interface so it can be used as a scan destination, similar to
// sql.NullString. When the scanned value is null, String is set to the zero value.
type NullString struct{ S *string }

// Scan implements the Scanner interface.
func (nt *NullString) Scan(value interface{}) error {
	switch v := value.(type) {
	case []byte:
		*nt.S = string(v)
	case string:
		*nt.S = v
	}
	return nil
}

// Value implements the driver Valuer interface.
func (nt NullString) Value() (driver.Value, error) {
	if nt.S == nil {
		return nil, nil
	}
	return *nt.S, nil
}

// NullInt32 represents an int32 that may be null. NullInt32 implements the
// sql.Scanner interface so it can be used as a scan destination, similar to
// sql.NullString. When the scanned value is null, int32 is set to the zero value.
type NullInt32 struct{ N *int32 }

// Scan implements the Scanner interface.
func (n *NullInt32) Scan(value interface{}) error {
	switch value := value.(type) {
	case int64:
		*n.N = int32(value)
	case int32:
		*n.N = value
	case nil:
		return nil
	default:
		return fmt.Errorf("value is not int64: %T", value)
	}
	return nil
}

// Value implements the driver Valuer interface.
func (n NullInt32) Value() (driver.Value, error) {
	if n.N == nil {
		return nil, nil
	}
	return *n.N, nil
}

// JSONInt64Set represents an int64 set as a JSONB object where the keys are
// the ids and the values are null. It implements the sql.Scanner interface so
// it can be used as a scan destination, similar to
// sql.NullString.
type JSONInt64Set struct{ Set *[]int64 }

// Scan implements the Scanner interface.
func (n *JSONInt64Set) Scan(value interface{}) error {
	set := make(map[int64]*struct{})

	switch value := value.(type) {
	case nil:
	case []byte:
		if err := json.Unmarshal(value, &set); err != nil {
			return err
		}
	default:
		return fmt.Errorf("value is not []byte: %T", value)
	}

	if *n.Set == nil {
		*n.Set = make([]int64, 0, len(set))
	} else {
		*n.Set = (*n.Set)[:0]
	}

	for id := range set {
		*n.Set = append(*n.Set, id)
	}

	return nil
}

// Value implements the driver Valuer interface.
func (n JSONInt64Set) Value() (driver.Value, error) {
	if n.Set == nil {
		return nil, nil
	}
	return *n.Set, nil
}
