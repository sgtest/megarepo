package database

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"time"

	"github.com/cockroachdb/errors"
	"github.com/keegancsmith/sqlf"
	"github.com/lib/pq"

	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
)

// AccessToken describes an access token. The actual token (that a caller must supply to
// authenticate) is not stored and is not present in this struct.
type AccessToken struct {
	ID            int64
	SubjectUserID int32 // the user whose privileges the access token grants
	Scopes        []string
	Note          string
	CreatorUserID int32
	// Internal determines whether or not the token shows up in the UI. Tokens
	// with internal=true are to be used with the executor service.
	Internal   bool
	CreatedAt  time.Time
	LastUsedAt *time.Time
}

// ErrAccessTokenNotFound occurs when a database operation expects a specific access token to exist
// but it does not exist.
var ErrAccessTokenNotFound = errors.New("access token not found")

type AccessTokenStore interface {
	Count(context.Context, AccessTokensListOptions) (int, error)
	Create(ctx context.Context, subjectUserID int32, scopes []string, note string, creatorUserID int32) (id int64, token string, err error)
	CreateInternal(ctx context.Context, subjectUserID int32, scopes []string, note string, creatorUserID int32) (id int64, token string, err error)
	DeleteByID(context.Context, int64) error
	DeleteByToken(ctx context.Context, tokenHexEncoded string) error
	GetByID(context.Context, int64) (*AccessToken, error)
	GetByToken(ctx context.Context, tokenHexEncoded string) (*AccessToken, error)
	HardDeleteByID(context.Context, int64) error
	List(context.Context, AccessTokensListOptions) ([]*AccessToken, error)
	Lookup(ctx context.Context, tokenHexEncoded, requiredScope string) (subjectUserID int32, err error)
	Transact(context.Context) (AccessTokenStore, error)
	With(basestore.ShareableStore) AccessTokenStore
	basestore.ShareableStore
}

// accessTokenStore implements autocert.Cache
type accessTokenStore struct {
	*basestore.Store
}

var _ AccessTokenStore = (*accessTokenStore)(nil)

// AccessTokens instantiates and returns a new AccessTokenStore with prepared statements.
func AccessTokens(db dbutil.DB) AccessTokenStore {
	return &accessTokenStore{Store: basestore.NewWithDB(db, sql.TxOptions{})}
}

// AccessTokensWith instantiates and returns a new AccessTokenStore using the other store handle.
func AccessTokensWith(other basestore.ShareableStore) AccessTokenStore {
	return &accessTokenStore{Store: basestore.NewWithHandle(other.Handle())}
}

func (s *accessTokenStore) With(other basestore.ShareableStore) AccessTokenStore {
	return &accessTokenStore{Store: s.Store.With(other)}
}

func (s *accessTokenStore) Transact(ctx context.Context) (AccessTokenStore, error) {
	txBase, err := s.Store.Transact(ctx)
	return &accessTokenStore{Store: txBase}, err
}

// Create creates an access token for the specified user. The secret token value itself is
// returned. The caller is responsible for presenting this value to the end user; Sourcegraph does
// not retain it (only a hash of it).
//
// The secret token value is a long random string; it is what API clients must provide to
// authenticate their requests. We store the SHA-256 hash of the secret token value in the
// database. This lets us verify a token's validity (in the (*accessTokens).Lookup method) quickly,
// while still ensuring that an attacker who obtains the access_tokens DB table would not be able to
// impersonate a token holder. We don't use bcrypt because the original secret is a randomly
// generated string (not a password), so it's implausible for an attacker to brute-force the input
// space; also bcrypt is slow and would add noticeable latency to each request that supplied a
// token.
//
// 🚨 SECURITY: The caller must ensure that the actor is permitted to create tokens for the
// specified user (i.e., that the actor is either the user or a site admin).
func (s *accessTokenStore) Create(ctx context.Context, subjectUserID int32, scopes []string, note string, creatorUserID int32) (id int64, token string, err error) {
	if Mocks.AccessTokens.Create != nil {
		return Mocks.AccessTokens.Create(subjectUserID, scopes, note, creatorUserID)
	}

	return s.createToken(ctx, subjectUserID, scopes, note, creatorUserID, false)
}

// CreateInternal creates an *internal* access token for the specified user. An
// internal access token will be used by Sourcegraph to talk to its API from
// other services, i.e. executor jobs. Internal tokens do not show up in the UI.
//
// See the documentation for Create for more details.
//
// 🚨 SECURITY: The caller must ensure that the actor is permitted to create tokens for the
// specified user (i.e., that the actor is either the user or a site admin).
func (s *accessTokenStore) CreateInternal(ctx context.Context, subjectUserID int32, scopes []string, note string, creatorUserID int32) (id int64, token string, err error) {
	if Mocks.AccessTokens.CreateInternal != nil {
		return Mocks.AccessTokens.CreateInternal(subjectUserID, scopes, note, creatorUserID)
	}

	return s.createToken(ctx, subjectUserID, scopes, note, creatorUserID, true)
}

func (s *accessTokenStore) createToken(ctx context.Context, subjectUserID int32, scopes []string, note string, creatorUserID int32, internal bool) (id int64, token string, err error) {
	var b [20]byte
	if _, err := rand.Read(b[:]); err != nil {
		return 0, "", err
	}
	token = hex.EncodeToString(b[:])

	if len(scopes) == 0 {
		// Prevent mistakes. There is no point in creating an access token with no scopes, and the
		// GraphQL API wouldn't let you do so anyway.
		return 0, "", errors.New("access tokens without scopes are not supported")
	}

	if err := s.Handle().DB().QueryRowContext(ctx,
		// Include users table query (with "FOR UPDATE") to ensure that subject/creator users have
		// not been deleted. If they were deleted, the query will return an error.
		`
WITH subject_user AS (
  SELECT id FROM users WHERE id=$1 AND deleted_at IS NULL FOR UPDATE
),
creator_user AS (
  SELECT id FROM users WHERE id=$5 AND deleted_at IS NULL FOR UPDATE
),
insert_values AS (
  SELECT subject_user.id AS subject_user_id, $2::text[] AS scopes, $3::bytea AS value_sha256, $4::text AS note, creator_user.id AS creator_user_id, $6::boolean AS internal
  FROM subject_user, creator_user
)
INSERT INTO access_tokens(subject_user_id, scopes, value_sha256, note, creator_user_id, internal) SELECT * FROM insert_values RETURNING id
`,
		subjectUserID, pq.Array(scopes), toSHA256Bytes(b[:]), note, creatorUserID, internal,
	).Scan(&id); err != nil {
		return 0, "", err
	}
	return id, token, nil
}

// Lookup looks up the access token. If it's valid and contains the required scope, it returns the
// subject's user ID. Otherwise ErrAccessTokenNotFound is returned.
//
// Calling Lookup also updates the access token's last-used-at date.
//
// 🚨 SECURITY: This returns a user ID if and only if the tokenHexEncoded corresponds to a valid,
// non-deleted access token.
func (s *accessTokenStore) Lookup(ctx context.Context, tokenHexEncoded, requiredScope string) (subjectUserID int32, err error) {
	if Mocks.AccessTokens.Lookup != nil {
		return Mocks.AccessTokens.Lookup(tokenHexEncoded, requiredScope)
	}

	if requiredScope == "" {
		return 0, errors.New("no scope provided in access token lookup")
	}

	token, err := hex.DecodeString(tokenHexEncoded)
	if err != nil {
		return 0, errors.Wrap(err, "AccessTokens.Lookup")
	}

	if err := s.Handle().DB().QueryRowContext(ctx,
		// Ensure that subject and creator users still exist.
		`
UPDATE access_tokens t SET last_used_at=now()
WHERE t.id IN (
	SELECT t2.id FROM access_tokens t2
	JOIN users subject_user ON t2.subject_user_id=subject_user.id AND subject_user.deleted_at IS NULL
	JOIN users creator_user ON t2.creator_user_id=creator_user.id AND creator_user.deleted_at IS NULL
	WHERE t2.value_sha256=$1 AND t2.deleted_at IS NULL AND
	$2 = ANY (t2.scopes)
)
RETURNING t.subject_user_id
`,
		toSHA256Bytes(token), requiredScope,
	).Scan(&subjectUserID); err != nil {
		if err == sql.ErrNoRows {
			return 0, ErrAccessTokenNotFound
		}
		return 0, err
	}
	return subjectUserID, nil
}

// GetByID retrieves the access token (if any) given its ID.
//
// 🚨 SECURITY: The caller must ensure that the actor is permitted to view this access token.
func (s *accessTokenStore) GetByID(ctx context.Context, id int64) (*AccessToken, error) {
	if Mocks.AccessTokens.GetByID != nil {
		return Mocks.AccessTokens.GetByID(id)
	}

	return s.get(ctx, []*sqlf.Query{sqlf.Sprintf("id=%d", id)})
}

// GetByToken retrieves the access token (if any) given its hex encoded string.
//
// 🚨 SECURITY: The caller must ensure that the actor is permitted to view this access token.
func (s *accessTokenStore) GetByToken(ctx context.Context, tokenHexEncoded string) (*AccessToken, error) {
	token, err := hex.DecodeString(tokenHexEncoded)
	if err != nil {
		return nil, errors.Wrap(err, "AccessTokens.GetByToken")
	}

	return s.get(ctx, []*sqlf.Query{sqlf.Sprintf("value_sha256=%s", toSHA256Bytes(token))})
}

func (s *accessTokenStore) get(ctx context.Context, conds []*sqlf.Query) (*AccessToken, error) {
	results, err := s.list(ctx, conds, nil)
	if err != nil {
		return nil, err
	}
	if len(results) == 0 {
		return nil, ErrAccessTokenNotFound
	}
	return results[0], nil
}

// AccessTokensListOptions contains options for listing access tokens.
type AccessTokensListOptions struct {
	SubjectUserID  int32 // only list access tokens with this user as the subject
	LastUsedAfter  *time.Time
	LastUsedBefore *time.Time
	*LimitOffset
}

func (o AccessTokensListOptions) sqlConditions() []*sqlf.Query {
	conds := []*sqlf.Query{
		sqlf.Sprintf("deleted_at IS NULL"),
		// We never want internal access tokens to show up in the UI.
		sqlf.Sprintf("internal IS FALSE"),
	}
	if o.SubjectUserID != 0 {
		conds = append(conds, sqlf.Sprintf("subject_user_id=%d", o.SubjectUserID))
	}
	if o.LastUsedAfter != nil {
		conds = append(conds, sqlf.Sprintf("last_used_at>%d", o.LastUsedAfter))
	}
	if o.LastUsedBefore != nil {
		conds = append(conds, sqlf.Sprintf("last_used_at<%d", o.LastUsedBefore))
	}
	return conds
}

// List lists all access tokens that satisfy the options, except internal tokens.
//
// 🚨 SECURITY: The caller must ensure that the actor is permitted to list with the specified
// options.
func (s *accessTokenStore) List(ctx context.Context, opt AccessTokensListOptions) ([]*AccessToken, error) {
	return s.list(ctx, opt.sqlConditions(), opt.LimitOffset)
}

func (s *accessTokenStore) list(ctx context.Context, conds []*sqlf.Query, limitOffset *LimitOffset) ([]*AccessToken, error) {
	q := sqlf.Sprintf(`
SELECT id, subject_user_id, scopes, note, creator_user_id, internal, created_at, last_used_at FROM access_tokens
WHERE (%s)
ORDER BY now() - created_at < interval '5 minutes' DESC, -- show recently created tokens first
last_used_at DESC NULLS FIRST, -- ensure newly created tokens show first
created_at DESC
%s`,
		sqlf.Join(conds, ") AND ("),
		limitOffset.SQL(),
	)

	rows, err := s.Query(ctx, q)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var results []*AccessToken
	for rows.Next() {
		var t AccessToken
		if err := rows.Scan(&t.ID, &t.SubjectUserID, pq.Array(&t.Scopes), &t.Note, &t.CreatorUserID, &t.Internal, &t.CreatedAt, &t.LastUsedAt); err != nil {
			return nil, err
		}
		results = append(results, &t)
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}

	return results, nil
}

// Count counts all access tokens, except internal tokens, that satisfy the options (ignoring limit and offset).
//
// 🚨 SECURITY: The caller must ensure that the actor is permitted to count the tokens.
func (s *accessTokenStore) Count(ctx context.Context, opt AccessTokensListOptions) (int, error) {
	q := sqlf.Sprintf("SELECT COUNT(*) FROM access_tokens WHERE (%s)", sqlf.Join(opt.sqlConditions(), ") AND ("))
	var count int
	if err := s.QueryRow(ctx, q).Scan(&count); err != nil {
		return 0, err
	}
	return count, nil
}

// DeleteByID deletes an access token given its ID.
//
// 🚨 SECURITY: The caller must ensure that the actor is permitted to delete the token.
func (s *accessTokenStore) DeleteByID(ctx context.Context, id int64) error {
	if Mocks.AccessTokens.DeleteByID != nil {
		return Mocks.AccessTokens.DeleteByID(id)
	}
	return s.delete(ctx, sqlf.Sprintf("id=%d", id))
}

// HardDeleteByID hard-deletes an access token given its ID.
//
// 🚨 SECURITY: The caller must ensure that the actor is permitted to delete the token.
func (s *accessTokenStore) HardDeleteByID(ctx context.Context, id int64) error {
	if Mocks.AccessTokens.HardDeleteByID != nil {
		return Mocks.AccessTokens.HardDeleteByID(id)
	}
	res, err := s.ExecResult(ctx, sqlf.Sprintf("DELETE FROM access_tokens WHERE id = %s", id))
	if err != nil {
		return err
	}
	nrows, err := res.RowsAffected()
	if err != nil {
		return err
	}
	if nrows == 0 {
		return ErrAccessTokenNotFound
	}
	return nil
}

// DeleteByToken deletes an access token given the secret token value itself (i.e., the same value
// that an API client would use to authenticate).
func (s *accessTokenStore) DeleteByToken(ctx context.Context, tokenHexEncoded string) error {
	token, err := hex.DecodeString(tokenHexEncoded)
	if err != nil {
		return errors.Wrap(err, "AccessTokens.DeleteByToken")
	}

	return s.delete(ctx, sqlf.Sprintf("value_sha256=%s", toSHA256Bytes(token)))
}

func (s *accessTokenStore) delete(ctx context.Context, cond *sqlf.Query) error {
	conds := []*sqlf.Query{cond, sqlf.Sprintf("deleted_at IS NULL")}
	q := sqlf.Sprintf("UPDATE access_tokens SET deleted_at=now() WHERE (%s)", sqlf.Join(conds, ") AND ("))

	res, err := s.ExecResult(ctx, q)
	if err != nil {
		return err
	}
	nrows, err := res.RowsAffected()
	if err != nil {
		return err
	}
	if nrows == 0 {
		return ErrAccessTokenNotFound
	}
	return nil
}

func toSHA256Bytes(input []byte) []byte {
	b := sha256.Sum256(input)
	return b[:]
}

type MockAccessTokens struct {
	Create         func(subjectUserID int32, scopes []string, note string, creatorUserID int32) (id int64, token string, err error)
	CreateInternal func(subjectUserID int32, scopes []string, note string, creatorUserID int32) (id int64, token string, err error)
	DeleteByID     func(id int64) error
	HardDeleteByID func(id int64) error
	Lookup         func(tokenHexEncoded, requiredScope string) (subjectUserID int32, err error)
	GetByID        func(id int64) (*AccessToken, error)
}
