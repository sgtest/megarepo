package cliutil

import (
	"context"

	"github.com/urfave/cli/v2"

	descriptions "github.com/sourcegraph/sourcegraph/internal/database/migration/schemas"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

func Drift(commandName string, factory RunnerFactory, outFactory func() *output.Output) *cli.Command {
	schemaNameFlag := &cli.StringFlag{
		Name:     "db",
		Usage:    "The target `schema` to compare.",
		Required: true,
	}
	versionFlag := &cli.StringFlag{
		Name:     "version",
		Usage:    "The target schema version. Must be resolvable as a git revlike on the sourcegraph repository.",
		Required: true,
	}

	action := makeAction(outFactory, func(ctx context.Context, cmd *cli.Context, out *output.Output) error {
		_, store, err := setupStore(ctx, factory, schemaNameFlag.Get(cmd))
		if err != nil {
			return err
		}

		schemas, err := store.Describe(ctx)
		if err != nil {
			return err
		}
		schema := schemas["public"]

		expected, err := fetchSchema(schemaNameFlag.Get(cmd), versionFlag.Get(cmd))
		if err != nil {
			return err
		}

		canonicalize(schema)
		canonicalize(expected)

		return compareSchemaDescriptions(out, schema, expected)
	})

	return &cli.Command{
		Name:        "drift",
		Usage:       "Detect differences between the current database schema and the expected schema",
		Description: ConstructLongHelp(),
		Action:      action,
		Flags: []cli.Flag{
			schemaNameFlag,
			versionFlag,
		},
	}
}

func canonicalize(schema descriptions.SchemaDescription) {
	descriptions.Canonicalize(schema)

	for i, table := range schema.Tables {
		for j := range table.Columns {
			schema.Tables[i].Columns[j].Index = -1
		}
	}
}
