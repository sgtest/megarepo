package app

import (
	"database/sql"
	"io"
	"net/http"

	"github.com/inconshreveable/log15"

	"github.com/sourcegraph/sourcegraph/internal/auth"
	"github.com/sourcegraph/sourcegraph/internal/database"
)

// latestPingHandler fetches the most recent ping data from the event log
// (if any is present) and returns it as JSON.
func latestPingHandler(db database.DB) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		// 🚨SECURITY: Only site admins may access ping data.
		if err := auth.CheckCurrentUserIsSiteAdmin(r.Context(), db); err != nil {
			w.WriteHeader(http.StatusUnauthorized)
			return
		}

		w.Header().Set("Content-Type", "application/json")
		ping, err := db.EventLogs().LatestPing(r.Context())
		switch err {
		case sql.ErrNoRows:
			_, _ = io.WriteString(w, "{}")
		case nil:
			_, _ = io.WriteString(w, string(ping.Argument))
		default:
			log15.Error("pings.latest", "error", err)
			w.WriteHeader(http.StatusInternalServerError)
		}
	}
}
