package lsif

import (
	"context"
	"fmt"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/keegancsmith/sqlf"
	"github.com/sourcegraph/log/logtest"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
)

func TestDiagnosticsCountMigrator(t *testing.T) {
	logger := logtest.Scoped(t)
	rawDB := lastDBWithLSIF(logger, t)
	db := database.NewDB(logger, rawDB)
	store := basestore.NewWithHandle(db.Handle())
	migrator := NewDiagnosticsCountMigrator(store, 250, 1)
	serializer := newSerializer()

	assertProgress := func(expectedProgress float64, applyReverse bool) {
		if progress, err := migrator.Progress(context.Background(), applyReverse); err != nil {
			t.Fatalf("unexpected error querying progress: %s", err)
		} else if progress != expectedProgress {
			t.Errorf("unexpected progress. want=%.2f have=%.2f", expectedProgress, progress)
		}
	}

	assertCounts := func(expectedCounts []int) {
		query := sqlf.Sprintf(`SELECT num_diagnostics FROM lsif_data_documents ORDER BY path`)

		if counts, err := basestore.ScanInts(store.Query(context.Background(), query)); err != nil {
			t.Fatalf("unexpected error querying num diagnostics: %s", err)
		} else if diff := cmp.Diff(expectedCounts, counts); diff != "" {
			t.Errorf("unexpected counts (-want +got):\n%s", diff)
		}
	}

	n := 500
	expectedCounts := make([]int, 0, n)
	diagnostics := make([]DiagnosticData, 0, n)

	for i := 0; i < n; i++ {
		expectedCounts = append(expectedCounts, i+1)
		diagnostics = append(diagnostics, DiagnosticData{Code: fmt.Sprintf("c%d", i)})

		data, err := serializer.MarshalLegacyDocumentData(DocumentData{
			Diagnostics: diagnostics,
		})
		if err != nil {
			t.Fatalf("unexpected error serializing document data: %s", err)
		}

		if err := store.Exec(context.Background(), sqlf.Sprintf(
			"INSERT INTO lsif_data_documents (dump_id, path, data, schema_version, num_diagnostics) VALUES (%s, %s, %s, 1, 0)",
			42+i/(n/2), // 50% id=42, 50% id=43
			fmt.Sprintf("p%04d", i),
			data,
		)); err != nil {
			t.Fatalf("unexpected error inserting row: %s", err)
		}
	}

	assertProgress(0, false)

	if err := migrator.Up(context.Background()); err != nil {
		t.Fatalf("unexpected error performing up migration: %s", err)
	}
	assertProgress(0.5, false)

	if err := migrator.Up(context.Background()); err != nil {
		t.Fatalf("unexpected error performing up migration: %s", err)
	}
	assertProgress(1, false)

	assertCounts(expectedCounts)

	if err := migrator.Down(context.Background()); err != nil {
		t.Fatalf("unexpected error performing down migration: %s", err)
	}
	assertProgress(0.5, true)

	if err := migrator.Down(context.Background()); err != nil {
		t.Fatalf("unexpected error performing down migration: %s", err)
	}
	assertProgress(0, true)
}
