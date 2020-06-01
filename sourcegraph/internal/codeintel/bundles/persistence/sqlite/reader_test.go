package sqlite

import (
	"context"
	"io/ioutil"
	"os"
	"path/filepath"
	"testing"

	"github.com/google/go-cmp/cmp"
	persistence "github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/persistence"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/types"
	"github.com/sourcegraph/sourcegraph/internal/observation"
)

func TestReadMeta(t *testing.T) {
	meta, err := testReader(t).ReadMeta(context.Background())
	if err != nil {
		t.Fatalf("unexpected error reading meta: %s", err)
	}
	if meta.NumResultChunks != 4 {
		t.Errorf("unexpected numResultChunks. want=%d have=%d", 4, meta.NumResultChunks)
	}
}

func TestReadDocument(t *testing.T) {
	data, exists, err := testReader(t).ReadDocument(context.Background(), "protocol/writer.go")
	if err != nil {
		t.Fatalf("unexpected error reading document: %s", err)
	}
	if !exists {
		t.Errorf("expected document to exist")
	}

	expectedRange := types.RangeData{
		StartLine:          145,
		StartCharacter:     17,
		EndLine:            145,
		EndCharacter:       28,
		DefinitionResultID: types.ID("2873"),
		ReferenceResultID:  types.ID("16518"),
		HoverResultID:      types.ID("2879"),
		MonikerIDs:         []types.ID{types.ID("2876")},
	}
	if diff := cmp.Diff(expectedRange, data.Ranges[types.ID("2870")]); diff != "" {
		t.Errorf("unexpected range data (-want +got):\n%s", diff)
	}

	expectedHoverData := "```go\n" + `func (*Writer).EmitMoniker(kind string, scheme string, identifier string) (string, error)` + "\n```"
	if diff := cmp.Diff(expectedHoverData, data.HoverResults[types.ID("2879")]); diff != "" {
		t.Errorf("unexpected hover data (-want +got):\n%s", diff)
	}

	expectedMoniker := types.MonikerData{
		Kind:                 "export",
		Scheme:               "gomod",
		Identifier:           "github.com/sourcegraph/lsif-go/protocol:EmitMoniker",
		PackageInformationID: types.ID("213"),
	}
	if diff := cmp.Diff(expectedMoniker, data.Monikers[types.ID("2876")]); diff != "" {
		t.Errorf("unexpected moniker data (-want +got):\n%s", diff)
	}

	expectedPackageInformation := types.PackageInformationData{
		Name:    "github.com/sourcegraph/lsif-go",
		Version: "v0.0.0-ad3507cbeb18",
	}
	if diff := cmp.Diff(expectedPackageInformation, data.PackageInformation[types.ID("213")]); diff != "" {
		t.Errorf("unexpected package information data (-want +got):\n%s", diff)
	}
}

func TestReadResultChunk(t *testing.T) {
	data, exists, err := testReader(t).ReadResultChunk(context.Background(), 3)
	if err != nil {
		t.Fatalf("unexpected error reading result chunk: %s", err)
	}
	if !exists {
		t.Errorf("expected result chunk to exist")
	}

	if path := data.DocumentPaths[types.ID("302")]; path != "protocol/protocol.go" {
		t.Errorf("unexpected document path. want=%s have=%s", "protocol/protocol.go", path)
	}

	expectedDocumentRanges := []types.DocumentIDRangeID{
		{DocumentID: "3981", RangeID: "4940"},
		{DocumentID: "3981", RangeID: "10759"},
		{DocumentID: "3981", RangeID: "10986"},
	}
	if diff := cmp.Diff(expectedDocumentRanges, data.DocumentIDRangeIDs[types.ID("14233")]); diff != "" {
		t.Errorf("unexpected document ranges (-want +got):\n%s", diff)
	}
}

func TestReadDefinitions(t *testing.T) {
	definitions, totalCount, err := testReader(t).ReadDefinitions(context.Background(), "gomod", "github.com/sourcegraph/lsif-go/protocol:Vertex", 3, 4)
	if err != nil {
		t.Fatalf("unexpected error getting definitions: %s", err)
	}
	if totalCount != 11 {
		t.Errorf("unexpected total count. want=%d have=%d", 11, totalCount)
	}

	expectedDefinitions := []types.Location{
		{URI: "protocol/protocol.go", StartLine: 334, StartCharacter: 1, EndLine: 334, EndCharacter: 7},
		{URI: "protocol/protocol.go", StartLine: 139, StartCharacter: 1, EndLine: 139, EndCharacter: 7},
		{URI: "protocol/protocol.go", StartLine: 384, StartCharacter: 1, EndLine: 384, EndCharacter: 7},
		{URI: "protocol/protocol.go", StartLine: 357, StartCharacter: 1, EndLine: 357, EndCharacter: 7},
	}
	if diff := cmp.Diff(expectedDefinitions, definitions); diff != "" {
		t.Errorf("unexpected definitions (-want +got):\n%s", diff)
	}
}

func TestReadReferences(t *testing.T) {
	references, totalCount, err := testReader(t).ReadReferences(context.Background(), "gomod", "golang.org/x/tools/go/packages:Package", 3, 4)
	if err != nil {
		t.Fatalf("unexpected error getting references: %s", err)
	}
	if totalCount != 25 {
		t.Errorf("unexpected total count. want=%d have=%d", 25, totalCount)
	}

	expectedReferences := []types.Location{
		{URI: "internal/index/helper.go", StartLine: 184, StartCharacter: 56, EndLine: 184, EndCharacter: 63},
		{URI: "internal/index/helper.go", StartLine: 35, StartCharacter: 56, EndLine: 35, EndCharacter: 63},
		{URI: "internal/index/helper.go", StartLine: 184, StartCharacter: 35, EndLine: 184, EndCharacter: 42},
		{URI: "internal/index/helper.go", StartLine: 48, StartCharacter: 44, EndLine: 48, EndCharacter: 51},
	}
	if diff := cmp.Diff(expectedReferences, references); diff != "" {
		t.Errorf("unexpected references (-want +got):\n%s", diff)
	}
}

func testReader(t *testing.T) persistence.Reader {
	reader, err := NewReader(context.Background(), copyFile(t, "./testdata/lsif-go@ad3507cb.lsif.db"))
	if err != nil {
		t.Fatalf("unexpected error opening database: %s", err)
	}
	t.Cleanup(func() { _ = reader.Close() })

	// Wrap in observed, as that's how it's used in production
	return persistence.NewObserved(reader, &observation.TestContext)
}

func copyFile(t *testing.T, source string) string {
	tempDir, err := ioutil.TempDir("", "")
	if err != nil {
		t.Fatalf("unexpected error creating temp dir: %s", err)
	}
	t.Cleanup(func() { _ = os.RemoveAll(tempDir) })

	input, err := ioutil.ReadFile(source)
	if err != nil {
		t.Fatalf("unexpected error reading file: %s", err)
	}

	dest := filepath.Join(tempDir, "test.sqlite")
	if err := ioutil.WriteFile(dest, input, os.ModePerm); err != nil {
		t.Fatalf("unexpected error writing file: %s", err)
	}

	return dest
}
