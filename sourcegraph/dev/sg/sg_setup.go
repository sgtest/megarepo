package main

import (
	"fmt"
	"os"
	"runtime"

	"github.com/urfave/cli/v2"

	"github.com/sourcegraph/sourcegraph/dev/sg/dependencies"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/std"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

var setupCommand = &cli.Command{
	Name:     "setup",
	Usage:    "Validate and set up your local dev environment!",
	Category: CategoryEnv,
	Flags: []cli.Flag{
		&cli.BoolFlag{
			Name:    "check",
			Aliases: []string{"c"},
			Usage:   "Run checks and report setup state",
		},
		&cli.BoolFlag{
			Name:    "fix",
			Aliases: []string{"f"},
			Usage:   "Fix all checks",
		},
		&cli.BoolFlag{
			Name:  "oss",
			Usage: "Omit Sourcegraph-teammate-specific setup",
		},
	},
	Action: func(cmd *cli.Context) error {
		if runtime.GOOS != "linux" && runtime.GOOS != "darwin" {
			std.Out.WriteLine(output.Styled(output.StyleWarning, "'sg setup' currently only supports macOS and Linux"))
			return NewEmptyExitErr(1)
		}

		currentOS := runtime.GOOS
		if overridesOS, ok := os.LookupEnv("SG_FORCE_OS"); ok {
			currentOS = overridesOS
		}

		setup := dependencies.Setup(cmd.App.Reader, std.Out, dependencies.OS(currentOS))
		setup.AnalyticsCategory = "setup"
		setup.RenderDescription = func(out *std.Output) {
			printSgSetupWelcomeScreen(out)
			out.WriteAlertf("                INFO: You can quit any time by typing ctrl-c.\n")
		}
		setup.RunPostFixChecks = true

		args := dependencies.CheckArgs{
			Teammate:            !cmd.Bool("oss"),
			ConfigFile:          configFile,
			ConfigOverwriteFile: configOverwriteFile,
		}

		switch {
		case cmd.Bool("check"):
			err := setup.Check(cmd.Context, args)
			if err != nil {
				std.Out.WriteSuggestionf("Run 'sg setup -fix' to try and automatically fix issues!")
			}
			return err

		case cmd.Bool("fix"):
			return setup.Fix(cmd.Context, args)

		default:
			// Prompt for details if flags are not set
			if !cmd.IsSet("oss") {
				std.Out.Promptf("Are you a Sourcegraph teammate? (y/n)")
				var s string
				if _, err := fmt.Scan(&s); err != nil {
					return err
				}
				args.Teammate = s == "y"
			}
			return setup.Interactive(cmd.Context, args)
		}
	},
}

func printSgSetupWelcomeScreen(out *std.Output) {
	genLine := func(style output.Style, content string) string {
		return fmt.Sprintf("%s%s%s", output.CombineStyles(output.StyleBold, style), content, output.StyleReset)
	}

	boxContent := func(content string) string { return genLine(output.StyleWhiteOnPurple, content) }
	shadow := func(content string) string { return genLine(output.StyleGreyBackground, content) }

	out.Write(boxContent(`┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ sg ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓`))
	out.Write(boxContent(`┃            _       __     __                             __                ┃`))
	out.Write(boxContent(`┃           | |     / /__  / /________  ____ ___  ___     / /_____           ┃`) + shadow(`  `))
	out.Write(boxContent(`┃           | | /| / / _ \/ / ___/ __ \/ __ '__ \/ _ \   / __/ __ \          ┃`) + shadow(`  `))
	out.Write(boxContent(`┃           | |/ |/ /  __/ / /__/ /_/ / / / / / /  __/  / /_/ /_/ /          ┃`) + shadow(`  `))
	out.Write(boxContent(`┃           |__/|__/\___/_/\___/\____/_/ /_/ /_/\___/   \__/\____/           ┃`) + shadow(`  `))
	out.Write(boxContent(`┃                                           __              __               ┃`) + shadow(`  `))
	out.Write(boxContent(`┃                  ___________   ________  / /___  ______  / /               ┃`) + shadow(`  `))
	out.Write(boxContent(`┃                 / ___/ __  /  / ___/ _ \/ __/ / / / __ \/ /                ┃`) + shadow(`  `))
	out.Write(boxContent(`┃                (__  ) /_/ /  (__  )  __/ /_/ /_/ / /_/ /_/                 ┃`) + shadow(`  `))
	out.Write(boxContent(`┃               /____/\__, /  /____/\___/\__/\__,_/ .___(_)                  ┃`) + shadow(`  `))
	out.Write(boxContent(`┃                    /____/                      /_/                         ┃`) + shadow(`  `))
	out.Write(boxContent(`┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛`) + shadow(`  `))
	out.Write(`  ` + shadow(`                                                                              `))
	out.Write(`  ` + shadow(`                                                                              `))
}
