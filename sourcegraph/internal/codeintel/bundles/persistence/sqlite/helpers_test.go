package sqlite

import (
	"context"
	"io/ioutil"
	"os"
	"path/filepath"
	"testing"

	persistence "github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/persistence"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func testReader(t *testing.T, filename string) persistence.Reader {
	tempDir, err := ioutil.TempDir("", "")
	if err != nil {
		t.Fatalf("unexpected error creating temp dir: %s", err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(tempDir) })

	input, err := ioutil.ReadFile(filename)
	if err != nil {
		t.Fatalf("unexpected error reading file: %s", err)
	}

	dest := filepath.Join(tempDir, "test.sqlite")

	// Copy the sqlite file to a temporary directory before opening so that
	// if a migration is ran it does not overwrite the original test data.
	if err := ioutil.WriteFile(dest, input, os.ModePerm); err != nil {
		t.Fatalf("unexpected error writing file: %s", err)
	}

	cache, err := NewCache(1)
	if err != nil {
		t.Fatalf("unexpected error creating cache: %s", err)
	}

	reader, err := NewReader(context.Background(), dest, cache)
	if err != nil {
		t.Fatalf("unexpected error opening database: %s", err)
	}
	t.Cleanup(func() { _ = reader.Close() })

	// Wrap in observed, as that's how it's used in production
	return persistence.NewObserved(reader, &observation.TestContext)
}
