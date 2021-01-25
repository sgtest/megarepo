package database

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"sync"

	"github.com/jackc/pgconn"
	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/dbconn"
	"github.com/sourcegraph/sourcegraph/internal/database/dbutil"
	"github.com/sourcegraph/sourcegraph/internal/types"
)

type OrgMemberStore struct {
	*basestore.Store

	once sync.Once
}

// OrgMembers instantiates and returns a new OrgMemberStore with prepared statements.
func OrgMembers(db dbutil.DB) *OrgMemberStore {
	return &OrgMemberStore{Store: basestore.NewWithDB(db, sql.TxOptions{})}
}

// NewOrgMemberStoreWithDB instantiates and returns a new OrgMemberStore using the other store handle.
func OrgMembersWith(other basestore.ShareableStore) *OrgMemberStore {
	return &OrgMemberStore{Store: basestore.NewWithHandle(other.Handle())}
}

func (s *OrgMemberStore) With(other basestore.ShareableStore) *OrgMemberStore {
	return &OrgMemberStore{Store: s.Store.With(other)}
}

func (m *OrgMemberStore) Transact(ctx context.Context) (*OrgMemberStore, error) {
	txBase, err := m.Store.Transact(ctx)
	return &OrgMemberStore{Store: txBase}, err
}

// ensureStore instantiates a basestore.Store if necessary, using the dbconn.Global handle.
// This function ensures access to dbconn happens after the rest of the code or tests have
// initialized it.
func (m *OrgMemberStore) ensureStore() {
	m.once.Do(func() {
		if m.Store == nil {
			m.Store = basestore.NewWithDB(dbconn.Global, sql.TxOptions{})
		}
	})
}

func (m *OrgMemberStore) Create(ctx context.Context, orgID, userID int32) (*types.OrgMembership, error) {
	m.ensureStore()

	om := types.OrgMembership{
		OrgID:  orgID,
		UserID: userID,
	}
	err := m.Handle().DB().QueryRowContext(
		ctx,
		"INSERT INTO org_members(org_id, user_id) VALUES($1, $2) RETURNING id, created_at, updated_at",
		om.OrgID, om.UserID).Scan(&om.ID, &om.CreatedAt, &om.UpdatedAt)
	if err != nil {
		if pgErr, ok := err.(*pgconn.PgError); ok {
			if pgErr.ConstraintName == "org_members_org_id_user_id_key" {
				return nil, errors.New("user is already a member of the organization")
			}
		}
		return nil, err
	}
	return &om, nil
}

func (m *OrgMemberStore) GetByUserID(ctx context.Context, userID int32) ([]*types.OrgMembership, error) {
	return m.getBySQL(ctx, "INNER JOIN users ON org_members.user_id=users.id WHERE org_members.user_id=$1 AND users.deleted_at IS NULL", userID)
}

func (m *OrgMemberStore) GetByOrgIDAndUserID(ctx context.Context, orgID, userID int32) (*types.OrgMembership, error) {
	if Mocks.OrgMembers.GetByOrgIDAndUserID != nil {
		return Mocks.OrgMembers.GetByOrgIDAndUserID(ctx, orgID, userID)
	}
	return m.getOneBySQL(ctx, "INNER JOIN users ON org_members.user_id=users.id WHERE org_id=$1 AND user_id=$2 AND users.deleted_at IS NULL LIMIT 1", orgID, userID)
}

func (m *OrgMemberStore) Remove(ctx context.Context, orgID, userID int32) error {
	m.ensureStore()

	_, err := m.Handle().DB().ExecContext(ctx, "DELETE FROM org_members WHERE (org_id=$1 AND user_id=$2)", orgID, userID)
	return err
}

// GetByOrgID returns a list of all members of a given organization.
func (m *OrgMemberStore) GetByOrgID(ctx context.Context, orgID int32) ([]*types.OrgMembership, error) {
	org, err := OrgsWith(m).GetByID(ctx, orgID)
	if err != nil {
		return nil, err
	}
	return m.getBySQL(ctx, "INNER JOIN users ON org_members.user_id = users.id WHERE org_id=$1 AND users.deleted_at IS NULL ORDER BY upper(users.display_name), users.id", org.ID)
}

// ErrOrgMemberNotFound is the error that is returned when
// a user is not in an org.
type ErrOrgMemberNotFound struct {
	args []interface{}
}

func (err *ErrOrgMemberNotFound) Error() string {
	return fmt.Sprintf("org member not found: %v", err.args)
}

func (ErrOrgMemberNotFound) NotFound() bool { return true }

func (m *OrgMemberStore) getOneBySQL(ctx context.Context, query string, args ...interface{}) (*types.OrgMembership, error) {
	members, err := m.getBySQL(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	if len(members) != 1 {
		return nil, &ErrOrgMemberNotFound{args}
	}
	return members[0], nil
}

func (m *OrgMemberStore) getBySQL(ctx context.Context, query string, args ...interface{}) ([]*types.OrgMembership, error) {
	m.ensureStore()

	rows, err := m.Handle().DB().QueryContext(ctx, "SELECT org_members.id, org_members.org_id, org_members.user_id, org_members.created_at, org_members.updated_at FROM org_members "+query, args...)
	if err != nil {
		return nil, err
	}

	members := []*types.OrgMembership{}
	defer rows.Close()
	for rows.Next() {
		m := types.OrgMembership{}
		err := rows.Scan(&m.ID, &m.OrgID, &m.UserID, &m.CreatedAt, &m.UpdatedAt)
		if err != nil {
			return nil, err
		}
		members = append(members, &m)
	}
	if err = rows.Err(); err != nil {
		return nil, err
	}
	return members, nil
}

// CreateMembershipInOrgsForAllUsers causes *ALL* users to become members of every org in the
// orgNames list.
func (m *OrgMemberStore) CreateMembershipInOrgsForAllUsers(ctx context.Context, orgNames []string) error {
	if len(orgNames) == 0 {
		return nil
	}

	m.ensureStore()

	orgNameVars := []*sqlf.Query{}
	for _, orgName := range orgNames {
		orgNameVars = append(orgNameVars, sqlf.Sprintf("%s", orgName))
	}

	sqlQuery := sqlf.Sprintf(`
			WITH org_ids AS (SELECT id FROM orgs WHERE name IN (%s)),
				 user_ids AS (SELECT id FROM users WHERE deleted_at IS NULL),
				 to_join AS (SELECT org_ids.id AS org_id, user_ids.id AS user_id
						  FROM org_ids join user_ids ON true
						  LEFT JOIN org_members ON org_members.org_id=org_ids.id AND
									org_members.user_id=user_ids.id
						  WHERE org_members.id is null)
			INSERT INTO org_members(org_id,user_id) SELECT to_join.org_id, to_join.user_id FROM to_join;`,
		sqlf.Join(orgNameVars, ","))

	err := m.Exec(ctx, sqlQuery)
	return err
}
