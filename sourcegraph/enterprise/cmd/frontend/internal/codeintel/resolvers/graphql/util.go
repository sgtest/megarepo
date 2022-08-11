package graphql

import (
	"github.com/sourcegraph/go-lsp"

	"github.com/sourcegraph/sourcegraph/internal/codeintel/codenav/shared"
	store "github.com/sourcegraph/sourcegraph/internal/codeintel/stores/dbstore"
	"github.com/sourcegraph/sourcegraph/internal/codeintel/stores/lsifstore"
	"github.com/sourcegraph/sourcegraph/lib/codeintel/precise"
)

// strPtr creates a pointer to the given value. If the value is an
// empty string, a nil pointer is returned.
func strPtr(val string) *string {
	if val == "" {
		return nil
	}

	return &val
}

// intPtr creates a pointer to the given value.
func intPtr(val int32) *int32 {
	return &val
}

// intPtr creates a pointer to the given value.
func boolPtr(val bool) *bool {
	return &val
}

// toInt32 translates the given int pointer into an int32 pointer.
func toInt32(val *int) *int32 {
	if val == nil {
		return nil
	}

	v := int32(*val)
	return &v
}

// derefString returns the underlying value in the given pointer.
// If the pointer is nil, the default value is returned.
func derefString(val *string, defaultValue string) string {
	if val != nil {
		return *val
	}
	return defaultValue
}

// derefInt32 returns the underlying value in the given pointer.
// If the pointer is nil, the default value is returned.
func derefInt32(val *int32, defaultValue int) int {
	if val != nil {
		return int(*val)
	}
	return defaultValue
}

// derefBool returns the underlying value in the given pointer.
// If the pointer is nil, the default value is returned.
func derefBool(val *bool, defaultValue bool) bool {
	if val != nil {
		return *val
	}
	return defaultValue
}

// convertRange creates an LSP range from a bundle range.
func convertRange(r lsifstore.Range) lsp.Range {
	return lsp.Range{Start: convertPosition(r.Start.Line, r.Start.Character), End: convertPosition(r.End.Line, r.End.Character)}
}

// convertPosition creates an LSP position from a line and character pair.
func convertPosition(line, character int) lsp.Position {
	return lsp.Position{Line: line, Character: character}
}

func sharedRangeTolsifstoreRange(r shared.Range) lsifstore.Range {
	return lsifstore.Range{
		Start: lsifstore.Position(r.Start),
		End:   lsifstore.Position(r.End),
	}
}

func sharedRangeTolspRange(r shared.Range) lsp.Range {
	return lsp.Range{Start: convertPosition(r.Start.Line, r.Start.Character), End: convertPosition(r.End.Line, r.End.Character)}
}

func sharedRangeToAdjustedRange(rng []shared.AdjustedCodeIntelligenceRange) []AdjustedCodeIntelligenceRange {
	adjustedRange := make([]AdjustedCodeIntelligenceRange, 0, len(rng))
	for _, r := range rng {

		definitions := make([]AdjustedLocation, 0, len(r.Definitions))
		for _, d := range r.Definitions {
			def := AdjustedLocation{
				Dump:           store.Dump(d.Dump),
				Path:           d.Path,
				AdjustedCommit: d.TargetCommit,
				AdjustedRange: lsifstore.Range{
					Start: lsifstore.Position(d.TargetRange.Start),
					End:   lsifstore.Position(d.TargetRange.End),
				},
			}
			definitions = append(definitions, def)
		}

		references := make([]AdjustedLocation, 0, len(r.References))
		for _, d := range r.References {
			ref := AdjustedLocation{
				Dump:           store.Dump(d.Dump),
				Path:           d.Path,
				AdjustedCommit: d.TargetCommit,
				AdjustedRange: lsifstore.Range{
					Start: lsifstore.Position(d.TargetRange.Start),
					End:   lsifstore.Position(d.TargetRange.End),
				},
			}
			references = append(references, ref)
		}

		implementations := make([]AdjustedLocation, 0, len(r.Implementations))
		for _, d := range r.Implementations {
			impl := AdjustedLocation{
				Dump:           store.Dump(d.Dump),
				Path:           d.Path,
				AdjustedCommit: d.TargetCommit,
				AdjustedRange: lsifstore.Range{
					Start: lsifstore.Position(d.TargetRange.Start),
					End:   lsifstore.Position(d.TargetRange.End),
				},
			}
			implementations = append(implementations, impl)
		}

		adj := AdjustedCodeIntelligenceRange{
			Range: lsifstore.Range{
				Start: lsifstore.Position(r.Range.Start),
				End:   lsifstore.Position(r.Range.End),
			},
			Definitions:     definitions,
			References:      references,
			Implementations: implementations,
			HoverText:       r.HoverText,
		}

		adjustedRange = append(adjustedRange, adj)
	}

	return adjustedRange
}

func uploadLocationToAdjustedLocations(location []shared.UploadLocation) []AdjustedLocation {
	uploadLocation := make([]AdjustedLocation, 0, len(location))
	for _, loc := range location {
		dump := store.Dump(loc.Dump)
		adjustedRange := lsifstore.Range{
			Start: lsifstore.Position{
				Line:      loc.TargetRange.Start.Line,
				Character: loc.TargetRange.Start.Character,
			},
			End: lsifstore.Position{
				Line:      loc.TargetRange.End.Line,
				Character: loc.TargetRange.End.Character,
			},
		}

		uploadLocation = append(uploadLocation, AdjustedLocation{
			Dump:           dump,
			Path:           loc.Path,
			AdjustedCommit: loc.TargetCommit,
			AdjustedRange:  adjustedRange,
		})
	}

	return uploadLocation
}

func sharedDumpToDbstoreUpload(dump shared.Dump) store.Upload {
	return store.Upload{
		ID:                dump.ID,
		Commit:            dump.Commit,
		Root:              dump.Root,
		VisibleAtTip:      dump.VisibleAtTip,
		UploadedAt:        dump.UploadedAt,
		State:             dump.State,
		FailureMessage:    dump.FailureMessage,
		StartedAt:         dump.StartedAt,
		FinishedAt:        dump.FinishedAt,
		ProcessAfter:      dump.ProcessAfter,
		NumResets:         dump.NumResets,
		NumFailures:       dump.NumFailures,
		RepositoryID:      dump.RepositoryID,
		RepositoryName:    dump.RepositoryName,
		Indexer:           dump.Indexer,
		IndexerVersion:    dump.IndexerVersion,
		NumParts:          0,
		UploadedParts:     []int{},
		UploadSize:        nil,
		Rank:              nil,
		AssociatedIndexID: dump.AssociatedIndexID,
	}
}

func sharedDiagnosticAtUploadToAdjustedDiagnostic(shared []shared.DiagnosticAtUpload) []AdjustedDiagnostic {
	adjustedDiagnostics := make([]AdjustedDiagnostic, 0, len(shared))
	for _, diag := range shared {
		diagnosticData := precise.DiagnosticData{
			Severity:       diag.Severity,
			Code:           diag.Code,
			Message:        diag.Message,
			Source:         diag.Source,
			StartLine:      diag.StartLine,
			StartCharacter: diag.StartCharacter,
			EndLine:        diag.EndLine,
			EndCharacter:   diag.EndCharacter,
		}
		lsifDiag := lsifstore.Diagnostic{
			DiagnosticData: diagnosticData,
			DumpID:         diag.DumpID,
			Path:           diag.Path,
		}

		adjusted := AdjustedDiagnostic{
			Diagnostic:     lsifDiag,
			Dump:           store.Dump(diag.Dump),
			AdjustedCommit: diag.AdjustedCommit,
			AdjustedRange: lsifstore.Range{
				Start: lsifstore.Position(diag.AdjustedRange.Start),
				End:   lsifstore.Position(diag.AdjustedRange.End),
			},
		}
		adjustedDiagnostics = append(adjustedDiagnostics, adjusted)
	}
	return adjustedDiagnostics
}
