# Getting started with developing Sourcegraph

The best way to become familiar with the Sourcegraph repository is by
reading the code at https://sourcegraph.com/github.com/sourcegraph/sourcegraph.

## Environment

The Sourcegraph server is actually a collection of smaller binaries, each of
which performs one task. The core entrypoint for the Sourcegraph development
server is [dev/launch.sh](../dev/launch.sh), which will initialize the environment, and start a
process manager that runs all of the binaries.

See [the Architecture doc](architecture.md) for a full description of what each
of these services does.

The sections below describe the the dependencies that you need to have to be able to run `dev/launch.sh` properly.

## Step 1: Get the code

Run this command to get the Sourcegraph source code on your local machine:

```
go get github.com/sourcegraph/sourcegraph
```

This is your "Sourcegraph repository directory".

### Step 2: Install Git, Go, Docker, Node.js, PostgreSQL, Redis

You will need Git, Go (version 1.11), Docker, PostgreSQL (version 9), Node.js
(version 8 or 10), and Redis installed to run `dev/launch.sh`.

You have two options for installing:

#### Option A: Homebrew setup for macOS

This is a streamlined setup for Mac machines.

1.  Install [Docker for Mac](https://docs.docker.com/docker-for-mac/).
2.  Install [Homebrew](https://brew.sh).
3.  Install Go, Node, PostgreSQL 9, Redis, Git with the following command:

    ```
    brew install go node redis postgresql@9.6 git gnu-sed
    ```

4.  Set up your [Go Workspace](https://golang.org/doc/code.html#Workspaces)

5.  Configure PostgreSQL and Redis to start automatically

    ```
    brew services start postgresql@9.6
    brew services start redis
    ```

    (You can stop them later by calling `stop` instead of `start` above.)

6.  Ensure `psql`, the PostgreSQL command line client, is on your `$PATH`.
    Homebrew does not put it there by default. Homebrew gives you the command to run to insert `psql` in your path in the "Caveats" section of `brew info postgresql@9.6`. Alternatively, you can use the command below. It might need to be adjusted depending on your Homebrew prefix (`/usr/local` below) and shell (bash below).

    ```bash
    hash psql || { echo 'export PATH="/usr/local/opt/postgresql@9.6/bin:$PATH"' >> ~/.bash_profile }
    source ~/.bash_profile
    ```

7.  Open a new Terminal window to ensure `psql` is now on your `$PATH`.

#### Option B: Linux / Manual Install

For Linux users or if you don't want to use Homebrew on macOS, you'll need the
following packages:

- [git](https://git-scm.com/book/en/v2/Getting-Started-Installing-Git)
- [Go](https://golang.org/doc/install) (v1.11 or higher)
- [Node JS](https://nodejs.org/en/download/) (v8.0.0 or higher)
- [make](https://www.gnu.org/software/make/)
- [Docker](https://docs.docker.com/engine/installation/) (v1.8 or higher)
  - if using Mac OS, we recommend using Docker for Mac instead of `docker-machine`
- [PostgreSQL](https://wiki.postgresql.org/wiki/Detailed_installation_guides) (v9.2 to v9.6.x)
- [Redis](http://redis.io/) (v3.0.7 or higher)

##### NodeJS on Ubuntu

Ubuntu installs a fairly old NodeJS by default. To get a more recent version:

```
curl -sL https://deb.nodesource.com/setup_10.x | sudo -E bash -
sudo apt-get install -y nodejs
```

As of this writing, `setup_8.x` also works, but you may want to prefer the newer
one.

##### Redis on Linux

You can follow these [instructions to install Redis
natively](http://redis.io/topics/quickstart). If you have Docker installed and
are running Linux, however, the easiest way to get Redis up and running is
probably:

```
dockerd # if docker isn't already running
docker run -p 6379:6379 -v $REDIS_DATA_DIR redis
```

_ `$REDIS_DATA_DIR` should be an absolute path to a folder where you intend to store Redis data._

You need to have the redis image running when you run the Sourcegraph
`dev/launch.sh` script. If you do not have docker access without root, run these
commands under `sudo`.

### Step 3: Install Yarn

Run the following command to install Yarn, a package manager for Node.js.

```
npm install -g yarn
```

## Step 4: Initialize your database

You need a fresh Postgres database, and a database user that has full ownership
of that database.

### I. Create a database for the current Unix user

If you are running on Linux, you may need to become the `postgres` user to
administer Postgres.

```bash
# Linux only
sudo su - postgres
```

After that, create the database - `createdb` with no arguments creates
a database with a name matching the current user.

```bash
createdb
```

### II. Create the Sourcegraph user and password

```
createuser --superuser sourcegraph
psql -c "ALTER USER sourcegraph WITH PASSWORD 'sourcegraph';"
```

### III. Create the Sourcegraph database

```
createdb --owner=sourcegraph --encoding=UTF8 --template=template0 sourcegraph
```

### IV. Configure database settings in your environment

The Sourcegraph server reads PostgreSQL connection
configuration from the [`PG*` environment
variables](http://www.postgresql.org/docs/current/static/libpq-envars.html); for
example, in your `~/.bashrc`:

```
export PGPORT=5432
export PGHOST=localhost
export PGUSER=sourcegraph
export PGPASSWORD=sourcegraph
export PGDATABASE=sourcegraph
export PGSSLMODE=disable
```

You can also use a tool like [`envdir`][envdir] or [a `.dotenv` file][dotenv] to
source these env vars on demand when you start the server.

[envdir]: https://cr.yp.to/daemontools/envdir.html
[dotenv]: https://github.com/joho/godotenv

### More info

For more information about data storage, [read our full PostgreSQL Guide
page][database-init].

Migrations are applied automatically.

[database-init]: ./storage.md

## Step 5: Start Docker

Start the Docker binary. You have two options:

#### Option A: Docker for Mac

This is the easy way - just launch Docker.app and wait for it to finish loading.

#### Option B: docker-machine

The Docker daemon should be running in the background, which you can test by
running `docker ps`. If you're on OS X and using `docker-machine` instead of
Docker for Mac, you may have to run:

```
docker-machine start default
eval $(docker-machine env)
```

## Step 6: Start the Server

You're finally ready to run launch.sh. In the terminal, `cd` to the directory
that contains the Sourcegraph source code, and run:

```
./dev/launch.sh
```

This will continuously compile your code and live reload your locally running
instance of Sourcegraph. Navigate your browser to http://localhost:3080 to
see if everything worked.

### Troubleshooting

#### Problems with node_modules or Javascript packages

Noticing problems with <code>node_modules/</code> or package versions? Try
running this command to clear the local package cache.

```
yarn cache clean; rm -rf node_modules web/node_modules; yarn; cd web; yarn
```

##### dial tcp 127.0.0.1:3090: connect: connection refused

This means the `frontend` server failed to start, for some reason. Look through
the previous logs for possible explanations, such as failure to contact the
`redis` server, or database migrations failing.

#### Database migration failures

While developing Sourcegraph, you may run into:

`frontend | failed to migrate the DB. Please contact hi@sourcegraph.com for further assistance:Dirty database version 1514702776. Fix and force version.`

You may have to run migrations manually. First, install the Go [migrate](https://github.com/golang-migrate/migrate/tree/master/cli#installation) CLI, and run something like:

Then try:

`dev/migrate.sh up`

If you get something like `error: Dirty database version 1514702776. Fix and force version.`, you need to roll things back and start from scratch.

```bash
dev/migrate.sh drop
dev/migrate.sh up
```

#### Internal Server Error

If you see this error when opening the app:

`500 Internal Server Error template: app.html:21:70: executing "app.html" at <version "styles/styl...>: error calling version: open ui/assets/styles/style.bundle.css: no such file or directory`

that means Webpack hasn't finished compiling the styles yet (it takes about 3 minutes).
Simply wait a little while for a message from webpack like `web | Time: 180000ms` to appear
in the terminal.

#### Increase maximum available file descriptors.

`./dev/launch.sh` may ask you to run ulimit to increase the maximum number
of available file descriptors for a process. You can make this setting
permanent for every shell session by adding the following line to your
`.*rc` file (usually `.bashrc` or `.zshrc`):

```bash
# increase max number of file descriptors for running a sourcegraph instance.
ulimit -n 10000
```

If you ever need to wipe your local database, run the following command.

```
./dev/drop-entire-local-database.sh
```

## How to Run Tests

See [testing.md](testing.md) for details.

## CPU/RAM/bandwidth usage

On first install, the program will use quite a bit of bandwidth to concurrently
download all of the Go and Node packages. After packages have been installed,
the Javascript assets will be compiled into a single Javascript file, which
can take up to 5 minutes, and can be heavy on the CPU at times.

After the initial install/compile is complete, the Docker for Mac binary uses
about 1.5GB of RAM. The numerous different Go binaries don't use that much RAM
or CPU each, about 5MB of RAM each.

## How to debug live code

How to debug a program with Visual Studio Code:

### Debug TypeScript code

Requires "Debugger for Chrome" extension.

- Quit Chrome
- Launch Chrome (Canary) from the command line with a remote debugging port:
  - Mac OS: `/Applications/Google\ Chrome\ Canary.app/Contents/MacOS/Google\ Chrome\ Canary --remote-debugging-port=9222`
  - Windows: `start chrome.exe –remote-debugging-port=9222`
  - Linux: `chromium-browser --remote-debugging-port=9222`
- Go to http://localhost:3080
- Open the Debugger in VSCode: "View" > "Debug"
- Launch the `(ui) http://localhost:3080/*` debug configuration
- Set breakpoints, enjoy

### Debug Go code

**Note: If you run into an error `could not launch process: decoding dwarf section info at offset 0x0: too short` make sure you are on the latest delve version**

- Install [Delve](https://github.com/derekparker/delve)
- Run `DELVE=frontend,searcher ./dev/launch.sh` (`DELVE` accepts a comma-separated list of components as specified in [../dev/Procfile](../dev/Procfile))
- Set a breakpoint in VS Code (there's a bug where setting the breakpoint after attaching results in "Unverified breakpoint")
- Run "Attach to $component" in the VS Code debug view
- The process should start once the debugger is attached

Known issues:

- At the time of writing there is an issue with homebrew formula so workarounds are required.
  - Use homebrew and then google any errors you encounter.
- There doesn't seem to be a clean way to stop debugging (https://github.com/derekparker/delve/issues/1057).
  - The workaround is to manually kill the process when you are done.

## Go dependency management

We use Go modules to manage Go dependencies in this repository. The CI test
suite will check whether you have updated `go.mod` and `go.sum` correctly - in
particular, running `go mod tidy && go mod vendor` should not generate a Git
diff.

## Codegen

The Sourcegraph repository relies on code generation triggered by `go generate`. Code generation is used for a variety of tasks:

- generating code for mocking interfaces
- generate wrappers for interfaces (e.g., `./server/internal/middleware/*` packages)
- pack app templates and assets into binaries

To generate everything, just run:

```
./dev/generate.sh
```

Note: Sometimes, there are erroneous diffs. This occurs for a few
reasons, none of which are legitimate (i.e., they are tech debt items
we need to address):

- The codegen tools might emit code that depends on system configuration,
  such as the system timezone or packages you have in your GOPATH. We
  need to submit PRs to the tools to eliminate these issues.
- You might have existing but gitignored files that the codegen tools
  read on your disk that other developers don't have. (This occurs for
  app assets especially.)

If you think a diff is erroneous, don't commit it. Add a tech debt
item to the issue tracker and assign the person who you think is
responsible (or ask).

## Code style guide

See [docs/style.md](style.md).

## Windows support

Running Sourcegraph on Windows is not actively tested, but should be possible within the Windows Subsystem for Linux (WSL).
Sourcegraph currently relies on Unix specifics in several places, which makes it currently not possible to run Sourcegraph directly inside Windows without WSL.
We are happy to accept contributions here! :)
