package correlation

import (
	"context"
	"fmt"
	"io"
	"path/filepath"
	"strings"

	"github.com/inconshreveable/log15"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/correlation/datastructures"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/correlation/lsif"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/correlation/lsif/jsonlines"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/existence"
)

// Correlate reads LSIF data from the given reader and returns a correlation state object with
// the same data canonicalized and pruned for storage.
func Correlate(ctx context.Context, r io.Reader, dumpID int, root string, getChildren existence.GetChildrenFunc) (*GroupedBundleData, error) {
	// Read raw upload stream and return a correlation state
	state, err := correlateFromReader(r, root)
	if err != nil {
		return nil, err
	}

	// Remove duplicate elements, collapse linked elements
	canonicalize(state)

	// Remove elements we don't need to store
	if err := prune(ctx, state, root, getChildren); err != nil {
		return nil, err
	}

	groupedBundleData, err := groupBundleData(state, dumpID)
	if err != nil {
		return nil, err
	}

	return groupedBundleData, nil
}

// correlateFromReader reads the given upload stream and returns a correlation state object.
// The data in the correlation state is neither canonicalized nor pruned.
func correlateFromReader(r io.Reader, root string) (*State, error) {
	ctx, cancel := context.WithCancel(context.Background())
	ch := jsonlines.Read(ctx, r)
	defer func() {
		// stop producer from reading more input on correlation error
		cancel()

		for range ch {
			// drain whatever is in the channel to help out GC
		}
	}()

	wrappedState := newWrappedState(root)

	i := 0
	for pair := range ch {
		i++

		if pair.Err != nil {
			return nil, fmt.Errorf("dump malformed on element %d: %s", i, pair.Err)
		}

		if err := correlateElement(wrappedState, pair.Element); err != nil {
			return nil, fmt.Errorf("dump malformed on element %d: %s", i, err)
		}
	}

	if wrappedState.LSIFVersion == "" {
		return nil, ErrMissingMetaData
	}

	return wrappedState.State, nil
}

type wrappedState struct {
	*State
	dumpRoot            string
	unsupportedVertexes datastructures.IDSet
}

func newWrappedState(dumpRoot string) *wrappedState {
	return &wrappedState{
		State:               newState(),
		dumpRoot:            dumpRoot,
		unsupportedVertexes: datastructures.IDSet{},
	}
}

// correlateElement maps a single vertex or edge element into the correlation state.
func correlateElement(state *wrappedState, element lsif.Element) error {
	switch element.Type {
	case "vertex":
		return correlateVertex(state, element)
	case "edge":
		return correlateEdge(state, element)
	}

	return fmt.Errorf("unknown element type %s", element.Type)
}

var vertexHandlers = map[string]func(state *wrappedState, element lsif.Element) error{
	"metaData":           correlateMetaData,
	"document":           correlateDocument,
	"range":              correlateRange,
	"resultSet":          correlateResultSet,
	"definitionResult":   correlateDefinitionResult,
	"referenceResult":    correlateReferenceResult,
	"hoverResult":        correlateHoverResult,
	"moniker":            correlateMoniker,
	"packageInformation": correlatePackageInformation,
	"diagnosticResult":   correlateDiagnosticResult,
}

// correlateElement maps a single vertex element into the correlation state.
func correlateVertex(state *wrappedState, element lsif.Element) error {
	handler, ok := vertexHandlers[element.Label]
	if !ok {
		// Can safely skip, but need to mark this in case we have an edge
		// later that legally refers to this element by identifier. If we
		// don't track this, item edges related to something other than a
		// definition or reference result will result in a spurious error
		// although the LSIF index is valid.
		state.unsupportedVertexes.Add(element.ID)
		return nil
	}

	return handler(state, element)
}

var edgeHandlers = map[string]func(state *wrappedState, id string, edge lsif.Edge) error{
	"contains":                correlateContainsEdge,
	"next":                    correlateNextEdge,
	"item":                    correlateItemEdge,
	"textDocument/definition": correlateTextDocumentDefinitionEdge,
	"textDocument/references": correlateTextDocumentReferencesEdge,
	"textDocument/hover":      correlateTextDocumentHoverEdge,
	"moniker":                 correlateMonikerEdge,
	"nextMoniker":             correlateNextMonikerEdge,
	"packageInformation":      correlatePackageInformationEdge,
	"textDocument/diagnostic": correlateDiagnosticEdge,
}

// correlateElement maps a single edge element into the correlation state.
func correlateEdge(state *wrappedState, element lsif.Element) error {
	edge, ok := element.Payload.(lsif.Edge)
	if !ok {
		return ErrUnexpectedPayload
	}

	handler, ok := edgeHandlers[element.Label]
	if !ok {
		// We don't care, can safely skip
		return nil
	}

	return handler(state, element.ID, edge)
}

func correlateMetaData(state *wrappedState, element lsif.Element) error {
	payload, ok := element.Payload.(lsif.MetaData)
	if !ok {
		return ErrUnexpectedPayload
	}

	// We assume that the project root in the LSIF dump is either:
	//
	//   (1) the root of the LSIF dump, or
	//   (2) the root of the repository
	//
	// These are the common cases and we don't explicitly support
	// anything else. Here we normalize to (1) by appending the dump
	// root if it's not already suffixed by it.

	if !strings.HasSuffix(payload.ProjectRoot, "/") {
		payload.ProjectRoot += "/"
	}

	if state.dumpRoot != "" && !strings.HasSuffix(payload.ProjectRoot, state.dumpRoot) {
		payload.ProjectRoot += state.dumpRoot
	}

	state.LSIFVersion = payload.Version
	state.ProjectRoot = payload.ProjectRoot
	return nil
}

func correlateDocument(state *wrappedState, element lsif.Element) error {
	payload, ok := element.Payload.(lsif.Document)
	if !ok {
		return ErrUnexpectedPayload
	}

	if state.ProjectRoot == "" {
		return ErrMissingMetaData
	}

	relativeURI, err := filepath.Rel(state.ProjectRoot, payload.URI)
	if err != nil {
		return fmt.Errorf("document URI %q is not relative to project root %q (%s)", payload.URI, state.ProjectRoot, err)
	}

	payload.URI = relativeURI
	state.DocumentData[element.ID] = payload
	return nil
}

func correlateRange(state *wrappedState, element lsif.Element) error {
	payload, ok := element.Payload.(lsif.Range)
	if !ok {
		return ErrUnexpectedPayload
	}

	state.RangeData[element.ID] = payload
	return nil
}

func correlateResultSet(state *wrappedState, element lsif.Element) error {
	state.ResultSetData[element.ID] = lsif.ResultSet{
		MonikerIDs: datastructures.IDSet{},
	}
	return nil
}

func correlateDefinitionResult(state *wrappedState, element lsif.Element) error {
	state.DefinitionData[element.ID] = map[string]datastructures.IDSet{}
	return nil
}

func correlateReferenceResult(state *wrappedState, element lsif.Element) error {
	state.ReferenceData[element.ID] = map[string]datastructures.IDSet{}
	return nil
}

func correlateHoverResult(state *wrappedState, element lsif.Element) error {
	payload, ok := element.Payload.(string)
	if !ok {
		return ErrUnexpectedPayload
	}

	state.HoverData[element.ID] = payload
	return nil
}

func correlateMoniker(state *wrappedState, element lsif.Element) error {
	payload, ok := element.Payload.(lsif.Moniker)
	if !ok {
		return ErrUnexpectedPayload
	}

	state.MonikerData[element.ID] = payload
	return nil
}

func correlatePackageInformation(state *wrappedState, element lsif.Element) error {
	payload, ok := element.Payload.(lsif.PackageInformation)
	if !ok {
		return ErrUnexpectedPayload
	}

	state.PackageInformationData[element.ID] = payload
	return nil
}

func correlateDiagnosticResult(state *wrappedState, element lsif.Element) error {
	payload, ok := element.Payload.(lsif.DiagnosticResult)
	if !ok {
		return ErrUnexpectedPayload
	}

	state.Diagnostics[element.ID] = payload
	return nil
}

func correlateContainsEdge(state *wrappedState, id string, edge lsif.Edge) error {
	document, ok := state.DocumentData[edge.OutV]
	if !ok {
		// Do not track this relation for project vertices
		return nil
	}

	for _, inV := range edge.InVs {
		if _, ok := state.RangeData[inV]; !ok {
			return malformedDump(id, edge.InV, "range")
		}
		document.Contains.Add(inV)
	}
	return nil
}

func correlateNextEdge(state *wrappedState, id string, edge lsif.Edge) error {
	if _, ok := state.ResultSetData[edge.InV]; !ok {
		return malformedDump(id, edge.InV, "resultSet")
	}

	if _, ok := state.RangeData[edge.OutV]; ok {
		state.NextData[edge.OutV] = edge.InV
	} else if _, ok := state.ResultSetData[edge.OutV]; ok {
		state.NextData[edge.OutV] = edge.InV
	} else {
		return malformedDump(id, edge.OutV, "range", "resultSet")
	}
	return nil
}

func correlateItemEdge(state *wrappedState, id string, edge lsif.Edge) error {
	if documentMap, ok := state.DefinitionData[edge.OutV]; ok {
		for _, inV := range edge.InVs {
			if _, ok := state.RangeData[inV]; !ok {
				return malformedDump(id, edge.InV, "range")
			}

			// Link definition data to defining range
			documentMap.GetOrCreate(edge.Document).Add(inV)
		}

		return nil
	}

	if documentMap, ok := state.ReferenceData[edge.OutV]; ok {
		for _, inV := range edge.InVs {
			if _, ok := state.ReferenceData[inV]; ok {
				// Link reference data identifiers together
				state.LinkedReferenceResults.Union(edge.OutV, inV)
			} else {
				if _, ok = state.RangeData[inV]; !ok {
					return malformedDump(id, edge.InV, "range")
				}

				// Link reference data to a reference range
				documentMap.GetOrCreate(edge.Document).Add(inV)
			}
		}

		return nil
	}

	if !state.unsupportedVertexes.Contains(edge.OutV) {
		return malformedDump(id, edge.OutV, "vertex")
	}

	log15.Debug("Skipping edge from an unsupported vertex")
	return nil
}

func correlateTextDocumentDefinitionEdge(state *wrappedState, id string, edge lsif.Edge) error {
	if _, ok := state.DefinitionData[edge.InV]; !ok {
		return malformedDump(id, edge.InV, "definitionResult")
	}

	if source, ok := state.RangeData[edge.OutV]; ok {
		state.RangeData[edge.OutV] = source.SetDefinitionResultID(edge.InV)
	} else if source, ok := state.ResultSetData[edge.OutV]; ok {
		state.ResultSetData[edge.OutV] = source.SetDefinitionResultID(edge.InV)
	} else {
		return malformedDump(id, edge.OutV, "range", "resultSet")
	}
	return nil
}

func correlateTextDocumentReferencesEdge(state *wrappedState, id string, edge lsif.Edge) error {
	if _, ok := state.ReferenceData[edge.InV]; !ok {
		return malformedDump(id, edge.InV, "referenceResult")
	}

	if source, ok := state.RangeData[edge.OutV]; ok {
		state.RangeData[edge.OutV] = source.SetReferenceResultID(edge.InV)
	} else if source, ok := state.ResultSetData[edge.OutV]; ok {
		state.ResultSetData[edge.OutV] = source.SetReferenceResultID(edge.InV)
	} else {
		return malformedDump(id, edge.OutV, "range", "resultSet")
	}
	return nil
}

func correlateTextDocumentHoverEdge(state *wrappedState, id string, edge lsif.Edge) error {
	if _, ok := state.HoverData[edge.InV]; !ok {
		return malformedDump(id, edge.InV, "hoverResult")
	}

	if source, ok := state.RangeData[edge.OutV]; ok {
		state.RangeData[edge.OutV] = source.SetHoverResultID(edge.InV)
	} else if source, ok := state.ResultSetData[edge.OutV]; ok {
		state.ResultSetData[edge.OutV] = source.SetHoverResultID(edge.InV)
	} else {
		return malformedDump(id, edge.OutV, "range", "resultSet")
	}
	return nil
}

func correlateMonikerEdge(state *wrappedState, id string, edge lsif.Edge) error {
	if _, ok := state.MonikerData[edge.InV]; !ok {
		return malformedDump(id, edge.InV, "moniker")
	}

	ids := datastructures.IDSet{}
	ids.Add(edge.InV)

	if source, ok := state.RangeData[edge.OutV]; ok {
		state.RangeData[edge.OutV] = source.SetMonikerIDs(ids)
	} else if source, ok := state.ResultSetData[edge.OutV]; ok {
		state.ResultSetData[edge.OutV] = source.SetMonikerIDs(ids)
	} else {
		return malformedDump(id, edge.OutV, "range", "resultSet")
	}
	return nil
}

func correlateNextMonikerEdge(state *wrappedState, id string, edge lsif.Edge) error {
	if _, ok := state.MonikerData[edge.InV]; !ok {
		return malformedDump(id, edge.InV, "moniker")
	}
	if _, ok := state.MonikerData[edge.OutV]; !ok {
		return malformedDump(id, edge.OutV, "moniker")
	}

	state.LinkedMonikers.Union(edge.InV, edge.OutV)
	return nil
}

func correlatePackageInformationEdge(state *wrappedState, id string, edge lsif.Edge) error {
	if _, ok := state.PackageInformationData[edge.InV]; !ok {
		return malformedDump(id, edge.InV, "packageInformation")
	}

	source, ok := state.MonikerData[edge.OutV]
	if !ok {
		return malformedDump(id, edge.OutV, "moniker")
	}
	state.MonikerData[edge.OutV] = source.SetPackageInformationID(edge.InV)

	switch source.Kind {
	case "import":
		// keep list of imported monikers
		state.ImportedMonikers.Add(edge.OutV)
	case "export":
		// keep list of exported monikers
		state.ExportedMonikers.Add(edge.OutV)
	}

	return nil
}

func correlateDiagnosticEdge(state *wrappedState, id string, edge lsif.Edge) error {
	document, ok := state.DocumentData[edge.OutV]
	if !ok {
		return malformedDump(id, edge.OutV, "document")
	}

	if _, ok := state.Diagnostics[edge.InV]; !ok {
		return malformedDump(id, edge.InV, "diagnosticResult")
	}

	document.Diagnostics.Add(edge.InV)
	return nil
}
