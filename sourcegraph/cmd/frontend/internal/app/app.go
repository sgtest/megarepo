package app

import (
	"net/http"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/globals"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/errorutil"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/router"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/internal/app/ui"
	"github.com/sourcegraph/sourcegraph/internal/auth/accessrequest"
	"github.com/sourcegraph/sourcegraph/internal/auth/userpasswd"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/conf/deploy"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/session"
	"github.com/sourcegraph/sourcegraph/internal/trace"
)

// NewHandler returns a new app handler that uses the app router.
//
// 🚨 SECURITY: The caller MUST wrap the returned handler in middleware that checks authentication
// and sets the actor in the request context.
func NewHandler(db database.DB, logger log.Logger, githubAppSetupHandler http.Handler) http.Handler {
	session.SetSessionStore(session.NewRedisStore(func() bool {
		if deploy.IsApp() {
			// Safari / WebKit-based browsers refuse to set cookies on localhost as it is not treated
			// as a secure domain, in contrast to all other browsers.
			// https://bugs.webkit.org/show_bug.cgi?id=232088
			// As a result, if secure is set to true here then it becomes impossible to sign into
			// Sourcegraph using Safari/WebKit.
			return false
		}
		return globals.ExternalURL().Scheme == "https"
	}))

	logger = logger.Scoped("appHandler", "handles routes for all app related requests")

	r := router.Router()

	m := http.NewServeMux()

	m.Handle("/", r)

	r.Get(router.RobotsTxt).Handler(trace.Route(http.HandlerFunc(robotsTxt)))
	r.Get(router.SitemapXmlGz).Handler(trace.Route(http.HandlerFunc(sitemapXmlGz)))
	r.Get(router.Favicon).Handler(trace.Route(http.HandlerFunc(favicon)))
	r.Get(router.OpenSearch).Handler(trace.Route(http.HandlerFunc(openSearch)))

	r.Get(router.RepoBadge).Handler(trace.Route(errorutil.Handler(serveRepoBadge(db))))

	// Redirects
	r.Get(router.OldToolsRedirect).Handler(trace.Route(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, "/beta", http.StatusMovedPermanently)
	})))

	r.Get(router.GopherconLiveBlog).Handler(trace.Route(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, "https://about.sourcegraph.com/go", http.StatusFound)
	})))

	r.Get(router.UI).Handler(ui.Router())

	lockoutStore := userpasswd.NewLockoutStoreFromConf(conf.AuthLockout())

	r.Get(router.SignUp).Handler(trace.Route(userpasswd.HandleSignUp(logger, db)))
	r.Get(router.RequestAccess).Handler(trace.Route(accessrequest.HandleRequestAccess(logger, db)))
	r.Get(router.SiteInit).Handler(trace.Route(userpasswd.HandleSiteInit(logger, db)))
	r.Get(router.SignIn).Handler(trace.Route(userpasswd.HandleSignIn(logger, db, lockoutStore)))
	r.Get(router.SignOut).Handler(trace.Route(serveSignOutHandler(db)))
	r.Get(router.UnlockAccount).Handler(trace.Route(userpasswd.HandleUnlockAccount(logger, db, lockoutStore)))
	r.Get(router.UnlockUserAccount).Handler(trace.Route(userpasswd.HandleUnlockUserAccount(logger, db, lockoutStore)))
	r.Get(router.ResetPasswordInit).Handler(trace.Route(userpasswd.HandleResetPasswordInit(logger, db)))
	r.Get(router.ResetPasswordCode).Handler(trace.Route(userpasswd.HandleResetPasswordCode(logger, db)))
	r.Get(router.VerifyEmail).Handler(trace.Route(serveVerifyEmail(db)))

	r.Get(router.CheckUsernameTaken).Handler(trace.Route(userpasswd.HandleCheckUsernameTaken(logger, db)))

	// Usage statistics ZIP download
	r.Get(router.UsageStatsDownload).Handler(trace.Route(usageStatsArchiveHandler(db)))

	// One-click export ZIP download
	r.Get(router.OneClickExportArchive).Handler(trace.Route(oneClickExportHandler(db, logger)))

	// Ping retrieval
	r.Get(router.LatestPing).Handler(trace.Route(latestPingHandler(db)))

	// Sourcegraph GitHub App setup (Cloud and on-prem)
	r.Get(router.SetupGitHubAppCloud).Handler(trace.Route(githubAppSetupHandler))
	r.Get(router.SetupGitHubApp).Handler(trace.Route(githubAppSetupHandler))

	r.Get(router.Editor).Handler(trace.Route(errorutil.Handler(serveEditor(db))))

	r.Get(router.DebugHeaders).Handler(trace.Route(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		h := r.Header.Clone()
		// We redact Cookie to prevent XSS attacks from stealing sessions.
		if len(h.Values("Cookie")) > 0 {
			h.Set("Cookie", "REDACTED")
		}
		_ = h.Write(w)
	})))
	addDebugHandlers(r.Get(router.Debug).Subrouter(), db)

	rickRoll := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Redirect(w, r, "https://www.youtube.com/watch?v=dQw4w9WgXcQ", http.StatusFound)
	})
	for _, p := range []string{"/.env", "/admin.php", "/wp-login.php", "/wp-admin"} {
		m.Handle(p, rickRoll)
	}

	return m
}
