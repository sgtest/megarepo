package main

import (
	"bufio"
	"context"
	"crypto/x509"
	"encoding/pem"
	"flag"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"

	"github.com/cockroachdb/errors"
	"github.com/garyburd/redigo/redis"
	"github.com/jackc/pgx/v4"
	"github.com/peterbourgon/ff/v3/ffcli"

	"github.com/sourcegraph/sourcegraph/dev/sg/root"
	"github.com/sourcegraph/sourcegraph/internal/database/postgresdsn"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

var (
	setupFlagSet = flag.NewFlagSet("sg setup", flag.ExitOnError)
	setupCommand = &ffcli.Command{
		Name:       "setup",
		ShortUsage: "sg setup",
		ShortHelp:  "Reports which version of Sourcegraph is currently live in the given environment",
		LongHelp:   "Run 'sg setup' to setup the local dev environment",
		FlagSet:    setupFlagSet,
		Exec:       setupExec,
	}
)

func setupExec(ctx context.Context, args []string) error {
	if runtime.GOOS != "linux" && runtime.GOOS != "darwin" {
		out.WriteLine(output.Linef("", output.StyleWarning, "'sg setup' currently only supports macOS and Linux"))
		os.Exit(1)
	}

	currentOS := runtime.GOOS
	if overridesOS, ok := os.LookupEnv("SG_FORCE_OS"); ok {
		currentOS = overridesOS
	}

	var categories []dependencyCategory
	if currentOS == "darwin" {
		categories = macOSDependencies
	} else {
		// DEPRECATED: The new 'sg setup' doesn't work on Linux yet, so we fall back to the old one.
		writeWarningLine("'sg setup' on Linux provides instructions for Ubuntu Linux. If you're using another distribution, instructions might need to be adjusted.")
		return deprecatedSetupForLinux(ctx)
	}

	// Check whether we're in the sourcegraph/sourcegraph repository so we can
	// skip categories/dependencies that depend on the repository.
	_, err := root.RepositoryRoot()
	inRepo := err == nil

	failed := []int{}
	all := []int{}
	skipped := []int{}
	employeeFailed := []int{}
	for i := range categories {
		failed = append(failed, i)
		all = append(all, i)
	}

	for len(failed) != 0 {
		out.ClearScreen()

		writeOrangeLine("-------------------------------------")
		writeOrangeLine("|        Welcome to sg setup!       |")
		writeOrangeLine("-------------------------------------")

		for i, category := range categories {
			idx := i + 1

			if category.requiresRepository && !inRepo {
				writeSkippedLine("%d. %s %s[SKIPPED. Requires 'sg setup' to be run in 'sourcegraph' repository]%s", idx, category.name, output.StyleBold, output.StyleReset)
				skipped = append(skipped, idx)
				failed = removeEntry(failed, i)
				continue
			}

			pending := out.Pending(output.Linef("", output.StylePending, "%d. %s - Determining status...", idx, category.name))
			for _, dep := range category.dependencies {
				dep.Update(ctx)
			}
			pending.Destroy()

			if combined := category.CombinedState(); combined {
				writeSuccessLine("%d. %s", idx, category.name)
				failed = removeEntry(failed, i)
			} else {
				nonEmployeeState := category.CombinedStateNonEmployees()
				if nonEmployeeState {
					writeWarningLine("%d. %s", idx, category.name)
					employeeFailed = append(skipped, idx)
				} else {
					writeFailureLine("%d. %s", idx, category.name)
				}
			}
		}

		if len(failed) == 0 && len(employeeFailed) == 0 {
			if len(skipped) == 0 && len(employeeFailed) == 0 {
				out.Write("")
				out.WriteLine(output.Linef(output.EmojiOk, output.StyleBold, "Everything looks good! Happy hacking!"))
			}

			if len(skipped) != 0 {
				out.Write("")
				writeWarningLine("Some checks were skipped because 'sg setup' is not run in the 'sourcegraph' repository.")
				writeFingerPointingLine("Restart 'sg setup' in the 'sourcegraph' repository to continue.")
			}

			return nil
		}

		out.Write("")

		if len(employeeFailed) != 0 && len(failed) == len(employeeFailed) {
			writeWarningLine("Some checks that are only relevant for Sourcegraph employees failed.\nIf you're not a Sourcegraph employee you're good to go. Hit Ctrl-C.\n\nIf you're a Sourcegraph employee: which one do you want to fix?")
		} else {
			writeWarningLine("Some checks failed. Which one do you want to fix?")
		}

		idx, err := getNumberOutOf(all)
		if err != nil {
			if err == io.EOF {
				return nil
			}
			return err
		}
		selectedCategory := categories[idx]

		out.ClearScreen()

		err = presentFailedCategoryWithOptions(ctx, idx, &selectedCategory)
		if err != nil {
			if err == io.EOF {
				return nil
			}
			return err
		}
	}

	return nil
}

var macOSDependencies = []dependencyCategory{
	{
		name: "Install homebrew",
		dependencies: []*dependency{
			{
				name:  "brew",
				check: checkInPath("brew"),
				instructionsComment: `We depend on having the Homebrew package manager available on macOS.

Follow the instructions at https://brew.sh to install it, then rerun 'sg setup'.`,
			},
		},
		autoFixing: false,
	},
	{
		name: "Install base utilities (git, docker, ...)",
		dependencies: []*dependency{
			{name: "git", check: checkInPath("git"), instructionsCommands: `brew install git`},
			{name: "gnu-sed", check: checkInPath("gsed"), instructionsCommands: "brew install gnu-sed"},
			{name: "comby", check: checkInPath("comby"), instructionsCommands: "brew install comby"},
			{name: "pcre", check: checkInPath("pcregrep"), instructionsCommands: `brew install pcre`},
			{name: "sqlite", check: checkInPath("sqlite3"), instructionsCommands: `brew install sqlite`},
			{name: "jq", check: checkInPath("jq"), instructionsCommands: `brew install jq`},
			{name: "bash", check: checkCommandOutputContains("bash --version", "version 5"), instructionsCommands: `brew install bash`},
			{
				name:                 "docker",
				check:                wrapCheckErr(checkInPath("docker"), "if Docker is installed and the check fails, you might need to start Docker.app and restart terminal and 'sg setup'"),
				instructionsCommands: `brew install --cask docker`,
			},
		},
		autoFixing: true,
	},
	{
		name: "Clone repositories",
		dependencies: []*dependency{
			{
				name:  "SSH authentication with GitHub.com",
				check: checkCommandOutputContains("ssh -o UserKnownHostsFile=/dev/null -o StrictHostKeyChecking=no -T git@github.com", "successfully authenticated"),
				instructionsComment: `` +
					`Make sure that you can clone git repositories from GitHub via SSH.
See here on how to set that up:

https://docs.github.com/en/authentication/connecting-to-github-with-ssh
`,
			},
			{
				name:                 "github.com/sourcegraph/sourcegraph",
				check:                checkInMainRepoOrRepoInDirectory,
				instructionsCommands: `git clone git@github.com:sourcegraph/sourcegraph.git`,
				instructionsComment: `` +
					`The 'sourcegraph' repository contains the Sourcegraph codebase and everything to run Sourcegraph locally.`,
			},
			{
				name:                 "github.com/sourcegraph/dev-private",
				check:                checkDevPrivateInParentOrInCurrentDirectory,
				instructionsCommands: `git clone git@github.com:sourcegraph/dev-private.git`,
				instructionsComment: `` +
					`In order to run the local development environment as a Sourcegraph employee,
you'll need to clone another repository: github.com/sourcegraph/dev-private.

It contains convenient preconfigured settings and code host connections.

It needs to be cloned into the same folder as sourcegraph/sourcegraph,
so they sit alongside each other, like this:

   /dir
   |-- dev-private
   +-- sourcegraph

NOTE: You can ignore this if you're not a Sourcegraph employee.
`,
				onlyEmployees: true,
			},
		},
		autoFixing: true,
	},
	{
		name:               "Programming languages & tooling",
		requiresRepository: true,
		// TODO: Can we provide an autofix here that installs asdf, reloads shell, installs language versions?
		dependencies: []*dependency{
			{
				name: "go", check: checkInPath("go"),
				instructionsComment: `` +
					`Souregraph requires Go to be installed.

Check the .tool-versions file for which version.

We *highly recommend* using the asdf version manager to install and manage
programming languages and tools. Find out how to install asdf here:

	https://asdf-vm.com/guide/getting-started.html

Once you have asdf, execute the commands below.`,
				instructionsCommands: `
asdf plugin-add golang https://github.com/kennyp/asdf-golang.git
asdf install golang
`,
			},
			{
				name: "yarn", check: checkInPath("yarn"),
				instructionsComment: `` +
					`Souregraph requires Yarn to be installed.

Check the .tool-versions file for which version.

We *highly recommend* using the asdf version manager to install and manage
programming languages and tools. Find out how to install asdf here:

	https://asdf-vm.com/guide/getting-started.html

Once you have asdf, execute the commands below.`,
				instructionsCommands: `
brew install gpg
asdf plugin-add yarn
asdf install yarn 
`,
			},
			{
				name:  "node",
				check: checkInPath("node"),
				instructionsComment: `` +
					`Souregraph requires Node.JS to be installed.

Check the .tool-versions file for which version.

We *highly recommend* using the asdf version manager to install and manage
programming languages and tools. Find out how to install asdf here:

	https://asdf-vm.com/guide/getting-started.html

Once you have asdf, execute the commands below.`,
				instructionsCommands: `
asdf plugin add nodejs https://github.com/asdf-vm/asdf-nodejs.git 
echo 'legacy_version_file = yes' >> ~/.asdfrc
asdf install nodejs
`,
			},
		},
	},
	{
		name:               "Setup PostgreSQL database",
		requiresRepository: true,
		dependencies: []*dependency{
			// TODO: We could probably split this check up into two:
			// 1. Check whether Postgres is running
			// 2. Check whether Sourcegraph database exists
			{
				name:  "Connection to 'sourcegraph' database",
				check: checkPostgresConnection,
				instructionsComment: `` +
					`Sourcegraph requires the PostgreSQL database to be running.

We recommend installing it with Homebrew and starting it as a system service.
If you know what you're doing, you can also install PostgreSQL another way.
For example: you can use Postgres.app by following the instructions at
https://postgresapp.com but you also need to run the commands listed below
that create users and databsaes: 'createdb', 'createuser', ...

If you're not sure: use the recommended commands to install PostgreSQL, start it
and create the 'sourcegraph' database.`,
				instructionsCommands: `brew reinstall postgresql && brew services start postgresql 
sleep 10
createdb
createuser --superuser sourcegraph || true
psql -c "ALTER USER sourcegraph WITH PASSWORD 'sourcegraph';"
createdb --owner=sourcegraph --encoding=UTF8 --template=template0 sourcegraph
`,
			},
			{
				name:  "psql",
				check: checkInPath("psql"),
				instructionsComment: `` +
					`psql, the PostgreSQL CLI client, needs to be available in your $PATH.

If you've installed PostgreSQL with Homebrew that should be the case.

If you used another method, make sure psql is available.`,
			},
		},
	},
	{
		name:               "Setup Redis database",
		autoFixing:         true,
		requiresRepository: true,
		dependencies: []*dependency{
			{
				name:  "Connection to Redis",
				check: retryCheck(checkRedisConnection, 5, 500*time.Millisecond),
				instructionsComment: `` +
					`Sourcegraph requires the Redis database to be running.
					We recommend installing it with Homebrew and starting it as a system service.`,
				instructionsCommands: "brew reinstall redis && brew services start redis",
			},
		},
	},
	{
		name:               "Setup proxy for local development",
		requiresRepository: true,
		dependencies: []*dependency{
			{
				name:  "/etc/hosts contains sourcegraph.test",
				check: checkFileContains("/etc/hosts", "sourcegraph.test"),
				instructionsComment: `` +
					`Sourcegraph should be reachable under https://sourcegraph.test:3443.
					To do that, we need to add sourcegraph.test to the /etc/hosts file.`,
				instructionsCommands: `./dev/add_https_domain_to_hosts.sh`,
			},
			{
				name:  "Caddy root certificate is trusted by system",
				check: checkCaddyTrusted,
				instructionsComment: `` +
					`In order to use TLS to access your local Sourcegraph instance, you need to
trust the certificate created by Caddy, the proxy we use locally.

YOU NEED TO RESTART 'sg setup' AFTER RUNNING THIS COMMAND!`,
				instructionsCommands:   `./dev/caddy.sh trust`,
				requiresSgSetupRestart: true,
			},
		},
	},
}

func deprecatedSetupForLinux(ctx context.Context) error {
	var instructions []instruction
	instructions = append(instructions, linuxInstructionsBeforeClone...)
	// clone instructions come after dependency instructions because we need
	// `git` installed to `git` clone.
	instructions = append(instructions, cloneInstructions...)
	instructions = append(instructions, linuxInstructionsAfterClone...)
	instructions = append(instructions, httpReverseProxyInstructions...)

	conditions := map[string]bool{}

	i := 0
	for _, instruction := range instructions {
		if instruction.ifBool != "" {
			val, ok := conditions[instruction.ifBool]
			if !ok {
				out.WriteLine(output.Line("", output.StyleWarning, "Something went wrong."))
				os.Exit(1)
			}
			if !val {
				continue
			}
		}
		if instruction.ifNotBool != "" {
			val, ok := conditions[instruction.ifNotBool]
			if !ok {
				out.WriteLine(output.Line("", output.StyleWarning, "Something went wrong."))
				os.Exit(1)
			}
			if val {
				continue
			}
		}

		i++
		out.WriteLine(output.Line("", output.StylePending, "------------------------------------------"))
		out.Writef("%sStep %d:%s%s %s%s", output.StylePending, i, output.StyleReset, output.StyleSuccess, instruction.prompt, output.StyleReset)
		out.Write("")

		if instruction.comment != "" {
			out.Write(instruction.comment)
			out.Write("")
		}

		if instruction.command != "" {
			out.WriteLine(output.Line("", output.StyleSuggestion, "Run the following command(s) in another terminal:\n"))
			out.WriteLine(output.Line("", output.CombineStyles(output.StyleBold, output.StyleYellow), strings.TrimSpace(instruction.command)))

			out.WriteLine(output.Linef("", output.StyleSuggestion, "Hit return to confirm that you ran the command..."))
			input := bufio.NewScanner(os.Stdin)
			input.Scan()
		}

		if instruction.readsBool != "" {
			// out.WriteLine(output.Linef("", output.StylePending, instruction.prompt))
			val := getBool()
			conditions[instruction.readsBool] = val
		}
	}
	return nil
}

type instruction struct {
	prompt, comment, command string

	readsBool string
	ifBool    string
	ifNotBool string
}

var linuxInstructionsBeforeClone = []instruction{
	{
		prompt:  `Update repositories`,
		command: `sudo apt-get update`,
	},
	{
		prompt:  `Install dependencies`,
		command: `sudo apt install -y make git-all libpcre3-dev libsqlite3-dev pkg-config jq libnss3-tools`,
	},
}

var linuxInstructionsAfterClone = []instruction{
	{
		prompt:  `Add package repositories`,
		comment: "In order to install dependencies, we need to add some repositories to apt.",
		command: `
# Go
sudo add-apt-repository ppa:longsleep/golang-backports

# Docker
curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo apt-key add -
sudo add-apt-repository "deb [arch=amd64] https://download.docker.com/linux/ubuntu $(lsb_release -cs) stable"

# Yarn
curl -sS https://dl.yarnpkg.com/debian/pubkey.gpg | sudo apt-key add -
echo "deb https://dl.yarnpkg.com/debian/ stable main" | sudo tee /etc/apt/sources.list.d/yarn.list`,
	},
	{
		prompt:  `Update repositories`,
		command: `sudo apt-get update`,
	},
	{
		prompt: `Install dependencies`,
		command: `sudo apt install -y make git-all libpcre3-dev libsqlite3-dev pkg-config golang-go docker-ce docker-ce-cli containerd.io yarn jq libnss3-tools

# Install comby
curl -L https://github.com/comby-tools/comby/releases/download/0.11.3/comby-0.11.3-x86_64-linux.tar.gz | tar xvz

# The extracted binary must be in your $PATH available as 'comby'.
# Here's how you'd move it to /usr/local/bin (which is most likely in your $PATH):
chmod +x comby-*-linux
mv comby-*-linux /usr/local/bin/comby

# Install nvm (to manage Node.js)
NVM_VERSION="$(curl https://api.github.com/repos/nvm-sh/nvm/releases/latest | jq -r .name)"
curl -L https://raw.githubusercontent.com/nvm-sh/nvm/"$NVM_VERSION"/install.sh -o install-nvm.sh
sh install-nvm.sh

# In sourcegraph repository directory: install current recommendend version of Node JS
nvm install`,
	},
	{
		prompt:    `Do you want to use Docker to run PostgreSQL and Redis?`,
		readsBool: `docker`,
	},
	{
		ifBool: "docker",
		prompt: "Nothing to do yet!",
		comment: `We provide a docker compose file at dev/redis-postgres.yml to make it easy to run Redis and PostgreSQL as docker containers.

NOTE: Although Ubuntu provides a docker-compose package, we recommend to install the latest version via pip so that it is compatible with our compose file.

See the official docker compose documentation at https://docs.docker.com/compose/install/ for more details on different installation options.
`,
	},
	// step 3, inserted here for convenience
	{
		ifBool:  "docker",
		prompt:  `The docker daemon might already be running, but if necessary you can use the following commands to start it:`,
		comment: `If you have issues running Docker, try adding your user to the docker group, and/or updating the socket file permissions, or try running these commands under sudo.`,
		command: `# as a system service
sudo systemctl enable --now docker

# manually
dockerd`,
	},
	{
		ifNotBool: "docker",
		prompt:    `Install PostgreSQL and Redis with the following commands`,
		command: `sudo apt install -y redis-server
sudo apt install -y postgresql postgresql-contrib`,
	},
	{
		ifNotBool: "docker",
		prompt:    `(optional) Start the services (and configure them to start automatically)`,
		command: `sudo systemctl enable --now postgresql
sudo systemctl enable --now redis-server.service`,
	},
	{
		ifBool: "docker",
		prompt: `Even though you're going to run the database in docker you will probably want to install the CLI tooling for Redis and Postgres

redis-tools will provide redis-cli and postgresql will provide createdb and createuser`,
		command: `sudo apt install -y redis-tools postgresql postgresql-contrib`,
	},
	// step 4
	{
		ifBool: "docker",
		prompt: `To initialize your database, you may have to set the appropriate environment variables before running the createdb command:`,
		comment: `The Sourcegraph server reads PostgreSQL connection configuration from the PG* environment variables.

The development server startup script as well as the docker compose file provide default settings, so it will work out of the box.`,
		command: `createdb --user=sourcegraph --owner=sourcegraph --host=localhost --encoding=UTF8 --template=template0 sourcegraph`,
	},
	{
		ifNotBool: "docker",
		prompt:    `Create a database for the current Unix user`,
		comment:   `You need a fresh Postgres database and a database user that has full ownership of that database.`,
		command: `sudo su - postgres
createdb`,
	},
	{
		ifNotBool: "docker",
		prompt:    `Create the Sourcegraph user and password`,
		command: `createuser --superuser sourcegraph
psql -c "ALTER USER sourcegraph WITH PASSWORD 'sourcegraph';"`,
	},
	{
		ifNotBool: "docker",
		prompt:    `Create the Sourcegraph database`,
		command:   `createdb --owner=sourcegraph --encoding=UTF8 --template=template0 sourcegraph`,
	},
}

var cloneInstructions = []instruction{
	{
		prompt:  `Cloning the code`,
		comment: `We're now going to clone the Sourcegraph repository. Make sure you execute the following command in a folder where you want to keep the repository. Command will create a new sub-folder (sourcegraph) in this folder.`,
		command: `git clone git@github.com:sourcegraph/sourcegraph.git`,
	},
	{
		prompt:    "Are you a Sourcegraph employee?",
		readsBool: "employee",
	},
	{
		prompt: `Getting access to private resources`,
		comment: `In order to run the local development environment as a Sourcegraph employee, you'll need to clone another repository: sourcegraph/dev-private. It contains convenient preconfigured settings and code host connections.
It needs to be cloned into the same folder as sourcegraph/sourcegraph, so they sit alongside each other.
To illustrate:
 /dir
 |-- dev-private
 +-- sourcegraph
NOTE: Ensure that you periodically pull the latest changes from sourcegraph/dev-private as the secrets are updated from time to time.`,
		command: `git clone git@github.com:sourcegraph/dev-private.git`,
		ifBool:  "employee",
	},
}

var httpReverseProxyInstructions = []instruction{
	{
		prompt: `Making sourcegraph.test accessible`,
		comment: `In order to make Sourcegraph's development environment accessible under https://sourcegraph.test:3443 we need to add an entry to /etc/hosts.

The following command will add this entry. It may prompt you for your password.

Execute it in the 'sourcegraph' repository you cloned.`,
		command: `./dev/add_https_domain_to_hosts.sh`,
	},
	{
		prompt: `Initialize Caddy 2`,
		comment: `Caddy 2 automatically manages self-signed certificates and configures your system so that your web browser can properly recognize them.

The following command adds Caddy's keys to the system certificate store.

Execute it in the 'sourcegraph' repository you cloned.`,
		command: `./dev/caddy.sh trust`,
	},
}

func getBool() bool {
	var s string

	fmt.Printf("(y/N): ")
	_, err := fmt.Scan(&s)
	if err != nil {
		panic(err)
	}

	s = strings.TrimSpace(s)
	s = strings.ToLower(s)

	if s == "y" || s == "yes" {
		return true
	}
	return false
}

func presentFailedCategoryWithOptions(ctx context.Context, categoryIdx int, category *dependencyCategory) error {
	printCategoryHeaderAndDependencies(categoryIdx, category)

	choices := map[int]string{1: "I want to fix these manually"}
	if category.autoFixing {
		choices[2] = "I'm feeling lucky. You try fixing all of it for me."
		choices[3] = "Go back"
	} else {
		choices[2] = "Go back"
	}

	choice, err := getChoice(choices)
	if err != nil {
		return err
	}

	switch choice {
	case 1:
		err = fixCategoryManually(ctx, categoryIdx, category)
	case 2:
		out.ClearScreen()
		err = fixCategoryAutomatically(ctx, category)
	case 3:
		return nil
	}
	return err
}

func printCategoryHeaderAndDependencies(categoryIdx int, category *dependencyCategory) {
	out.WriteLine(output.Linef(output.EmojiLightbulb, output.CombineStyles(output.StyleSearchQuery, output.StyleBold), "%d. %s", categoryIdx, category.name))
	out.Write("")
	out.Write("Checks:")

	for i, dep := range category.dependencies {
		idx := i + 1
		if dep.IsMet() {
			writeSuccessLine("%d. %s", idx, dep.name)
		} else {
			var printer func(fmtStr string, args ...interface{})
			if dep.onlyEmployees {
				printer = writeWarningLine
			} else {
				printer = writeFailureLine
			}

			if dep.err != nil {
				printer("%d. %s: %s", idx, dep.name, dep.err)
			} else {
				printer("%d. %s: %s", idx, dep.name, "check failed")
			}
		}
	}
}

func fixCategoryAutomatically(ctx context.Context, category *dependencyCategory) error {
	for _, dep := range category.dependencies {
		if dep.IsMet() {
			continue
		}

		if err := fixDependencyAutomatically(ctx, dep); err != nil {
			return err
		}
	}

	return nil
}

func fixDependencyAutomatically(ctx context.Context, dep *dependency) error {
	writeFingerPointingLine("Trying my hardest to fix %q automatically...", dep.name)

	// Look up which shell the user is using, because that's most likely the
	// one that has all the environment correctly setup.
	shell, ok := os.LookupEnv("SHELL")
	if !ok {
		// If we can't find the shell in the environment, we fall back to `bash`
		shell = "bash"
	}

	// The most common shells (bash, zsh, fish, ash) support the `-c` flag.
	cmd := exec.CommandContext(ctx, shell, "-c", dep.instructionsCommands)
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	if err := cmd.Run(); err != nil {
		writeFailureLine("Failed to run command: %s", err)
		return err
	}

	writeSuccessLine("Done! %q should be fixed now!", dep.name)

	if dep.requiresSgSetupRestart {
		writeFingerPointingLine("This command requires restarting of 'sg setup' to pick up the changes.")
		os.Exit(0)
	}

	return nil
}

func fixCategoryManually(ctx context.Context, categoryIdx int, category *dependencyCategory) error {
	for {
		toFix := []int{}

		for i, dep := range category.dependencies {
			if dep.IsMet() {
				continue
			}

			toFix = append(toFix, i)
		}

		if len(toFix) == 0 {
			break
		}

		var idx int

		if len(toFix) == 1 {
			idx = toFix[0]
		} else {
			writeFingerPointingLine("Which one do you want to fix?")
			var err error
			idx, err = getNumberOutOf(toFix)
			if err != nil {
				if err == io.EOF {
					return nil
				}
				return err
			}
		}

		dep := category.dependencies[idx]

		out.WriteLine(output.Linef(output.EmojiFailure, output.CombineStyles(output.StyleWarning, output.StyleBold), "%s", dep.name))
		out.Write("")

		if dep.err != nil {
			out.WriteLine(output.Linef("", output.StyleBold, "Encountered the following error:\n\n%s%s\n", output.StyleReset, dep.err))
		}

		out.WriteLine(output.Linef("", output.StyleBold, "How to fix:"))

		if dep.instructionsComment != "" {
			out.Write("")
			out.Write(dep.instructionsComment)
		}

		// If we don't have anything do run, we simply print instructions to
		// the user
		if dep.instructionsCommands == "" {
			writeFingerPointingLine("Hit return once you're done")
			waitForReturn()
		} else {
			// Otherwise we print the command(s) and ask the user whether we should run it or not
			out.Write("")
			if category.requiresRepository {
				out.Writef("Run the following command(s) %sin the 'sourcegraph' repository%s:", output.StyleBold, output.StyleReset)
			} else {
				out.Write("Run the following command(s):")
			}
			out.Write("")

			out.WriteLine(output.Line("", output.CombineStyles(output.StyleBold, output.StyleYellow), strings.TrimSpace(dep.instructionsCommands)))

			choice, err := getChoice(map[int]string{
				1: "I'll fix this manually (either by running the command or doing something else)",
				2: "You can run the command for me",
				3: "Go back",
			})
			if err != nil {
				return err
			}

			switch choice {
			case 1:
				writeFingerPointingLine("Hit return once you're done")
				waitForReturn()
			case 2:
				if err := fixDependencyAutomatically(ctx, dep); err != nil {
					return err
				}
			case 3:
				return nil
			}
		}

		pending := out.Pending(output.Linef("", output.StylePending, "Determining status..."))
		for _, dep := range category.dependencies {
			dep.Update(ctx)
		}
		pending.Destroy()

		printCategoryHeaderAndDependencies(categoryIdx, category)
	}

	return nil
}

func removeEntry(s []int, val int) (result []int) {
	for _, e := range s {
		if e != val {
			result = append(result, e)
		}
	}
	return result
}

func checkCommandOutputContains(cmd, contains string) func(context.Context) error {
	return func(ctx context.Context) error {
		elems := strings.Split(cmd, " ")
		out, _ := exec.Command(elems[0], elems[1:]...).CombinedOutput()
		if !strings.Contains(string(out), contains) {
			return errors.Newf("command output of %q doesn't contain %q", cmd, contains)
		}
		return nil
	}
}

func checkFileContains(fileName, content string) func(context.Context) error {
	return func(ctx context.Context) error {
		file, err := os.Open(fileName)
		if err != nil {
			return errors.Wrapf(err, "failed to check that %q contains %q", fileName, content)
		}
		defer file.Close()

		scanner := bufio.NewScanner(file)
		for scanner.Scan() {
			line := scanner.Text()
			if strings.Contains(line, content) {
				return nil
			}
		}

		if err := scanner.Err(); err != nil {
			return err
		}

		return errors.Newf("file %q did not contain %q", fileName, content)
	}
}

func checkInPath(cmd string) func(context.Context) error {
	return func(ctx context.Context) error {
		_, err := exec.LookPath(cmd)
		if err != nil {
			return err
		}
		return nil
	}
}

func checkInMainRepoOrRepoInDirectory(ctx context.Context) error {
	_, err := root.RepositoryRoot()
	if err != nil {
		ok, err := pathExists("sourcegraph")
		if !ok || err != nil {
			return errors.New("'sg setup' is not run in sourcegraph and repository is also not found in current directory")
		}
		return nil
	}
	return nil
}

func checkDevPrivateInParentOrInCurrentDirectory(context.Context) error {
	ok, err := pathExists("dev-private")
	if ok && err == nil {
		return nil
	}
	wd, err := os.Getwd()
	if err != nil {
		return errors.Wrap(err, "failed to check for dev-private repository")
	}

	p := filepath.Join(wd, "..", "dev-private")
	ok, err = pathExists(p)
	if ok && err == nil {
		return nil
	}
	return errors.New("could not find dev-private repository either in current directory or one above")
}

func checkPostgresConnection(ctx context.Context) error {
	// This check runs only in the `sourcegraph/sourcegraph` repository, so
	// we try to parse the globalConf and use its `Env` to configure the
	// Postgres connection.
	ok, _ := parseConf(*configFlag, *overwriteConfigFlag)
	if !ok {
		return errors.New("failed to read sg.config.yaml. This step of `sg setup` needs to be run in the `sourcegraph` repository")
	}

	getEnv := func(key string) string {
		// First look into process env, emulating the logic in makeEnv used
		// in internal/run/run.go
		val, ok := os.LookupEnv(key)
		if ok {
			return val
		}
		// Otherwise check in globalConf.Env
		return globalConf.Env[key]
	}

	dns := postgresdsn.New("", "", getEnv)
	conn, err := pgx.Connect(ctx, dns)
	if err != nil {
		return errors.Wrap(err, "failed to connect to Postgres database")
	}
	defer conn.Close(ctx)

	var result int
	row := conn.QueryRow(ctx, "SELECT 1;")
	if err := row.Scan(&result); err != nil {
		return errors.Wrap(err, "failed to read from Postgres database")
	}
	if result != 1 {
		return errors.New("failed to read a test value from Postgres database")
	}
	return nil
}

func checkRedisConnection(context.Context) error {
	conn, err := redis.Dial("tcp", ":6379", redis.DialConnectTimeout(5*time.Second))
	if err != nil {
		return errors.Wrap(err, "failed to connect to Redis at 127.0.0.1:6379")
	}

	if _, err := conn.Do("SET", "sg-setup", "was-here"); err != nil {
		return errors.Wrap(err, "failed to write to Redis at 127.0.0.1:6379")
	}

	retval, err := redis.String(conn.Do("GET", "sg-setup"))
	if err != nil {
		return errors.Wrap(err, "failed to read from Redis at 127.0.0.1:6379")
	}

	if retval != "was-here" {
		return errors.New("failed to test write in Redis")
	}
	return nil
}

type dependencyCheck func(context.Context) error

type dependency struct {
	name string

	check dependencyCheck

	onlyEmployees bool

	err error

	instructionsComment    string
	instructionsCommands   string
	requiresSgSetupRestart bool
}

func (d *dependency) IsMet() bool { return d.err == nil }

func (d *dependency) Update(ctx context.Context) {
	d.err = nil
	d.err = d.check(ctx)
}

type dependencyCategory struct {
	name         string
	dependencies []*dependency

	autoFixing         bool
	requiresRepository bool
}

func (cat *dependencyCategory) CombinedState() bool {
	for _, dep := range cat.dependencies {
		if !dep.IsMet() {
			return false
		}
	}
	return true
}

func (cat *dependencyCategory) CombinedStateNonEmployees() bool {
	for _, dep := range cat.dependencies {
		if !dep.IsMet() && !dep.onlyEmployees {
			return false
		}
	}
	return true
}

func getNumberOutOf(numbers []int) (int, error) {
	var strs []string
	var idx = make(map[int]struct{})
	for _, num := range numbers {
		strs = append(strs, fmt.Sprintf("%d", num+1))
		idx[num+1] = struct{}{}
	}

	for {
		fmt.Printf("[%s]: ", strings.Join(strs, ","))
		var num int
		_, err := fmt.Scan(&num)
		if err != nil {
			return 0, err
		}

		if _, ok := idx[num]; ok {
			return num - 1, nil
		}
		fmt.Printf("%d is an invalid choice :( Let's try again?\n", num)
	}
}

func waitForReturn() { fmt.Scanln() }

func getChoice(choices map[int]string) (int, error) {
	for {
		out.Write("")
		writeFingerPointingLine("What do you want to do?")

		for i := 0; i < len(choices); i++ {
			num := i + 1
			desc, ok := choices[num]
			if !ok {
				return 0, errors.Newf("internal error: %d not found in provided choices", i)
			}
			out.Writef("%s[%d]%s: %s", output.StyleBold, num, output.StyleReset, desc)
		}

		fmt.Printf("Enter choice: ")

		var s int
		_, err := fmt.Scan(&s)
		if err != nil {
			return 0, err
		}

		if _, ok := choices[s]; ok {
			return s, nil
		}
		writeFailureLine("Invalid choice")
	}
}

func retryCheck(check dependencyCheck, retries int, sleep time.Duration) dependencyCheck {
	return func(ctx context.Context) (err error) {
		for i := 0; i < retries; i++ {
			err = check(ctx)
			if err != nil {
				return err
			}
			time.Sleep(sleep)
		}
		return err
	}
}

func wrapCheckErr(check dependencyCheck, message string) dependencyCheck {
	return func(ctx context.Context) error {
		err := check(ctx)
		if err != nil {
			return errors.Wrap(err, message)
		}
		return nil
	}
}

func checkCaddyTrusted(ctx context.Context) error {
	certPath, err := caddySourcegraphCertificatePath()
	if err != nil {
		return errors.Wrap(err, "failed to determine path where proxy stores certificates")
	}

	ok, err := pathExists(certPath)
	if !ok || err != nil {
		return errors.New("sourcegraph.test certificate not found. highly likely it's not trusted by system")
	}

	rawCert, err := os.ReadFile(certPath)
	if err != nil {
		return errors.Wrap(err, "could not read certificate")
	}

	cert, err := pemDecodeSingleCert(rawCert)
	if err != nil {
		return errors.Wrap(err, "decoding cert failed")
	}

	if trusted(cert) {
		return nil
	}
	return errors.New("doesn't look like certificate is trusted")
}

// caddyAppDataDir returns the location of the sourcegraph.test certificate
// that Caddy created or would create.
//
// It's copy&pasted&modified from here: https://sourcegraph.com/github.com/caddyserver/caddy@9ee68c1bd57d72e8a969f1da492bd51bfa5ed9a0/-/blob/storage.go?L114
func caddySourcegraphCertificatePath() (string, error) {
	if basedir := os.Getenv("XDG_DATA_HOME"); basedir != "" {
		return filepath.Join(basedir, "caddy"), nil
	}

	home, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}

	var appDataDir string
	switch runtime.GOOS {
	case "darwin":
		appDataDir = filepath.Join(home, "Library", "Application Support", "Caddy")
	case "linux":
		appDataDir = filepath.Join(home, ".local", "share", "caddy")
	default:
		return "", errors.Newf("unsupported OS: %s", runtime.GOOS)
	}

	return filepath.Join(appDataDir, "pki", "authorities", "local", "root.crt"), nil
}

func trusted(cert *x509.Certificate) bool {
	chains, err := cert.Verify(x509.VerifyOptions{})
	return len(chains) > 0 && err == nil
}

func pemDecodeSingleCert(pemDER []byte) (*x509.Certificate, error) {
	pemBlock, _ := pem.Decode(pemDER)
	if pemBlock == nil {
		return nil, fmt.Errorf("no PEM block found")
	}
	if pemBlock.Type != "CERTIFICATE" {
		return nil, fmt.Errorf("expected PEM block type to be CERTIFICATE, but got '%s'", pemBlock.Type)
	}
	return x509.ParseCertificate(pemBlock.Bytes)
}
