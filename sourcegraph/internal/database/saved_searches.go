package database

import (
	"context"
	"database/sql"
	"sync"

	"github.com/keegancsmith/sqlf"
	otlog "github.com/opentracing/opentracing-go/log"
	"github.com/pkg/errors"

	"github.com/sourcegraph/sourcegraph/internal/api"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/trace"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

type SavedSearchStore struct {
	*basestore.Store

	once sync.Once
}

// SavedSearches instantiates and returns a new SavedSearchStore with prepared statements.
func SavedSearches(db dbutil.DB) *SavedSearchStore {
	return &SavedSearchStore{Store: basestore.NewWithDB(db, sql.TxOptions{})}
}

// NewSavedSearchStoreWithDB instantiates and returns a new SavedSearchStore using the other store handle.
func SavedSearchesWith(other basestore.ShareableStore) *SavedSearchStore {
	return &SavedSearchStore{Store: basestore.NewWithHandle(other.Handle())}
}

func (s *SavedSearchStore) With(other basestore.ShareableStore) *SavedSearchStore {
	return &SavedSearchStore{Store: s.Store.With(other)}
}

func (s *SavedSearchStore) Transact(ctx context.Context) (*SavedSearchStore, error) {
	txBase, err := s.Store.Transact(ctx)
	return &SavedSearchStore{Store: txBase}, err
}

// ensureStore instantiates a basestore.Store if necessary, using the dbconn.Global handle.
// This function ensures access to dbconn happens after the rest of the code or tests have
// initialized it.
func (s *SavedSearchStore) ensureStore() {
	s.once.Do(func() {
		if s.Store == nil {
			s.Store = basestore.NewWithDB(dbconn.Global, sql.TxOptions{})
		}
	})
}

// IsEmpty tells if there are no saved searches (at all) on this Sourcegraph
// instance.
func (s *SavedSearchStore) IsEmpty(ctx context.Context) (bool, error) {
	s.ensureStore()

	q := `SELECT true FROM saved_searches LIMIT 1`
	var isNotEmpty bool
	err := s.Handle().DB().QueryRowContext(ctx, q).Scan(&isNotEmpty)
	if err != nil {
		if err == sql.ErrNoRows {
			return true, nil
		}
		return false, err
	}
	return false, nil
}

// ListAll lists all the saved searches on an instance.
//
// 🚨 SECURITY: This method does NOT verify the user's identity or that the
// user is an admin. It is the callers responsibility to ensure that only users
// with the proper permissions can access the returned saved searches.
func (s *SavedSearchStore) ListAll(ctx context.Context) (savedSearches []api.SavedQuerySpecAndConfig, err error) {
	if Mocks.SavedSearches.ListAll != nil {
		return Mocks.SavedSearches.ListAll(ctx)
	}
	s.ensureStore()

	tr, ctx := trace.New(ctx, "database.SavedSearches.ListAll", "")
	defer func() {
		tr.SetError(err)
		tr.LogFields(otlog.Int("count", len(savedSearches)))
		tr.Finish()
	}()

	q := sqlf.Sprintf(`SELECT
		id,
		description,
		query,
		notify_owner,
		notify_slack,
		user_id,
		org_id,
		slack_webhook_url FROM saved_searches
	`)
	rows, err := s.Query(ctx, q)
	if err != nil {
		return nil, errors.Wrap(err, "QueryContext")
	}

	for rows.Next() {
		var sq api.SavedQuerySpecAndConfig
		if err := rows.Scan(
			&sq.Config.Key,
			&sq.Config.Description,
			&sq.Config.Query,
			&sq.Config.Notify,
			&sq.Config.NotifySlack,
			&sq.Config.UserID,
			&sq.Config.OrgID,
			&sq.Config.SlackWebhookURL); err != nil {
			return nil, errors.Wrap(err, "Scan")
		}
		sq.Spec.Key = sq.Config.Key
		if sq.Config.UserID != nil {
			sq.Spec.Subject.User = sq.Config.UserID
		} else if sq.Config.OrgID != nil {
			sq.Spec.Subject.Org = sq.Config.OrgID
		}

		savedSearches = append(savedSearches, sq)
	}
	return savedSearches, nil
}

// GetByID returns the saved search with the given ID.
//
// 🚨 SECURITY: This method does NOT verify the user's identity or that the
// user is an admin. It is the callers responsibility to ensure this response
// only makes it to users with proper permissions to access the saved search.
func (s *SavedSearchStore) GetByID(ctx context.Context, id int32) (*api.SavedQuerySpecAndConfig, error) {
	if Mocks.SavedSearches.GetByID != nil {
		return Mocks.SavedSearches.GetByID(ctx, id)
	}
	s.ensureStore()

	var sq api.SavedQuerySpecAndConfig
	err := s.Handle().DB().QueryRowContext(ctx, `SELECT
		id,
		description,
		query,
		notify_owner,
		notify_slack,
		user_id,
		org_id,
		slack_webhook_url
		FROM saved_searches WHERE id=$1`, id).Scan(
		&sq.Config.Key,
		&sq.Config.Description,
		&sq.Config.Query,
		&sq.Config.Notify,
		&sq.Config.NotifySlack,
		&sq.Config.UserID,
		&sq.Config.OrgID,
		&sq.Config.SlackWebhookURL)
	if err != nil {
		return nil, err
	}
	sq.Spec.Key = sq.Config.Key
	if sq.Config.UserID != nil {
		sq.Spec.Subject.User = sq.Config.UserID
	} else if sq.Config.OrgID != nil {
		sq.Spec.Subject.Org = sq.Config.OrgID
	}
	return &sq, err
}

// ListSavedSearchesByUserID lists all the saved searches associated with a
// user, including saved searches in organizations the user is a member of.
//
// 🚨 SECURITY: This method does NOT verify the user's identity or that the
// user is an admin. It is the callers responsibility to ensure that only the
// specified user or users with proper permissions can access the returned
// saved searches.
func (s *SavedSearchStore) ListSavedSearchesByUserID(ctx context.Context, userID int32) ([]*types.SavedSearch, error) {
	if Mocks.SavedSearches.ListSavedSearchesByUserID != nil {
		return Mocks.SavedSearches.ListSavedSearchesByUserID(ctx, userID)
	}
	s.ensureStore()

	var savedSearches []*types.SavedSearch
	orgs, err := OrgsWith(s).GetByUserID(ctx, userID)
	if err != nil {
		return nil, err
	}
	var orgIDs []int32
	for _, org := range orgs {
		orgIDs = append(orgIDs, org.ID)
	}
	var orgConditions []*sqlf.Query
	for _, orgID := range orgIDs {
		orgConditions = append(orgConditions, sqlf.Sprintf("org_id=%d", orgID))
	}
	conds := sqlf.Sprintf("WHERE user_id=%d", userID)

	if len(orgConditions) > 0 {
		conds = sqlf.Sprintf("%v OR %v", conds, sqlf.Join(orgConditions, " OR "))
	}

	query := sqlf.Sprintf(`SELECT
		id,
		description,
		query,
		notify_owner,
		notify_slack,
		user_id,
		org_id,
		slack_webhook_url
		FROM saved_searches %v`, conds)

	rows, err := s.Query(ctx, query)
	if err != nil {
		return nil, errors.Wrap(err, "QueryContext(2)")
	}
	for rows.Next() {
		var ss types.SavedSearch
		if err := rows.Scan(&ss.ID, &ss.Description, &ss.Query, &ss.Notify, &ss.NotifySlack, &ss.UserID, &ss.OrgID, &ss.SlackWebhookURL); err != nil {
			return nil, errors.Wrap(err, "Scan(2)")
		}
		savedSearches = append(savedSearches, &ss)
	}
	return savedSearches, nil
}

// ListSavedSearchesByUserID lists all the saved searches associated with an
// organization.
//
// 🚨 SECURITY: This method does NOT verify the user's identity or that the
// user is an admin. It is the callers responsibility to ensure only admins or
// members of the specified organization can access the returned saved
// searches.
func (s *SavedSearchStore) ListSavedSearchesByOrgID(ctx context.Context, orgID int32) ([]*types.SavedSearch, error) {
	s.ensureStore()

	var savedSearches []*types.SavedSearch
	conds := sqlf.Sprintf("WHERE org_id=%d", orgID)
	query := sqlf.Sprintf(`SELECT
		id,
		description,
		query,
		notify_owner,
		notify_slack,
		user_id,
		org_id,
		slack_webhook_url
		FROM saved_searches %v`, conds)

	rows, err := s.Query(ctx, query)
	if err != nil {
		return nil, errors.Wrap(err, "QueryContext")
	}
	for rows.Next() {
		var ss types.SavedSearch
		if err := rows.Scan(&ss.ID, &ss.Description, &ss.Query, &ss.Notify, &ss.NotifySlack, &ss.UserID, &ss.OrgID, &ss.SlackWebhookURL); err != nil {
			return nil, errors.Wrap(err, "Scan")
		}

		savedSearches = append(savedSearches, &ss)
	}
	return savedSearches, nil
}

// Create creates a new saved search with the specified parameters. The ID
// field must be zero, or an error will be returned.
//
// 🚨 SECURITY: This method does NOT verify the user's identity or that the
// user is an admin. It is the callers responsibility to ensure the user has
// proper permissions to create the saved search.
func (s *SavedSearchStore) Create(ctx context.Context, newSavedSearch *types.SavedSearch) (savedQuery *types.SavedSearch, err error) {
	if Mocks.SavedSearches.Create != nil {
		return Mocks.SavedSearches.Create(ctx, newSavedSearch)
	}
	s.ensureStore()

	if newSavedSearch.ID != 0 {
		return nil, errors.New("newSavedSearch.ID must be zero")
	}

	tr, ctx := trace.New(ctx, "database.SavedSearches.Create", "")
	defer func() {
		tr.SetError(err)
		tr.Finish()
	}()

	savedQuery = &types.SavedSearch{
		Description: newSavedSearch.Description,
		Query:       newSavedSearch.Query,
		Notify:      newSavedSearch.Notify,
		NotifySlack: newSavedSearch.NotifySlack,
		UserID:      newSavedSearch.UserID,
		OrgID:       newSavedSearch.OrgID,
	}

	err = s.Handle().DB().QueryRowContext(ctx, `INSERT INTO saved_searches(
			description,
			query,
			notify_owner,
			notify_slack,
			user_id,
			org_id
		) VALUES($1, $2, $3, $4, $5, $6) RETURNING id`,
		newSavedSearch.Description,
		savedQuery.Query,
		newSavedSearch.Notify,
		newSavedSearch.NotifySlack,
		newSavedSearch.UserID,
		newSavedSearch.OrgID,
	).Scan(&savedQuery.ID)
	if err != nil {
		return nil, err
	}
	return savedQuery, nil
}

// Update updates an existing saved search.
//
// 🚨 SECURITY: This method does NOT verify the user's identity or that the
// user is an admin. It is the callers responsibility to ensure the user has
// proper permissions to perform the update.
func (s *SavedSearchStore) Update(ctx context.Context, savedSearch *types.SavedSearch) (savedQuery *types.SavedSearch, err error) {
	if Mocks.SavedSearches.Update != nil {
		return Mocks.SavedSearches.Update(ctx, savedSearch)
	}
	s.ensureStore()

	tr, ctx := trace.New(ctx, "database.SavedSearches.Update", "")
	defer func() {
		tr.SetError(err)
		tr.Finish()
	}()

	savedQuery = &types.SavedSearch{
		Description:     savedSearch.Description,
		Query:           savedSearch.Query,
		Notify:          savedSearch.Notify,
		NotifySlack:     savedSearch.NotifySlack,
		UserID:          savedSearch.UserID,
		OrgID:           savedSearch.OrgID,
		SlackWebhookURL: savedSearch.SlackWebhookURL,
	}

	fieldUpdates := []*sqlf.Query{
		sqlf.Sprintf("updated_at=now()"),
		sqlf.Sprintf("description=%s", savedSearch.Description),
		sqlf.Sprintf("query=%s", savedSearch.Query),
		sqlf.Sprintf("notify_owner=%t", savedSearch.Notify),
		sqlf.Sprintf("notify_slack=%t", savedSearch.NotifySlack),
		sqlf.Sprintf("user_id=%v", savedSearch.UserID),
		sqlf.Sprintf("org_id=%v", savedSearch.OrgID),
		sqlf.Sprintf("slack_webhook_url=%v", savedSearch.SlackWebhookURL),
	}

	updateQuery := sqlf.Sprintf(`UPDATE saved_searches SET %s WHERE ID=%v RETURNING id`, sqlf.Join(fieldUpdates, ", "), savedSearch.ID)
	if err := s.QueryRow(ctx, updateQuery).Scan(&savedQuery.ID); err != nil {
		return nil, err
	}
	return savedQuery, nil
}

// Delete hard-deletes an existing saved search.
//
// 🚨 SECURITY: This method does NOT verify the user's identity or that the
// user is an admin. It is the callers responsibility to ensure the user has
// proper permissions to perform the delete.
func (s *SavedSearchStore) Delete(ctx context.Context, id int32) (err error) {
	if Mocks.SavedSearches.Delete != nil {
		return Mocks.SavedSearches.Delete(ctx, id)
	}
	s.ensureStore()

	tr, ctx := trace.New(ctx, "database.SavedSearches.Delete", "")
	defer func() {
		tr.SetError(err)
		tr.Finish()
	}()
	_, err = s.Handle().DB().ExecContext(ctx, `DELETE FROM saved_searches WHERE ID=$1`, id)
	return err
}
