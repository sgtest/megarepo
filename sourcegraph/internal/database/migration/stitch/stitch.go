package stitch

import (
	"strings"

	"github.com/google/go-cmp/cmp"
	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/internal/database/migration/definition"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// StitchDefinitions constructs the migration graph of the given schema over time.
//
// Stitch is an undoing of squashing. We construct the migration graph by layering the definitions of the
// migrations as they're defined in each of the given git revisions. Migration definitions with the same
// identifier will be "merged" by some custom rules/edge-case logic.
//
// NOTE: This should only be used at development or build time - the root parameter should point to a
// valid git clone root directory. Resulting errors are apparent.
func StitchDefinitions(schemaName, root string, revs []string) (*definition.Definitions, error) {
	definitionMap, err := overlayDefinitions(schemaName, root, revs)
	if err != nil {
		return nil, err
	}

	migrationDefinitions := make([]definition.Definition, 0, len(definitionMap))
	for _, v := range definitionMap {
		migrationDefinitions = append(migrationDefinitions, v)
	}

	return definition.NewDefinitions(migrationDefinitions)
}

// overlayDefinitions combines the definitions defined at all of the given git revisions for the given schema,
// then spot-rewrites portions of definitions to ensure they can be reordered to form a valid migration graph
// (as it would be defined today).
//
// An error is returned if the git revision's contents cannot be rewritten into a format readable by the
// current migration definition utilities. An error is also returned if migrations with the same identifier
// differ in a significant way (e.g., definitions, parents) and there is not an explicit exception to deal
// with it in this code.
func overlayDefinitions(schemaName, root string, revs []string) (map[int]definition.Definition, error) {
	definitionMap := map[int]definition.Definition{}
	for _, rev := range revs {
		if err := overlayDefinition(schemaName, root, rev, definitionMap); err != nil {
			return nil, err
		}
	}

	linkVirtualPrivilegedMigrations(definitionMap)
	return definitionMap, nil
}

const squashedMigrationPrefix = "squashed migrations"

// overlayDefinition reads migrations from a locally available git revision for the given schema, then
// extends the given map of definitions with migrations that have not yet been inserted.
//
// An error is returned if the git revision's contents cannot be rewritten into a format readable by the
// current migration definition utilities. An error is also returned if migrations with the same identifier
// differ in a significant way (e.g., definitions, parents) and there is not an explicit exception to deal
// with it in this code.
func overlayDefinition(schemaName, root, rev string, definitionMap map[int]definition.Definition) error {
	fs, err := readMigrations(schemaName, root, rev)
	if err != nil {
		return err
	}

	revDefinitions, err := definition.ReadDefinitions(fs, migrationPath(schemaName))
	if err != nil {
		return errors.Wrap(err, "@"+rev)
	}

	for i, newDefinition := range revDefinitions.All() {
		isSquashedMigration := i <= 1

		// Enforce the assumption that (i <= 1 <-> squashed migration) by checking against the migration
		// definition's name. This should prevent situations where we read data for for some particular
		// version incorrectly.

		if isSquashedMigration && !strings.HasPrefix(newDefinition.Name, squashedMigrationPrefix) {
			return errors.Newf("expected migration %d@%s to have a name prefixed with %q", newDefinition.ID, rev, squashedMigrationPrefix)
		}

		existingDefinition, ok := definitionMap[newDefinition.ID]
		if !ok {
			// New file, no clash
			definitionMap[newDefinition.ID] = newDefinition
			continue
		}
		if isSquashedMigration || areEqualDefinitions(newDefinition, existingDefinition) {
			// Existing file, but identical definitions, or
			// Existing file, but squashed in newer version (do not ovewrite)
			continue
		}
		if overrideAllowed(newDefinition.ID) {
			// Explicitly accepted overwrite in newer version
			definitionMap[newDefinition.ID] = newDefinition
			continue
		}

		return errors.Newf("migration %d unexpectedly edited in release %s", newDefinition.ID, rev)
	}

	return nil
}

func areEqualDefinitions(x, y definition.Definition) bool {
	// Names can be different (we parsed names from filepaths and manually humanized them)
	x.Name = y.Name

	return cmp.Diff(x, y, cmp.Comparer(func(x, y *sqlf.Query) bool {
		// Note: migrations do not have args to compare here, so we can compare only
		// the query text safely. If we ever need to add runtime arguments to the
		// migration runner, this assumption _might_ change.
		return x.Query(sqlf.PostgresBindVar) == y.Query(sqlf.PostgresBindVar)
	})) == ""
}

var allowedOverrideMap = map[int]struct{}{
	// frontend
	1528395798: {}, // https://github.com/sourcegraph/sourcegraph/pull/21092 - fixes bad view definition
	1528395836: {}, // https://github.com/sourcegraph/sourcegraph/pull/21092 - fixes bad view definition
	1528395851: {}, // https://github.com/sourcegraph/sourcegraph/pull/29352 - fixes bad view definition

	1528395840: {}, // https://github.com/sourcegraph/sourcegraph/pull/23622 - performance issues
	1528395841: {}, // https://github.com/sourcegraph/sourcegraph/pull/23622 - performance issues
	1528395963: {}, // https://github.com/sourcegraph/sourcegraph/pull/29395 - adds a truncation statement

	1528395869: {}, // https://github.com/sourcegraph/sourcegraph/pull/24807 - adds missing COMMIT;
	1528395880: {}, // https://github.com/sourcegraph/sourcegraph/pull/28772 - rewritten to be idempotent
	1528395955: {}, // https://github.com/sourcegraph/sourcegraph/pull/31656 - rewritten to be idempotent
	1528395959: {}, // https://github.com/sourcegraph/sourcegraph/pull/31656 - rewritten to be idempotent
	1528395965: {}, // https://github.com/sourcegraph/sourcegraph/pull/31656 - rewritten to be idempotent
	1528395970: {}, // https://github.com/sourcegraph/sourcegraph/pull/31656 - rewritten to be idempotent
	1528395971: {}, // https://github.com/sourcegraph/sourcegraph/pull/31656 - rewritten to be idempotent
	1644515056: {}, // https://github.com/sourcegraph/sourcegraph/pull/31656 - rewritten to be idempotent
	1645554732: {}, // https://github.com/sourcegraph/sourcegraph/pull/31656 - rewritten to be idempotent

	// codeintel
	1000000020: {}, // https://github.com/sourcegraph/sourcegraph/pull/28772 - rewritten to be idempotent

	// codeiensights
	1000000002: {}, // https://github.com/sourcegraph/sourcegraph/pull/28713 - fixed SQL error
	1000000001: {}, // https://github.com/sourcegraph/sourcegraph/pull/30781 - removed timescsaledb
	1000000004: {}, // https://github.com/sourcegraph/sourcegraph/pull/30781 - removed timescsaledb
	1000000010: {}, // https://github.com/sourcegraph/sourcegraph/pull/30781 - removed timescsaledb
}

func overrideAllowed(id int) bool {
	_, ok := allowedOverrideMap[id]
	return ok
}
