package app

import (
	"archive/zip"
	"bytes"
	"context"
	"net/http"
	"net/http/httptest"
	"testing"

	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/dbtest"
)

func TestUsageStatsArchiveHandler(t *testing.T) {
	logger := logtest.Scoped(t)
	db := database.NewDB(logger, dbtest.NewDB(logger, t))

	t.Run("non-admins can't download archive", func(t *testing.T) {
		req, _ := http.NewRequest("GET", "", nil)
		rec := httptest.NewRecorder()
		usageStatsArchiveHandler(db)(rec, req)

		if have, want := rec.Code, http.StatusUnauthorized; have != want {
			t.Errorf("status code: have %d, want %d", have, want)
		}
	})

	t.Run("admins can download archive", func(t *testing.T) {
		req, _ := http.NewRequest("GET", "", nil)
		rec := httptest.NewRecorder()
		usageStatsArchiveHandler(db)(rec, req.WithContext(actor.WithInternalActor(context.Background())))

		contentType := rec.Header().Get("Content-Type")
		if have, want := contentType, "application/zip"; have != want {
			t.Errorf("Content-Type: have %q, want %q", have, want)
		}

		contentDisposition := rec.Header().Get("Content-Disposition")
		if have, want := contentDisposition, "attachment; filename=\"SourcegraphUsersUsageArchive.zip\""; have != want {
			t.Errorf("Content-Disposition: have %q, want %q", have, want)
		}

		zr, err := zip.NewReader(bytes.NewReader(rec.Body.Bytes()), int64(rec.Body.Len()))
		if err != nil {
			t.Errorf("Body: Failed to open ZIP: %s", err)
		}

		if len(zr.File) == 0 {
			t.Errorf("Zero files in ZIP archive")
		}
	})
}
