package lsifstore

import (
	"context"

	"github.com/keegancsmith/sqlf"
	"github.com/opentracing/opentracing-go/log"
	"github.com/sourcegraph/scip/bindings/go/scip"
	"go.opentelemetry.io/otel/attribute"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/codenav/shared"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/codeintel/precise"
)

// GetDiagnostics returns the diagnostics for the documents that have the given path prefix. This method
// also returns the size of the complete result set to aid in pagination.
func (s *store) GetDiagnostics(ctx context.Context, bundleID int, prefix string, limit, offset int) (_ []shared.Diagnostic, _ int, err error) {
	ctx, trace, endObservation := s.operations.getDiagnostics.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("bundleID", bundleID),
		log.String("prefix", prefix),
		log.Int("limit", limit),
		log.Int("offset", offset),
	}})
	defer endObservation(1, observation.Args{})

	documentData, err := s.scanDocumentData(s.db.Query(ctx, sqlf.Sprintf(
		diagnosticsQuery,
		bundleID,
		prefix+"%",
	)))
	if err != nil {
		return nil, 0, err
	}
	trace.AddEvent("scanDocumentData", attribute.Int("numDocuments", len(documentData)))

	totalCount := 0
	for _, documentData := range documentData {
		for _, occurrence := range documentData.SCIPData.Occurrences {
			totalCount += len(occurrence.Diagnostics)
		}
	}
	trace.AddEvent("found", attribute.Int("totalCount", totalCount))

	diagnostics := make([]shared.Diagnostic, 0, limit)
	for _, documentData := range documentData {
	occurrenceLoop:
		for _, occurrence := range documentData.SCIPData.Occurrences {
			if len(occurrence.Diagnostics) == 0 {
				continue
			}

			r := scip.NewRange(occurrence.Range)

			for _, diagnostic := range occurrence.Diagnostics {
				offset--

				if offset < 0 && len(diagnostics) < limit {
					diagnostics = append(diagnostics, shared.Diagnostic{
						DumpID: bundleID,
						Path:   documentData.Path,
						DiagnosticData: precise.DiagnosticData{
							Severity:       int(diagnostic.Severity),
							Code:           diagnostic.Code,
							Message:        diagnostic.Message,
							Source:         diagnostic.Source,
							StartLine:      int(r.Start.Line),
							StartCharacter: int(r.Start.Character),
							EndLine:        int(r.End.Line),
							EndCharacter:   int(r.End.Character),
						},
					})
				} else {
					break occurrenceLoop
				}
			}
		}
	}

	return diagnostics, totalCount, nil
}

const diagnosticsQuery = `
SELECT
	sd.id,
	sid.document_path,
	sd.raw_scip_payload
FROM codeintel_scip_document_lookup sid
JOIN codeintel_scip_documents sd ON sd.id = sid.document_id
WHERE
	sid.upload_id = %s AND
	sid.document_path = %s
LIMIT 1
`
