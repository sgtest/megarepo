package cloudsql

import (
	"context"
	"database/sql"
	"fmt"
	"net"

	"cloud.google.com/go/cloudsqlconn"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/jackc/pgx/v5/stdlib"
	"go.opentelemetry.io/otel"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/codes"
	"go.opentelemetry.io/otel/trace"

	"github.com/sourcegraph/sourcegraph/lib/errors"
)

var tracer = otel.GetTracerProvider().Tracer("msp/cloudsql/pgx")

type pgxTracer struct{}

// Select tracing hooks we want to implement.
var _ pgx.QueryTracer = pgxTracer{}
var _ pgx.ConnectTracer = pgxTracer{}

// TraceQueryStart is called at the beginning of Query, QueryRow, and Exec calls. The returned context is used for the
// rest of the call and will be passed to TraceQueryEnd.
func (pgxTracer) TraceQueryStart(ctx context.Context, _ *pgx.Conn, data pgx.TraceQueryStartData) context.Context {
	ctx, _ = tracer.Start(ctx, "pgx.Query", trace.WithAttributes(
		attribute.String("query", data.SQL),
		attribute.Int("args.len", len(data.Args)),
	))
	return ctx
}

func (pgxTracer) TraceQueryEnd(ctx context.Context, conn *pgx.Conn, data pgx.TraceQueryEndData) {
	span := trace.SpanFromContext(ctx)
	defer span.End()

	span.SetAttributes(
		attribute.String("command_tag", data.CommandTag.String()),
		attribute.Int64("rows_affected", data.CommandTag.RowsAffected()),
	)
	switch {
	case data.CommandTag.Insert():
		span.SetName("pgx.Query: INSERT")
	case data.CommandTag.Update():
		span.SetName("pgx.Query: UPDATE")
	case data.CommandTag.Delete():
		span.SetName("pgx.Query: DELETE")
	case data.CommandTag.Select():
		span.SetName("pgx.Query: SELECT")
	}

	if data.Err != nil {
		span.SetStatus(codes.Error, data.Err.Error())
	}
}

func (pgxTracer) TraceConnectStart(ctx context.Context, data pgx.TraceConnectStartData) context.Context {
	ctx, _ = tracer.Start(ctx, "pgx.Connect", trace.WithAttributes(
		attribute.String("database", data.ConnConfig.Database),
		attribute.String("instance", fmt.Sprintf("%s:%d", data.ConnConfig.Host, data.ConnConfig.Port)),
		attribute.String("user", data.ConnConfig.User)))
	return ctx
}

func (pgxTracer) TraceConnectEnd(ctx context.Context, data pgx.TraceConnectEndData) {
	span := trace.SpanFromContext(ctx)
	defer span.End()

	if data.Err != nil {
		span.SetStatus(codes.Error, data.Err.Error())
	}
}

type ConnConfig struct {
	// ConnectionName is the CloudSQL connection name,
	// e.g. '${project}:${region}:${instance}'
	ConnectionName *string
	// User is the Cloud SQL user to connect as, e.g. 'test-sa@test-project.iam'
	User *string
	// Database to connect to.
	Database string
	// DialOptions are any additional options to pass to the underlying
	// cloud-sql-proxy driver.
	DialOptions []cloudsqlconn.DialOption
}

// Open opens a *sql.DB connection to the Cloud SQL instance specified by the
// ConnConfig.
//
// 🔔 If you are connecting to a MSP-provisioned Cloud SQL instance,
// DO NOT use this - instead, use runtime.Contract.PostgreSQL.OpenDatabase
// instead.
func Open(
	ctx context.Context,
	cfg ConnConfig,
) (*sql.DB, error) {
	config, err := getCloudSQLConnConfig(ctx, cfg)
	if err != nil {
		return nil, errors.Wrap(err, "get CloudSQL connection config")
	}
	return sql.Open("pgx", stdlib.RegisterConnConfig(config.ConnConfig))
}

// GetConnectionPool is an alternative to OpenDatabase that returns a
// github.com/jackc/pgx/v5/pgxpool to the CloudSQL instance specified by
// the ConnConfig, for services that prefer to use 'pgx' directly. A pool returns
// without waiting for any connections to be established. Acquire a connection
// immediately after creating the pool to check if a connection can successfully
// be established.
//
// 🔔 If you are connecting to a MSP-provisioned Cloud SQL instance,
// DO NOT use this - instead, use runtime.Contract.PostgreSQL.GetConnectionPool
// instead.
func GetConnectionPool(
	ctx context.Context,
	cfg ConnConfig,
) (*pgxpool.Pool, error) {
	config, err := getCloudSQLConnConfig(ctx, cfg)
	if err != nil {
		return nil, errors.Wrap(err, "get CloudSQL connection config")
	}
	return pgxpool.NewWithConfig(ctx, config)
}

// getCloudSQLConnConfig generates a pgx connection configuration for using
// a Cloud SQL instance using IAM auth.
func getCloudSQLConnConfig(
	ctx context.Context,
	cfg ConnConfig,
) (*pgxpool.Config, error) {
	if cfg.ConnectionName == nil || cfg.User == nil {
		return nil, errors.New("missing required PostgreSQL configuration")
	}

	// https://github.com/GoogleCloudPlatform/cloud-sql-go-connector?tab=readme-ov-file#automatic-iam-database-authentication
	dsn := fmt.Sprintf("user=%s dbname=%s", *cfg.User, cfg.Database)
	config, err := pgxpool.ParseConfig(dsn)
	if err != nil {
		return nil, errors.Wrap(err, "pgx.ParseConfig")
	}
	customDialer, err := cloudsqlconn.NewDialer(ctx,
		// always the case when using Cloud SQL in MSP
		cloudsqlconn.WithIAMAuthN(),
		// allow passthrough of additional dial options
		cloudsqlconn.WithDefaultDialOptions(cfg.DialOptions...))
	if err != nil {
		return nil, errors.Wrap(err, "cloudsqlconn.NewDialer")
	}
	// Use the Cloud SQL connector to handle connecting to the instance.
	// This approach does *NOT* require the Cloud SQL proxy.
	config.ConnConfig.DialFunc = func(ctx context.Context, _, _ string) (net.Conn, error) {
		return customDialer.Dial(ctx, *cfg.ConnectionName)
	}
	// Attach tracing
	config.ConnConfig.Tracer = pgxTracer{}

	return config, nil
}
