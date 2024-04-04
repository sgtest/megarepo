package contract

import (
	"bytes"
	"context"
	"database/sql"
	"fmt"
	"net"
	"text/template"

	"cloud.google.com/go/cloudsqlconn"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/stdlib"
	"github.com/sourcegraph/sourcegraph/lib/errors"
)

type postgreSQLContract struct {
	customDSNTemplate *string

	instanceConnectionName *string
	instanceConnectionUser *string
}

func loadPostgreSQLContract(env *Env) postgreSQLContract {
	return postgreSQLContract{
		customDSNTemplate: env.GetOptional("PGDSN",
			"custom PostgreSQL DSN with templatized database, e.g. 'user=foo database={{ .Database }}'"),

		instanceConnectionName: env.GetOptional("PGINSTANCE", "Cloud SQL instance connection name"),
		instanceConnectionUser: env.GetOptional("PGUSER", "Cloud SQL user"),
	}
}

// Configured indicates if a PostgreSQL instance is configured for use. It does
// not guarantee the presence of any databases within the instance.
func (c postgreSQLContract) Configured() bool {
	return c.customDSNTemplate != nil ||
		(c.instanceConnectionName != nil && c.instanceConnectionUser == nil)
}

// OpenDatabase returns a standard library DB pointing to the configured
// PostgreSQL database. In MSP, we connect to a Cloud SQL instance over IAM auth.
//
// In development, the connection can be overridden with the PGDSN environment
// variable.
func (c postgreSQLContract) OpenDatabase(ctx context.Context, database string) (*sql.DB, error) {
	if c.customDSNTemplate != nil {
		tmpl, err := template.New("PGDSN").Parse(*c.customDSNTemplate)
		if err != nil {
			return nil, errors.Wrap(err, "PGDSN is not a valid template")
		}
		var dsn bytes.Buffer
		if err := tmpl.Execute(&dsn, struct{ Database string }{Database: database}); err != nil {
			return nil, errors.Wrap(err, "PGDSN template is invalid")
		}
		return sql.Open("pgx", dsn.String())
	}

	config, err := c.getCloudSQLConnConfig(ctx, database)
	if err != nil {
		return nil, errors.Wrap(err, "get CloudSQL connection config")
	}
	return sql.Open("pgx", stdlib.RegisterConnConfig(config))
}

// getCloudSQLConnConfig generates a pgx connection configuration for using
// a Cloud SQL instance using IAM auth.
func (c postgreSQLContract) getCloudSQLConnConfig(ctx context.Context, database string) (*pgx.ConnConfig, error) {
	if c.instanceConnectionName == nil || c.instanceConnectionUser == nil {
		return nil, errors.New("missing required PostgreSQL configuration")
	}

	// https://github.com/GoogleCloudPlatform/cloud-sql-go-connector?tab=readme-ov-file#automatic-iam-database-authentication
	dsn := fmt.Sprintf("user=%s dbname=%s", *c.instanceConnectionUser, database)
	config, err := pgx.ParseConfig(dsn)
	if err != nil {
		return nil, errors.Wrap(err, "pgx.ParseConfig")
	}
	d, err := cloudsqlconn.NewDialer(ctx,
		cloudsqlconn.WithIAMAuthN(),
		// MSP uses private IP
		cloudsqlconn.WithDefaultDialOptions(cloudsqlconn.WithPrivateIP()))
	if err != nil {
		return nil, errors.Wrap(err, "cloudsqlconn.NewDialer")
	}
	// Use the Cloud SQL connector to handle connecting to the instance.
	// This approach does *NOT* require the Cloud SQL proxy.
	config.DialFunc = func(ctx context.Context, _, _ string) (net.Conn, error) {
		return d.Dial(ctx, *c.instanceConnectionName)
	}
	return config, nil
}
