package cliutil

import (
	"context"
	"fmt"

	"github.com/urfave/cli/v2"

	"github.com/sourcegraph/sourcegraph/internal/database/migration/runner"
	"github.com/sourcegraph/sourcegraph/internal/oobmigration"
	"github.com/sourcegraph/sourcegraph/internal/version"
	"github.com/sourcegraph/sourcegraph/internal/version/upgradestore"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

func Up(commandName string, factory RunnerFactory, outFactory OutputFactory, development bool) *cli.Command {
	schemaNamesFlag := &cli.StringSliceFlag{
		Name:  "db",
		Usage: "The target `schema(s)` to modify. Comma-separated values are accepted. Supply \"all\" to migrate all schemas.",
		Value: cli.NewStringSlice("all"),
	}
	unprivilegedOnlyFlag := &cli.BoolFlag{
		Name:  "unprivileged-only",
		Usage: "Refuse to apply privileged migrations.",
		Value: false,
	}
	noopPrivilegedFlag := &cli.BoolFlag{
		Name:  "noop-privileged",
		Usage: "Skip application of privileged migrations, but record that they have been applied. This assumes the user has already applied the required privileged migrations with elevated permissions.",
		Value: false,
	}
	ignoreSingleDirtyLogFlag := &cli.BoolFlag{
		Name:  "ignore-single-dirty-log",
		Usage: "Ignore a previously failed attempt if it will be immediately retried by this operation.",
		Value: development,
	}
	skipUpgradeValidationFlag := &cli.BoolFlag{
		Name:  "skip-upgrade-validation",
		Usage: "Do not attempt to compare the previous instance version with the target instance version for upgrade compatibility. Please refer to https://docs.sourcegraph.com/admin/updates#update-policy for our instance upgrade compatibility policy.",
		// NOTE: version 0.0.0+dev (the development version) effectively skips this check as well
		Value: development,
	}
	skipOutOfBandMigrationValidationFlag := &cli.BoolFlag{
		Name:  "skip-oobmigration-validation",
		Usage: "Do not attempt to validate the progress of out-of-band migrations.",
		// NOTE: version 0.0.0+dev (the development version) effectively skips this check as well
		Value: development,
	}

	makeOptions := func(cmd *cli.Context, out *output.Output, schemaNames []string) (runner.Options, error) {
		operations := make([]runner.MigrationOperation, 0, len(schemaNames))
		for _, schemaName := range schemaNames {
			operations = append(operations, runner.MigrationOperation{
				SchemaName: schemaName,
				Type:       runner.MigrationOperationTypeUpgrade,
			})
		}

		privilegedMode, err := getPivilegedModeFromFlags(cmd, out, unprivilegedOnlyFlag, noopPrivilegedFlag)
		if err != nil {
			return runner.Options{}, err
		}

		return runner.Options{
			Operations:           operations,
			PrivilegedMode:       privilegedMode,
			IgnoreSingleDirtyLog: ignoreSingleDirtyLogFlag.Get(cmd),
		}, nil
	}

	action := makeAction(outFactory, func(ctx context.Context, cmd *cli.Context, out *output.Output) error {
		schemaNames, err := sanitizeSchemaNames(schemaNamesFlag.Get(cmd))
		if err != nil {
			return err
		}
		if len(schemaNames) == 0 {
			return flagHelp(out, "supply a schema via -db")
		}

		r, err := setupRunner(ctx, factory, schemaNames...)
		if err != nil {
			return err
		}

		options, err := makeOptions(cmd, out, schemaNames)
		if err != nil {
			return err
		}

		db, err := extractDatabase(ctx, r)
		if err != nil {
			return err
		}
		if !skipUpgradeValidationFlag.Get(cmd) {
			if err := upgradestore.New(db).ValidateUpgrade(ctx, "frontend", version.Version()); err != nil {
				return err
			}
		}
		if !skipOutOfBandMigrationValidationFlag.Get(cmd) {
			if err := oobmigration.ValidateOutOfBandMigrationRunner(ctx, db, outOfBandMigrationRunner(db)); err != nil {
				return err
			}
		}

		if err := r.Run(ctx, options); err != nil {
			return err
		}

		// Note: we print this here because there is no output on an already-updated database
		out.WriteLine(output.Emoji(output.EmojiSuccess, "Schema(s) are up-to-date!"))
		return nil
	})

	return &cli.Command{
		Name:        "up",
		UsageText:   fmt.Sprintf("%s up [-db=<schema>]", commandName),
		Usage:       "Apply all migrations",
		Description: ConstructLongHelp(),
		Action:      action,
		Flags: []cli.Flag{
			schemaNamesFlag,
			unprivilegedOnlyFlag,
			noopPrivilegedFlag,
			ignoreSingleDirtyLogFlag,
			skipUpgradeValidationFlag,
			skipOutOfBandMigrationValidationFlag,
		},
	}
}
