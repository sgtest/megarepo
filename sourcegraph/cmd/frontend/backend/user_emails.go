package backend

import (
	"context"
	"crypto/rand"
	"encoding/base64"
	"net/url"

	"github.com/pkg/errors"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/globals"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/envvar"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/router"
	"github.com/sourcegraph/sourcegraph/pkg/conf"
	"github.com/sourcegraph/sourcegraph/pkg/errcode"
	"github.com/sourcegraph/sourcegraph/pkg/txemail"
)

// UserEmails contains backend methods related to user email addresses.
var UserEmails = &userEmails{}

type userEmails struct{}

// Add adds an email address to a user. If email verification is required, it sends an email
// verification email.
func (userEmails) Add(ctx context.Context, userID int32, email string) error {
	// 🚨 SECURITY: Only the user and site admins can add an email address to a user.
	if err := CheckSiteAdminOrSameUser(ctx, userID); err != nil {
		return err
	}

	// Prevent abuse (users adding emails of other people whom they want to annoy) with the
	// following abuse prevention checks.
	if isSiteAdmin := CheckCurrentUserIsSiteAdmin(ctx) == nil; !isSiteAdmin {
		if conf.EmailVerificationRequired() {
			emails, err := db.UserEmails.ListByUser(ctx, userID)
			if err != nil {
				return err
			}

			var verifiedCount, unverifiedCount int
			for _, email := range emails {
				if email.VerifiedAt == nil {
					unverifiedCount++
				} else {
					verifiedCount++
				}
			}

			// Abuse prevention check 1: Require user to have at least one verified email address
			// before adding another.
			//
			// (We need to also allow users who have zero addresses to add one, or else they could
			// delete all emails and then get into an unrecoverable state.)
			//
			// TODO(sqs): prevent users from deleting their last email, when we have the true notion
			// of a "primary" email address.)
			if verifiedCount == 0 && len(emails) != 0 {
				return errors.New("refusing to add additional email address for user without a verified email address")
			}

			// Abuse prevention check 2: Forbid user from having many unverified emails to prevent attackers from using this to
			// send spam or a high volume of annoying emails.
			const maxUnverified = 3
			if unverifiedCount >= maxUnverified {
				return errors.New("refusing to add email address because the user has too many existing unverified email addresses")
			}
		}
		if envvar.SourcegraphDotComMode() {
			// Abuse prevention check 3: Set a quota on Sourcegraph.com users to prevent abuse.
			//
			// There is no quota for on-prem instances because we assume they can trust their users
			// to not abuse adding emails.
			//
			// TODO(sqs): This reuses the "invite quota", which is really just a number that counts
			// down (not specific to invites). Generalize this to just "quota" (remove "invite" from
			// the name).
			if ok, err := db.Users.CheckAndDecrementInviteQuota(ctx, userID); err != nil {
				return err
			} else if !ok {
				return errors.New("email address quota exceeded (contact support to increase the quota)")
			}
		}
	}

	var code *string
	if conf.EmailVerificationRequired() {
		tmp, err := MakeEmailVerificationCode()
		if err != nil {
			return err
		}
		code = &tmp
	}

	// Another user may have already verified this email address. If so, do not send another
	// verification email (it would be pointless and also be an abuse vector). Do not tell the
	// user that another user has already verified it, to avoid needlessly leaking the existence
	// of emails.
	var emailAlreadyExistsAndIsVerified bool
	if _, err := db.Users.GetByVerifiedEmail(ctx, email); err != nil && !errcode.IsNotFound(err) {
		return err
	} else if err == nil {
		emailAlreadyExistsAndIsVerified = true
	}

	if err := db.UserEmails.Add(ctx, userID, email, code); err != nil {
		return err
	}

	if conf.EmailVerificationRequired() && !emailAlreadyExistsAndIsVerified {
		// Send email verification email.
		if err := SendUserEmailVerificationEmail(ctx, email, *code); err != nil {
			return errors.Wrap(err, "SendUserEmailVerificationEmail")
		}
	}

	return nil
}

// MakeEmailVerificationCode returns a random string that can be used as an email verification
// code. If there is not enough entropy to create a random string, it returns a non-nil error.
func MakeEmailVerificationCode() (string, error) {
	emailCodeBytes := make([]byte, 20)
	if _, err := rand.Read(emailCodeBytes); err != nil {
		return "", err
	}
	return base64.StdEncoding.EncodeToString(emailCodeBytes), nil
}

// SendUserEmailVerificationEmail sends an email to the user to verify the email address. The code
// is the verification code that the user must provide to verify their access to the email address.
func SendUserEmailVerificationEmail(ctx context.Context, email, code string) error {
	q := make(url.Values)
	q.Set("code", code)
	q.Set("email", email)
	verifyEmailPath, _ := router.Router().Get(router.VerifyEmail).URLPath()
	return txemail.Send(ctx, txemail.Message{
		To:       []string{email},
		Template: verifyEmailTemplates,
		Data: struct {
			Email string
			URL   string
		}{
			Email: email,
			URL: globals.AppURL.ResolveReference(&url.URL{
				Path:     verifyEmailPath.Path,
				RawQuery: q.Encode(),
			}).String(),
		},
	})
}

var (
	verifyEmailTemplates = txemail.MustValidate(txemail.Templates{
		Subject: `Verify your email on Sourcegraph`,
		Text: `
Verify your email address {{printf "%q" .Email}} on Sourcegraph by following this link:

  {{.URL}}
`,
		HTML: `
<p>Verify your email address {{printf "%q" .Email}} on Sourcegraph by following this link:</p>

<p><strong><a href="{{.URL}}">Verify email address</a></p>
`,
	})
)
