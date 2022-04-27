package store

import (
	"context"
	"sort"
	"strings"

	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/internal/database/migration/schemas"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func (s *Store) Describe(ctx context.Context) (_ map[string]schemas.SchemaDescription, err error) {
	ctx, _, endObservation := s.operations.describe.With(ctx, &err, observation.Args{})
	defer endObservation(1, observation.Args{})

	descriptions := map[string]schemas.SchemaDescription{}
	updateDescription := func(schemaName string, f func(description *schemas.SchemaDescription)) {
		if _, ok := descriptions[schemaName]; !ok {
			descriptions[schemaName] = schemas.SchemaDescription{}
		}

		description := descriptions[schemaName]
		f(&description)
		descriptions[schemaName] = description
	}

	extensions, err := s.listExtensions(ctx)
	if err != nil {
		return nil, errors.Wrap(err, "store.listExtensions")
	}
	for _, extension := range extensions {
		updateDescription(extension.SchemaName, func(description *schemas.SchemaDescription) {
			description.Extensions = append(description.Extensions, extension.ExtensionName)
		})
	}

	enums, err := s.listEnums(ctx)
	if err != nil {
		return nil, errors.Wrap(err, "store.listEnums")
	}
	for _, enum := range enums {
		updateDescription(enum.SchemaName, func(description *schemas.SchemaDescription) {
			for i, e := range description.Enums {
				if e.Name != enum.TypeName {
					continue
				}

				description.Enums[i].Labels = append(description.Enums[i].Labels, enum.Label)
				return
			}

			description.Enums = append(description.Enums, schemas.EnumDescription{Name: enum.TypeName, Labels: []string{enum.Label}})
		})
	}

	functions, err := s.listFunctions(ctx)
	if err != nil {
		return nil, errors.Wrap(err, "store.listFunctions")
	}
	for _, function := range functions {
		updateDescription(function.SchemaName, func(description *schemas.SchemaDescription) {
			description.Functions = append(description.Functions, schemas.FunctionDescription{
				Name:       function.FunctionName,
				Definition: function.Definition,
			})
		})
	}

	sequences, err := s.listSequences(ctx)
	if err != nil {
		return nil, errors.Wrap(err, "store.listSequences")
	}
	for _, sequence := range sequences {
		updateDescription(sequence.SchemaName, func(description *schemas.SchemaDescription) {
			description.Sequences = append(description.Sequences, schemas.SequenceDescription{
				Name:         sequence.SequenceName,
				TypeName:     sequence.DataType,
				StartValue:   sequence.StartValue,
				MinimumValue: sequence.MinimumValue,
				MaximumValue: sequence.MaximumValue,
				Increment:    sequence.Increment,
				CycleOption:  sequence.CycleOption,
			})
		})
	}

	tableMap := map[string]map[string]schemas.TableDescription{}
	updateTableMap := func(schemaName, tableName string, f func(table *schemas.TableDescription)) {
		if _, ok := tableMap[schemaName]; !ok {
			tableMap[schemaName] = map[string]schemas.TableDescription{}
		}

		if _, ok := tableMap[schemaName][tableName]; !ok {
			tableMap[schemaName][tableName] = schemas.TableDescription{
				Columns:  []schemas.ColumnDescription{},
				Indexes:  []schemas.IndexDescription{},
				Triggers: []schemas.TriggerDescription{},
			}
		}

		table := tableMap[schemaName][tableName]
		f(&table)
		tableMap[schemaName][tableName] = table
	}

	tables, err := s.listTables(ctx)
	if err != nil {
		return nil, errors.Wrap(err, "store.listTables")
	}
	for _, table := range tables {
		updateTableMap(table.SchemaName, table.TableName, func(t *schemas.TableDescription) {
			t.Name = table.TableName
			t.Comment = table.Comment
		})
	}

	columns, err := s.listColumns(ctx)
	if err != nil {
		return nil, errors.Wrap(err, "store.listColumns")
	}
	for _, column := range columns {
		updateTableMap(column.SchemaName, column.TableName, func(table *schemas.TableDescription) {
			table.Columns = append(table.Columns, schemas.ColumnDescription{
				Name:                   column.ColumnName,
				Index:                  column.Index,
				TypeName:               column.DataType,
				IsNullable:             column.IsNullable,
				Default:                column.Default,
				CharacterMaximumLength: column.CharacterMaximumLength,
				IsIdentity:             column.IsIdentity,
				IdentityGeneration:     column.IdentityGeneration,
				IsGenerated:            column.IsGenerated,
				GenerationExpression:   column.GenerationExpression,
				Comment:                column.Comment,
			})
		})
	}

	indexes, err := s.listIndexes(ctx)
	if err != nil {
		return nil, errors.Wrap(err, "store.listIndexes")
	}
	for _, index := range indexes {
		updateTableMap(index.SchemaName, index.TableName, func(table *schemas.TableDescription) {
			table.Indexes = append(table.Indexes, schemas.IndexDescription{
				Name:                 index.IndexName,
				IsPrimaryKey:         index.IsPrimaryKey,
				IsUnique:             index.IsUnique,
				IsExclusion:          index.IsExclusion,
				IsDeferrable:         index.IsDeferrable,
				IndexDefinition:      index.IndexDefinition,
				ConstraintType:       index.ConstraintType,
				ConstraintDefinition: index.ConstraintDefinition,
			})
		})
	}

	constraints, err := s.listConstraints(ctx)
	if err != nil {
		return nil, errors.Wrap(err, "store.listConstraints")
	}
	for _, constraint := range constraints {
		updateTableMap(constraint.SchemaName, constraint.TableName, func(table *schemas.TableDescription) {
			table.Constraints = append(table.Constraints, schemas.ConstraintDescription{
				Name:                 constraint.ConstraintName,
				ConstraintType:       constraint.ConstraintType,
				IsDeferrable:         constraint.IsDeferrable,
				RefTableName:         constraint.RefTableName,
				ConstraintDefinition: constraint.ConstraintDefinition,
			})
		})
	}

	triggers, err := s.listTriggers(ctx)
	if err != nil {
		return nil, errors.Wrap(err, "store.listTriggers")
	}
	for _, trigger := range triggers {
		updateTableMap(trigger.SchemaName, trigger.TableName, func(table *schemas.TableDescription) {
			table.Triggers = append(table.Triggers, schemas.TriggerDescription{
				Name:       trigger.TriggerName,
				Definition: trigger.TriggerDefinition,
			})
		})
	}

	for schemaName, tables := range tableMap {
		tableNames := make([]string, 0, len(tables))
		for tableName := range tables {
			tableNames = append(tableNames, tableName)
		}
		sort.Strings(tableNames)

		for _, tableName := range tableNames {
			updateDescription(schemaName, func(description *schemas.SchemaDescription) {
				description.Tables = append(description.Tables, tables[tableName])
			})
		}
	}

	views, err := s.listViews(ctx)
	if err != nil {
		return nil, errors.Wrap(err, "store.listViews")
	}
	for _, view := range views {
		updateDescription(view.SchemaName, func(description *schemas.SchemaDescription) {
			description.Views = append(description.Views, schemas.ViewDescription{
				Name:       view.ViewName,
				Definition: view.Definition,
			})
		})
	}

	return descriptions, nil
}

func (s *Store) listExtensions(ctx context.Context) ([]Extension, error) {
	return scanExtensions(s.Query(ctx, sqlf.Sprintf(listExtensionsQuery)))
}

const listExtensionsQuery = `
-- source: internal/database/migration/store/store.go:listExtensions
SELECT
	n.nspname AS schemaName,
	e.extname AS extensionName
FROM pg_catalog.pg_extension e
JOIN pg_catalog.pg_namespace n ON n.oid = e.extnamespace
WHERE
	n.nspname NOT LIKE 'pg_%%' AND
	n.nspname != 'information_schema'
ORDER BY
	n.nspname,
	e.extname
`

func (s *Store) listEnums(ctx context.Context) ([]enum, error) {
	return scanEnums(s.Query(ctx, sqlf.Sprintf(listEnumQuery)))
}

const listEnumQuery = `
-- source: internal/database/migration/store/store.go:listEnums
SELECT
	n.nspname AS schemaName,
	t.typname AS typeName,
	e.enumlabel AS label
FROM pg_catalog.pg_enum e
JOIN pg_catalog.pg_type t ON t.oid = e.enumtypid
JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
WHERE
	n.nspname NOT LIKE 'pg_%%' AND
	n.nspname != 'information_schema'
ORDER BY
	n.nspname,
	t.typname,
	e.enumsortorder
`

func (s *Store) listFunctions(ctx context.Context) ([]function, error) {
	return scanFunctions(s.Query(ctx, sqlf.Sprintf(listFunctionsQuery)))
}

const listFunctionsQuery = `
-- source: internal/database/migration/store/store.go:listFunctions
SELECT
	n.nspname AS schemaName,
	p.proname AS functionName,
	p.oid::regprocedure AS fancy,
	t.typname AS returnType,
	pg_get_functiondef(p.oid) AS definition
FROM pg_catalog.pg_proc p
JOIN pg_catalog.pg_type t ON t.oid = p.prorettype
JOIN pg_catalog.pg_namespace n ON n.oid = p.pronamespace
JOIN pg_language l ON (
	l.oid = p.prolang AND l.lanname IN ('sql', 'plpgsql', 'c')
)
WHERE
	n.nspname NOT LIKE 'pg_%%' AND
	n.nspname != 'information_schema'
ORDER BY
	n.nspname,
	p.proname
`

func (s *Store) listSequences(ctx context.Context) ([]sequence, error) {
	return scanSequences(s.Query(ctx, sqlf.Sprintf(listSequencesQuery)))
}

const listSequencesQuery = `
-- source: internal/database/migration/store/store.go:listSequences
SELECT
	s.sequence_schema AS schemaName,
	s.sequence_name AS sequenceName,
	s.data_type AS dataType,
	s.start_value AS startValue,
	s.minimum_value AS minimumValue,
	s.maximum_value AS maximumValue,
	s.increment AS increment,
	s.cycle_option AS cycleOption
FROM information_schema.sequences s
WHERE
	s.sequence_schema NOT LIKE 'pg_%%' AND
	s.sequence_schema != 'information_schema'
ORDER BY
	s.sequence_schema,
	s.sequence_name
`

func (s *Store) listTables(ctx context.Context) ([]table, error) {
	return scanTables(s.Query(ctx, sqlf.Sprintf(listTablesQuery)))
}

const listTablesQuery = `
-- source: internal/database/migration/store/store.go:listTables
SELECT
	t.table_schema AS schemaName,
	t.table_name AS tableName,
	obj_description(t.table_name::regclass) AS comment
FROM information_schema.tables t
WHERE
	t.table_type = 'BASE TABLE' AND
	t.table_schema NOT LIKE 'pg_%%' AND
	t.table_schema != 'information_schema'
ORDER BY
	t.table_schema,
	t.table_name
`

func (s *Store) listColumns(ctx context.Context) ([]column, error) {
	return scanColumns(s.Query(ctx, sqlf.Sprintf(listColumnsQuery)))
}

const listColumnsQuery = `
-- source: internal/database/migration/store/store.go:listColumns
WITH tables AS (
	SELECT
		t.table_schema,
		t.table_name
	FROM information_schema.tables t
	WHERE
		t.table_type = 'BASE TABLE' AND
		t.table_schema NOT LIKE 'pg_%%' AND
		t.table_schema != 'information_schema'
)
SELECT
	c.table_schema AS schemaName,
	c.table_name AS tableName,
	c.column_name AS columnName,
	c.ordinal_position AS index,
	CASE
		WHEN c.data_type = 'ARRAY'           THEN e.data_type || '[]'
		WHEN c.data_type = 'USER-DEFINED'    THEN c.udt_name
		WHEN c.character_maximum_length != 0 THEN c.data_type || '(' || c.character_maximum_length::text || ')'
		ELSE c.data_type
	END as dataType,
	c.is_nullable AS isNullable,
	c.column_default AS columnDefault,
	c.character_maximum_length AS characterMaximumLength,
	c.is_identity AS isIdentity,
	c.identity_generation AS identityGeneration,
	c.is_generated AS isGenerated,
	c.generation_expression AS generationExpression,
	pg_catalog.col_description(c.table_name::regclass::oid, c.ordinal_position::int) AS comment
FROM information_schema.columns c
LEFT JOIN information_schema.element_types e ON
	(c.table_catalog,  c.table_schema,  c.table_name, 'TABLE',        c.dtd_identifier) =
	(e.object_catalog, e.object_schema, e.object_name, e.object_type, e.collection_type_identifier)
WHERE (c.table_schema, c.table_name) IN (SELECT table_schema, table_name FROM tables)
ORDER BY
	c.table_schema,
	c.table_name,
	c.column_name
`

func (s *Store) listIndexes(ctx context.Context) ([]index, error) {
	return scanIndexes(s.Query(ctx, sqlf.Sprintf(listIndexesQuery)))
}

const listIndexesQuery = `
-- source: internal/database/migration/store/store.go:listIndexes
SELECT
	n.nspname AS schemaName,
	table_class.relname AS tableName,
	index_class.relname AS indexName,
	i.indisprimary AS isPrimaryKey,
	i.indisunique AS isUnique,
	i.indisexclusion AS isExclusion,
	con.condeferrable AS isDeferrable,
	pg_catalog.pg_get_indexdef(i.indexrelid, 0, true) AS indexDefinition,
	con.contype AS constraintType,
	pg_catalog.pg_get_constraintdef(con.oid, true) AS constraintDefinition
FROM pg_catalog.pg_index i
JOIN pg_catalog.pg_class table_class ON table_class.oid = i.indrelid
JOIN pg_catalog.pg_class index_class ON index_class.oid = i.indexrelid
JOIN pg_catalog.pg_namespace n ON n.oid = table_class.relnamespace
LEFT OUTER JOIN pg_catalog.pg_constraint con ON (
	con.conrelid = i.indrelid AND
	con.conindid = i.indexrelid AND
	con.contype IN ('p', 'u', 'x')
)
WHERE
	n.nspname NOT LIKE 'pg_%%' AND
	n.nspname != 'information_schema'
ORDER BY
	n.nspname,
	table_class.relname,
	index_class.relname
`

func (s *Store) listConstraints(ctx context.Context) ([]constraint, error) {
	return scanConstraints(s.Query(ctx, sqlf.Sprintf(listConstraintsQuery)))
}

const listConstraintsQuery = `
-- source: internal/database/migration/store/store.go:listConstraints
SELECT
	n.nspname AS schemaName,
	table_class.relname AS tableName,
	con.conname AS constraintName,
	con.contype AS constraintType,
	con.condeferrable AS isDeferrable,
	reftable_class.relname AS refTableName,
	pg_catalog.pg_get_constraintdef(con.oid, true) AS constraintDefintion
FROM pg_catalog.pg_constraint con
JOIN pg_catalog.pg_class table_class ON table_class.oid = con.conrelid
JOIN pg_catalog.pg_namespace n ON n.oid = table_class.relnamespace
LEFT OUTER JOIN pg_catalog.pg_class reftable_class ON reftable_class.oid = con.confrelid
WHERE
	n.nspname NOT LIKE 'pg_%%' AND
	n.nspname != 'information_schema' AND
	con.contype IN ('c', 'f', 't')
ORDER BY
	n.nspname,
	table_class.relname,
	con.conname
`

func (s *Store) listTriggers(ctx context.Context) ([]trigger, error) {
	return scanTriggers(s.Query(ctx, sqlf.Sprintf(listTriggersQuery)))
}

const listTriggersQuery = `
-- source: internal/database/migration/store/store.go:listTriggers
SELECT
	n.nspname AS schemaName,
	c.relname AS tableName,
	t.tgname AS triggerName,
	pg_catalog.pg_get_triggerdef(t.oid, true) AS triggerDefinition
FROM pg_catalog.pg_trigger t
JOIN pg_catalog.pg_class c ON c.oid = t.tgrelid
JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
WHERE
	n.nspname NOT LIKE 'pg_%%' AND
	n.nspname != 'information_schema' AND
	NOT t.tgisinternal
ORDER BY
	n.nspname,
	c.relname,
	t.tgname
`

func (s *Store) listViews(ctx context.Context) ([]view, error) {
	return scanViews(s.Query(ctx, sqlf.Sprintf(listViewsQuery)))
}

const listViewsQuery = `
-- source: internal/database/migration/store/store.go:listViews
SELECT
	v.schemaname AS schemaName,
	v.viewname AS viewName,
	v.definition AS definition
FROM pg_catalog.pg_views v
WHERE
	v.schemaname NOT LIKE 'pg_%%' AND
	v.schemaname != 'information_schema' AND
	v.viewname NOT LIKE 'pg_stat_%%'
ORDER BY
	v.schemaname,
	v.viewname
`

// isTruthy covers both truthy strings and the SQL spec YES_NO values in a
// unified way.
func isTruthy(catalogValue string) bool {
	lower := strings.ToLower(catalogValue)
	return lower == "yes" || lower == "true"
}
