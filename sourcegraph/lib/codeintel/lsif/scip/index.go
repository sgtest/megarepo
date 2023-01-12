package scip

import (
	"bytes"
	"context"
	"io"

	"github.com/sourcegraph/scip/bindings/go/scip"

	"github.com/sourcegraph/sourcegraph/lib/codeintel/lsif/conversion"
	"github.com/sourcegraph/sourcegraph/lib/codeintel/precise"
	"github.com/sourcegraph/sourcegraph/lib/codeintel/upload"
)

const unknownIndexer = "lsif-void"

// ConvertLSIF converts the given raw LSIF reader into a SCIP index.
func ConvertLSIF(ctx context.Context, uploadID int, r io.Reader, root string) (*scip.Index, error) {
	var buf bytes.Buffer
	indexerName, err := upload.ReadIndexerName(io.TeeReader(r, &buf))
	if err != nil {
		indexerName = unknownIndexer
	}

	groupedBundleData, err := conversion.Correlate(ctx, io.MultiReader(bytes.NewReader(buf.Bytes()), r), root, nil)
	if err != nil {
		return nil, err
	}

	resultChunks := map[int]precise.ResultChunkData{}
	for resultChunk := range groupedBundleData.ResultChunks {
		resultChunks[resultChunk.Index] = resultChunk.ResultChunk
	}

	definitionMatcher := func(
		targetPath string,
		targetRangeID precise.ID,
		definitionResultID precise.ID,
	) bool {
		definitionResultChunk, ok := resultChunks[precise.HashKey(definitionResultID, groupedBundleData.Meta.NumResultChunks)]
		if !ok {
			return false
		}

		for _, pair := range definitionResultChunk.DocumentIDRangeIDs[definitionResultID] {
			if targetPath == definitionResultChunk.DocumentPaths[pair.DocumentID] && pair.RangeID == targetRangeID {
				return true
			}
		}

		return false
	}

	var documents []*scip.Document
	for document := range groupedBundleData.Documents {
		documents = append(documents, ConvertLSIFDocument(
			uploadID,
			definitionMatcher,
			indexerName,
			document.Path,
			document.Document,
		))
	}

	metadata := &scip.Metadata{
		Version:              0,
		ToolInfo:             &scip.ToolInfo{Name: indexerName},
		ProjectRoot:          groupedBundleData.ProjectRoot,
		TextDocumentEncoding: scip.TextEncoding_UnspecifiedTextEncoding,
	}

	return &scip.Index{
		Metadata:  metadata,
		Documents: documents,
	}, nil
}
