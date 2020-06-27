package correlation

import (
	"context"

	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/correlation/datastructures"
	"github.com/sourcegraph/sourcegraph/enterprise/cmd/precise-code-intel-worker/internal/existence"
)

// prune removes references to documents in the given correlation state that do not exist in
// the git clone at the target commit. This is a necessary step as documents not in git will
// not be the source of any queries (and take up unnecessary space in the converted index),
// and may be the target of a definition or reference (and references a file we do not have).
func prune(ctx context.Context, state *State, root string, getChildren existence.GetChildrenFunc) error {
	paths := make([]string, 0, len(state.DocumentData))
	for _, doc := range state.DocumentData {
		paths = append(paths, doc.URI)
	}

	checker, err := existence.NewExistenceChecker(ctx, root, paths, getChildren)
	if err != nil {
		return err
	}

	for documentID, doc := range state.DocumentData {
		if !checker.Exists(doc.URI) {
			// Document does not exist in git
			delete(state.DocumentData, documentID)
		}
	}

	pruneFromDefinitionReferences(state, state.DefinitionData)
	pruneFromDefinitionReferences(state, state.ReferenceData)
	return nil
}

func pruneFromDefinitionReferences(state *State, definitionReferenceData map[string]datastructures.DefaultIDSetMap) {
	for _, documentRanges := range definitionReferenceData {
		for documentID := range documentRanges {
			if _, ok := state.DocumentData[documentID]; !ok {
				// Document was pruned, remove reference
				delete(documentRanges, documentID)
			}
		}
	}
}
