package main

import (
	"context"
	"database/sql"
	"fmt"
	"os"

	"github.com/inconshreveable/log15"
	"github.com/opentracing/opentracing-go"
	"github.com/prometheus/client_golang/prometheus"

	connections "github.com/sourcegraph/sourcegraph/internal/database/connections/live"
	"github.com/sourcegraph/sourcegraph/internal/database/migration/cliutil"
	"github.com/sourcegraph/sourcegraph/internal/database/migration/runner"
	"github.com/sourcegraph/sourcegraph/internal/database/migration/store"
	"github.com/sourcegraph/sourcegraph/internal/database/postgresdsn"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/lib/output"
)

const appName = "migrator"

var out = output.NewOutput(os.Stdout, output.OutputOpts{
	ForceColor: true,
	ForceTTY:   true,
})

func main() {
	args := os.Args[1:]
	if len(args) == 0 {
		args = append(args, "up")
	}

	if err := mainErr(context.Background(), args); err != nil {
		fmt.Printf("error: %s\n", err)
		os.Exit(1)
	}
}

func mainErr(ctx context.Context, args []string) error {
	command := cliutil.Flags(appName, newRunFunc(), out)

	if err := command.Parse(args); err != nil {
		return err
	}

	return command.Run(ctx)
}

func newRunFunc() cliutil.RunFunc {
	observationContext := &observation.Context{
		Logger:     log15.Root(),
		Tracer:     &trace.Tracer{Tracer: opentracing.GlobalTracer()},
		Registerer: prometheus.DefaultRegisterer,
	}
	operations := store.NewOperations(observationContext)

	return func(ctx context.Context, options runner.Options) error {
		dsns, err := postgresdsn.DSNsBySchema(options.SchemaNames)
		if err != nil {
			return err
		}

		storeFactory := func(db *sql.DB, migrationsTable string) connections.Store {
			return store.NewWithDB(db, migrationsTable, operations)
		}

		return connections.RunnerFromDSNs(dsns, appName, storeFactory).Run(ctx, options)
	}
}
