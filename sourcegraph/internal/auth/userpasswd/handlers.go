package userpasswd

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/mail"
	"strings"
	"time"

	"github.com/gorilla/mux"
	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/conf/deploy"
	"github.com/sourcegraph/sourcegraph/internal/lazyregexp"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/hubspot"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/hubspot/hubspotutil"
	sgactor "github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/auth"
	"github.com/sourcegraph/sourcegraph/internal/authz"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/cookie"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/deviceid"
	"github.com/sourcegraph/sourcegraph/internal/errcode"
	"github.com/sourcegraph/sourcegraph/internal/featureflag"
	"github.com/sourcegraph/sourcegraph/internal/session"
	"github.com/sourcegraph/sourcegraph/internal/suspiciousnames"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/internal/usagestats"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type credentials struct {
	Email           string `json:"email"`
	Username        string `json:"username"`
	Password        string `json:"password"`
	AnonymousUserID string `json:"anonymousUserId"`
	FirstSourceURL  string `json:"firstSourceUrl"`
	LastSourceURL   string `json:"lastSourceUrl"`
}

type unlockAccountInfo struct {
	Token string `json:"token"`
}

type unlockUserAccountInfo struct {
	Username string `json:"username"`
}

// HandleSignUp handles submission of the user signup form.
func HandleSignUp(logger log.Logger, db database.DB) http.HandlerFunc {
	logger = logger.Scoped("HandleSignUp", "sign up request handler")
	return func(w http.ResponseWriter, r *http.Request) {
		if handleEnabledCheck(logger, w) {
			return
		}
		if pc, _ := GetProviderConfig(); !pc.AllowSignup {
			http.Error(w, "Signup is not enabled (builtin auth provider allowSignup site configuration option)", http.StatusNotFound)
			return
		}
		handleSignUp(logger, db, w, r, false)
	}
}

// HandleSiteInit handles submission of the site initialization form, where the initial site admin user is created.
func HandleSiteInit(logger log.Logger, db database.DB) http.HandlerFunc {
	logger = logger.Scoped("HandleSiteInit", "initial size initialization request handler")
	return func(w http.ResponseWriter, r *http.Request) {
		// This only succeeds if the site is not yet initialized and there are no users yet. It doesn't
		// allow signups after those conditions become true, so we don't need to check the builtin auth
		// provider's allowSignup in site config.
		handleSignUp(logger, db, w, r, true)
	}
}

// checkEmailAbuse performs abuse prevention checks to prevent email abuse, i.e. users using emails
// of other people whom they want to annoy.
func checkEmailAbuse(ctx context.Context, db database.DB, addr string) (abused bool, reason string, err error) {
	email, err := db.UserEmails().GetLatestVerificationSentEmail(ctx, addr)
	if err != nil {
		if errcode.IsNotFound(err) {
			return false, "", nil
		}
		return false, "", err
	}

	// NOTE: We could check if email is already used here but that complicates the logic
	// and the reused problem should be better handled in the user creation.

	if email.NeedsVerificationCoolDown() {
		return true, "too frequent attempt since last verification email sent", nil
	}

	return false, "", nil
}

// handleSignUp is called to create a new user account. It is called for the normal user signup process (where a
// non-admin user is created) and for the site initialization process (where the initial site admin user account is
// created).
//
// 🚨 SECURITY: Any change to this function could introduce security exploits
// and/or break sign up / initial admin account creation. Be careful.
func handleSignUp(logger log.Logger, db database.DB, w http.ResponseWriter, r *http.Request, failIfNewUserIsNotInitialSiteAdmin bool) {
	if r.Method != "POST" {
		http.Error(w, fmt.Sprintf("unsupported method %s", r.Method), http.StatusBadRequest)
		return
	}
	var creds credentials
	if err := json.NewDecoder(r.Body).Decode(&creds); err != nil {
		http.Error(w, "could not decode request body", http.StatusBadRequest)
		return
	}
	err, statusCode, usr := unsafeSignUp(r.Context(), logger, db, creds, failIfNewUserIsNotInitialSiteAdmin)
	if err != nil {
		http.Error(w, err.Error(), statusCode)
		return
	}

	// Write the session cookie
	a := &sgactor.Actor{UID: usr.ID}
	if err := session.SetActor(w, r, a, 0, usr.CreatedAt); err != nil {
		httpLogError(logger.Error, w, "Could not create new user session", http.StatusInternalServerError, log.Error(err))
	}

	// Track user data
	if r.UserAgent() != "Sourcegraph e2etest-bot" || r.UserAgent() != "test" {
		go hubspotutil.SyncUser(creds.Email, hubspotutil.SignupEventID, &hubspot.ContactProperties{AnonymousUserID: creds.AnonymousUserID, FirstSourceURL: creds.FirstSourceURL, LastSourceURL: creds.LastSourceURL, DatabaseID: usr.ID})
	}

	if err = usagestats.LogBackendEvent(db, usr.ID, deviceid.FromContext(r.Context()), "SignUpSucceeded", nil, nil, featureflag.GetEvaluatedFlagSet(r.Context()), nil); err != nil {
		logger.Warn("Failed to log event SignUpSucceeded", log.Error(err))
	}
}

// unsafeSignUp is called to create a new user account. It is called for the normal user signup process (where a
// non-admin user is created) and for the site initialization process (where the initial site admin user account is
// created).
//
// 🚨 SECURITY: Any change to this function could introduce security exploits
// and/or break sign up / initial admin account creation. Be careful.
func unsafeSignUp(
	ctx context.Context,
	logger log.Logger,
	db database.DB,
	creds credentials,
	failIfNewUserIsNotInitialSiteAdmin bool,
) (error, int, *types.User) {
	const defaultErrorMessage = "Signup failed unexpectedly."

	if err := suspiciousnames.CheckNameAllowedForUserOrOrganization(creds.Username); err != nil {
		return err, http.StatusBadRequest, nil
	}
	if err := CheckEmailFormat(creds.Email); err != nil {
		return err, http.StatusBadRequest, nil
	}

	// Create the user.
	//
	// We don't need to check the builtin auth provider's allowSignup because we assume the caller
	// of handleSignUp checks it, or else that failIfNewUserIsNotInitialSiteAdmin == true (in which
	// case the only signup allowed is that of the initial site admin).
	newUserData := database.NewUser{
		Email:                 creds.Email,
		Username:              creds.Username,
		Password:              creds.Password,
		FailIfNotInitialUser:  failIfNewUserIsNotInitialSiteAdmin,
		EnforcePasswordLength: true,
		TosAccepted:           true, // Users created via the signup form are considered to have accepted the Terms of Service.
	}
	if failIfNewUserIsNotInitialSiteAdmin {
		// The email of the initial site admin is considered to be verified.
		newUserData.EmailIsVerified = true
	} else {
		code, err := backend.MakeEmailVerificationCode()
		if err != nil {
			logger.Error("Error generating email verification code for new user.", log.String("email", creds.Email), log.String("username", creds.Username), log.Error(err))
			return errors.New(defaultErrorMessage), http.StatusInternalServerError, nil
		}
		newUserData.EmailVerificationCode = code
	}

	// Prevent abuse (users adding emails of other people whom they want to annoy) with the
	// following abuse prevention checks.
	if conf.EmailVerificationRequired() && !newUserData.EmailIsVerified {
		abused, reason, err := checkEmailAbuse(ctx, db, creds.Email)
		if err != nil {
			logger.Error("Error checking email abuse", log.String("email", creds.Email), log.Error(err))
			return errors.New(defaultErrorMessage), http.StatusInternalServerError, nil
		} else if abused {
			logger.Error("Possible email address abuse prevented", log.String("email", creds.Email), log.String("reason", reason))
			msg := "Email address is possibly being abused, please try again later or use a different email address."
			return errors.New(msg), http.StatusTooManyRequests, nil
		}
	}

	usr, err := db.Users().Create(ctx, newUserData)
	if err != nil {
		var (
			message    string
			statusCode int
		)
		switch {
		case database.IsUsernameExists(err):
			message = "Username is already in use. Try a different username."
			statusCode = http.StatusConflict
		case database.IsEmailExists(err):
			message = "Email address is already in use. Try signing into that account instead, or use a different email address."
			statusCode = http.StatusConflict
		case errcode.PresentationMessage(err) != "":
			message = errcode.PresentationMessage(err)
			statusCode = http.StatusConflict
		default:
			// Do not show non-allowed error messages to user, in case they contain sensitive or confusing
			// information.
			message = defaultErrorMessage
			statusCode = http.StatusInternalServerError
		}
		if deploy.IsApp() && strings.Contains(err.Error(), "site_already_initialized") {
			return nil, http.StatusOK, nil
		}
		logger.Error("Error in user signup.", log.String("email", creds.Email), log.String("username", creds.Username), log.Error(err))
		if err = usagestats.LogBackendEvent(db, sgactor.FromContext(ctx).UID, deviceid.FromContext(ctx), "SignUpFailed", nil, nil, featureflag.GetEvaluatedFlagSet(ctx), nil); err != nil {
			logger.Warn("Failed to log event SignUpFailed", log.Error(err))
		}
		return errors.New(message), statusCode, nil
	}

	if err = db.Authz().GrantPendingPermissions(ctx, &database.GrantPendingPermissionsArgs{
		UserID: usr.ID,
		Perm:   authz.Read,
		Type:   authz.PermRepos,
	}); err != nil {
		logger.Error("Failed to grant user pending permissions", log.Int32("userID", usr.ID), log.Error(err))
	}

	if conf.EmailVerificationRequired() && !newUserData.EmailIsVerified {
		if err := backend.SendUserEmailVerificationEmail(ctx, usr.Username, creds.Email, newUserData.EmailVerificationCode); err != nil {
			logger.Error("failed to send email verification (continuing, user's email will be unverified)", log.String("email", creds.Email), log.Error(err))
		} else if err = db.UserEmails().SetLastVerification(ctx, usr.ID, creds.Email, newUserData.EmailVerificationCode, time.Now()); err != nil {
			logger.Error("failed to set email last verification sent at (user's email is verified)", log.String("email", creds.Email), log.Error(err))
		}
	}
	return nil, http.StatusOK, usr
}

func CheckEmailFormat(email string) error {
	// Max email length is 320 chars https://datatracker.ietf.org/doc/html/rfc3696#section-3
	if len(email) > 320 {
		return errors.Newf("maximum email length is 320, got %d", len(email))
	}
	if _, err := mail.ParseAddress(email); err != nil {
		return err
	}
	return nil
}

func getByEmailOrUsername(ctx context.Context, db database.DB, emailOrUsername string) (*types.User, error) {
	if strings.Contains(emailOrUsername, "@") {
		return db.Users().GetByVerifiedEmail(ctx, emailOrUsername)
	}
	return db.Users().GetByUsername(ctx, emailOrUsername)
}

// HandleSignIn accepts a POST containing username-password credentials and
// authenticates the current session if the credentials are valid.
//
// The account will be locked out after consecutive failed attempts in a certain
// period of time.
func HandleSignIn(logger log.Logger, db database.DB, store LockoutStore) http.HandlerFunc {
	logger = logger.Scoped("HandleSignin", "sign in request handler")
	return func(w http.ResponseWriter, r *http.Request) {
		if handleEnabledCheck(logger, w) {
			return
		}

		var user types.User

		signInResult := database.SecurityEventNameSignInAttempted
		logSignInEvent(r, db, &user, &signInResult)

		// We have more failure scenarios and ONLY one successful scenario. By default,
		// assume a SignInFailed state so that the deferred logSignInEvent function call
		// will log the correct security event in case of a failure.
		signInResult = database.SecurityEventNameSignInFailed
		defer func() {
			logSignInEvent(r, db, &user, &signInResult)
			checkAccountLockout(store, &user, &signInResult)
		}()

		if r.Method != http.MethodPost {
			http.Error(w, fmt.Sprintf("Unsupported method %s", r.Method), http.StatusBadRequest)
			return
		}
		var creds credentials
		if err := json.NewDecoder(r.Body).Decode(&creds); err != nil {
			http.Error(w, "Could not decode request body", http.StatusBadRequest)
			return
		}

		ctx := r.Context()

		// Validate user. Allow login by both email and username (for convenience).
		u, err := getByEmailOrUsername(ctx, db, creds.Email)
		if err != nil {
			httpLogError(logger.Warn, w, "Authentication failed", http.StatusUnauthorized, log.Error(err))
			return
		}
		user = *u

		if reason, locked := store.IsLockedOut(user.ID); locked {
			func() {
				if !conf.CanSendEmail() || store.UnlockEmailSent(user.ID) {
					return
				}

				recipient, _, err := db.UserEmails().GetPrimaryEmail(ctx, user.ID)
				if err != nil {
					logger.Error("Error getting primary email address", log.Int32("userID", user.ID), log.Error(err))
					return
				}

				err = store.SendUnlockAccountEmail(ctx, user.ID, recipient)
				if err != nil {
					logger.Error("Error sending unlock account email", log.Int32("userID", user.ID), log.Error(err))
					return
				}
			}()

			httpLogError(logger.Error, w, fmt.Sprintf("Account has been locked out due to %q", reason), http.StatusUnprocessableEntity)
			return
		}

		// 🚨 SECURITY: check password
		correct, err := db.Users().IsPassword(ctx, user.ID, creds.Password)
		if err != nil {
			httpLogError(logger.Error, w, "Error checking password", http.StatusInternalServerError, log.Error(err))
			return
		}
		if !correct {
			httpLogError(logger.Warn, w, "Authentication failed", http.StatusUnauthorized)
			return
		}

		// Write the session cookie
		actor := sgactor.Actor{
			UID: user.ID,
		}
		if err := session.SetActor(w, r, &actor, 0, user.CreatedAt); err != nil {
			httpLogError(logger.Error, w, "Could not create new user session", http.StatusInternalServerError, log.Error(err))
			return
		}

		signInResult = database.SecurityEventNameSignInSucceeded
	}
}

func HandleUnlockAccount(logger log.Logger, _ database.DB, store LockoutStore) http.HandlerFunc {
	logger = logger.Scoped("HandleUnlockAccount", "unlock account request handler")
	return func(w http.ResponseWriter, r *http.Request) {
		if handleEnabledCheck(logger, w) {
			return
		}

		if r.Method != http.MethodPost {
			http.Error(w, fmt.Sprintf("Unsupported method %s", r.Method), http.StatusBadRequest)
			return
		}

		var unlockAccountInfo unlockAccountInfo
		if err := json.NewDecoder(r.Body).Decode(&unlockAccountInfo); err != nil {
			http.Error(w, "Could not decode request body", http.StatusBadRequest)
			return
		}

		if unlockAccountInfo.Token == "" {
			http.Error(w, "Bad request: missing token", http.StatusBadRequest)
			return
		}

		valid, err := store.VerifyUnlockAccountTokenAndReset(unlockAccountInfo.Token)

		if !valid || err != nil {
			errStr := "invalid token provided"
			if err != nil {
				errStr = err.Error()
			}
			httpLogError(logger.Warn, w, errStr, http.StatusUnauthorized)
			return
		}
	}
}

func HandleUnlockUserAccount(_ log.Logger, db database.DB, store LockoutStore) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if err := auth.CheckCurrentUserIsSiteAdmin(r.Context(), db); err != nil {
			http.Error(w, "Only site admins can unlock user accounts", http.StatusUnauthorized)
			return
		}

		if r.Method != http.MethodPost {
			http.Error(w, fmt.Sprintf("Unsupported method %s", r.Method), http.StatusBadRequest)
			return
		}

		var unlockUserAccountInfo unlockUserAccountInfo
		if err := json.NewDecoder(r.Body).Decode(&unlockUserAccountInfo); err != nil {
			http.Error(w, "Could not decode request body", http.StatusBadRequest)
			return
		}

		if unlockUserAccountInfo.Username == "" {
			http.Error(w, "Bad request: missing username", http.StatusBadRequest)
			return
		}

		user, err := db.Users().GetByUsername(r.Context(), unlockUserAccountInfo.Username)
		if err != nil {
			http.Error(w,
				fmt.Sprintf("Not found: could not find user with username %q", unlockUserAccountInfo.Username),
				http.StatusNotFound)
			return
		}

		_, isLocked := store.IsLockedOut(user.ID)
		if !isLocked {
			http.Error(w,
				fmt.Sprintf("User with username %q is not locked", unlockUserAccountInfo.Username),
				http.StatusBadRequest)
			return
		}

		store.Reset(user.ID)
	}
}

func logSignInEvent(r *http.Request, db database.DB, user *types.User, name *database.SecurityEventName) {
	var anonymousID string
	event := &database.SecurityEvent{
		Name:            *name,
		URL:             r.URL.Path,
		UserID:          uint32(user.ID),
		AnonymousUserID: anonymousID,
		Source:          "BACKEND",
		Timestamp:       time.Now(),
	}

	// Safe to ignore this error
	event.AnonymousUserID, _ = cookie.AnonymousUID(r)
	_ = usagestats.LogBackendEvent(db, user.ID, deviceid.FromContext(r.Context()), string(*name), nil, nil, featureflag.GetEvaluatedFlagSet(r.Context()), nil)
	db.SecurityEventLogs().LogEvent(r.Context(), event)
}

func checkAccountLockout(store LockoutStore, user *types.User, event *database.SecurityEventName) {
	if user.ID <= 0 {
		return
	}

	if *event == database.SecurityEventNameSignInSucceeded {
		store.Reset(user.ID)
	} else if *event == database.SecurityEventNameSignInFailed {
		store.IncreaseFailedAttempt(user.ID)
	}
}

// HandleCheckUsernameTaken checks availability of username for signup form
func HandleCheckUsernameTaken(logger log.Logger, db database.DB) http.HandlerFunc {
	logger = logger.Scoped("HandleCheckUsernameTaken", "checks for username uniqueness")
	return func(w http.ResponseWriter, r *http.Request) {
		vars := mux.Vars(r)
		username, err := NormalizeUsername(vars["username"])
		if err != nil {
			w.WriteHeader(http.StatusBadRequest)
			return
		}

		_, err = db.Namespaces().GetByName(r.Context(), username)
		if err == database.ErrNamespaceNotFound {
			w.WriteHeader(http.StatusNotFound)
			return
		}
		if err != nil {
			httpLogError(logger.Error, w, "Error checking username uniqueness", http.StatusInternalServerError, log.Error(err))
			return
		}

		w.WriteHeader(http.StatusOK)
	}
}

func httpLogError(logFunc func(string, ...log.Field), w http.ResponseWriter, msg string, code int, errArgs ...log.Field) {
	logFunc(msg, errArgs...)
	http.Error(w, msg, code)
}

// NormalizeUsername normalizes a proposed username into a format that meets Sourcegraph's
// username formatting rules (based on, but not identical to
// https://web.archive.org/web/20180215000330/https://help.github.com/enterprise/2.11/admin/guides/user-management/using-ldap):
//
// - Any characters not in `[a-zA-Z0-9-._]` are replaced with `-`
// - Usernames with exactly one `@` character are interpreted as an email address, so the username will be extracted by truncating at the `@` character.
// - Usernames with two or more `@` characters are not considered an email address, so the `@` will be treated as a non-standard character and be replaced with `-`
// - Usernames with consecutive `-` or `.` characters are not allowed, so they are replaced with a single `-` or `.`
// - Usernames that start with `.` or `-` are not allowed, starting periods and dashes are removed
// - Usernames that end with `.` are not allowed, ending periods are removed
//
// Usernames that could not be converted return an error.
//
// Note: Do not forget to change database constraints on "users" and "orgs" tables.
func NormalizeUsername(name string) (string, error) {
	origName := name

	// If the username is an email address, extract the username part.
	if i := strings.Index(name, "@"); i != -1 && i == strings.LastIndex(name, "@") {
		name = name[:i]
	}

	// Replace all non-alphanumeric characters with a dash.
	name = disallowedCharacter.ReplaceAllString(name, "-")

	// Replace all consecutive dashes and periods with a single dash.
	name = consecutivePeriodsDashes.ReplaceAllString(name, "-")

	// Trim leading and trailing dashes and periods.
	name = sequencesToTrim.ReplaceAllString(name, "")

	if name == "" {
		return "", errors.Errorf("username %q could not be normalized to acceptable format", origName)
	}

	if err := suspiciousnames.CheckNameAllowedForUserOrOrganization(name); err != nil {
		return "", err
	}

	return name, nil
}

var (
	disallowedCharacter      = lazyregexp.New(`[^\w\-\.]`)
	consecutivePeriodsDashes = lazyregexp.New(`[\-\.]{2,}`)
	sequencesToTrim          = lazyregexp.New(`(^[\-\.])|(\.$)|`)
)
