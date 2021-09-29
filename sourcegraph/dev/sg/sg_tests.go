package main

import (
	"context"
	"flag"
	"os"

	"github.com/peterbourgon/ff/v3/ffcli"

	"github.com/sourcegraph/sourcegraph/dev/sg/internal/run"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

var (
	testFlagSet = flag.NewFlagSet("sg test", flag.ExitOnError)
	testCommand = &ffcli.Command{
		Name:       "test",
		ShortUsage: "sg test <testsuite>",
		ShortHelp:  "Run the given test suite.",
		LongHelp:   "Run the given test suite.",
		FlagSet:    testFlagSet,
		Exec:       testExec,
	}
)

func testExec(ctx context.Context, args []string) error {
	ok, errLine := parseConf(*configFlag, *overwriteConfigFlag)
	if !ok {
		out.WriteLine(errLine)
		os.Exit(1)
	}

	if len(args) == 0 {
		out.WriteLine(output.Linef("", output.StyleWarning, "No test suite specified\n"))
		return flag.ErrHelp
	}

	cmd, ok := globalConf.Tests[args[0]]
	if !ok {
		out.WriteLine(output.Linef("", output.StyleWarning, "ERROR: test suite %q not found :(\n", args[0]))
		return flag.ErrHelp
	}

	return run.Test(ctx, cmd, args[1:], globalConf.Env)
}
