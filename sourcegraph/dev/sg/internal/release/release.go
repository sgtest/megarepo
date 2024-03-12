package release

import (
	"fmt"
	"strings"

	"github.com/sourcegraph/run"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/category"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/urfave/cli/v2"
)

var releaseRunFlags = []cli.Flag{
	&cli.StringFlag{
		Name:  "workdir",
		Value: ".",
		Usage: "Set the working directory to load release scripts from",
	},
	&cli.StringFlag{
		Name:  "type",
		Value: "patch",
		Usage: "Select release type: major, minor, patch",
	},
	&cli.StringFlag{
		Name:  "branch",
		Usage: "Branch to create release from, usually `main` or `5.3` if you're cutting a patch release",
	},
	&cli.BoolFlag{
		Name:  "pretend",
		Value: false,
		Usage: "Preview all the commands that would be performed",
	},
	&cli.StringFlag{
		Name:  "version",
		Value: "v6.6.666",
		Usage: "Force version",
	},
	&cli.StringFlag{
		Name:  "inputs",
		Usage: "Set inputs to use for a given release, ex: --input=server=v5.2.404040,foobar=ffefe",
	},
	&cli.BoolFlag{
		Name:  "config-from-commit",
		Value: false,
		Usage: "Infer run configuration from last commit instead of flags.",
	},
}

var Command = &cli.Command{
	Name:     "release",
	Usage:    "Sourcegraph release utilities",
	Category: category.Util,
	Subcommands: []*cli.Command{
		{
			Name:     "cve-check",
			Usage:    "Check all CVEs found in a buildkite build against a set of preapproved CVEs for a release",
			Category: category.Util,
			Action:   cveCheck,
			Flags: []cli.Flag{
				&buildNumberFlag,
				&referenceUriFlag,
			},
			UsageText: `sg release cve-check -u https://handbook.sourcegraph.com/departments/security/tooling/trivy/4-2-0/ -b 184191`,
		},
		{
			Name:     "run",
			Usage:    "Run steps defined in release manifest. Those are meant to be run in CI",
			Category: category.Util,
			Subcommands: []*cli.Command{
				{
					Name:  "test",
					Flags: releaseRunFlags,
					Usage: "Run test steps as defined in the release manifest",
					Action: func(cctx *cli.Context) error {
						r, err := newReleaseRunnerFromCliContext(cctx)
						if err != nil {
							return err
						}
						return r.Test(cctx.Context)
					},
				},
				{
					Name:  "internal",
					Usage: "todo",
					Subcommands: []*cli.Command{
						{
							Name:  "finalize",
							Usage: "Run internal release finalize steps",
							Flags: releaseRunFlags,
							Action: func(cctx *cli.Context) error {
								r, err := newReleaseRunnerFromCliContext(cctx)
								if err != nil {
									return err
								}
								return r.InternalFinalize(cctx.Context)
							},
						},
					},
				},
				{
					Name:  "promote-to-public",
					Usage: "TODO",
					Subcommands: []*cli.Command{
						{
							Name:  "finalize",
							Usage: "Run internal release finalize steps",
							Flags: releaseRunFlags,
							Action: func(cctx *cli.Context) error {
								r, err := newReleaseRunnerFromCliContext(cctx)
								if err != nil {
									return err
								}
								return r.PromoteFinalize(cctx.Context)
							},
						},
					},
				},
			},
		},
		{
			Name:      "create",
			Usage:     "Create a release for a given product",
			UsageText: "sg release create --workdir [path] --type patch",
			Category:  category.Util,
			Flags:     releaseRunFlags,
			Action: func(cctx *cli.Context) error {
				r, err := newReleaseRunnerFromCliContext(cctx)
				if err != nil {
					return err
				}
				return r.CreateRelease(cctx.Context)
			},
		},
		{
			Name:      "promote-to-public",
			Usage:     "Promete an existing release to the public",
			UsageText: "sg release promote-to-public --workdir [path] --type patch",
			Category:  category.Util,
			Flags:     releaseRunFlags,
			Action: func(cctx *cli.Context) error {
				r, err := newReleaseRunnerFromCliContext(cctx)
				if err != nil {
					return err
				}
				return r.Promote(cctx.Context)
			},
		},
	},
}

func newReleaseRunnerFromCliContext(cctx *cli.Context) (*releaseRunner, error) {
	workdir := cctx.String("workdir")
	pretend := cctx.Bool("pretend")
	// Normalize the version string, to prevent issues where this was given with the wrong convention
	// which requires a full rebuild.
	version := fmt.Sprintf("v%s", strings.TrimPrefix(cctx.String("version"), "v"))
	typ := cctx.String("type")
	inputs := cctx.String("inputs")
	branch := cctx.String("branch")

	if cctx.Bool("config-from-commit") {
		cmd := run.Cmd(cctx.Context, "git", "log", "-1")
		cmd.Dir(workdir)
		lines, err := cmd.Run().Lines()
		if err != nil {
			return nil, err
		}

		// config dump is always the last line.
		configRaw := lines[len(lines)-1]
		if !strings.HasPrefix(strings.TrimSpace(configRaw), "{") {
			return nil, errors.New("Trying to infer config from last commit, but did not find serialized config")
		}
		rc, err := parseReleaseConfig(configRaw)
		if err != nil {
			return nil, err
		}

		version = rc.Version
		typ = rc.Type
		inputs = rc.Inputs
	}

	return NewReleaseRunner(cctx.Context, workdir, version, inputs, typ, branch, pretend)
}
