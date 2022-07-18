package cliutil

import (
	"context"
	"flag"
	"strconv"
	"time"

	"github.com/urfave/cli/v2"

	"github.com/sourcegraph/log"

	"github.com/sourcegraph/sourcegraph/internal/database"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/migration/runner"
	"github.com/sourcegraph/sourcegraph/internal/database/migration/schemas"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/oobmigration"
	"github.com/sourcegraph/sourcegraph/lib/errors"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

type actionFunction func(ctx context.Context, cmd *cli.Context, out *output.Output) error

// makeAction creates a new migration action function. It is expected that these
// commands accept zero arguments and define their own flags.
func makeAction(outFactory OutputFactory, f actionFunction) func(cmd *cli.Context) error {
	return func(cmd *cli.Context) error {
		if cmd.NArg() != 0 {
			return flagHelp(outFactory(), "too many arguments")
		}

		return f(cmd.Context, cmd, outFactory())
	}
}

// flagHelp returns an error that prints the specified error message with usage text.
func flagHelp(out *output.Output, message string, args ...any) error {
	out.WriteLine(output.Linef("", output.StyleWarning, "ERROR: "+message, args...))
	return flag.ErrHelp
}

// setupRunner initializes and returns the runner associated witht the given schema.
func setupRunner(ctx context.Context, factory RunnerFactory, schemaNames ...string) (Runner, error) {
	runner, err := factory(ctx, schemaNames)
	if err != nil {
		return nil, err
	}

	return runner, nil
}

// setupStore initializes and returns the store associated witht the given schema.
func setupStore(ctx context.Context, factory RunnerFactory, schemaName string) (Runner, Store, error) {
	runner, err := setupRunner(ctx, factory, schemaName)
	if err != nil {
		return nil, nil, err
	}

	store, err := runner.Store(ctx, schemaName)
	if err != nil {
		return nil, nil, err
	}

	return runner, store, nil
}

// sanitizeSchemaNames sanitizies the given string slice from the user.
func sanitizeSchemaNames(schemaNames []string) ([]string, error) {
	if len(schemaNames) == 1 && schemaNames[0] == "" {
		schemaNames = nil
	}

	if len(schemaNames) == 1 && schemaNames[0] == "all" {
		return schemas.SchemaNames, nil
	}

	return schemaNames, nil
}

// parseTargets parses the given strings as integers.
func parseTargets(targets []string) ([]int, error) {
	if len(targets) == 1 && targets[0] == "" {
		targets = nil
	}

	versions := make([]int, 0, len(targets))
	for _, target := range targets {
		version, err := strconv.Atoi(target)
		if err != nil {
			return nil, err
		}

		versions = append(versions, version)
	}

	return versions, nil
}

// getPivilegedModeFromFlags transforms the given flags into an equivalent PrivilegedMode value. A user error is
// returned if the supplied flags form an invalid state.
func getPivilegedModeFromFlags(cmd *cli.Context, out *output.Output, unprivilegedOnlyFlag, noopPrivilegedFlag *cli.BoolFlag) (runner.PrivilegedMode, error) {
	unprivilegedOnly := unprivilegedOnlyFlag.Get(cmd)
	noopPrivileged := noopPrivilegedFlag.Get(cmd)
	if unprivilegedOnly && noopPrivileged {
		return runner.InvalidPrivilegedMode, flagHelp(out, "-unprivileged-only and -noop-privileged are mutually exclusive")
	}

	if unprivilegedOnly {
		return runner.RefusePrivilegedMigrations, nil
	}
	if noopPrivileged {
		return runner.NoopPrivilegedMigrations, nil
	}

	return runner.ApplyPrivilegedMigrations, nil
}

func extractDatabase(ctx context.Context, r Runner) (database.DB, error) {
	store, err := r.Store(ctx, "frontend")
	if err != nil {
		return nil, err
	}

	// NOTE: The migration runner package cannot import basestore without
	// creating a cyclic import in db connection packages. Hence, we cannot
	// embed basestore.ShareableStore here and must "backdoor" extract the
	// database connection.
	shareableStore, ok := basestore.Raw(store)
	if !ok {
		return nil, errors.New("store does not support direct database handle access")
	}

	return database.NewDB(log.Scoped("migrator", ""), shareableStore), nil
}

var migratorObservationContext = &observation.TestContext

func outOfBandMigrationRunner(db database.DB) *oobmigration.Runner {
	return oobmigration.NewRunnerWithDB(db, time.Second, migratorObservationContext)
}
