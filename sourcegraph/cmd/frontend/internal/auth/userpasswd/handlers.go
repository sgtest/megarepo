package userpasswd

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"

	"github.com/sourcegraph/sourcegraph/pkg/errcode"
	"github.com/sourcegraph/sourcegraph/pkg/hubspot/hubspotutil"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/tracking"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/pkg/suspiciousnames"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/session"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/types"
	"github.com/sourcegraph/sourcegraph/pkg/actor"
	"github.com/sourcegraph/sourcegraph/pkg/conf"
	log15 "gopkg.in/inconshreveable/log15.v2"
)

type credentials struct {
	Email    string `json:"email"`
	Username string `json:"username"`
	Password string `json:"password"`
}

// HandleSignUp handles submission of the user signup form.
func HandleSignUp(w http.ResponseWriter, r *http.Request) {
	if handleEnabledCheck(w) {
		return
	}
	if pc, _ := getProviderConfig(); !pc.AllowSignup {
		http.Error(w, "Signup is not enabled (builtin auth provider allowSignup site configuration option)", http.StatusNotFound)
		return
	}
	handleSignUp(w, r, false)
}

// HandleSiteInit handles submission of the site initialization form, where the initial site admin user is created.
func HandleSiteInit(w http.ResponseWriter, r *http.Request) {
	// This only succeeds if the site is not yet initialized and there are no users yet. It doesn't
	// allow signups after those conditions become true, so we don't need to check auth.allowSignup
	// in site config.
	handleSignUp(w, r, true)
}

// doServeSignUp is called to create a new user account. It is called for the normal user signup process (where a
// non-admin user is created) and for the site initialization process (where the initial site admin user account is
// created).
//
// 🚨 SECURITY: Any change to this function could introduce security exploits
// and/or break sign up / initial admin account creation. Be careful.
func handleSignUp(w http.ResponseWriter, r *http.Request, failIfNewUserIsNotInitialSiteAdmin bool) {
	if r.Method != "POST" {
		http.Error(w, fmt.Sprintf("unsupported method %s", r.Method), http.StatusBadRequest)
		return
	}
	var creds credentials
	if err := json.NewDecoder(r.Body).Decode(&creds); err != nil {
		http.Error(w, "could not decode request body", http.StatusBadRequest)
		return
	}

	const defaultErrorMessage = "Signup failed unexpectedly."

	if err := suspiciousnames.CheckNameAllowedForUserOrOrganization(creds.Username); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	// Create the user.
	//
	// We don't need to check auth.allowSignup because we assume the caller of doServeSignUp checks
	// it, or else that failIfNewUserIsNotInitialSiteAdmin == true (in which case the only signup
	// allowed is that of the initial site admin).
	newUserData := db.NewUser{
		Email:                creds.Email,
		Username:             creds.Username,
		Password:             creds.Password,
		FailIfNotInitialUser: failIfNewUserIsNotInitialSiteAdmin,
	}
	if failIfNewUserIsNotInitialSiteAdmin {
		// The email of the initial site admin is considered to be verified.
		newUserData.EmailIsVerified = true
	} else {
		code, err := backend.MakeEmailVerificationCode()
		if err != nil {
			log15.Error("Error generating email verification code for new user.", "email", creds.Email, "username", creds.Username, "error", err)
			http.Error(w, defaultErrorMessage, http.StatusInternalServerError)
			return
		}
		newUserData.EmailVerificationCode = code
	}
	usr, err := db.Users.Create(r.Context(), newUserData)
	if err != nil {
		var (
			message    string
			statusCode int
		)
		switch {
		case db.IsUsernameExists(err):
			message = "Username is already in use. Try a different username."
			statusCode = http.StatusConflict
		case db.IsEmailExists(err):
			message = "Email address is already in use. Try signing into that account instead, or use a different email address."
			statusCode = http.StatusConflict
		case errcode.PresentationMessage(err) != "":
			message = errcode.PresentationMessage(err)
			statusCode = http.StatusConflict
		default:
			// Do not show non-whitelisted error messages to user, in case they contain sensitive or confusing
			// information.
			message = defaultErrorMessage
			statusCode = http.StatusInternalServerError
		}
		log15.Error("Error in user signup.", "email", creds.Email, "username", creds.Username, "error", err)
		http.Error(w, message, statusCode)
		return
	}
	actor := &actor.Actor{UID: usr.ID}

	if conf.EmailVerificationRequired() && !newUserData.EmailIsVerified {
		if err := backend.SendUserEmailVerificationEmail(r.Context(), creds.Email, newUserData.EmailVerificationCode); err != nil {
			log15.Error("failed to send email verification (continuing, user's email will be unverified)", "email", creds.Email, "err", err)
		}
	}

	// Write the session cookie
	if session.SetActor(w, r, actor, 0); err != nil {
		httpLogAndError(w, "Could not create new user session", http.StatusInternalServerError)
	}

	// Track user data
	if r.UserAgent() != "Sourcegraph e2etest-bot" {
		go tracking.SyncUser(creds.Email, hubspotutil.SignupEventID, nil)
	}
}

func getByEmailOrUsername(ctx context.Context, emailOrUsername string) (*types.User, error) {
	if strings.Contains(emailOrUsername, "@") {
		return db.Users.GetByVerifiedEmail(ctx, emailOrUsername)
	}
	return db.Users.GetByUsername(ctx, emailOrUsername)
}

// HandleSignIn accepts a POST containing username-password credentials and authenticates the
// current session if the credentials are valid.
func HandleSignIn(w http.ResponseWriter, r *http.Request) {
	if handleEnabledCheck(w) {
		return
	}

	ctx := r.Context()

	if r.Method != "POST" {
		http.Error(w, fmt.Sprintf("Unsupported method %s", r.Method), http.StatusBadRequest)
		return
	}
	var creds credentials
	if err := json.NewDecoder(r.Body).Decode(&creds); err != nil {
		http.Error(w, "Could not decode request body", http.StatusBadRequest)
		return
	}

	// Validate user. Allow login by both email and username (for convenience).
	usr, err := getByEmailOrUsername(ctx, creds.Email)
	if err != nil {
		httpLogAndError(w, "Authentication failed", http.StatusUnauthorized, "err", err)
		return
	}
	// 🚨 SECURITY: check password
	correct, err := db.Users.IsPassword(ctx, usr.ID, creds.Password)
	if err != nil {
		httpLogAndError(w, "Error checking password", http.StatusInternalServerError, "err", err)
		return
	}
	if !correct {
		httpLogAndError(w, "Authentication failed", http.StatusUnauthorized)
		return
	}
	actor := &actor.Actor{UID: usr.ID}

	// Write the session cookie
	if session.SetActor(w, r, actor, 0); err != nil {
		httpLogAndError(w, "Could not create new user session", http.StatusInternalServerError)
		return
	}
}

func httpLogAndError(w http.ResponseWriter, msg string, code int, errArgs ...interface{}) {
	log15.Error(msg, errArgs...)
	http.Error(w, msg, code)
}
