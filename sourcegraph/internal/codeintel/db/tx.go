package db

import (
	"context"

	"github.com/hashicorp/go-multierror"
	"github.com/keegancsmith/sqlf"
	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/internal/db/dbutil"
)

// DoneFn is the function type of DB's Done method.
type DoneFn func(err error) error

// noopDoneFn is a behaviorless DoneFn.
func noopDoneFn(err error) error {
	return err
}

// ErrNotTransactable occurs when Transact is called on a Database whose underlying
// db handle does not support beginning a transaction.
var ErrNotTransactable = errors.New("db: not transactable")

// Transact returns a Database whose methods operate within the context of a transaction.
// This method will return an error if the underlying DB cannot be interface upgraded
// to a TxBeginner.
func (db *dbImpl) Transact(ctx context.Context) (DB, error) {
	tx, _, err := db.transact(ctx)
	return tx, err
}

// transact returns a Database whose methods operate within the context of a transaction.
// This method also returns a boolean flag indicating whether a new transaction was created.
func (db *dbImpl) transact(ctx context.Context) (*dbImpl, bool, error) {
	if _, ok := db.db.(dbutil.Tx); ok {
		// Already in a Tx
		return db, false, nil
	}

	tb, ok := db.db.(dbutil.TxBeginner)
	if !ok {
		// Not a Tx nor a TxBeginner
		return nil, false, ErrNotTransactable
	}

	tx, err := tb.BeginTx(ctx, nil)
	if err != nil {
		return nil, false, errors.Wrap(err, "db: BeginTx")
	}

	return &dbImpl{db: tx}, true, nil
}

// ErrNoTransaction occurs when Savepoint or RollbackToSavepoint is called outside of a transaction.
var ErrNoTransaction = errors.New("db: not in a transaction")

// Savepoint creates a named position in the transaction from which all additional work can
// be discarded.
func (db *dbImpl) Savepoint(ctx context.Context, name string) error {
	if _, ok := db.db.(dbutil.Tx); !ok {
		return ErrNoTransaction
	}

	// Unfortunately, it's a syntax error to supply this as a param
	return db.exec(ctx, sqlf.Sprintf("SAVEPOINT "+name))
}

// RollbackToSavepoint throws away all the work on the underlying transaction since the
// savepoint with the given name was created.
func (db *dbImpl) RollbackToSavepoint(ctx context.Context, name string) error {
	if _, ok := db.db.(dbutil.Tx); !ok {
		return ErrNoTransaction
	}

	// Unfortunately, it's a syntax error to supply this as a param
	return db.exec(ctx, sqlf.Sprintf("ROLLBACK TO SAVEPOINT "+name))
}

// Done commits underlying the transaction on a nil error value and performs a rollback
// otherwise. If an error occurs during commit or rollback of the transaction, the error
// is added to the resulting error value. If the Database does not wrap a transaction the
// original error value is returned unchanged.
func (db *dbImpl) Done(err error) error {
	if tx, ok := db.db.(dbutil.Tx); ok {
		if err != nil {
			if rollErr := tx.Rollback(); rollErr != nil {
				err = multierror.Append(err, rollErr)
			}
		} else {
			if commitErr := tx.Commit(); commitErr != nil {
				err = multierror.Append(err, commitErr)
			}
		}
	}

	return err
}
