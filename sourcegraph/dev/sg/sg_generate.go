package main

import (
	"context"
	"flag"
	"fmt"
	"time"

	"github.com/urfave/cli/v2"

	"github.com/sourcegraph/sourcegraph/dev/sg/internal/generate"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/stdout"
	"github.com/sourcegraph/sourcegraph/dev/sg/root"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

var (
	generateQuiet bool
)

var generateCommand = &cli.Command{
	Name:      "generate",
	ArgsUsage: "[target]",
	Usage:     "Run code and docs generation tasks",
	Description: `Run code and docs generation tasks - if no target is provided, all target are run with default arguments.

Verbose mode can be enabled with the global verbose flag, e.g.

	sg --verbose generate ...
`,
	Category: CategoryDev,
	Flags: []cli.Flag{
		&cli.BoolFlag{
			Name:        "quiet",
			Aliases:     []string{"q"},
			Usage:       "Suppress all output but errors from generate tasks",
			Destination: &generateQuiet,
		},
	},
	Before: func(cmd *cli.Context) error {
		if verbose && generateQuiet {
			return errors.Errorf("-q and --verbose flags are exclusive")
		}
		return nil
	},
	Action: func(cmd *cli.Context) error {
		if cmd.NArg() > 0 {
			writeFailureLinef("unrecognized command %q provided", cmd.Args().First())
			return flag.ErrHelp
		}
		return allGenerateTargets.RunAll(cmd.Context)
	},
	Subcommands: allGenerateTargets.Commands(),
}

func runGenerateAndReport(ctx context.Context, t generate.Target, args []string) error {
	_, err := root.RepositoryRoot()
	if err != nil {
		return err
	}
	writeFingerPointingLinef("Running target %q (%s)", t.Name, t.Help)
	report := t.Runner(ctx, args)
	fmt.Printf(report.Output)
	writeSuccessLinef("Target %q done (%ds)", t.Name, report.Duration/time.Second)
	return nil
}

type generateTargets []generate.Target

func (gt generateTargets) RunAll(ctx context.Context) error {
	for _, t := range gt {
		if err := runGenerateAndReport(ctx, t, []string{}); err != nil {
			return errors.Wrap(err, t.Name)
		}
	}
	return nil
}

// Commands converts all lint targets to CLI commands
func (gt generateTargets) Commands() (cmds []*cli.Command) {
	actionFactory := func(c generate.Target) cli.ActionFunc {
		return func(cmd *cli.Context) error {
			_, err := root.RepositoryRoot()
			if err != nil {
				return err
			}
			report := c.Runner(cmd.Context, cmd.Args().Tail())
			fmt.Printf(report.Output)
			stdout.Out.WriteLine(output.Linef(output.EmojiSuccess, output.StyleSuccess, "(%ds)", report.Duration/time.Second))
			return nil
		}
	}
	for _, c := range gt {
		cmds = append(cmds, &cli.Command{
			Name:   c.Name,
			Usage:  c.Help,
			Action: actionFactory(c),
		})
	}
	return cmds
}
