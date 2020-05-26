package correlation

import (
	"sort"

	"github.com/sourcegraph/sourcegraph/cmd/precise-code-intel-worker/internal/correlation/datastructures"
	"github.com/sourcegraph/sourcegraph/cmd/precise-code-intel-worker/internal/correlation/lsif"
)

// canonicalize deduplicates data in the raw correlation state and collapses range,
// result set, and moniker data that form chains via next edges.
func canonicalize(state *State) {
	fns := []func(state *State){
		canonicalizeDocuments,
		canonicalizeReferenceResults,
		canonicalizeResultSets,
		canonicalizeRanges,
	}

	for _, fn := range fns {
		fn(state)
	}
}

// canonicalizeDocuments determines if multiple documents are defined with the same URI. This can
// happen in some indexers (such as lsif-tsc) that index dependent projects into the same index
// as the target project. For each set of documents that share a path, we choose one document to
// be the canonical representative and merge the contains, definition, and reference data into the
// unique canonical document. This function guarantees that duplicate document IDs are removed from
// the correlation state.
func canonicalizeDocuments(state *State) {
	documentIDs := map[string][]string{}
	for documentID, doc := range state.DocumentData {
		documentIDs[doc.URI] = append(documentIDs[doc.URI], documentID)
	}
	for _, v := range documentIDs {
		sort.Strings(v)
	}

	for documentID, doc := range state.DocumentData {
		// Choose canonical document alphabetically
		if canonicalID := documentIDs[doc.URI][0]; documentID != canonicalID {
			for id := range state.DocumentData[documentID].Contains {
				// Move ranges into the canonical document
				state.DocumentData[canonicalID].Contains.Add(id)
			}

			// Move definition/reference data into the canonical document
			canonicalizeDocumentsInDefinitionReferences(state, state.DefinitionData, documentID, canonicalID)
			canonicalizeDocumentsInDefinitionReferences(state, state.ReferenceData, documentID, canonicalID)

			// Remove non-canonical document
			delete(state.DocumentData, documentID)
		}
	}
}

// canonicalizeDocumentsInDefinitionReferences moves definition or reference result data from the
// given document to the given canonical document and removes all references to the non-canonical
// document.
func canonicalizeDocumentsInDefinitionReferences(state *State, definitionReferenceData map[string]datastructures.DefaultIDSetMap, documentID, canonicalID string) {
	for _, documentRanges := range definitionReferenceData {
		rangeIDs, ok := documentRanges[documentID]
		if !ok {
			continue
		}

		// Move definition/reference data into the canonical document
		documentRanges.GetOrCreate(canonicalID).AddAll(rangeIDs)

		// Remove references to non-canonical document
		delete(documentRanges, documentID)
	}
}

// canonicalizeReferenceResults determines which reference results are linked together. For each
// set of linked reference results, we choose one reference result to be the canonical representative
// and merge the data into the unique canonical result set. All non-canonical results are removed from
// the correlation state and references to non-canonical results are updated to refer to the canonical
// choice.
func canonicalizeReferenceResults(state *State) {
	// Maintain a map from a reference result to its canonical identifier
	canonicalIDs := map[string]string{}

	for referenceResultID := range state.LinkedReferenceResults {
		if _, ok := canonicalIDs[referenceResultID]; ok {
			// Already processed
			continue
		}

		// Find all reachable items in this set
		linkedIDs := state.LinkedReferenceResults.ExtractSet(referenceResultID)
		canonicalID, _ := linkedIDs.Choose()
		canonicalReferenceResult := state.ReferenceData[canonicalID]

		for linkedID := range linkedIDs {
			// Mark canonical choice
			canonicalIDs[linkedID] = canonicalID

			if linkedID != canonicalID {
				for documentID, rangeIDs := range state.ReferenceData[linkedID] {
					// Move range data into the canonical document
					canonicalReferenceResult.GetOrCreate(documentID).AddAll(rangeIDs)
				}
			}
		}
	}

	for id, item := range state.RangeData {
		if canonicalID, ok := canonicalIDs[item.ReferenceResultID]; ok {
			// Update reference result identifier to canonical choice
			state.RangeData[id] = item.SetReferenceResultID(canonicalID)
		}
	}

	for id, item := range state.ResultSetData {
		if canonicalID, ok := canonicalIDs[item.ReferenceResultID]; ok {
			// Update reference result identifier to canonical choice
			state.ResultSetData[id] = item.SetReferenceResultID(canonicalID)
		}
	}

	// Invert the map to get a set of canonical identifiers
	inverseMap := map[string]struct{}{}
	for _, canonicalID := range canonicalIDs {
		inverseMap[canonicalID] = struct{}{}
	}

	for referenceResultID := range canonicalIDs {
		if _, ok := inverseMap[referenceResultID]; !ok {
			// Remove non-canonical reference result
			delete(state.ReferenceData, referenceResultID)
		}
	}
}

// canonicalizeResultSets runs canonicalizeResultSet on each result set in the correlation state.
// This will collapse result sets down recursively so that if a result set's next element also has
// a next element, then both sets merge down into the original result set.
func canonicalizeResultSets(state *State) {
	for resultSetID, resultSetData := range state.ResultSetData {
		canonicalizeResultSetData(state, resultSetID, resultSetData)
	}

	for resultSetID, resultSetData := range state.ResultSetData {
		state.ResultSetData[resultSetID] = resultSetData.SetMonikerIDs(gatherMonikers(state, resultSetData.MonikerIDs))
	}
}

// canonicalizeResultSets "merges down" the definition, reference, and hover result identifiers
// from the element's "next" result set if such an element exists and the identifier is not already.
// defined. This also merges down the moniker ids by unioning the sets.
//
// This method is assumed to be invoked only after canonicalizeResultSets, otherwise the next element
// of a range may not have all of the necessary data to perform this canonicalization step.
func canonicalizeRanges(state *State) {
	for rangeID, rangeData := range state.RangeData {
		if _, nextItem, ok := next(state, rangeID); ok {
			// Merge range and next element
			rangeData = mergeNextRangeData(rangeData, nextItem)
			// Delete next data to prevent us from re-performing this step
			delete(state.NextData, rangeID)
		}

		state.RangeData[rangeID] = rangeData.SetMonikerIDs(gatherMonikers(state, rangeData.MonikerIDs))
	}
}

// canonicalizeResultSets "merges down" the definition, reference, and hover result identifiers
// from the element's "next" result set if such an element exists and the identifier is not
// already defined. This also merges down the moniker ids by unioning the sets.
func canonicalizeResultSetData(state *State, id string, item lsif.ResultSet) lsif.ResultSet {
	if nextID, nextItem, ok := next(state, id); ok {
		// Recursively canonicalize the next element
		nextItem = canonicalizeResultSetData(state, nextID, nextItem)
		// Merge result set and canonicalized next element
		item = mergeNextResultSetData(item, nextItem)
		// Delete next data to prevent us from re-performing this step
		delete(state.NextData, id)
	}

	state.ResultSetData[id] = item
	return item
}

// mergeNextResultSetData merges the definition, reference, and hover result identifiers from
// nextItem into item when not already defined. The moniker identifiers of nextItem are unioned
// into the moniker identifiers of item.
func mergeNextResultSetData(item, nextItem lsif.ResultSet) lsif.ResultSet {
	if item.DefinitionResultID == "" {
		item = item.SetDefinitionResultID(nextItem.DefinitionResultID)
	}
	if item.ReferenceResultID == "" {
		item = item.SetReferenceResultID(nextItem.ReferenceResultID)
	}
	if item.HoverResultID == "" {
		item = item.SetHoverResultID(nextItem.HoverResultID)
	}

	item.MonikerIDs.AddAll(nextItem.MonikerIDs)
	return item
}

// mergeNextRangeData merges the definition, reference, and hover result identifiers from nextItem
// into item when not already defined. The moniker identifiers of nextItem are unioned into the
// moniker identifiers of item.
func mergeNextRangeData(item lsif.Range, nextItem lsif.ResultSet) lsif.Range {
	if item.DefinitionResultID == "" {
		item = item.SetDefinitionResultID(nextItem.DefinitionResultID)
	}
	if item.ReferenceResultID == "" {
		item = item.SetReferenceResultID(nextItem.ReferenceResultID)
	}
	if item.HoverResultID == "" {
		item = item.SetHoverResultID(nextItem.HoverResultID)
	}

	item.MonikerIDs.AddAll(nextItem.MonikerIDs)
	return item
}

// gatherMonikers returns a new set of moniker identifiers based off the given set. The returned
// set will additionall contain the transitive closure of all moniker identifiers linked to any
// moniker identifier in the original set. This ignores adding any local-kind monikers to the new
// set.
func gatherMonikers(state *State, source datastructures.IDSet) datastructures.IDSet {
	monikers := datastructures.IDSet{}
	for sourceID := range source {
		for id := range state.LinkedMonikers.ExtractSet(sourceID) {
			if state.MonikerData[id].Kind != "local" {
				monikers.Add(id)
			}
		}
	}

	return monikers
}

// next returns the "next" identifier and result set element for the given identifier, if one exists.
func next(state *State, id string) (string, lsif.ResultSet, bool) {
	nextID, ok := state.NextData[id]
	if !ok {
		return "", lsif.ResultSet{}, false
	}

	return nextID, state.ResultSetData[nextID], true
}
