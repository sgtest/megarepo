package db

import (
	"context"
	"crypto/subtle"
	"database/sql"
	"errors"
	"fmt"
	"time"

	"github.com/sourcegraph/sourcegraph/pkg/dbconn"
)

// UserEmail represents a row in the `user_emails` table.
type UserEmail struct {
	UserID           int32
	Email            string
	CreatedAt        time.Time
	VerificationCode *string
	VerifiedAt       *time.Time
}

// userEmailNotFoundError is the error that is returned when a user email is not found.
type userEmailNotFoundError struct {
	args []interface{}
}

func (err userEmailNotFoundError) Error() string {
	return fmt.Sprintf("user email not found: %v", err.args)
}

func (err userEmailNotFoundError) NotFound() bool {
	return true
}

// userEmails provides access to the `user_emails` table.
type userEmails struct{}

// GetInitialSiteAdminEmail returns a best guess of the email of the initial Sourcegraph installer/site admin.
// Because the initial site admin's email isn't marked, this returns the email of the active site admin with
// the lowest user ID.
//
// If the site has not yet been initialized, returns an empty string.
func (*userEmails) GetInitialSiteAdminEmail(ctx context.Context) (email string, err error) {
	if init, err := siteInitialized(ctx); err != nil || !init {
		return "", err
	}
	if err := dbconn.Global.QueryRowContext(ctx, "SELECT email FROM user_emails JOIN users ON user_emails.user_id=users.id WHERE users.site_admin AND users.deleted_at IS NULL ORDER BY users.id ASC LIMIT 1").Scan(&email); err != nil {
		return "", errors.New("initial site admin email not found")
	}
	return email, nil
}

// GetPrimaryEmail gets the oldest email associated with the user, preferring a verified email to an
// unverified email.
func (*userEmails) GetPrimaryEmail(ctx context.Context, id int32) (email string, verified bool, err error) {
	if Mocks.UserEmails.GetPrimaryEmail != nil {
		return Mocks.UserEmails.GetPrimaryEmail(ctx, id)
	}

	if err := dbconn.Global.QueryRowContext(ctx, "SELECT email, verified_at IS NOT NULL AS verified FROM user_emails WHERE user_id=$1 ORDER BY (verified_at IS NOT NULL) DESC, created_at ASC, email ASC LIMIT 1",
		id,
	).Scan(&email, &verified); err != nil {
		return "", false, userEmailNotFoundError{[]interface{}{fmt.Sprintf("id %d", id)}}
	}
	return email, verified, nil
}

// Get gets information about the user's associated email address.
func (*userEmails) Get(ctx context.Context, userID int32, email string) (emailCanonicalCase string, verified bool, err error) {
	if Mocks.UserEmails.Get != nil {
		return Mocks.UserEmails.Get(userID, email)
	}

	if err := dbconn.Global.QueryRowContext(ctx, "SELECT email, verified_at IS NOT NULL AS verified FROM user_emails WHERE user_id=$1 AND email=$2",
		userID, email,
	).Scan(&emailCanonicalCase, &verified); err != nil {
		return "", false, userEmailNotFoundError{[]interface{}{fmt.Sprintf("userID %d email %q", userID, email)}}
	}
	return emailCanonicalCase, verified, nil
}

// Add adds new user email. When added, it is always unverified.
func (*userEmails) Add(ctx context.Context, userID int32, email string, verificationCode *string) error {
	_, err := dbconn.Global.ExecContext(ctx, "INSERT INTO user_emails(user_id, email, verification_code) VALUES($1, $2, $3)", userID, email, verificationCode)
	return err
}

// Remove removes a user email. It returns an error if there is no such email associated with the user.
func (*userEmails) Remove(ctx context.Context, userID int32, email string) error {
	res, err := dbconn.Global.ExecContext(ctx, "DELETE FROM user_emails WHERE user_id=$1 AND email=$2", userID, email)
	if err != nil {
		return err
	}
	nrows, err := res.RowsAffected()
	if err != nil {
		return err
	}
	if nrows == 0 {
		return errors.New("user email not found")
	}
	return nil
}

// Verify verifies the user's email address given the email verification code. If the code is not
// correct (not the one originally used when creating the user or adding the user email), then it
// returns false.
func (*userEmails) Verify(ctx context.Context, userID int32, email, code string) (bool, error) {
	var dbCode sql.NullString
	if err := dbconn.Global.QueryRowContext(ctx, "SELECT verification_code FROM user_emails WHERE user_id=$1 AND email=$2", userID, email).Scan(&dbCode); err != nil {
		return false, err
	}
	if !dbCode.Valid {
		return false, errors.New("email already verified")
	}
	// 🚨 SECURITY: Use constant-time comparisons to avoid leaking the verification code via timing attack. It is not important to avoid leaking the *length* of the code, because the length of verification codes is constant.
	if len(dbCode.String) != len(code) || subtle.ConstantTimeCompare([]byte(dbCode.String), []byte(code)) != 1 {
		return false, nil
	}
	if _, err := dbconn.Global.ExecContext(ctx, "UPDATE user_emails SET verification_code=null, verified_at=now() WHERE user_id=$1 AND email=$2", userID, email); err != nil {
		return false, err
	}
	return true, nil
}

// SetVerified bypasses the normal email verification code process and manually sets the verified
// status for an email.
func (*userEmails) SetVerified(ctx context.Context, userID int32, email string, verified bool) error {
	var res sql.Result
	var err error
	if verified {
		// Mark as verified.
		res, err = dbconn.Global.ExecContext(ctx, "UPDATE user_emails SET verification_code=null, verified_at=now() WHERE user_id=$1 AND email=$2", userID, email)
	} else {
		// Mark as unverified.
		res, err = dbconn.Global.ExecContext(ctx, "UPDATE user_emails SET verification_code=null, verified_at=null WHERE user_id=$1 AND email=$2", userID, email)
	}
	if err != nil {
		return err
	}
	nrows, err := res.RowsAffected()
	if err != nil {
		return err
	}
	if nrows == 0 {
		return errors.New("user email not found")
	}
	return nil
}

// getBySQL returns user emails matching the SQL query, if any exist.
func (*userEmails) getBySQL(ctx context.Context, query string, args ...interface{}) ([]*UserEmail, error) {
	rows, err := dbconn.Global.QueryContext(ctx,
		`SELECT user_emails.user_id, user_emails.email, user_emails.created_at, user_emails.verification_code,
				user_emails.verified_at FROM user_emails `+query, args...)
	if err != nil {
		return nil, err
	}

	var userEmails []*UserEmail
	defer rows.Close()
	for rows.Next() {
		var v UserEmail
		err := rows.Scan(&v.UserID, &v.Email, &v.CreatedAt, &v.VerificationCode, &v.VerifiedAt)
		if err != nil {
			return nil, err
		}
		userEmails = append(userEmails, &v)
	}
	if err = rows.Err(); err != nil {
		return nil, err
	}
	return userEmails, nil
}

func (*userEmails) ListByUser(ctx context.Context, userID int32) ([]*UserEmail, error) {
	if Mocks.UserEmails.ListByUser != nil {
		return Mocks.UserEmails.ListByUser(userID)
	}

	return (&userEmails{}).getBySQL(ctx, "WHERE user_id=$1 ORDER BY created_at ASC, email ASC", userID)
}
