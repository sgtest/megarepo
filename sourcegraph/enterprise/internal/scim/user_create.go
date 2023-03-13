package scim

import (
	"context"
	"net/http"
	"strconv"
	"time"

	"github.com/elimity-com/scim"
	scimerrors "github.com/elimity-com/scim/errors"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/auth"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/globals"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/env"
	"github.com/sourcegraph/sourcegraph/internal/extsvc"
	"github.com/sourcegraph/sourcegraph/internal/goroutine"
	"github.com/sourcegraph/sourcegraph/internal/txemail"
	"github.com/sourcegraph/sourcegraph/internal/txemail/txtypes"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// Reusing the env variables from email invites because the intent is the same as the welcome email
var (
	disableEmailInvites, _   = strconv.ParseBool(env.Get("DISABLE_EMAIL_INVITES", "false", "Disable email invitations entirely."))
	debugEmailInvitesMock, _ = strconv.ParseBool(env.Get("DEBUG_EMAIL_INVITES_MOCK", "false", "Do not actually send email invitations, instead just print that we did."))
)

// Create stores given attributes. Returns a resource with the attributes that are stored and a (new) unique identifier.
func (h *UserResourceHandler) Create(r *http.Request, attributes scim.ResourceAttributes) (scim.Resource, error) {
	if err := checkBodyNotEmpty(r); err != nil {
		return scim.Resource{}, err
	}

	// Extract external ID, primary email, username, and display name from attributes to variables
	primaryEmail, otherEmails := extractPrimaryEmail(attributes)
	if primaryEmail == "" {
		return scim.Resource{}, scimerrors.ScimErrorBadParams([]string{"emails missing"})
	}
	displayName := extractDisplayName(attributes)

	// Try to match emails to existing users
	allEmails := append([]string{primaryEmail}, otherEmails...)
	existingEmails, err := h.db.UserEmails().GetVerifiedEmails(r.Context(), allEmails...)
	if err != nil {
		return scim.Resource{}, scimerrors.ScimError{Status: http.StatusInternalServerError, Detail: err.Error()}
	}
	existingUserIDs := make(map[int32]struct{})
	for _, email := range existingEmails {
		existingUserIDs[email.UserID] = struct{}{}
	}
	if len(existingUserIDs) > 1 {
		return scim.Resource{}, scimerrors.ScimError{Status: http.StatusConflict, Detail: "Emails match to multiple users"}
	}
	if len(existingUserIDs) == 1 {
		userID := int32(0)
		for id := range existingUserIDs {
			userID = id
		}
		// A user with the email(s) already exists → check if the user is not SCIM-controlled
		user, err := h.db.Users().GetByID(r.Context(), userID)
		if err != nil {
			return scim.Resource{}, scimerrors.ScimError{Status: http.StatusInternalServerError, Detail: err.Error()}
		}
		if user == nil {
			return scim.Resource{}, scimerrors.ScimError{Status: http.StatusInternalServerError, Detail: "User not found"}
		}
		if user.SCIMControlled {
			// This user creation would fail based on the email address, so we'll return a conflict error
			return scim.Resource{}, scimerrors.ScimError{Status: http.StatusConflict, Detail: "User already exists based on email address"}
		}

		// The user exists, but is not SCIM-controlled, so we'll update the user with the new attributes,
		// and make the user SCIM-controlled (which is the same as a replace)
		return h.Replace(r, strconv.FormatInt(int64(userID), 10), attributes)
	}

	// At this point we know that the user does not exist yet, so we'll create a new user

	// Make sure the username is unique, then create user with/without an external account ID
	var user *types.User
	err = h.db.WithTransact(r.Context(), func(tx database.DB) error {
		uniqueUsername, err := getUniqueUsername(r.Context(), tx.Users(), extractStringAttribute(attributes, AttrUserName))
		if err != nil {
			return err
		}

		// Create user
		newUser := database.NewUser{
			Email:           primaryEmail,
			Username:        uniqueUsername,
			DisplayName:     displayName,
			EmailIsVerified: true,
		}
		accountSpec := extsvc.AccountSpec{
			ServiceType: "scim",
			ServiceID:   "scim",
			AccountID:   getUniqueExternalID(attributes),
		}
		accountData, err := toAccountData(attributes)
		if err != nil {
			return scimerrors.ScimError{Status: http.StatusInternalServerError, Detail: err.Error()}
		}
		user, err = tx.UserExternalAccounts().CreateUserAndSave(r.Context(), newUser, accountSpec, accountData)

		if err != nil {
			if dbErr, ok := containsErrCannotCreateUserError(err); ok {
				code := dbErr.Code()
				if code == database.ErrorCodeUsernameExists || code == database.ErrorCodeEmailExists {
					return scimerrors.ScimError{Status: http.StatusConflict, Detail: err.Error()}
				}
			}
			return scimerrors.ScimError{Status: http.StatusInternalServerError, Detail: err.Error()}
		}
		return nil
	})
	if err != nil {
		multiErr, ok := err.(errors.MultiError)
		if !ok || len(multiErr.Errors()) == 0 {
			return scim.Resource{}, err
		}
		return scim.Resource{}, multiErr.Errors()[len(multiErr.Errors())-1]
	}

	// If there were additional emails provided, now that the user has been created
	// we can try to add and verify them each in a separate trx so that if it fails we can ignore
	// the error because they are not required.
	if len(otherEmails) > 0 {
		for _, email := range otherEmails {
			_ = h.db.WithTransact(r.Context(), func(tx database.DB) error {
				err := tx.UserEmails().Add(r.Context(), user.ID, email, nil)
				if err != nil {
					return err
				}
				return tx.UserEmails().SetVerified(r.Context(), user.ID, email, true)
			})
		}
	}

	// Attempt to send emails in the background.
	goroutine.Go(func() {
		_ = sendPasswordResetEmail(r.Context(), h.getLogger(), h.db, user, primaryEmail)
		_ = sendWelcomeEmail(primaryEmail, globals.ExternalURL().String(), h.getLogger())
	})

	var now = time.Now()
	return scim.Resource{
		ID:         strconv.Itoa(int(user.ID)),
		ExternalID: getOptionalExternalID(attributes),
		Attributes: attributes,
		Meta: scim.Meta{
			Created:      &now,
			LastModified: &now,
		},
	}, nil
}

// extractPrimaryEmail extracts the primary email address from the given attributes.
// Tries to get the (first) email address marked as primary, otherwise uses the first email address it finds.
func extractPrimaryEmail(attributes scim.ResourceAttributes) (primaryEmail string, otherEmails []string) {
	if attributes[AttrEmails] == nil {
		return
	}
	emails := attributes[AttrEmails].([]interface{})
	otherEmails = make([]string, 0, len(emails))
	for _, emailRaw := range emails {
		email := emailRaw.(map[string]interface{})
		if email["primary"] == true && primaryEmail == "" {
			primaryEmail = email["value"].(string)
			continue
		}
		otherEmails = append(otherEmails, email["value"].(string))
	}
	if primaryEmail == "" && len(otherEmails) > 0 {
		primaryEmail, otherEmails = otherEmails[0], otherEmails[1:]
	}
	return
}

// extractDisplayName extracts the user's display name from the given attributes.
// Ii defaults to the username if no display name is available.
func extractDisplayName(attributes scim.ResourceAttributes) (displayName string) {
	if attributes[AttrDisplayName] != nil {
		displayName = attributes[AttrDisplayName].(string)
	} else if attributes[AttrName] != nil {
		name := attributes[AttrName].(map[string]interface{})
		if name[AttrNameFormatted] != nil {
			displayName = name[AttrNameFormatted].(string)
		} else if name[AttrNameGiven] != nil && name[AttrNameFamily] != nil {
			if name[AttrNameMiddle] != nil {
				displayName = name[AttrNameGiven].(string) + " " + name[AttrNameMiddle].(string) + " " + name[AttrNameFamily].(string)
			} else {
				displayName = name[AttrNameGiven].(string) + " " + name[AttrNameFamily].(string)
			}
		}
	} else if attributes[AttrNickName] != nil {
		displayName = attributes[AttrNickName].(string)
	}
	// Fallback to username
	if displayName == "" {
		displayName = attributes[AttrUserName].(string)
	}
	return
}

// containsErrCannotCreateUserError returns true if the given error contains at least one database.ErrCannotCreateUser.
// It also returns the first such error.
func containsErrCannotCreateUserError(err error) (database.ErrCannotCreateUser, bool) {
	if err == nil {
		return database.ErrCannotCreateUser{}, false
	}
	if _, ok := err.(database.ErrCannotCreateUser); ok {
		return err.(database.ErrCannotCreateUser), true
	}

	// Handle multiError
	if multiErr, ok := err.(errors.MultiError); ok {
		for _, err := range multiErr.Errors() {
			if _, ok := err.(database.ErrCannotCreateUser); ok {
				return err.(database.ErrCannotCreateUser), true
			}
		}
	}

	return database.ErrCannotCreateUser{}, false
}

// sendPasswordResetEmail sends a password reset email to the given user.
func sendPasswordResetEmail(ctx context.Context, logger log.Logger, db database.DB, user *types.User, primaryEmail string) bool {
	// Email user to ask to set up a password
	// This internally checks whether username/password login is enabled, whether we have an SMTP in place, etc.
	if disableEmailInvites {
		return true
	}
	if debugEmailInvitesMock {
		if logger != nil {
			logger.Info("password reset: mock pw reset email to Sourcegraph", log.String("sent", primaryEmail))
		}
		return true
	}
	_, err := auth.ResetPasswordURL(ctx, db, logger, user, primaryEmail, true)
	if err != nil {
		logger.Error("error sending password reset email", log.Error(err))
	}
	return false
}

// sendWelcomeEmail sends a welcome email to the given user.
func sendWelcomeEmail(email, siteURL string, logger log.Logger) error {
	if email != "" && conf.CanSendEmail() {
		if disableEmailInvites {
			return nil
		}
		if debugEmailInvitesMock {
			if logger != nil {
				logger.Info("email welcome: mock welcome to Sourcegraph", log.String("welcomed", email))
			}
			return nil
		}
		return txemail.Send(context.Background(), "user_welcome", txemail.Message{
			To:       []string{email},
			Template: emailTemplateEmailWelcomeSCIM,
			Data: struct {
				URL string
			}{
				URL: siteURL,
			},
		})
	}
	return nil
}

var emailTemplateEmailWelcomeSCIM = txemail.MustValidate(txtypes.Templates{
	Subject: `Welcome to Sourcegraph`,
	Text: `
Sourcegraph enables you to quickly understand, fix, and automate changes to your code.

You can use Sourcegraph to:
  - Search and navigate multiple repositories with cross-repository dependency navigation
  - Share links directly to lines of code to work more collaboratively together
  - Automate large-scale code changes with Batch Changes
  - Create code monitors to alert you about changes in code

Come experience the power of great code search.


{{.URL}}

Learn more about Sourcegraph:

https://about.sourcegraph.com
`,
	HTML: `
<p>Sourcegraph enables you to quickly understand, fix, and automate changes to your code.</p>

<p>
	You can use Sourcegraph to:<br/>
	<ul>
		<li>Search and navigate multiple repositories with cross-repository dependency navigation</li>
		<li>Share links directly to lines of code to work more collaboratively together</li>
		<li>Automate large-scale code changes with Batch Changes</li>
		<li>Create code monitors to alert you about changes in code</li>
	</ul>
</p>

<p><strong><a href="{{.URL}}">Come experience the power of great code search</a></strong></p>

<p><a href="https://about.sourcegraph.com">Learn more about Sourcegraph</a></p>
`,
})
