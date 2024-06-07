package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"time"

	hashstructure "github.com/mitchellh/hashstructure/v2"
	"github.com/sourcegraph/log"
	"github.com/urfave/cli/v2"

	"github.com/sourcegraph/sourcegraph/dev/sg/ci"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/analytics"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/background"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/check"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/release"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/run"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/secrets"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/sgconf"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/std"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/usershell"
	"github.com/sourcegraph/sourcegraph/dev/sg/interrupt"
	"github.com/sourcegraph/sourcegraph/dev/sg/msp"
	"github.com/sourcegraph/sourcegraph/dev/sg/root"
	"github.com/sourcegraph/sourcegraph/dev/sg/sams"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

func main() {
	// Do not add initialization here, do all setup in sg.Before - this is a necessary
	// workaround because we don't have control over the bash completion flag, which is
	// part of urfave/cli internals.
	if os.Args[len(os.Args)-1] == "--generate-bash-completion" {
		bashCompletionsMode = true
	}

	if err := sg.RunContext(context.Background(), os.Args); err != nil {
		// We want to prefer an already-initialized std.Out no matter what happens,
		// because that can be configured (e.g. with '--disable-output-detection'). Only
		// if something went horribly wrong and std.Out is not yet initialized should we
		// attempt an initialization here.
		if std.Out == nil {
			std.Out = std.NewOutput(os.Stdout, false)
		}
		// Do not treat error message as a format string
		std.Out.WriteFailuref("%s", err.Error())
		os.Exit(1)
	}
}

// Values to be stamped at build time.
var (
	BuildCommit = "dev"     // git commit hash of build
	ReleaseName = "unknown" // human-friendlier name for the release, e.g. '2024-04-24-16-44-3623ecb2'
)

var (
	// configFile is the path to use with sgconf.Get - it must not be used before flag
	// initialization.
	configFile string
	// configOverwriteFile is the path to use with sgconf.Get - it must not be used before
	// flag initialization.
	configOverwriteFile string
	// disableOverwrite causes configuration to ignore configOverwriteFile.
	disableOverwrite bool

	// Global verbose mode
	verbose bool

	// postInitHooks is useful for doing anything that requires flags to be set beforehand,
	// e.g. generating help text based on parsed config, and are called before any command
	// Action is executed. These should run quickly and must fail gracefully.
	//
	// Commands can register postInitHooks in an 'init()' function that appends to this
	// slice.
	postInitHooks []func(cmd *cli.Context)

	// bashCompletionsMode determines if we are in bash completion mode. In this mode,
	// sg should respond quickly, so most setup tasks (e.g. postInitHooks) are skipped.
	//
	// Do not run complicated tasks, etc. in Before or After hooks when in this mode.
	bashCompletionsMode bool
)

const sgBugReportTemplate = "https://github.com/sourcegraph/sourcegraph/issues/new?template=sg_bug.md"

// sg is the main sg CLI application.
var sg = &cli.App{
	Usage:       "The Sourcegraph developer tool!",
	Description: "Learn more: https://docs-legacy.sourcegraph.com/dev/background-information/sg",
	Version:     ReleaseName, // use friendly name as version
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
			Usage:       "load sg configuration from `file`",
			Aliases:     []string{"c"},
			EnvVars:     []string{"SG_CONFIG"},
			TakesFile:   true,
			Value:       sgconf.DefaultFile,
			Destination: &configFile,
		},
		&cli.StringFlag{
			Name:        "overwrite",
			Usage:       "load sg configuration from `file` that is gitignored and can be used to, for example, add credentials",
			Aliases:     []string{"o"},
			EnvVars:     []string{"SG_OVERWRITE"},
			TakesFile:   true,
			Value:       sgconf.DefaultOverwriteFile,
			Destination: &configOverwriteFile,
		},
		&cli.BoolFlag{
			Name:        "disable-overwrite",
			Usage:       "disable loading additional sg configuration from overwrite file (see -overwrite)",
			EnvVars:     []string{"SG_DISABLE_OVERWRITE"},
			Value:       false,
			Destination: &disableOverwrite,
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
			Name:        "disable-output-detection",
			Usage:       "use fixed output configuration instead of detecting terminal capabilities",
			EnvVars:     []string{"SG_DISABLE_OUTPUT_DETECTION"},
			Destination: &std.DisableOutputDetection,
		},
		&cli.BoolFlag{
			Name:        "no-dev-private",
			Usage:       "disable checking for dev-private - only useful for automation or ci",
			EnvVars:     []string{"SG_NO_DEV_PRIVATE"},
			Value:       false,
			Destination: &check.NoDevPrivateCheck,
		},
	},
	Before: func(cmd *cli.Context) (err error) {
		// All other setup pertains to running commands - to keep completions fast,
		// we skip all other setup when in bashCompletions mode.
		if bashCompletionsMode {
			return nil
		}

		// Lots of setup happens in Before - we want to make sure anything that
		// we collect a generate a helpful message here if anything goes wrong.
		defer func() {
			if p := recover(); p != nil {
				std.Out.WriteWarningf("Encountered panic - please open an issue with the command output:\n\t%s",
					sgBugReportTemplate)
				message := fmt.Sprintf("%v:\n%s", p, getRelevantStack())
				err = cli.Exit(message, 1)
			}
		}()

		// Let sg components register pre-interrupt hooks
		interrupt.Listen()

		// Configure global output
		std.Out = std.NewOutput(cmd.App.Writer, verbose)

		// Set up analytics and hooks for each command - do this as the first context
		// setup
		if !cmd.Bool("disable-analytics") {
			cmd.Context, err = analytics.WithContext(cmd.Context, cmd.App.Version)
			if err != nil {
				std.Out.WriteWarningf("Failed to initialize analytics: " + err.Error())
			}

			// Ensure analytics are persisted
			interrupt.Register(func() { _ = analytics.Persist(cmd.Context) })

			// Add analytics to each command
			addAnalyticsHooks([]string{"sg"}, cmd.App.Commands)
		}

		// Initialize context after analytics are set up
		cmd.Context, err = usershell.Context(cmd.Context)
		if err != nil {
			std.Out.WriteWarningf("Unable to infer user shell context: " + err.Error())
		}
		cmd.Context = background.Context(cmd.Context, verbose)
		interrupt.Register(func() { background.Wait(cmd.Context, std.Out) })

		// Configure logger, for commands that use components that use loggers
		if _, set := os.LookupEnv(log.EnvDevelopment); !set {
			os.Setenv(log.EnvDevelopment, "true")
		}
		if _, set := os.LookupEnv(log.EnvLogFormat); !set {
			os.Setenv(log.EnvLogFormat, "console")
		}
		liblog := log.Init(log.Resource{Name: "sg", Version: BuildCommit})
		interrupt.Register(liblog.Sync)

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
		skipBackgroundTasks := map[string]struct{}{
			"update":   {},
			"version":  {},
			"live":     {},
			"teammate": {},
		}
		if _, skipped := skipBackgroundTasks[cmd.Args().First()]; !skipped {
			background.Run(cmd.Context, func(ctx context.Context, out *std.Output) {
				err := checkSgVersionAndUpdate(ctx, out, cmd.Bool("skip-auto-update"))
				if err != nil {
					out.WriteWarningf("update check: %s", err)
				}
			})
		}

		// Call registered hooks last
		for _, hook := range postInitHooks {
			hook(cmd)
		}

		return nil
	},
	After: func(cmd *cli.Context) error {
		if !bashCompletionsMode {
			// Wait for background jobs to finish up, iff not in autocomplete mode
			background.Wait(cmd.Context, std.Out)
			// Persist analytics
			_ = analytics.Persist(cmd.Context)
		}

		return nil
	},
	Commands: []*cli.Command{
		// Common dev tasks
		startCommand,
		runCommand,
		ci.Command,
		testCommand,
		lintCommand,
		generateCommand,
		bazelCommand,
		dbCommand,
		migrationCommand,
		insightsCommand,
		telemetryCommand,
		monitoringCommand,
		contextCommand,
		deployCommand,
		wolfiCommand,
		backportCommand,

		// Dev environment
		secretCommand,
		setupCommand,
		srcCommand,
		srcInstanceCommand,
		imagesCommand,

		// Company
		teammateCommand,
		rfcCommand,
		liveCommand,
		opsCommand,
		auditCommand,
		pageCommand,
		cloudCommand,
		msp.Command,
		securityCommand,
		sams.Command,

		// Util
		analyticsCommand,
		doctorCommand,
		funkyLogoCommand,
		helpCommand,
		installCommand,
		release.Command,
		updateCommand,
		versionCommand,
		codyGatewayCommand,
	},
	ExitErrHandler: func(cmd *cli.Context, err error) {
		interrupt.Wait()
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
			// Do not treat error message as a format string
			std.Out.WriteFailuref("%s", errMsg)
		}

		// Determine exit code
		if exitErr, ok := err.(cli.ExitCoder); ok {
			os.Exit(exitErr.ExitCode())
		}
		os.Exit(1)
	},

	Suggest: true,

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

func getConfig() (*sgconf.Config, error) {
	if disableOverwrite {
		return sgconf.GetWithoutOverwrites(configFile)
	}
	return sgconf.Get(configFile, configOverwriteFile)
}

// watchConfig starts a file watcher for the sg configuration files. It returns a channel
// that will receive the updated configuration whenever the file changes to a valid configuration,
// distinct from the last parsed value (invalid or unparseable updates will be dropped). The
// initial configuration is read and sent on the channel before the function returns so it
// can be read immediately.
func watchConfig(ctx context.Context) (<-chan *sgconf.Config, error) {
	conf, err := sgconf.GetUnbuffered(configFile, configOverwriteFile, disableOverwrite)
	if err != nil {
		return nil, err
	}
	// Create a hash to compare future reads against
	hash, err := hashstructure.Hash(conf, hashstructure.FormatV2, nil)
	if err != nil {
		return nil, err
	}
	output := make(chan *sgconf.Config, 1)
	output <- conf

	// start file watcher on configuration files
	paths := []string{configFile}
	if !disableOverwrite && exists(configOverwriteFile) {
		paths = append(paths, configOverwriteFile)
	}
	updates, err := run.WatchPaths(ctx, paths)
	if err != nil {
		return nil, err
	}

	// watch for configuration updates
	go func() {
		for {
			select {
			case <-ctx.Done():
				close(output)
				return
			case <-updates:
				conf, err = sgconf.GetUnbuffered(configFile, configOverwriteFile, disableOverwrite)
				if err != nil {
					std.Out.WriteWarningf("Failed to reload configuration: %s", err)
					continue
				}
				newHash, err := hashstructure.Hash(conf, hashstructure.FormatV2, nil)
				if err != nil {
					std.Out.WriteWarningf("Failed to hash configuration: %s", err)
					continue
				}

				// if this is a true update, send it on the channel and remember its hash
				if newHash != hash {
					hash = newHash
					output <- conf
				}
			}
		}
	}()

	return output, err
}

func exists(file string) bool {
	_, err := os.Stat(file)
	return !os.IsNotExist(err)
}
