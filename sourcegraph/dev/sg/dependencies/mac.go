package dependencies

import (
	"context"

	"github.com/sourcegraph/sourcegraph/dev/sg/internal/check"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/std"
	"github.com/sourcegraph/sourcegraph/dev/sg/internal/usershell"
	"github.com/sourcegraph/sourcegraph/dev/sg/root"
)

const (
	depsHomebrew      = "Homebrew"
	depsBaseUtilities = "Base utilities"
	depsDocker        = "Docker"
	depsCloneRepo     = "Clone repositories"
)

// Mac declares Mac dependencies.
var Mac = []category{
	{
		Name: depsHomebrew,
		Checks: []*dependency{
			{
				Name:        "brew",
				Check:       checkAction(check.InPath("brew")),
				Fix:         cmdFix(`eval $(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)`),
				Description: `We depend on having the Homebrew package manager available on macOS: https://brew.sh`,
			},
		},
	},
	{
		Name:      depsBaseUtilities,
		DependsOn: []string{depsHomebrew},
		Checks: []*dependency{
			{
				Name:  "git",
				Check: checkAction(check.Git),
				Fix:   brewInstall("git"),
			},
			{
				Name:  "gnu-sed",
				Check: checkAction(check.InPath("gsed")),
				Fix:   brewInstall("gnu-sed"),
			},
			{
				Name:  "findutils",
				Check: checkAction(check.InPath("gfind")),
				Fix:   brewInstall("findutils"),
			},
			{
				Name:  "comby",
				Check: checkAction(check.InPath("comby")),
				Fix:   brewInstall("comby"),
			},
			{
				Name:  "pcre",
				Check: checkAction(check.InPath("pcregrep")),
				Fix:   brewInstall("pcre"),
			},
			{
				Name:  "sqlite",
				Check: checkAction(check.InPath("sqlite3")),
				Fix:   brewInstall("sqlite"),
			},
			{
				Name:  "jq",
				Check: checkAction(check.InPath("jq")),
				Fix:   brewInstall("jq"),
			},
			{
				Name:  "bash",
				Check: checkAction(check.CommandOutputContains("bash --version", "version 5")),
				Fix:   brewInstall("bash"),
			},
			{
				Name: "rosetta",
				Check: checkAction(
					check.Any(
						// will return true on non-m1 macs
						check.CommandOutputContains("uname -m", "x86_64"),
						// oahd is the process running rosetta
						check.CommandExitCode("pgrep oahd", 0)),
				),
				Fix: cmdFix(`softwareupdate --install-rosetta --agree-to-license`),
			},
			{
				Name:        "certutil",
				Description: "Required for caddy certificates.",
				Check:       checkAction(check.InPath("certutil")),
				Fix:         brewInstall("nss"),
			},
			{
				// Bazelisk is a wrapper for Bazel written in Go. It automatically picks a good version of Bazel given your current working directory
				// Bazelisk replaces the bazel binary in your path
				Name:  "bazelisk (bazel)",
				Check: checkAction(check.Bazelisk),
				Fix:   brewInstall("bazelisk"),
			},
			{
				Name:  "ibazel",
				Check: checkAction(check.InPath("ibazel")),
				Fix:   brewInstall("ibazel"),
			},
			{
				Name:  "asdf",
				Check: checkAction(check.ASDF),
				Fix: func(ctx context.Context, cio check.IO, args CheckArgs) error {
					if err := brewInstall("asdf")(ctx, cio, args); err != nil {
						return err
					}
					return usershell.Run(ctx,
						`echo ". ${HOMEBREW_PREFIX:-/usr/local}/opt/asdf/libexec/asdf.sh" >>`, usershell.ShellConfigPath(ctx),
					).Wait()
				},
			},
			{
				Name:    "p4 CLI (Perforce)",
				Check:   checkAction(check.InPath("p4")),
				Enabled: disableInCI(), // giving a SHA256 mismatch error in CI
				Fix:     caskInstall("p4"),
			},
		},
	},
	{
		Name:      depsDocker,
		Enabled:   disableInCI(), // Very wonky in CI
		DependsOn: []string{depsHomebrew},
		Checks: []*dependency{
			{
				Name:  "docker",
				Check: checkAction(check.Docker),
				Fix: check.CombineFix(
					caskInstall("docker"),
					cmdFix("open --hide --background /Applications/Docker.app"),
				),
			},
		},
	},
	categoryCloneRepositories(),
	categoryProgrammingLanguagesAndTools(
		// src-cli is installed differently on Ubuntu and Mac
		&dependency{
			Name:  "src",
			Check: checkAction(check.Combine(check.InPath("src"), checkSrcCliVersion(">= 4.2.0"))),
			Fix:   cmdFix(`brew upgrade sourcegraph/src-cli/src-cli || brew install sourcegraph/src-cli/src-cli`),
		},
		// gnu-parallel is never available by default on MacOs.
		&dependency{
			Name:  "gnu-parallel",
			Check: checkAction(check.InPath("parallel")),
			Fix:   brewInstall("parallel"),
		},
	),
	{
		Name:      "Postgres database",
		DependsOn: []string{depsHomebrew},
		Checks: []*dependency{
			{
				Name: "Install Postgres",
				Description: `psql, the PostgreSQL CLI client, needs to be available in your $PATH.

If you've installed PostgreSQL with Homebrew that should be the case.

If you used another method, make sure psql is available.`,
				Check: checkAction(check.Combine(
					check.InPath("psql"),
					check.CompareSemanticVersion("psql", "psql --version", ">= 15.0"),
				)),
				Fix: brewInstall("postgresql@15"),
			},
			{
				Name: "Start Postgres",
				// In the eventuality of the user using a non standard configuration and having
				// set it up appropriately in its configuration, we can bypass the standard postgres
				// check and directly check for the sourcegraph database.
				//
				// Because only the latest error is returned, it's better to finish with the real check
				// for error message clarity.
				Check: func(ctx context.Context, out *std.Output, args CheckArgs) error {
					if err := checkSourcegraphDatabase(ctx, out, args); err == nil {
						return nil
					}
					return check.PostgresConnection(ctx)
				},
				Description: `Sourcegraph requires the PostgreSQL database (v15+) to be running.

We recommend installing it with Homebrew and starting it as a system service.
If you know what you're doing, you can also install PostgreSQL another way.
For example: you can use https://postgresapp.com/

If you're not sure: use the recommended commands to install PostgreSQL.`,
				Fix: cmdFixes(
					"brew services start postgresql@15",
					"sleep 3",
				),
			},
			{
				Name:        "Connection to 'sourcegraph' database",
				Check:       checkSourcegraphDatabase,
				Description: `Once PostgreSQL is installed and running, we need to set up Sourcegraph database itself and a specific user.`,
				Fix: cmdFixes(
					"PGUSER=$USER createuser --superuser sourcegraph || true",
					`PGUSER=$USER PGDATABASE=postgres psql -c "ALTER USER sourcegraph WITH PASSWORD 'sourcegraph';"`,
					`createdb --owner=sourcegraph --encoding=UTF8 --template=template0 sourcegraph`,
				),
			},
		},
	},
	{
		Name:      "Redis database",
		DependsOn: []string{depsHomebrew},
		Checks: []*dependency{
			{
				Name: "Start Redis",
				Description: `Sourcegraph requires the Redis database to be running.
We recommend installing it with Homebrew and starting it as a system service.`,
				Check: checkAction(check.Redis),
				Fix: cmdFixes(
					"brew reinstall redis",
					"brew services start redis",
				),
			},
		},
	},
	{
		Name:      "sourcegraph.test development proxy",
		DependsOn: []string{depsBaseUtilities},
		Checks: []*dependency{
			{
				Name: "/etc/hosts contains sourcegraph.test",
				Description: `Sourcegraph should be reachable under https://sourcegraph.test:3443.
To do that, we need to add sourcegraph.test to the /etc/hosts file.`,
				Check: checkAction(check.FileContains("/etc/hosts", "sourcegraph.test")),
				Fix: func(ctx context.Context, cio check.IO, args CheckArgs) error {
					return root.Run(usershell.Command(ctx, `./dev/add_https_domain_to_hosts.sh`)).StreamLines(cio.Verbose)
				},
			},
			{
				Name: "Caddy root certificate is trusted by system",
				Description: `In order to use TLS to access your local Sourcegraph instance, you need to
trust the certificate created by Caddy, the proxy we use locally.

WARNING: if you just fixed (automatically or manually) this step, you must restart sg setup for the check to pass.`,
				Enabled: disableInCI(), // Can't seem to get this working
				Check:   checkAction(check.Caddy),
				Fix: func(ctx context.Context, cio check.IO, args CheckArgs) error {
					return root.Run(usershell.Command(ctx, `./dev/caddy.sh trust`)).StreamLines(cio.Verbose)
				},
			},
		},
	},
	categoryAdditionalSGConfiguration(),
	{
		Name:      "Cloud services",
		DependsOn: []string{depsHomebrew},
		Enabled:   disableInCI(),
		Checks: []*dependency{
			dependencyGcloud(),
		},
	},
}

// var homebrewPsqlVersion = regexp.MustCompile(`^psql (PostgreSQL) 15\.(\d+) (Homebrew)$`)
// var homebrewPostgresVersion = regexp.MustCompile(`^PostgreSQL (\d+)\.(\d+)$`)

// // var psqlCheck = check.Combine(check.InPath("psql"), )

// func checkPsqlVersion(ctx context.Context, out *std.Output, args CheckArgs) error {
// 	version, err := usershell.Run(ctx, "psql --version").String()
// 	if err != nil {
// 		return err
// 	}

// 	if !homebrewPsqlVersion.MatchString(version) {
// 		return errors.Newf("wanted psql is not installed with Homebrew")
// 	}
// }
