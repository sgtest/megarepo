package cliutil

import (
	"context"
	"fmt"

	"github.com/urfave/cli/v2"

	"github.com/sourcegraph/sourcegraph/internal/database/migration/runner"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

func UpTo(commandName string, factory RunnerFactory, outFactory OutputFactory, development bool) *cli.Command {
	schemaNameFlag := &cli.StringFlag{
		Name:     "db",
		Usage:    "The target `schema` to modify.",
		Required: true,
	}
	targetFlag := &cli.StringSliceFlag{
		Name:     "target",
		Usage:    "The `migration` to apply. Comma-separated values are accepted.",
		Required: true,
	}
	unprivilegedOnlyFlag := &cli.BoolFlag{
		Name:  "unprivileged-only",
		Usage: "Do not apply privileged migrations.",
		Value: false,
	}
	ignoreSingleDirtyLogFlag := &cli.BoolFlag{
		Name:  "ignore-single-dirty-log",
		Usage: "Ignore a previously failed attempt if it will be immediately retried by this operation.",
		Value: development,
	}

	makeOptions := func(cmd *cli.Context, versions []int) runner.Options {
		return runner.Options{
			Operations: []runner.MigrationOperation{
				{
					SchemaName:     schemaNameFlag.Get(cmd),
					Type:           runner.MigrationOperationTypeTargetedUp,
					TargetVersions: versions,
				},
			},
			UnprivilegedOnly:     unprivilegedOnlyFlag.Get(cmd),
			IgnoreSingleDirtyLog: ignoreSingleDirtyLogFlag.Get(cmd),
		}
	}

	action := makeAction(outFactory, func(ctx context.Context, cmd *cli.Context, out *output.Output) error {
		versions, err := parseTargets(targetFlag.Get(cmd))
		if err != nil {
			return err
		}
		if len(versions) == 0 {
			return flagHelp(out, "supply a target via -target")
		}

		r, err := setupRunner(ctx, factory, schemaNameFlag.Get(cmd))
		if err != nil {
			return err
		}
		return r.Run(ctx, makeOptions(cmd, versions))
	})

	return &cli.Command{
		Name:        "upto",
		UsageText:   fmt.Sprintf("%s upto -db=<schema> -target=<target>,<target>,...", commandName),
		Usage:       "Ensure a given migration has been applied - may apply dependency migrations",
		Description: ConstructLongHelp(),
		Action:      action,
		Flags: []cli.Flag{
			schemaNameFlag,
			targetFlag,
			unprivilegedOnlyFlag,
			ignoreSingleDirtyLogFlag,
		},
	}
}
