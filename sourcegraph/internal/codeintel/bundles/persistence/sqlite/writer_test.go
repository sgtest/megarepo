package sqlite

import (
	"context"
	"io/ioutil"
	"os"
	"path/filepath"
	"testing"

	"github.com/google/go-cmp/cmp"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/bundles/types"
)

func TestWrite(t *testing.T) {
	tempDir, err := ioutil.TempDir("", "")
	if err != nil {
		t.Fatalf("unexpected error creating temp directory: %s", err)
	}
	defer os.RemoveAll(tempDir)

	ctx := context.Background()
	filename := filepath.Join(tempDir, "test.db")

	writer, err := NewWriter(context.Background(), filename)
	if err != nil {
		t.Fatalf("unexpected error while opening writer: %s", err)
	}

	if err := writer.WriteMeta(ctx, types.MetaData{NumResultChunks: 7}); err != nil {
		t.Fatalf("unexpected error while writing: %s", err)
	}

	expectedDocumentData := types.DocumentData{
		Ranges: map[types.ID]types.RangeData{
			"r01": {StartLine: 1, StartCharacter: 2, EndLine: 3, EndCharacter: 4, DefinitionResultID: "x01", MonikerIDs: []types.ID{"m01", "m02"}},
			"r02": {StartLine: 2, StartCharacter: 3, EndLine: 4, EndCharacter: 5, ReferenceResultID: "x06", MonikerIDs: []types.ID{"m03", "m04"}},
			"r03": {StartLine: 3, StartCharacter: 4, EndLine: 5, EndCharacter: 6, DefinitionResultID: "x02"},
		},
		HoverResults: map[types.ID]string{},
		Monikers: map[types.ID]types.MonikerData{
			"m01": {Kind: "import", Scheme: "scheme A", Identifier: "ident A", PackageInformationID: "p01"},
			"m02": {Kind: "import", Scheme: "scheme B", Identifier: "ident B"},
			"m03": {Kind: "export", Scheme: "scheme C", Identifier: "ident C", PackageInformationID: "p02"},
			"m04": {Kind: "export", Scheme: "scheme D", Identifier: "ident D"},
		},
		PackageInformation: map[types.ID]types.PackageInformationData{
			"p01": {Name: "pkg A", Version: "0.1.0"},
			"p02": {Name: "pkg B", Version: "1.2.3"},
		},
	}
	if err := writer.WriteDocuments(ctx, map[string]types.DocumentData{"foo.go": expectedDocumentData}); err != nil {
		t.Fatalf("unexpected error while writing documents: %s", err)
	}

	expectedResultChunkData := types.ResultChunkData{
		DocumentPaths: map[types.ID]string{
			"d01": "foo.go",
			"d02": "bar.go",
			"d03": "baz.go",
		},
		DocumentIDRangeIDs: map[types.ID][]types.DocumentIDRangeID{
			"x01": {
				{DocumentID: "d01", RangeID: "r03"},
				{DocumentID: "d02", RangeID: "r04"},
				{DocumentID: "d03", RangeID: "r07"},
			},
			"x02": {
				{DocumentID: "d01", RangeID: "r02"},
				{DocumentID: "d02", RangeID: "r05"},
				{DocumentID: "d03", RangeID: "r08"},
			},
			"x03": {
				{DocumentID: "d01", RangeID: "r01"},
				{DocumentID: "d02", RangeID: "r06"},
				{DocumentID: "d03", RangeID: "r09"},
			},
		},
	}
	if err := writer.WriteResultChunks(ctx, map[int]types.ResultChunkData{7: expectedResultChunkData}); err != nil {
		t.Fatalf("unexpected error while writing result chunks: %s", err)
	}

	expectedDefinitions := []types.Location{
		{URI: "bar.go", StartLine: 4, StartCharacter: 5, EndLine: 6, EndCharacter: 7},
		{URI: "baz.go", StartLine: 7, StartCharacter: 8, EndLine: 9, EndCharacter: 0},
		{URI: "foo.go", StartLine: 3, StartCharacter: 4, EndLine: 5, EndCharacter: 6},
	}

	definitionMonikerLocations := []types.MonikerLocations{
		{
			Scheme:     "scheme A",
			Identifier: "ident A",
			Locations:  expectedDefinitions,
		},
	}
	if err := writer.WriteDefinitions(ctx, definitionMonikerLocations); err != nil {
		t.Fatalf("unexpected error while writing definitions: %s", err)
	}

	expectedReferences := []types.Location{
		{URI: "baz.go", StartLine: 7, StartCharacter: 8, EndLine: 9, EndCharacter: 0},
		{URI: "baz.go", StartLine: 9, StartCharacter: 0, EndLine: 1, EndCharacter: 2},
		{URI: "foo.go", StartLine: 3, StartCharacter: 4, EndLine: 5, EndCharacter: 6},
	}

	referenceMonikerLocations := []types.MonikerLocations{
		{
			Scheme:     "scheme C",
			Identifier: "ident C",
			Locations:  expectedReferences,
		},
	}
	if err := writer.WriteReferences(ctx, referenceMonikerLocations); err != nil {
		t.Fatalf("unexpected error while writing references: %s", err)
	}

	if err := writer.Close(nil); err != nil {
		t.Fatalf("unexpected error closing writer: %s", err)
	}

	cache, err := NewCache(1)
	if err != nil {
		t.Fatalf("unexpected error creating cache: %s", err)
	}

	reader, err := NewReader(context.Background(), filename, cache)
	if err != nil {
		t.Fatalf("unexpected error opening database: %s", err)
	}
	defer reader.Close()

	meta, err := reader.ReadMeta(ctx)
	if err != nil {
		t.Fatalf("unexpected error reading from database: %s", err)
	}
	if meta.NumResultChunks != 7 {
		t.Errorf("unexpected num result chunks. want=%d have=%d", 7, meta.NumResultChunks)
	}

	documentData, _, err := reader.ReadDocument(ctx, "foo.go")
	if err != nil {
		t.Fatalf("unexpected error reading from database: %s", err)
	}
	if diff := cmp.Diff(expectedDocumentData, documentData); diff != "" {
		t.Errorf("unexpected document data (-want +got):\n%s", diff)
	}

	resultChunkData, _, err := reader.ReadResultChunk(ctx, 7)
	if err != nil {
		t.Fatalf("unexpected error reading from database: %s", err)
	}
	if diff := cmp.Diff(expectedResultChunkData, resultChunkData); diff != "" {
		t.Errorf("unexpected result chunk data (-want +got):\n%s", diff)
	}

	definitions, _, err := reader.ReadDefinitions(ctx, "scheme A", "ident A", 0, 100)
	if err != nil {
		t.Fatalf("unexpected error reading from database: %s", err)
	}
	if diff := cmp.Diff(expectedDefinitions, definitions); diff != "" {
		t.Errorf("unexpected definitions (-want +got):\n%s", diff)
	}

	references, _, err := reader.ReadReferences(ctx, "scheme C", "ident C", 0, 100)
	if err != nil {
		t.Fatalf("unexpected error reading from database: %s", err)
	}
	if diff := cmp.Diff(expectedReferences, references); diff != "" {
		t.Errorf("unexpected references (-want +got):\n%s", diff)
	}
}
