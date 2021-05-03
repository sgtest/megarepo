package database

import (
	"context"
	"database/sql"
	"fmt"
	"time"

	"github.com/keegancsmith/sqlf"
	"github.com/pkg/errors"

	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/encryption"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/auth"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
)

// UserCredential represents a row in the `user_credentials` table.
type UserCredential struct {
	ID                  int64
	Domain              string
	UserID              int32
	ExternalServiceType string
	ExternalServiceID   string
	CreatedAt           time.Time
	UpdatedAt           time.Time

	// TODO(batch-change-credential-encryption): On or after Sourcegraph 3.30,
	// we should remove the credential and SSHMigrationApplied fields.
	SSHMigrationApplied bool
	credential          auth.Authenticator
	encryptedCredential []byte

	key encryption.Key
}

// Authenticator decrypts and creates the authenticator associated with the user
// credential.
func (uc *UserCredential) Authenticator(ctx context.Context) (auth.Authenticator, error) {
	if uc.credential != nil {
		return uc.credential, nil
	}

	if uc.encryptedCredential == nil {
		return nil, errors.New("no unencrypted or encrypted credential found")
	}

	var raw string
	if uc.key != nil {
		secret, err := uc.key.Decrypt(ctx, uc.encryptedCredential)
		if err != nil {
			return nil, errors.Wrap(err, "decrypting credential")
		}
		raw = secret.Secret()
	} else {
		raw = string(uc.encryptedCredential)
	}

	a, err := unmarshalAuthenticator(raw)
	if err != nil {
		return nil, errors.Wrap(err, "unmarshalling authenticator")
	}

	return a, nil
}

// SetAuthenticator encrypts and sets the authenticator within the user
// credential.
func (uc *UserCredential) SetAuthenticator(ctx context.Context, a auth.Authenticator) error {
	secret, err := encryptAuthenticator(ctx, uc.key, a)
	if err != nil {
		return err
	}

	// We must set credential to nil here: if we're in the middle of migrating
	// when this is called, we don't want the unencrypted credential to remain.
	uc.credential = nil
	uc.encryptedCredential = secret
	return nil
}

// This const block contains the valid domain values for user credentials.
const (
	UserCredentialDomainBatches = "batches"
)

// UserCredentialNotFoundErr is returned when a credential cannot be found from
// its ID or scope.
type UserCredentialNotFoundErr struct{ args []interface{} }

func (err UserCredentialNotFoundErr) Error() string {
	return fmt.Sprintf("user credential not found: %v", err.args)
}

func (UserCredentialNotFoundErr) NotFound() bool {
	return true
}

// UserCredentialsStore provides access to the `user_credentials` table.
type UserCredentialsStore struct {
	*basestore.Store
	key encryption.Key
}

// NewUserStoreWithDB instantiates and returns a new UserCredentialsStore with prepared statements.
func UserCredentials(db dbutil.DB, key encryption.Key) *UserCredentialsStore {
	return &UserCredentialsStore{
		Store: basestore.NewWithDB(db, sql.TxOptions{}),
		key:   key,
	}
}

// NewUserStoreWith instantiates and returns a new UserCredentialsStore using the other store handle.
func UserCredentialsWith(other basestore.ShareableStore, key encryption.Key) *UserCredentialsStore {
	return &UserCredentialsStore{
		Store: basestore.NewWithHandle(other.Handle()),
		key:   key,
	}
}

func (s *UserCredentialsStore) With(other basestore.ShareableStore) *UserCredentialsStore {
	return &UserCredentialsStore{Store: s.Store.With(other)}
}

func (s *UserCredentialsStore) Transact(ctx context.Context) (*UserCredentialsStore, error) {
	txBase, err := s.Store.Transact(ctx)
	return &UserCredentialsStore{Store: txBase}, err
}

// UserCredentialScope represents the unique scope for a credential. Only one
// credential may exist within a scope.
type UserCredentialScope struct {
	Domain              string
	UserID              int32
	ExternalServiceType string
	ExternalServiceID   string
}

// Create creates a new user credential based on the given scope and
// authenticator. If the scope already has a credential, an error will be
// returned.
func (s *UserCredentialsStore) Create(ctx context.Context, scope UserCredentialScope, credential auth.Authenticator) (*UserCredential, error) {
	if Mocks.UserCredentials.Create != nil {
		return Mocks.UserCredentials.Create(ctx, scope, credential)
	}

	enc, err := encryptAuthenticator(ctx, s.key, credential)
	if err != nil {
		return nil, err
	}

	q := sqlf.Sprintf(
		userCredentialsCreateQueryFmtstr,
		scope.Domain,
		scope.UserID,
		scope.ExternalServiceType,
		scope.ExternalServiceID,
		enc,
		sqlf.Join(userCredentialsColumns, ", "),
	)

	cred := UserCredential{key: s.key}
	row := s.QueryRow(ctx, q)
	if err := scanUserCredential(&cred, row); err != nil {
		return nil, err
	}

	return &cred, nil
}

// Update updates a user credential in the database. If the credential cannot be found,
// an error is returned.
func (s *UserCredentialsStore) Update(ctx context.Context, credential *UserCredential) error {
	if Mocks.UserCredentials.Update != nil {
		return Mocks.UserCredentials.Update(ctx, credential)
	}

	credential.UpdatedAt = timeutil.Now()

	q := sqlf.Sprintf(
		userCredentialsUpdateQueryFmtstr,
		credential.Domain,
		credential.UserID,
		credential.ExternalServiceType,
		credential.ExternalServiceID,
		&NullAuthenticator{A: &credential.credential},
		credential.encryptedCredential,
		credential.UpdatedAt,
		credential.SSHMigrationApplied,
		credential.ID,
		sqlf.Join(userCredentialsColumns, ", "),
	)

	row := s.QueryRow(ctx, q)
	if err := scanUserCredential(credential, row); err != nil {
		return err
	}

	return nil
}

// Delete deletes the given user credential. Note that there is no concept of a
// soft delete with user credentials: once deleted, the relevant records are
// _gone_, so that we don't hold any sensitive data unexpectedly. 💀
func (s *UserCredentialsStore) Delete(ctx context.Context, id int64) error {
	if Mocks.UserCredentials.Delete != nil {
		return Mocks.UserCredentials.Delete(ctx, id)
	}

	q := sqlf.Sprintf("DELETE FROM user_credentials WHERE id = %s", id)
	res, err := s.ExecResult(ctx, q)
	if err != nil {
		return err
	}

	if rows, err := res.RowsAffected(); err != nil {
		return err
	} else if rows == 0 {
		return UserCredentialNotFoundErr{args: []interface{}{id}}
	}

	return nil
}

// GetByID returns the user credential matching the given ID, or
// UserCredentialNotFoundErr if no such credential exists.
func (s *UserCredentialsStore) GetByID(ctx context.Context, id int64) (*UserCredential, error) {
	if Mocks.UserCredentials.GetByID != nil {
		return Mocks.UserCredentials.GetByID(ctx, id)
	}

	q := sqlf.Sprintf(
		"SELECT %s FROM user_credentials WHERE id = %s",
		sqlf.Join(userCredentialsColumns, ", "),
		id,
	)

	cred := UserCredential{key: s.key}
	row := s.QueryRow(ctx, q)
	if err := scanUserCredential(&cred, row); err == sql.ErrNoRows {
		return nil, UserCredentialNotFoundErr{args: []interface{}{id}}
	} else if err != nil {
		return nil, err
	}

	return &cred, nil
}

// GetByScope returns the user credential matching the given scope, or
// UserCredentialNotFoundErr if no such credential exists.
func (s *UserCredentialsStore) GetByScope(ctx context.Context, scope UserCredentialScope) (*UserCredential, error) {
	if Mocks.UserCredentials.GetByScope != nil {
		return Mocks.UserCredentials.GetByScope(ctx, scope)
	}

	q := sqlf.Sprintf(
		userCredentialsGetByScopeQueryFmtstr,
		sqlf.Join(userCredentialsColumns, ", "),
		scope.Domain,
		scope.UserID,
		scope.ExternalServiceType,
		scope.ExternalServiceID,
	)

	cred := UserCredential{key: s.key}
	row := s.QueryRow(ctx, q)
	if err := scanUserCredential(&cred, row); err == sql.ErrNoRows {
		return nil, UserCredentialNotFoundErr{args: []interface{}{scope}}
	} else if err != nil {
		return nil, err
	}

	return &cred, nil
}

// UserCredentialsListOpts provide the options when listing credentials. At
// least one field in Scope must be set.
type UserCredentialsListOpts struct {
	*LimitOffset
	Scope     UserCredentialScope
	ForUpdate bool

	// TODO(batch-change-credential-encryption): this should be removed once the
	// OOB SSH migration is removed.
	SSHMigrationApplied *bool

	// TODO(batch-change-credential-encryption): this should be removed once the
	// OOB user credential migration is removed.
	OnlyUnencrypted bool
}

// sql overrides LimitOffset.SQL() to give a LIMIT clause with one extra value
// so we can populate the next cursor.
func (opts *UserCredentialsListOpts) sql() *sqlf.Query {
	if opts.LimitOffset == nil || opts.Limit == 0 {
		return &sqlf.Query{}
	}

	return (&LimitOffset{Limit: opts.Limit + 1, Offset: opts.Offset}).SQL()
}

// List returns all user credentials matching the given options.
func (s *UserCredentialsStore) List(ctx context.Context, opts UserCredentialsListOpts) ([]*UserCredential, int, error) {
	if Mocks.UserCredentials.List != nil {
		return Mocks.UserCredentials.List(ctx, opts)
	}

	preds := []*sqlf.Query{}
	if opts.Scope.Domain != "" {
		preds = append(preds, sqlf.Sprintf("domain = %s", opts.Scope.Domain))
	}
	if opts.Scope.UserID != 0 {
		preds = append(preds, sqlf.Sprintf("user_id = %s", opts.Scope.UserID))
	}
	if opts.Scope.ExternalServiceType != "" {
		preds = append(preds, sqlf.Sprintf("external_service_type = %s", opts.Scope.ExternalServiceType))
	}
	if opts.Scope.ExternalServiceID != "" {
		preds = append(preds, sqlf.Sprintf("external_service_id = %s", opts.Scope.ExternalServiceID))
	}
	// TODO(batch-change-credential-encryption): remove once the OOB SSH
	// migration is removed.
	if opts.SSHMigrationApplied != nil {
		preds = append(preds, sqlf.Sprintf("ssh_migration_applied = %s", *opts.SSHMigrationApplied))
	}
	// TODO(batch-change-credential-encryption): remove once the OOB user
	// credential migration is removed.
	if opts.OnlyUnencrypted {
		preds = append(preds, sqlf.Sprintf("credential_enc IS NULL"))
	}

	if len(preds) == 0 {
		preds = append(preds, sqlf.Sprintf("TRUE"))
	}

	forUpdate := &sqlf.Query{}
	if opts.ForUpdate {
		forUpdate = sqlf.Sprintf("FOR UPDATE")
	}

	q := sqlf.Sprintf(
		userCredentialsListQueryFmtstr,
		sqlf.Join(userCredentialsColumns, ", "),
		sqlf.Join(preds, "\n AND "),
		opts.sql(),
		forUpdate,
	)

	rows, err := s.Query(ctx, q)
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()

	var creds []*UserCredential
	for rows.Next() {
		cred := UserCredential{key: s.key}
		if err := scanUserCredential(&cred, rows); err != nil {
			return nil, 0, err
		}
		creds = append(creds, &cred)
	}

	// Check if there were more results than the limit: if so, then we need to
	// set the return cursor and lop off the extra credential that we retrieved.
	next := 0
	if opts.LimitOffset != nil && opts.Limit != 0 && len(creds) == opts.Limit+1 {
		next = opts.Offset + opts.Limit
		creds = creds[:len(creds)-1]
	}

	return creds, next, nil
}

// 🐉 This marks the end of the public API. Beyond here are dragons.

// userCredentialsColumns are the columns that must be selected by
// user_credentials queries in order to use scanUserCredential().
var userCredentialsColumns = []*sqlf.Query{
	sqlf.Sprintf("id"),
	sqlf.Sprintf("domain"),
	sqlf.Sprintf("user_id"),
	sqlf.Sprintf("external_service_type"),
	sqlf.Sprintf("external_service_id"),
	sqlf.Sprintf("credential"),
	sqlf.Sprintf("credential_enc"),
	sqlf.Sprintf("created_at"),
	sqlf.Sprintf("updated_at"),
	sqlf.Sprintf("ssh_migration_applied"),
}

// The more unwieldy queries are below rather than inline in the above methods
// in a vain attempt to improve their readability.

const userCredentialsGetByScopeQueryFmtstr = `
-- source: internal/database/user_credentials.go:GetByScope
SELECT %s
FROM user_credentials
WHERE
	domain = %s AND
	user_id = %s AND
	external_service_type = %s AND
	external_service_id = %s
`

const userCredentialsListQueryFmtstr = `
-- source: internal/database/user_credentials.go:List
SELECT %s
FROM user_credentials
WHERE %s
ORDER BY created_at ASC, domain ASC, user_id ASC, external_service_id ASC
%s  -- LIMIT clause
%s  -- optional FOR UPDATE
`

const userCredentialsCreateQueryFmtstr = `
-- source: internal/database/user_credentials.go:Create
INSERT INTO
	user_credentials (
		domain,
		user_id,
		external_service_type,
		external_service_id,
		credential_enc,
		created_at,
		updated_at,
		ssh_migration_applied
	)
	VALUES (
		%s,
		%s,
		%s,
		%s,
		%s,
		NOW(),
		NOW(),
		TRUE
	)
	RETURNING %s
`

const userCredentialsUpdateQueryFmtstr = `
-- source: internal/database/user_credentials.go:Update
UPDATE user_credentials
SET
	domain = %s,
	user_id = %s,
	external_service_type = %s,
	external_service_id = %s,
	credential = %s,
	credential_enc = %s,
	updated_at = %s,
	ssh_migration_applied = %s
WHERE
	id = %s
RETURNING %s
`

// scanUserCredential scans a credential from the given scanner into the given
// credential.
//
// s is inspired by the BatchChange scanner type, but also matches sql.Row, which
// is generally used directly in this module.
func scanUserCredential(cred *UserCredential, s interface {
	Scan(...interface{}) error
}) error {
	return s.Scan(
		&cred.ID,
		&cred.Domain,
		&cred.UserID,
		&cred.ExternalServiceType,
		&cred.ExternalServiceID,
		&NullAuthenticator{A: &cred.credential},
		&cred.encryptedCredential,
		&cred.CreatedAt,
		&cred.UpdatedAt,
		&cred.SSHMigrationApplied,
	)
}
