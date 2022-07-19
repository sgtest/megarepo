package database

import (
	"context"
	"database/sql"
	"fmt"
	"time"

	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/log"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/encryption"
	"github.com/sourcegraph/sourcegraph/internal/extsvc/auth"
	"github.com/sourcegraph/sourcegraph/internal/timeutil"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// UserCredential represents a row in the `user_credentials` table.
type UserCredential struct {
	ID                  int64
	Domain              string
	UserID              int32
	ExternalServiceType string
	ExternalServiceID   string
	EncryptedCredential []byte
	EncryptionKeyID     string
	CreatedAt           time.Time
	UpdatedAt           time.Time

	// TODO(batch-change-credential-encryption): On or after Sourcegraph 3.30,
	// we should remove the credential and SSHMigrationApplied fields.
	SSHMigrationApplied bool

	key encryption.Key
}

// Authenticator decrypts and creates the authenticator associated with the user
// credential.
func (uc *UserCredential) Authenticator(ctx context.Context) (auth.Authenticator, error) {
	// The record includes a field indicating the encryption key ID. We don't
	// really have a way to look up a key by ID right now, so this is used as a
	// marker of whether we should expect a key or not.
	if uc.EncryptionKeyID == "" || uc.EncryptionKeyID == UserCredentialUnmigratedEncryptionKeyID {
		return UnmarshalAuthenticator(string(uc.EncryptedCredential))
	}
	if uc.key == nil {
		return nil, errors.New("user credential is encrypted, but no key is available to decrypt it")
	}

	secret, err := uc.key.Decrypt(ctx, uc.EncryptedCredential)
	if err != nil {
		return nil, errors.Wrap(err, "decrypting credential")
	}

	a, err := UnmarshalAuthenticator(secret.Secret())
	if err != nil {
		return nil, errors.Wrap(err, "unmarshalling authenticator")
	}

	return a, nil
}

// SetAuthenticator encrypts and sets the authenticator within the user
// credential.
func (uc *UserCredential) SetAuthenticator(ctx context.Context, a auth.Authenticator) error {
	// Set the key ID. This is cargo culted from external_accounts.go, and the
	// key ID doesn't appear to be actually useful as anything other than a
	// marker of whether the data is expected to be encrypted or not.
	id, err := keyID(ctx, uc.key)
	if err != nil {
		return errors.Wrap(err, "getting key version")
	}

	secret, err := EncryptAuthenticator(ctx, uc.key, a)
	if err != nil {
		return errors.Wrap(err, "encrypting authenticator")
	}

	uc.EncryptedCredential = secret
	uc.EncryptionKeyID = id

	return nil
}

const (
	// Valid domain values for user credentials.
	UserCredentialDomainBatches = "batches"

	// Placeholder encryption key IDs.
	UserCredentialPlaceholderEncryptionKeyID = "previously-migrated"
	UserCredentialUnmigratedEncryptionKeyID  = "unmigrated"
)

// UserCredentialNotFoundErr is returned when a credential cannot be found from
// its ID or scope.
type UserCredentialNotFoundErr struct{ args []any }

func (err UserCredentialNotFoundErr) Error() string {
	return fmt.Sprintf("user credential not found: %v", err.args)
}

func (UserCredentialNotFoundErr) NotFound() bool {
	return true
}

type UserCredentialsStore interface {
	basestore.ShareableStore
	With(basestore.ShareableStore) UserCredentialsStore
	Transact(context.Context) (UserCredentialsStore, error)
	Create(ctx context.Context, scope UserCredentialScope, credential auth.Authenticator) (*UserCredential, error)
	Update(context.Context, *UserCredential) error
	Delete(ctx context.Context, id int64) error
	GetByID(ctx context.Context, id int64) (*UserCredential, error)
	GetByScope(context.Context, UserCredentialScope) (*UserCredential, error)
	List(context.Context, UserCredentialsListOpts) ([]*UserCredential, int, error)
}

// userCredentialsStore provides access to the `user_credentials` table.
type userCredentialsStore struct {
	logger log.Logger
	*basestore.Store
	key encryption.Key
}

// UserCredentialsWith instantiates and returns a new UserCredentialsStore using the other store handle.
func UserCredentialsWith(logger log.Logger, other basestore.ShareableStore, key encryption.Key) UserCredentialsStore {
	return &userCredentialsStore{
		logger: logger,
		Store:  basestore.NewWithHandle(other.Handle()),
		key:    key,
	}
}

func (s *userCredentialsStore) With(other basestore.ShareableStore) UserCredentialsStore {
	return &userCredentialsStore{
		logger: s.logger,
		Store:  s.Store.With(other),
		key:    s.key,
	}
}

func (s *userCredentialsStore) Transact(ctx context.Context) (UserCredentialsStore, error) {
	txBase, err := s.Store.Transact(ctx)
	return &userCredentialsStore{
		logger: s.logger,
		Store:  txBase,
		key:    s.key,
	}, err
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
func (s *userCredentialsStore) Create(ctx context.Context, scope UserCredentialScope, credential auth.Authenticator) (*UserCredential, error) {
	// SECURITY: check that the current user is authorised to create a user
	// credential for the given user scope.
	if err := userCredentialsAuthzScope(ctx, NewDBWith(s.logger, s), scope); err != nil {
		return nil, err
	}

	id, err := keyID(ctx, s.key)
	if err != nil {
		return nil, err
	}

	enc, err := EncryptAuthenticator(ctx, s.key, credential)
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
		id,
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
func (s *userCredentialsStore) Update(ctx context.Context, credential *UserCredential) error {
	authz, err := userCredentialsAuthzQueryConds(ctx)
	if err != nil {
		return err
	}

	credential.UpdatedAt = timeutil.Now()

	q := sqlf.Sprintf(
		userCredentialsUpdateQueryFmtstr,
		credential.Domain,
		credential.UserID,
		credential.ExternalServiceType,
		credential.ExternalServiceID,
		credential.EncryptedCredential,
		credential.EncryptionKeyID,
		credential.UpdatedAt,
		credential.SSHMigrationApplied,
		credential.ID,
		authz,
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
func (s *userCredentialsStore) Delete(ctx context.Context, id int64) error {
	authz, err := userCredentialsAuthzQueryConds(ctx)
	if err != nil {
		return err
	}

	q := sqlf.Sprintf("DELETE FROM user_credentials WHERE id = %s AND %s", id, authz)
	res, err := s.ExecResult(ctx, q)
	if err != nil {
		return err
	}

	if rows, err := res.RowsAffected(); err != nil {
		return err
	} else if rows == 0 {
		return UserCredentialNotFoundErr{args: []any{id}}
	}

	return nil
}

// GetByID returns the user credential matching the given ID, or
// UserCredentialNotFoundErr if no such credential exists.
func (s *userCredentialsStore) GetByID(ctx context.Context, id int64) (*UserCredential, error) {
	authz, err := userCredentialsAuthzQueryConds(ctx)
	if err != nil {
		return nil, err
	}

	q := sqlf.Sprintf(
		"SELECT %s FROM user_credentials WHERE id = %s AND %s",
		sqlf.Join(userCredentialsColumns, ", "),
		id,
		authz,
	)

	cred := UserCredential{key: s.key}
	row := s.QueryRow(ctx, q)
	if err := scanUserCredential(&cred, row); err == sql.ErrNoRows {
		return nil, UserCredentialNotFoundErr{args: []any{id}}
	} else if err != nil {
		return nil, err
	}

	return &cred, nil
}

// GetByScope returns the user credential matching the given scope, or
// UserCredentialNotFoundErr if no such credential exists.
func (s *userCredentialsStore) GetByScope(ctx context.Context, scope UserCredentialScope) (*UserCredential, error) {
	authz, err := userCredentialsAuthzQueryConds(ctx)
	if err != nil {
		return nil, err
	}

	q := sqlf.Sprintf(
		userCredentialsGetByScopeQueryFmtstr,
		sqlf.Join(userCredentialsColumns, ", "),
		scope.Domain,
		scope.UserID,
		scope.ExternalServiceType,
		scope.ExternalServiceID,
		authz,
	)

	cred := UserCredential{key: s.key}
	row := s.QueryRow(ctx, q)
	if err := scanUserCredential(&cred, row); err == sql.ErrNoRows {
		return nil, UserCredentialNotFoundErr{args: []any{scope}}
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
	RequiresMigration bool

	// TODO(batch-change-credential-encryption): this should be removed once the
	// OOB user credential migration is removed.
	OnlyEncrypted bool
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
func (s *userCredentialsStore) List(ctx context.Context, opts UserCredentialsListOpts) ([]*UserCredential, int, error) {
	authz, err := userCredentialsAuthzQueryConds(ctx)
	if err != nil {
		return nil, 0, err
	}

	preds := []*sqlf.Query{authz}
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
	// TODO(batch-change-credential-encryption): remove the remaining predicates
	// once the OOB SSH migration is removed.
	if opts.SSHMigrationApplied != nil {
		preds = append(preds, sqlf.Sprintf("ssh_migration_applied = %s", *opts.SSHMigrationApplied))
	}
	if opts.RequiresMigration {
		preds = append(preds, sqlf.Sprintf(
			"encryption_key_id IN (%s, %s)",
			UserCredentialPlaceholderEncryptionKeyID,
			UserCredentialUnmigratedEncryptionKeyID,
		))
	}
	if opts.OnlyEncrypted {
		preds = append(preds, sqlf.Sprintf(
			"encryption_key_id NOT IN ('', %s)",
			UserCredentialUnmigratedEncryptionKeyID,
		))
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
	sqlf.Sprintf("encryption_key_id"),
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
	external_service_id = %s AND
	%s -- authz query conds
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
		credential,
		encryption_key_id,
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
	encryption_key_id = %s,
	updated_at = %s,
	ssh_migration_applied = %s
WHERE
	id = %s AND
	%s -- authz query conds
RETURNING %s
`

// scanUserCredential scans a credential from the given scanner into the given
// credential.
//
// s is inspired by the BatchChange scanner type, but also matches sql.Row, which
// is generally used directly in this module.
func scanUserCredential(cred *UserCredential, s interface {
	Scan(...any) error
}) error {
	return s.Scan(
		&cred.ID,
		&cred.Domain,
		&cred.UserID,
		&cred.ExternalServiceType,
		&cred.ExternalServiceID,
		&cred.EncryptedCredential,
		&cred.EncryptionKeyID,
		&cred.CreatedAt,
		&cred.UpdatedAt,
		&cred.SSHMigrationApplied,
	)
}

func keyID(ctx context.Context, key encryption.Key) (string, error) {
	if key != nil {
		version, err := key.Version(ctx)
		if err != nil {
			return "", errors.Wrap(err, "getting key version")
		}
		return version.JSON(), nil
	}

	return "", nil
}

var errUserCredentialCreateAuthz = errors.New("current user cannot create a user credential in this scope")

func userCredentialsAuthzScope(ctx context.Context, db DB, scope UserCredentialScope) error {
	a := actor.FromContext(ctx)
	if a.IsInternal() {
		return nil
	}

	user, err := db.Users().GetByCurrentAuthUser(ctx)
	if err != nil {
		return errors.Wrap(err, "getting auth user from context")
	}
	if user.SiteAdmin && !conf.Get().AuthzEnforceForSiteAdmins {
		return nil
	}

	if user.ID != scope.UserID {
		return errUserCredentialCreateAuthz
	}

	return nil
}

func userCredentialsAuthzQueryConds(ctx context.Context) (*sqlf.Query, error) {
	a := actor.FromContext(ctx)
	if a.IsInternal() {
		return sqlf.Sprintf("(TRUE)"), nil
	}

	return sqlf.Sprintf(
		userCredentialsAuthzQueryCondsFmtstr,
		a.UID,
		!conf.Get().AuthzEnforceForSiteAdmins,
		a.UID,
	), nil
}

const userCredentialsAuthzQueryCondsFmtstr = `
(
	(
		user_credentials.user_id = %s  -- user credential user is the same as the actor
	)
	OR
	(
		%s  -- negated authz.enforceForSiteAdmins site config setting
		AND EXISTS (
			SELECT 1
			FROM users
			WHERE site_admin = TRUE AND id = %s  -- actor user ID
		)
	)
)
`
