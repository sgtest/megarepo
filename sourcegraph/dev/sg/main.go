package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/urfave/cli/v2"

	"github.com/sourcegraph/sourcegraph/dev/sg/internal/analytics"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/secrets"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/sgconf"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/std"
	"github.com/sourcegraph/sourcegraph/dev/sg/interrupt"
	"github.com/sourcegraph/sourcegraph/dev/sg/root"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/log"
)

func main() {
	// Do not add initialization here, do all setup in sg.Before.
	if os.Args[len(os.Args)-1] == "--generate-bash-completion" {
		batchCompletionMode = true
	}
	if err := sg.RunContext(context.Background(), os.Args); err != nil {
		std.Out.WriteFailuref(err.Error())
		os.Exit(1)
	}
}

var (
	BuildCommit = "dev"

	// configFile is the path to use with sgconf.Get - it must not be used before flag
	// initialization.
	configFile string
	// configOverwriteFile is the path to use with sgconf.Get - it must not be used before
	// flag initialization.
	configOverwriteFile string

	// Global verbose mode
	verbose bool

	// postInitHooks is useful for doing anything that requires flags to be set beforehand,
	// e.g. generating help text based on parsed config, and are called before any command
	// Action is executed. These should run quickly and must fail gracefully.
	//
	// Commands can register postInitHooks in an 'init()' function that appends to this
	// slice.
	postInitHooks []func(cmd *cli.Context)

	// batchCompletionMode determines if we are in bash completion mode. In this mode,
	// sg should respond quickly, so most setup tasks (e.g. postInitHooks) are skipped.
	//
	// Do not run complicated tasks, etc. in Before or After hooks when in this mode.
	batchCompletionMode bool
)

const sgBugReportTemplate = "https://github.com/sourcegraph/sourcegraph/issues/new?template=sg_bug.md"

// sg is the main sg CLI application.
//
//go:generate go run . help -full -output ./doc/dev/background-information/sg/reference.md
var sg = &cli.App{
	Usage:       "The Sourcegraph developer tool!",
	Description: "Learn more: https://docs.sourcegraph.com/dev/background-information/sg",
	Version:     BuildCommit,
	Compiled:    time.Now(),
	Flags: []cli.Flag{
		&cli.BoolFlag{
			Name:        "verbose",
			Usage:       "toggle verbose mode",
			Aliases:     []string{"v"},
			EnvVars:     []string{"SG_VERBOSE"},
			Value:       false,
			Destination: &verbose,
		},
		&cli.StringFlag{
			Name:        "config",
			Aliases:     []string{"c"},
			Usage:       "load sg configuration from `file`",
			EnvVars:     []string{"SG_CONFIG"},
			TakesFile:   true,
			Value:       sgconf.DefaultFile,
			Destination: &configFile,
		},
		&cli.StringFlag{
			Name:        "overwrite",
			Aliases:     []string{"o"},
			Usage:       "load sg configuration from `file` that is gitignored and can be used to, for example, add credentials",
			EnvVars:     []string{"SG_OVERWRITE"},
			TakesFile:   true,
			Value:       sgconf.DefaultOverwriteFile,
			Destination: &configOverwriteFile,
		},
		&cli.BoolFlag{
			Name:    "skip-auto-update",
			Usage:   "prevent sg from automatically updating itself",
			EnvVars: []string{"SG_SKIP_AUTO_UPDATE"},
			Value:   BuildCommit == "dev", // Default to skip in dev
		},
		&cli.BoolFlag{
			Name:    "disable-analytics",
			Usage:   "disable event logging (logged to '~/.sourcegraph/events')",
			EnvVars: []string{"SG_DISABLE_ANALYTICS"},
			Value:   BuildCommit == "dev", // Default to skip in dev
		},
		&cli.BoolFlag{
			Name:    "disable-output-detection",
			Usage:   "use fixed output configuration instead of detecting terminal capabilities",
			EnvVars: []string{"SG_DISBALE_OUTPUT_DETECTION"},
		},
	},
	Before: func(cmd *cli.Context) (err error) {
		if batchCompletionMode {
			// All other setup pertains to running commands - to keep completions fast,
			// we skip all other setup.
			return nil
		}

		var (
			start                  = time.Now()
			disableAnalytics       = cmd.Bool("disable-analytics")
			disableOutputDetection = cmd.Bool("disable-output-detection")
		)

		// Let sg components register pre-interrupt hooks
		interrupt.Listen()

		// Configure global output
		if disableOutputDetection {
			std.Out = std.NewFixedOutput(cmd.App.Writer, verbose)
		} else {
			std.Out = std.NewOutput(cmd.App.Writer, verbose)
		}

		// Set up analytics and hooks for each command.
		if !disableAnalytics {
			cmd.Context = analytics.WithContext(cmd.Context, cmd.App.Version)
			addAnalyticsHooks(start, []string{"sg"}, cmd.App.Commands)

			// Lots of setup happens in Before - we want to make sure anything that
			// happens here is tracked. We set this up here after setting up output and
			// some initial safe setup.
			defer func() {
				if p := recover(); p != nil {
					std.Out.WriteWarningf("Encountered panic - please open an issue with the command output:\n\t%s",
						sgBugReportTemplate)
					message := fmt.Sprintf("%v:\n%s", p, getRelevantStack())
					err = cli.NewExitError(message, 1)

					analytics.LogEvent(cmd.Context, "sg_before", nil, start, "panic")
					analytics.Persist(cmd.Context, "sg", cmd.FlagNames())
				}
			}()
		}

		// Configure logger, for commands that use components that use loggers
		os.Setenv("SRC_DEVELOPMENT", "true")
		os.Setenv("SRC_LOG_FORMAT", "console")
		syncLogs := log.Init(log.Resource{Name: "sg"})
		interrupt.Register(func() { syncLogs() })

		// Add autosuggestion hooks to commands with subcommands but no action
		addSuggestionHooks(cmd.App.Commands)

		// Validate configuration flags, which is required for sgconf.Get to work everywhere else.
		if configFile == "" {
			return errors.Newf("--config must not be empty")
		}
		if configOverwriteFile == "" {
			return errors.Newf("--overwrite must not be empty")
		}

		// Set up access to secrets
		secretsStore, err := loadSecrets()
		if err != nil {
			std.Out.WriteWarningf("failed to open secrets: %s", err)
		} else {
			cmd.Context = secrets.WithContext(cmd.Context, secretsStore)
		}

		// We always try to set this, since we often want to watch files, start commands, etc...
		if err := setMaxOpenFiles(); err != nil {
			std.Out.WriteWarningf("Failed to set max open files: %s", err)
		}

		// Check for updates, unless we are running update manually.
		if cmd.Args().First() != "update" {
			err := checkSgVersionAndUpdate(cmd.Context, cmd.Bool("skip-auto-update"))
			if err != nil {
				std.Out.WriteWarningf("update check: %s", err)
				// Do not exit here, so we don't break user flow when they want to
				// run `sg` but updating fails
			}
		}

		// Call registered hooks last
		for _, hook := range postInitHooks {
			hook(cmd)
		}

		return nil
	},
	Commands: []*cli.Command{
		// Common dev tasks
		startCommand,
		runCommand,
		ciCommand,
		testCommand,
		lintCommand,
		generateCommand,
		dbCommand,
		migrationCommand,

		// Dev environment
		doctorCommand,
		secretCommand,
		setupCommand,

		// Company
		teammateCommand,
		rfcCommand,
		liveCommand,
		opsCommand,
		auditCommand,
		analyticsCommand,

		// Util
		helpCommand,
		versionCommand,
		updateCommand,
		installCommand,
		funkyLogoCommand,
	},
	ExitErrHandler: func(cmd *cli.Context, err error) {
		if err == nil {
			return
		}

		// Show help text only
		if errors.Is(err, flag.ErrHelp) {
			cli.ShowSubcommandHelpAndExit(cmd, 1)
		}

		// Render error
		errMsg := err.Error()
		if errMsg != "" {
			std.Out.WriteFailuref(errMsg)
		}

		// Determine exit code
		if exitErr, ok := err.(cli.ExitCoder); ok {
			os.Exit(exitErr.ExitCode())
		}
		os.Exit(1)
	},

	CommandNotFound: suggestCommands,

	EnableBashCompletion:   true,
	UseShortOptionHandling: true,

	HideVersion:     true,
	HideHelpCommand: true,
}

func loadSecrets() (*secrets.Store, error) {
	homePath, err := root.GetSGHomePath()
	if err != nil {
		return nil, err
	}
	fp := filepath.Join(homePath, secrets.DefaultFile)
	return secrets.LoadFromFile(fp)
}
