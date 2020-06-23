package backend

import (
	"context"
	"database/sql"
	"fmt"
	"time"

	"github.com/Masterminds/semver"
	"github.com/keegancsmith/sqlf"
	"github.com/sourcegraph/sourcegraph/internal/db/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/db/dbutil"
)

// UpgradeError is returned by UpdateServiceVersion when it faces an
// upgrade policy violation error.
type UpgradeError struct {
	Service  string
	Previous *semver.Version
	Latest   *semver.Version
}

// Error implements the error interface.
func (e UpgradeError) Error() string {
	return fmt.Sprintf(
		"upgrading %q from %q to %q is not allowed, please refer to %s",
		e.Service,
		e.Previous,
		e.Latest,
		"https://docs.sourcegraph.com/#upgrading-sourcegraph",
	)

}

// UpdateServiceVersion updates the latest version for the given Sourcegraph
// service. It enforces our documented upgrade policy.
// https://docs.sourcegraph.com/#upgrading-sourcegraph
func UpdateServiceVersion(ctx context.Context, service, version string) error {
	return dbutil.Transaction(ctx, dbconn.Global, func(tx *sql.Tx) (err error) {
		var prev string

		q := sqlf.Sprintf(getVersionQuery, service)
		row := tx.QueryRowContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
		if err = row.Scan(&prev); err != nil && err != sql.ErrNoRows {
			return err
		}

		latest, _ := semver.NewVersion(version)
		previous, _ := semver.NewVersion(prev)

		if !IsValidUpgrade(previous, latest) {
			return &UpgradeError{Service: service, Previous: previous, Latest: latest}
		}

		q = sqlf.Sprintf(
			upsertVersionQuery,
			service,
			version,
			time.Now().UTC(),
			prev,
		)

		_, err = tx.ExecContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
		return err
	})
}

const getVersionQuery = `SELECT version FROM versions WHERE service = %s`

const upsertVersionQuery = `
INSERT INTO versions (service, version, updated_at)
VALUES (%s, %s, %s) ON CONFLICT (service) DO
UPDATE SET (version, updated_at) =
	(excluded.version, excluded.updated_at)
WHERE versions.version = %s`

// IsValidUpgrade returns true if the given previous and
// latest versions comply with our documented upgrade policy.
// All roll-backs or downgrades are supported.
//
// https://docs.sourcegraph.com/#upgrading-sourcegraph
func IsValidUpgrade(previous, latest *semver.Version) bool {
	switch {
	case previous == nil || latest == nil:
		return true
	case previous.Major() == 0 && previous.Minor() == 0 && previous.Patch() == 0:
		// https://github.com/sourcegraph/sourcegraph/issues/11666
		//
		// TODO(slimsag): Remove this switch case Oct, 1st 2020
		return true
	case previous.Major() > latest.Major():
		return true
	case previous.Major() == latest.Major():
		return previous.Minor() >= latest.Minor() ||
			previous.Minor() == latest.Minor()-1
	case previous.Major() == latest.Major()-1:
		return latest.Minor() == 0
	default:
		return false
	}
}
