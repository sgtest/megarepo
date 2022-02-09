package cliutil

import (
	"context"
	"flag"
	"fmt"
	"strconv"
	"strings"

	"github.com/peterbourgon/ff/v3/ffcli"

	"github.com/sourcegraph/sourcegraph/internal/database/migration/runner"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

func UpTo(commandName string, factory RunnerFactory, out *output.Output) *ffcli.Command {
	var (
		flagSet        = flag.NewFlagSet(fmt.Sprintf("%s upto", commandName), flag.ExitOnError)
		schemaNameFlag = flagSet.String("db", "", `The target schema to modify.`)
		targetsFlag    = flagSet.String("target", "", "The migration to apply. Comma-separated values are accepted.")
	)

	exec := func(ctx context.Context, args []string) error {
		if len(args) != 0 {
			out.WriteLine(output.Linef("", output.StyleWarning, "ERROR: too many arguments"))
			return flag.ErrHelp
		}

		if *schemaNameFlag == "" {
			out.WriteLine(output.Linef("", output.StyleWarning, "ERROR: supply a schema via -db"))
			return flag.ErrHelp
		}

		targets := strings.Split(*targetsFlag, ",")
		if len(targets) == 0 {
			out.WriteLine(output.Linef("", output.StyleWarning, "ERROR: supply a migration target via -target"))
			return flag.ErrHelp
		}

		versions := make([]int, 0, len(targets))
		for _, target := range targets {
			version, err := strconv.Atoi(target)
			if err != nil {
				return err
			}

			versions = append(versions, version)
		}

		r, err := factory(ctx, []string{*schemaNameFlag})
		if err != nil {
			return err
		}

		return r.Run(ctx, runner.Options{
			Operations: []runner.MigrationOperation{
				{
					SchemaName:     *schemaNameFlag,
					Type:           runner.MigrationOperationTypeTargetedUp,
					TargetVersions: versions,
				},
			},
		})
	}

	return &ffcli.Command{
		Name:       "upto",
		ShortUsage: fmt.Sprintf("%s upto -db=<schema> -target=<target>,<target>,...", commandName),
		ShortHelp:  "Ensure a given migration has been applied - may apply dependency migrations",
		FlagSet:    flagSet,
		Exec:       exec,
		LongHelp:   ConstructLongHelp(),
	}
}
