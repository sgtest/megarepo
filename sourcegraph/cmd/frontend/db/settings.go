package db

import (
	"context"
	"database/sql"
	"fmt"

	multierror "github.com/hashicorp/go-multierror"
	"github.com/keegancsmith/sqlf"
	"github.com/sourcegraph/jsonx"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/db/dbconn"
	"github.com/sourcegraph/sourcegraph/pkg/api"
	"github.com/sourcegraph/sourcegraph/pkg/trace"
)

type settings struct{}

func (o *settings) CreateIfUpToDate(ctx context.Context, subject api.ConfigurationSubject, lastID *int32, authorUserID int32, contents string) (latestSetting *api.Settings, err error) {
	if Mocks.Settings.CreateIfUpToDate != nil {
		return Mocks.Settings.CreateIfUpToDate(ctx, subject, lastID, authorUserID, contents)
	}

	// Validate JSON syntax before saving.
	if _, errs := jsonx.Parse(contents, jsonx.ParseOptions{Comments: true, TrailingCommas: true}); len(errs) > 0 {
		return nil, fmt.Errorf("invalid settings JSON: %v", errs)
	}

	s := api.Settings{
		Subject:      subject,
		AuthorUserID: authorUserID,
		Contents:     contents,
	}

	tx, err := dbconn.Global.BeginTx(ctx, nil)
	if err != nil {
		return nil, err
	}

	defer func() {
		if err != nil {
			rollErr := tx.Rollback()
			if rollErr != nil {
				err = multierror.Append(err, rollErr)
			}
			return
		}
		err = tx.Commit()
	}()

	latestSetting, err = o.getLatest(ctx, tx, subject)
	if err != nil {
		return nil, err
	}

	creatorIsUpToDate := latestSetting != nil && lastID != nil && latestSetting.ID == *lastID
	if latestSetting == nil || creatorIsUpToDate {
		err := tx.QueryRow(
			"INSERT INTO settings(org_id, user_id, author_user_id, contents) VALUES($1, $2, $3, $4) RETURNING id, created_at",
			s.Subject.Org, s.Subject.User, s.AuthorUserID, s.Contents).Scan(&s.ID, &s.CreatedAt)
		if err != nil {
			return nil, err
		}
		latestSetting = &s
	}

	return latestSetting, nil
}

func (o *settings) GetLatest(ctx context.Context, subject api.ConfigurationSubject) (*api.Settings, error) {
	if Mocks.Settings.GetLatest != nil {
		return Mocks.Settings.GetLatest(ctx, subject)
	}

	return o.getLatest(ctx, dbconn.Global, subject)
}

// ListAll lists ALL settings (across all users, orgs, etc).
//
// 🚨 SECURITY: This method does NOT verify the user is an admin. The caller is
// responsible for ensuring this or that the response never makes it to a user.
func (o *settings) ListAll(ctx context.Context) (_ []*api.Settings, err error) {
	tr, ctx := trace.New(ctx, "db.Settings.ListAll", "")
	defer func() {
		tr.SetError(err)
		tr.Finish()
	}()

	q := sqlf.Sprintf(`
		SELECT DISTINCT
			ON (org_id, user_id, author_user_id)
			id, org_id, user_id, author_user_id, contents, created_at
			FROM settings
			ORDER BY org_id, user_id, author_user_id, id DESC
	`)
	rows, err := dbconn.Global.QueryContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
	if err != nil {
		return nil, err
	}
	return o.parseQueryRows(ctx, rows)
}

func (o *settings) getLatest(ctx context.Context, queryTarget queryable, subject api.ConfigurationSubject) (*api.Settings, error) {
	var cond *sqlf.Query
	switch {
	case subject.Org != nil:
		cond = sqlf.Sprintf("org_id=%d", *subject.Org)
	case subject.User != nil:
		cond = sqlf.Sprintf("user_id=%d", *subject.User)
	default:
		// No org and no user represents global site settings.
		cond = sqlf.Sprintf("user_id IS NULL AND org_id IS NULL")
	}

	q := sqlf.Sprintf(`
		SELECT id, org_id, user_id, author_user_id, contents, created_at FROM settings
		WHERE %s
		ORDER BY id DESC LIMIT 1`, cond)
	rows, err := queryTarget.QueryContext(ctx, q.Query(sqlf.PostgresBindVar), q.Args()...)
	if err != nil {
		return nil, err
	}
	settings, err := o.parseQueryRows(ctx, rows)
	if err != nil {
		return nil, err
	}
	if len(settings) != 1 {
		// No configuration has been set for this subject yet.
		return nil, nil
	}
	return settings[0], nil
}

// queryable allows us to reuse the same logic for certain operations both
// inside and outside an explicit transaction.
type queryable interface {
	QueryContext(ctx context.Context, query string, args ...interface{}) (*sql.Rows, error)
}

func (o *settings) parseQueryRows(ctx context.Context, rows *sql.Rows) ([]*api.Settings, error) {
	settings := []*api.Settings{}
	defer rows.Close()
	for rows.Next() {
		s := api.Settings{}
		err := rows.Scan(&s.ID, &s.Subject.Org, &s.Subject.User, &s.AuthorUserID, &s.Contents, &s.CreatedAt)
		if err != nil {
			return nil, err
		}
		if s.Subject.Org == nil && s.Subject.User == nil {
			s.Subject.Site = true
		}
		settings = append(settings, &s)
	}
	if err := rows.Err(); err != nil {
		return nil, err
	}
	return settings, nil
}
