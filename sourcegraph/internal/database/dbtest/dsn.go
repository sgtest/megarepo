package dbtest

import (
	"net/url"
	"os"
	"os/user"

	"github.com/sourcegraph/sourcegraph/internal/database/postgresdsn"
)

func getDSN() (*url.URL, error) {
	defaults := map[string]string{
		"PGHOST":     "127.0.0.1",
		"PGPORT":     "5432",
		"PGUSER":     "sourcegraph",
		"PGPASSWORD": "sourcegraph",
		"PGDATABASE": "sourcegraph",
		"PGSSLMODE":  "disable",
		"PGTZ":       "UTC",
	}

	getenv := func(k string) string {
		if v := os.Getenv(k); v != "" {
			return v
		}
		return defaults[k]
	}

	username := ""
	if user, err := user.Current(); err == nil {
		username = user.Username
	}

	dsn := postgresdsn.New("", username, getenv)
	return url.Parse(dsn)
}
