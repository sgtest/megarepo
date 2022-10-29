package cliutil

import (
	"context"

	"github.com/urfave/cli/v2"

	descriptions "github.com/sourcegraph/sourcegraph/internal/database/migration/schemas"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

func Drift(commandName string, factory RunnerFactory, outFactory OutputFactory, expectedSchemaFactories ...ExpectedSchemaFactory) *cli.Command {
	schemaNameFlag := &cli.StringFlag{
		Name:     "db",
		Usage:    "The target `schema` to compare.",
		Required: true,
	}
	versionFlag := &cli.StringFlag{
		Name:     "version",
		Usage:    "The target schema version. Must be resolvable as a git revlike on the Sourcegraph repository.",
		Required: false,
	}
	fileFlag := &cli.StringFlag{
		Name:     "file",
		Usage:    "The target schema description file.",
		Required: false,
	}

	action := makeAction(outFactory, func(ctx context.Context, cmd *cli.Context, out *output.Output) error {
		schemaName := schemaNameFlag.Get(cmd)
		version := versionFlag.Get(cmd)
		file := fileFlag.Get(cmd)

		if (version == "" && file == "") || (version != "" && file != "") {
			return errors.New("must supply exactly one of -version or -file")
		}

		if file != "" {
			expectedSchemaFactories = []ExpectedSchemaFactory{ExplicitFileSchemaFactory(file)}
		}
		expectedSchema, err := fetchExpectedSchema(schemaName, version, out, expectedSchemaFactories)
		if err != nil {
			return err
		}

		_, store, err := setupStore(ctx, factory, schemaName)
		if err != nil {
			return err
		}
		schemas, err := store.Describe(ctx)
		if err != nil {
			return err
		}
		schema := schemas["public"]

		return compareSchemaDescriptions(out, schemaName, version, canonicalize(schema), canonicalize(expectedSchema))
	})

	return &cli.Command{
		Name:        "drift",
		Usage:       "Detect differences between the current database schema and the expected schema",
		Description: ConstructLongHelp(),
		Action:      action,
		Flags: []cli.Flag{
			schemaNameFlag,
			versionFlag,
			fileFlag,
		},
	}
}

func fetchExpectedSchema(
	schemaName string,
	version string,
	out *output.Output,
	expectedSchemaFactories []ExpectedSchemaFactory,
) (descriptions.SchemaDescription, error) {
	filename, err := getSchemaJSONFilename(schemaName)
	if err != nil {
		return descriptions.SchemaDescription{}, err
	}

	out.WriteLine(output.Line(output.EmojiInfo, output.StyleReset, "Locating schema description"))

	for i, factory := range expectedSchemaFactories {
		name, expectedSchema, err := factory(filename, version)
		if err != nil {
			suffix := ""
			if i < len(expectedSchemaFactories)-1 {
				suffix = " Will attempt a fallback source."
			}

			out.WriteLine(output.Linef(output.EmojiInfo, output.StyleReset, "Reading schema definition from %s... Schema not found (%s).%s", name, err, suffix))
			continue
		}

		out.WriteLine(output.Linef(output.EmojiSuccess, output.StyleReset, "Schema found at %s.", name))
		return expectedSchema, nil
	}

	return descriptions.SchemaDescription{}, errors.Newf("failed to locate target schema description")
}

func canonicalize(schemaDescription descriptions.SchemaDescription) descriptions.SchemaDescription {
	descriptions.Canonicalize(schemaDescription)

	filtered := schemaDescription.Tables[:0]
	for i, table := range schemaDescription.Tables {
		if table.Name == "migration_logs" {
			continue
		}

		for j := range table.Columns {
			schemaDescription.Tables[i].Columns[j].Index = -1
		}

		filtered = append(filtered, schemaDescription.Tables[i])
	}
	schemaDescription.Tables = filtered

	return schemaDescription
}
