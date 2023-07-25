package stitch

import (
	"strings"

	"github.com/google/go-cmp/cmp"
	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/internal/database/migration/definition"
	"github.com/sourcegraph/sourcegraph/internal/database/migration/shared"
	"github.com/sourcegraph/sourcegraph/internal/oobmigration"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

// StitchDefinitions constructs a migration graph over time, which includes both the stitched unified
// migration graph as defined over multiple releases, as well as a mapping fom schema names to their
// root and leaf migrations so that we can later determine what portion of the graph corresponds to a
// particular release.
//
// Stitch is an undoing of squashing. We construct the migration graph by layering the definitions of
// the migrations as they're defined in each of the given git revisions. Migration definitions with the
// same identifier will be "merged" by some custom rules/edge-case logic.
//
// NOTE: This should only be used at development or build time - the root parameter should point to a
// valid git clone root directory. Resulting errors are apparent.
func StitchDefinitions(schemaName, root string, revs []string) (shared.StitchedMigration, error) {
	definitionMap, boundsByRev, err := overlayDefinitions(schemaName, root, revs)
	if err != nil {
		return shared.StitchedMigration{}, err
	}

	migrationDefinitions := make([]definition.Definition, 0, len(definitionMap))
	for _, v := range definitionMap {
		migrationDefinitions = append(migrationDefinitions, v)
	}

	definitions, err := definition.NewDefinitions(migrationDefinitions)
	if err != nil {
		return shared.StitchedMigration{}, err
	}

	return shared.StitchedMigration{
		Definitions: definitions,
		BoundsByRev: boundsByRev,
	}, nil
}

var schemaBounds = map[string]oobmigration.Version{
	"frontend":     oobmigration.NewVersion(0, 0),
	"codeintel":    oobmigration.NewVersion(3, 21),
	"codeinsights": oobmigration.NewVersion(3, 24),
}

// overlayDefinitions combines the definitions defined at all of the given git revisions for the given schema,
// then spot-rewrites portions of definitions to ensure they can be reordered to form a valid migration graph
// (as it would be defined today). The root and leaf migration identifiers for each of the given revs are also
// returned.
//
// An error is returned if the git revision's contents cannot be rewritten into a format readable by the
// current migration definition utilities. An error is also returned if migrations with the same identifier
// differ in a significant way (e.g., definitions, parents) and there is not an explicit exception to deal
// with it in this code.
func overlayDefinitions(schemaName, root string, revs []string) (map[int]definition.Definition, map[string]shared.MigrationBounds, error) {
	definitionMap := map[int]definition.Definition{}
	boundsByRev := make(map[string]shared.MigrationBounds, len(revs))
	for _, rev := range revs {
		bounds, err := overlayDefinition(schemaName, root, rev, definitionMap)
		if err != nil {
			return nil, nil, err
		}

		boundsByRev[rev] = bounds
	}

	linkVirtualPrivilegedMigrations(definitionMap)
	return definitionMap, boundsByRev, nil
}

const squashedMigrationPrefix = "squashed migrations"

// overlayDefinition reads migrations from a locally available git revision for the given schema, then
// extends the given map of definitions with migrations that have not yet been inserted.
//
// This function returns the identifiers of the migration root and leaves at this revision, which will be
// necessary to distinguish where on the graph out-of-band migration interrupt points can "rest" to wait
// for data migrations to complete.
//
// An error is returned if the git revision's contents cannot be rewritten into a format readable by the
// current migration definition utilities. An error is also returned if migrations with the same identifier
// differ in a significant way (e.g., definitions, parents) and there is not an explicit exception to deal
// with it in this code.
func overlayDefinition(schemaName, root, rev string, definitionMap map[int]definition.Definition) (shared.MigrationBounds, error) {
	revVersion, ok := oobmigration.NewVersionFromString(rev)
	if !ok {
		return shared.MigrationBounds{}, errors.Newf("illegal rev %q", rev)
	}
	firstVersionForSchema, ok := schemaBounds[schemaName]
	if !ok {
		return shared.MigrationBounds{}, errors.Newf("illegal schema %q", rev)
	}
	if oobmigration.CompareVersions(revVersion, firstVersionForSchema) != oobmigration.VersionOrderAfter {
		return shared.MigrationBounds{PreCreation: true}, nil
	}

	fs, err := ReadMigrations(schemaName, root, rev)
	if err != nil {
		return shared.MigrationBounds{}, err
	}

	pathForSchemaAtRev, err := migrationPath(schemaName, rev)
	if err != nil {
		return shared.MigrationBounds{}, err
	}
	revDefinitions, err := definition.ReadDefinitions(fs, pathForSchemaAtRev)
	if err != nil {
		return shared.MigrationBounds{}, errors.Wrap(err, "@"+rev)
	}

	for i, newDefinition := range revDefinitions.All() {
		isSquashedMigration := i <= 1

		// Enforce the assumption that (i <= 1 <-> squashed migration) by checking against the migration
		// definition's name. This should prevent situations where we read data for for some particular
		// version incorrectly.

		if isSquashedMigration && !strings.HasPrefix(newDefinition.Name, squashedMigrationPrefix) {
			return shared.MigrationBounds{}, errors.Newf(
				"expected %s migration %d@%s to have a name prefixed with %q, have %q",
				schemaName,
				newDefinition.ID,
				rev,
				squashedMigrationPrefix,
				newDefinition.Name,
			)
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

		return shared.MigrationBounds{}, errors.Newf(
			"a migration (%d) from a previous version was unexpectedly edited in this release - if this change was intentional add this migration to the allowedOverrideMap  %s:\nup.sql:\n%s\n\ndown.sql:\n%s\n",
			newDefinition.ID,
			rev,
			cmp.Diff(
				existingDefinition.UpQuery.Query(sqlf.PostgresBindVar),
				newDefinition.UpQuery.Query(sqlf.PostgresBindVar),
			),
			cmp.Diff(
				existingDefinition.DownQuery.Query(sqlf.PostgresBindVar),
				newDefinition.DownQuery.Query(sqlf.PostgresBindVar),
			),
		)
	}

	leafIDs := []int{}
	for _, migration := range revDefinitions.Leaves() {
		leafIDs = append(leafIDs, migration.ID)
	}

	return shared.MigrationBounds{RootID: revDefinitions.Root().ID, LeafIDs: leafIDs}, nil
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
	1655481894: {}, // https://github.com/sourcegraph/sourcegraph/pull/40204 - fixed down mgiration reference
	1528395786: {}, // https://github.com/sourcegraph/sourcegraph/pull/18667 - drive-by edit of empty migration
	1528395701: {}, // https://github.com/sourcegraph/sourcegraph/pull/16203 - rewritten to avoid * in select
	1528395730: {}, // https://github.com/sourcegraph/sourcegraph/pull/15972 - drops/re-created view to avoid dependencies
	1663871069: {}, // https://github.com/sourcegraph/sourcegraph/pull/43390 - fixes malformed published value
	1648628900: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098
	1652707934: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098
	1655157509: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098
	1667220626: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098
	1670934184: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098
	1674455760: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098
	1675962678: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098
	1677003167: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098
	1676420496: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098
	1677944752: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098
	1678214530: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098
	1678456448: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098

	// codeintel
	1000000020: {}, // https://github.com/sourcegraph/sourcegraph/pull/28772 - rewritten to be idempotent
	1665531314: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098
	1670365552: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098

	// codeiensights
	1000000002: {}, // https://github.com/sourcegraph/sourcegraph/pull/28713 - fixed SQL error
	1000000001: {}, // https://github.com/sourcegraph/sourcegraph/pull/30781 - removed timescsaledb
	1000000004: {}, // https://github.com/sourcegraph/sourcegraph/pull/30781 - removed timescsaledb
	1000000010: {}, // https://github.com/sourcegraph/sourcegraph/pull/30781 - removed timescsaledb
	1659572248: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098
	1672921606: {}, // https://github.com/sourcegraph/sourcegraph/pull/52098
}

func overrideAllowed(id int) bool {
	_, ok := allowedOverrideMap[id]
	return ok
}
