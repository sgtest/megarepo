package cliutil

import (
	"context"

	"github.com/urfave/cli/v2"

	"github.com/sourcegraph/sourcegraph/internal/oobmigration"
	"github.com/sourcegraph/sourcegraph/internal/oobmigration/migrations"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

func Upgrade(
	commandName string,
	runnerFactory RunnerFactoryWithSchemas,
	outFactory OutputFactory,
	registerMigrators func(storeFactory migrations.StoreFactory) oobmigration.RegisterMigratorsFunc,
	expectedSchemaFactories ...ExpectedSchemaFactory,
) *cli.Command {
	fromFlag := &cli.StringFlag{
		Name:     "from",
		Usage:    "The source (current) instance version. Must be of the form `{Major}.{Minor}` or `v{Major}.{Minor}`.",
		Required: true,
	}
	toFlag := &cli.StringFlag{
		Name:     "to",
		Usage:    "The target instance version. Must be of the form `{Major}.{Minor}` or `v{Major}.{Minor}`.",
		Required: true,
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
	privilegedHashesFlag := &cli.StringSliceFlag{
		Name:  "privileged-hash",
		Usage: "Running --noop-privileged without this flag will print instructions and supply a value for use in a second invocation. Multiple privileged hash flags (for distinct schemas) may be supplied. Future (distinct) upgrade operations will require a unique hash.",
		Value: nil,
	}
	skipVersionCheckFlag := &cli.BoolFlag{
		Name:     "skip-version-check",
		Usage:    "Skip validation of the instance's current version.",
		Required: false,
	}
	skipDriftCheckFlag := &cli.BoolFlag{
		Name:     "skip-drift-check",
		Usage:    "Skip comparison of the instance's current schema against the expected version's schema.",
		Required: false,
	}
	dryRunFlag := &cli.BoolFlag{
		Name:     "dry-run",
		Usage:    "Print the upgrade plan but do not execute it.",
		Required: false,
	}
	disableAnimation := &cli.BoolFlag{
		Name:     "disable-animation",
		Usage:    "If set, progress bar animations are not displayed.",
		Required: false,
	}

	action := makeAction(outFactory, func(ctx context.Context, cmd *cli.Context, out *output.Output) error {
		from, ok := oobmigration.NewVersionFromString(fromFlag.Get(cmd))
		if !ok {
			return errors.New("bad format for -from")
		}
		to, ok := oobmigration.NewVersionFromString(toFlag.Get(cmd))
		if !ok {
			return errors.New("bad format for -to")
		}
		if oobmigration.CompareVersions(from, to) != oobmigration.VersionOrderBefore {
			return errors.Newf("invalid range (from=%s >= to=%s)", from, to)
		}

		// Construct inclusive upgrade range (with knowledge of major version changes)
		versionRange, err := oobmigration.UpgradeRange(from, to)
		if err != nil {
			return err
		}

		// Determine the set of versions that need to have out of band migrations completed
		// prior to a subsequent instance upgrade. We'll "pause" the migration at these points
		// and run the out of band migration routines to completion.
		interrupts, err := oobmigration.ScheduleMigrationInterrupts(from, to)
		if err != nil {
			return err
		}

		// Find the relevant schema and data migrations to perform (and in what order)
		// for the given version range.
		plan, err := planMigration(from, to, versionRange, interrupts)
		if err != nil {
			return err
		}

		privilegedMode, err := getPivilegedModeFromFlags(cmd, out, unprivilegedOnlyFlag, noopPrivilegedFlag)
		if err != nil {
			return err
		}

		// Perform the upgrade on the configured databases.
		return runMigration(
			ctx,
			runnerFactory,
			plan,
			privilegedMode,
			privilegedHashesFlag.Get(cmd),
			skipVersionCheckFlag.Get(cmd),
			skipDriftCheckFlag.Get(cmd),
			dryRunFlag.Get(cmd),
			true, // up
			!disableAnimation.Get(cmd),
			registerMigrators,
			expectedSchemaFactories,
			out,
		)
	})

	return &cli.Command{
		Name:        "upgrade",
		Usage:       "Upgrade Sourcegraph instance databases to a target version",
		Description: "",
		Action:      action,
		Flags: []cli.Flag{
			fromFlag,
			toFlag,
			unprivilegedOnlyFlag,
			noopPrivilegedFlag,
			privilegedHashesFlag,
			skipVersionCheckFlag,
			skipDriftCheckFlag,
			dryRunFlag,
			disableAnimation,
		},
	}
}
