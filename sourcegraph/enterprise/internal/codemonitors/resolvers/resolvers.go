package resolvers

import (
	"context"
	"database/sql"
	"fmt"
	"time"

	"github.com/graph-gophers/graphql-go"
	"github.com/graph-gophers/graphql-go/relay"
	"github.com/keegancsmith/sqlf"

	"github.com/sourcegraph/sourcegraph/cmd/frontend/backend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend"
	"github.com/sourcegraph/sourcegraph/cmd/frontend/graphqlbackend/graphqlutil"
	cm "github.com/sourcegraph/sourcegraph/enterprise/internal/codemonitors"
	"github.com/sourcegraph/sourcegraph/internal/actor"
	"github.com/sourcegraph/sourcegraph/internal/db/dbutil"
)

// NewResolver returns a new Resolver that uses the given db
func NewResolver(db dbutil.DB) graphqlbackend.CodeMonitorsResolver {
	return &Resolver{store: cm.NewStore(db)}
}

// newResolverWithClock is used in tests to set the clock manually.
func newResolverWithClock(db dbutil.DB, clock func() time.Time) graphqlbackend.CodeMonitorsResolver {
	return &Resolver{store: cm.NewStoreWithClock(db, clock)}
}

type Resolver struct {
	store *cm.Store
}

func (r *Resolver) Now() time.Time {
	return r.store.Now()
}

func (r *Resolver) Monitors(ctx context.Context, userID int32, args *graphqlbackend.ListMonitorsArgs) (graphqlbackend.MonitorConnectionResolver, error) {
	q, err := monitorsQuery(userID, args)
	if err != nil {
		return nil, err
	}
	rows, err := r.store.Query(ctx, q)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	ms, err := scanMonitors(rows)
	if err != nil {
		return nil, err
	}
	// Hydrate the monitors with the resolver.
	for _, m := range ms {
		m.(*monitor).Resolver = r
	}
	return &monitorConnection{r, ms}, nil
}

const monitorByIDFmtStr = `
SELECT id, created_by, created_at, changed_by, changed_at, description, enabled, namespace_user_id, namespace_org_id
FROM cm_monitors
WHERE id = %s
`

func (r *Resolver) MonitorByID(ctx context.Context, ID graphql.ID) (m graphqlbackend.MonitorResolver, err error) {
	err = r.isAllowedToEdit(ctx, ID)
	if err != nil {
		return nil, err
	}
	var monitorID int64
	err = relay.UnmarshalSpec(ID, &monitorID)
	if err != nil {
		return nil, err
	}
	q := sqlf.Sprintf(monitorByIDFmtStr, monitorID)
	m, err = r.runMonitorQuery(ctx, q)
	if err != nil {
		return nil, err
	}
	// Hydrate monitor with resolver.
	m.(*monitor).Resolver = r
	return m, nil
}

func (r *Resolver) CreateCodeMonitor(ctx context.Context, args *graphqlbackend.CreateCodeMonitorArgs) (m graphqlbackend.MonitorResolver, err error) {
	err = r.isAllowedToCreate(ctx, args.Monitor.Namespace)
	if err != nil {
		return nil, err
	}
	// start transaction
	var txStore *cm.Store
	txStore, err = r.store.Transact(ctx)
	if err != nil {
		return nil, err
	}
	tx := Resolver{
		store: txStore,
	}
	defer func() { err = tx.store.Done(err) }()

	// create code monitor
	var q *sqlf.Query
	q, err = tx.createCodeMonitorQuery(ctx, args.Monitor)
	if err != nil {
		return nil, err
	}
	m, err = tx.runMonitorQuery(ctx, q)
	if err != nil {
		return nil, err
	}

	// Create trigger.
	err = tx.store.CreateTriggerQuery(ctx, m.(*monitor).id, args.Trigger)
	if err != nil {
		return nil, err
	}

	// create actions
	err = tx.store.CreateActions(ctx, args.Actions, m.(*monitor).id)
	if err != nil {
		return nil, err
	}

	// Hydrate monitor with Resolver.
	m.(*monitor).Resolver = r
	return m, nil
}

func (r *Resolver) ToggleCodeMonitor(ctx context.Context, args *graphqlbackend.ToggleCodeMonitorArgs) (graphqlbackend.MonitorResolver, error) {
	err := r.isAllowedToEdit(ctx, args.Id)
	if err != nil {
		return nil, fmt.Errorf("ToggleCodeMonitor: %w", err)
	}
	q, err := r.toggleCodeMonitorQuery(ctx, args)
	if err != nil {
		return nil, err
	}
	return r.runMonitorQuery(ctx, q)
}

func (r *Resolver) DeleteCodeMonitor(ctx context.Context, args *graphqlbackend.DeleteCodeMonitorArgs) (*graphqlbackend.EmptyResponse, error) {
	err := r.isAllowedToEdit(ctx, args.Id)
	if err != nil {
		return nil, fmt.Errorf("DeleteCodeMonitor: %w", err)
	}
	q, err := r.deleteCodeMonitorQuery(ctx, args)
	if err != nil {
		return nil, err
	}
	if err := r.store.Exec(ctx, q); err != nil {
		return nil, err
	}
	return &graphqlbackend.EmptyResponse{}, nil
}

func (r *Resolver) UpdateCodeMonitor(ctx context.Context, args *graphqlbackend.UpdateCodeMonitorArgs) (m graphqlbackend.MonitorResolver, err error) {
	err = r.isAllowedToEdit(ctx, args.Monitor.Id)
	if err != nil {
		return nil, fmt.Errorf("UpdateCodeMonitor: %w", err)
	}

	var monitorID int64
	err = relay.UnmarshalSpec(args.Monitor.Id, &monitorID)
	if err != nil {
		return nil, err
	}

	// Get all action IDs of the monitor.
	actionIDs, err := r.actionIDsForMonitorIDInt64(ctx, monitorID)
	if err != nil {
		return nil, err
	}

	toCreate, toDelete, err := splitActionIDs(ctx, args, actionIDs)
	if len(toDelete) == len(actionIDs) {
		return nil, fmt.Errorf("you tried to delete all actions, but every monitor must be connected to at least 1 action")
	}

	// Run all queries within a transaction.
	var tx *Resolver
	tx, err = r.transact(ctx)
	if err != nil {
		return nil, err
	}
	defer func() { err = tx.store.Done(err) }()

	err = tx.store.DeleteActionsInt64(ctx, toDelete, monitorID)
	if err != nil {
		return nil, err
	}
	err = tx.store.CreateActions(ctx, toCreate, monitorID)
	if err != nil {
		return nil, err
	}
	m, err = tx.updateCodeMonitor(ctx, args)
	if err != nil {
		return nil, err
	}
	// Hydrate monitor with Resolver.
	m.(*monitor).Resolver = r
	return m, nil
}

func (r *Resolver) actionIDsForMonitorIDInt64(ctx context.Context, monitorID int64) (actionIDs []graphql.ID, err error) {
	limit := 50
	var (
		ids   []graphql.ID
		after *string
		q     *sqlf.Query
	)
	// Paging.
	for {
		q, err = r.store.ReadActionEmailQuery(ctx, monitorID, &graphqlbackend.ListActionArgs{
			First: int32(limit),
			After: after,
		})
		if err != nil {
			return nil, err
		}
		es, cur, err := r.actionIDsForMonitorIDINT64SinglePage(ctx, q, limit)
		if err != nil {
			return nil, err
		}
		ids = append(ids, es...)
		if cur == nil {
			break
		}
		after = cur
	}
	return ids, nil
}

func (r *Resolver) actionIDsForMonitorIDINT64SinglePage(ctx context.Context, q *sqlf.Query, limit int) (IDs []graphql.ID, cursor *string, err error) {
	var rows *sql.Rows
	rows, err = r.store.Query(ctx, q)
	if err != nil {
		return nil, nil, err
	}
	defer rows.Close()

	var es []*cm.MonitorEmail
	es, err = cm.ScanEmails(rows)
	if err != nil {
		return nil, nil, err
	}
	IDs = make([]graphql.ID, 0, len(es))
	for _, e := range es {
		IDs = append(IDs, (&monitorEmail{MonitorEmail: e}).ID())
	}

	// Set the cursor if the result size equals limit.
	if len(IDs) == limit {
		stringID := string(IDs[len(IDs)-1])
		cursor = &stringID
	}
	return IDs, cursor, nil
}

// splitActionIDs splits actions into three buckets: create, delete and update.
// Note: args is mutated. After splitActionIDs, args only contains actions to be updated.
func splitActionIDs(ctx context.Context, args *graphqlbackend.UpdateCodeMonitorArgs, actionIDs []graphql.ID) (toCreate []*graphqlbackend.CreateActionArgs, toDelete []int64, err error) {
	aMap := make(map[graphql.ID]struct{}, len(actionIDs))
	for _, id := range actionIDs {
		aMap[id] = struct{}{}
	}
	var toUpdateActions []*graphqlbackend.EditActionArgs
	for _, a := range args.Actions {
		if a.Email.Id == nil {
			toCreate = append(toCreate, &graphqlbackend.CreateActionArgs{Email: a.Email.Update})
			continue
		}
		if _, ok := aMap[*a.Email.Id]; !ok {
			return nil, nil, fmt.Errorf("unknown ID=%s for action", *a.Email.Id)
		}
		toUpdateActions = append(toUpdateActions, a)
		delete(aMap, *a.Email.Id)
	}
	var actionID int64
	for k := range aMap {
		err = relay.UnmarshalSpec(k, &actionID)
		if err != nil {
			return nil, nil, err
		}
		toDelete = append(toDelete, actionID)
	}
	args.Actions = toUpdateActions
	return toCreate, toDelete, nil
}

func (r *Resolver) updateCodeMonitor(ctx context.Context, args *graphqlbackend.UpdateCodeMonitorArgs) (m graphqlbackend.MonitorResolver, err error) {
	var q *sqlf.Query
	// Update monitor.
	q, err = r.updateCodeMonitorQuery(ctx, args)
	if err != nil {
		return nil, err
	}
	m, err = r.runMonitorQuery(ctx, q)
	if err != nil {
		return nil, err
	}
	// Update trigger.
	err = r.store.UpdateTriggerQuery(ctx, args)
	if err != nil {
		return nil, err
	}
	// Update actions.
	if len(args.Actions) == 0 {
		return m, nil
	}
	var emailID int64
	var e *cm.MonitorEmail
	for i, action := range args.Actions {
		if action.Email == nil {
			return nil, fmt.Errorf("missing email object for action %d", i)
		}
		err = relay.UnmarshalSpec(*action.Email.Id, &emailID)
		if err != nil {
			return nil, err
		}
		err = r.store.DeleteRecipients(ctx, emailID)
		if err != nil {
			return nil, err
		}
		e, err = r.store.UpdateActionEmail(ctx, m.(*monitor).id, action)
		if err != nil {
			return nil, err
		}
		err = r.store.CreateRecipients(ctx, action.Email.Update.Recipients, e.Id)
		if err != nil {
			return nil, err
		}
	}
	return m, nil
}

func (r *Resolver) transact(ctx context.Context) (*Resolver, error) {
	txStore, err := r.store.Transact(ctx)
	if err != nil {
		return nil, err
	}
	return &Resolver{
		store: txStore,
	}, nil
}

func (r *Resolver) runMonitorQuery(ctx context.Context, q *sqlf.Query) (graphqlbackend.MonitorResolver, error) {
	rows, err := r.store.Query(ctx, q)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	ms, err := scanMonitors(rows)
	if err != nil {
		return nil, err
	}
	if len(ms) == 0 {
		return nil, fmt.Errorf("operation failed. Query should have returned 1 row")
	}
	return ms[0], nil
}

func scanMonitors(rows *sql.Rows) ([]graphqlbackend.MonitorResolver, error) {
	var ms []graphqlbackend.MonitorResolver
	for rows.Next() {
		m := &monitor{}
		if err := rows.Scan(
			&m.id,
			&m.createdBy,
			&m.createdAt,
			&m.changedBy,
			&m.changedAt,
			&m.description,
			&m.enabled,
			&m.namespaceUserID,
			&m.namespaceOrgID,
		); err != nil {
			return nil, err
		}
		ms = append(ms, m)
	}
	err := rows.Close()
	if err != nil {
		return nil, err
	}
	// Rows.Err will report the last error encountered by Rows.Scan.
	if err := rows.Err(); err != nil {
		return nil, err
	}
	return ms, nil
}

func nilOrInt32(n int32) *int32 {
	if n == 0 {
		return nil
	}
	return &n
}

var monitorColumns = []*sqlf.Query{
	sqlf.Sprintf("cm_monitors.id"),
	sqlf.Sprintf("cm_monitors.created_by"),
	sqlf.Sprintf("cm_monitors.created_at"),
	sqlf.Sprintf("cm_monitors.changed_by"),
	sqlf.Sprintf("cm_monitors.changed_at"),
	sqlf.Sprintf("cm_monitors.description"),
	sqlf.Sprintf("cm_monitors.enabled"),
	sqlf.Sprintf("cm_monitors.namespace_user_id"),
	sqlf.Sprintf("cm_monitors.namespace_org_id"),
}

func monitorsQuery(userID int32, args *graphqlbackend.ListMonitorsArgs) (*sqlf.Query, error) {
	const SelectMonitorsByOwner = `
SELECT id, created_by, created_at, changed_by, changed_at, description, enabled, namespace_user_id, namespace_org_id
FROM cm_monitors
WHERE namespace_user_id = %s
AND id > %s
ORDER BY id ASC
LIMIT %s
`
	after, err := unmarshalAfter(args.After)
	if err != nil {
		return nil, err
	}
	query := sqlf.Sprintf(
		SelectMonitorsByOwner,
		userID,
		after,
		args.First,
	)
	return query, nil
}

func (r *Resolver) createCodeMonitorQuery(ctx context.Context, args *graphqlbackend.CreateMonitorArgs) (*sqlf.Query, error) {
	const InsertCodeMonitorQuery = `
INSERT INTO cm_monitors
(created_at, created_by, changed_at, changed_by, description, enabled, namespace_user_id, namespace_org_id)
VALUES (%s,%s,%s,%s,%s,%s,%s,%s)
RETURNING %s;
`
	var userID int32
	var orgID int32
	err := graphqlbackend.UnmarshalNamespaceID(args.Namespace, &userID, &orgID)
	if err != nil {
		return nil, err
	}
	now := r.Now()
	a := actor.FromContext(ctx)
	return sqlf.Sprintf(
		InsertCodeMonitorQuery,
		now,
		a.UID,
		now,
		a.UID,
		args.Description,
		args.Enabled,
		nilOrInt32(userID),
		nilOrInt32(orgID),
		sqlf.Join(monitorColumns, ", "),
	), nil
}

func (r *Resolver) updateCodeMonitorQuery(ctx context.Context, args *graphqlbackend.UpdateCodeMonitorArgs) (*sqlf.Query, error) {
	const updateCodeMonitorQuery = `
UPDATE cm_monitors
SET description = %s,
	enabled	= %s,
	namespace_user_id = %s,
	namespace_org_id = %s,
	changed_by = %s,
	changed_at = %s
WHERE id = %s
RETURNING %s;
`
	var userID int32
	var orgID int32
	err := graphqlbackend.UnmarshalNamespaceID(args.Monitor.Update.Namespace, &userID, &orgID)
	if err != nil {
		return nil, err
	}
	now := r.Now()
	a := actor.FromContext(ctx)
	var monitorID int64
	err = relay.UnmarshalSpec(args.Monitor.Id, &monitorID)
	if err != nil {
		return nil, err
	}
	return sqlf.Sprintf(
		updateCodeMonitorQuery,
		args.Monitor.Update.Description,
		args.Monitor.Update.Enabled,
		nilOrInt32(userID),
		nilOrInt32(orgID),
		a.UID,
		now,
		monitorID,
		sqlf.Join(monitorColumns, ", "),
	), nil
}

func (r *Resolver) toggleCodeMonitorQuery(ctx context.Context, args *graphqlbackend.ToggleCodeMonitorArgs) (*sqlf.Query, error) {
	toggleCodeMonitorQuery := `
UPDATE cm_monitors
SET enabled = %s,
	changed_by = %s,
	changed_at = %s
WHERE id = %s
RETURNING %s
`
	var monitorID int64
	err := relay.UnmarshalSpec(args.Id, &monitorID)
	if err != nil {
		return nil, err
	}
	actorUID := actor.FromContext(ctx).UID
	query := sqlf.Sprintf(
		toggleCodeMonitorQuery,
		args.Enabled,
		actorUID,
		r.Now(),
		monitorID,
		sqlf.Join(monitorColumns, ", "),
	)
	return query, nil
}

func (r *Resolver) deleteCodeMonitorQuery(ctx context.Context, args *graphqlbackend.DeleteCodeMonitorArgs) (*sqlf.Query, error) {
	deleteCodeMonitorQuery := `DELETE FROM cm_monitors WHERE id = %s`
	var monitorID int64
	err := relay.UnmarshalSpec(args.Id, &monitorID)
	if err != nil {
		return nil, err
	}
	query := sqlf.Sprintf(
		deleteCodeMonitorQuery,
		monitorID,
	)
	return query, nil
}

// isAllowedToEdit checks whether an actor is allowed to edit a given monitor.
func (r *Resolver) isAllowedToEdit(ctx context.Context, monitorID graphql.ID) error {
	var monitorIDInt64 int64
	err := relay.UnmarshalSpec(monitorID, &monitorIDInt64)
	if err != nil {
		return err
	}
	owner, err := r.ownerForID64(ctx, monitorIDInt64)
	if err != nil {
		return err
	}
	return r.isAllowedToCreate(ctx, owner)
}

// isAllowedToCreate compares the owner of a monitor (user or org) to the actor of
// the request. A user can create a monitor if either of the following statements
// is true:
// - she is the owner
// - she is a member of the organization which is the owner of the monitor
// - she is a site-admin
func (r *Resolver) isAllowedToCreate(ctx context.Context, owner graphql.ID) error {
	var ownerInt32 int32
	err := relay.UnmarshalSpec(owner, &ownerInt32)
	if err != nil {
		return err
	}
	switch kind := relay.UnmarshalKind(owner); kind {
	case "User":
		return backend.CheckSiteAdminOrSameUser(ctx, ownerInt32)
	case "Org":
		return backend.CheckOrgAccess(ctx, ownerInt32)
	default:
		return fmt.Errorf("provided ID is not a namespace")
	}
}

func (r *Resolver) ownerForID64(ctx context.Context, monitorID int64) (owner graphql.ID, err error) {
	var (
		q      *sqlf.Query
		rows   *sql.Rows
		userID *int32
		orgID  *int32
	)
	q, err = ownerForID64Query(ctx, monitorID)
	if err != nil {
		return "", err
	}
	rows, err = r.store.Query(ctx, q)
	if err != nil {
		return "", err
	}
	defer rows.Close()

	for rows.Next() {
		if err = rows.Scan(
			&userID,
			&orgID,
		); err != nil {
			return "", err
		}
	}
	err = rows.Close()
	if err != nil {
		return "", err
	}
	// Rows.Err will report the last error encountered by Rows.Scan.
	if err = rows.Err(); err != nil {
		return "", err
	}
	if (userID == nil && orgID == nil) || (userID != nil && orgID != nil) {
		return "", fmt.Errorf("invalid owner")
	}
	if orgID != nil {
		return graphqlbackend.MarshalOrgID(*orgID), nil
	} else {
		return graphqlbackend.MarshalUserID(*userID), nil
	}
}

func ownerForID64Query(ctx context.Context, monitorID int64) (*sqlf.Query, error) {
	const ownerForId32Query = `SELECT namespace_user_id, namespace_org_id FROM cm_monitors WHERE id = %s`
	return sqlf.Sprintf(
		ownerForId32Query,
		monitorID,
	), nil
}

//
// MonitorConnection
//
type monitorConnection struct {
	*Resolver
	monitors []graphqlbackend.MonitorResolver
}

func (m *monitorConnection) Nodes(ctx context.Context) ([]graphqlbackend.MonitorResolver, error) {
	return m.monitors, nil
}

func (m *monitorConnection) TotalCount(ctx context.Context) (int32, error) {
	return int32(len(m.monitors)), nil
}

func (m *monitorConnection) PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error) {
	if len(m.monitors) == 0 {
		return graphqlutil.HasNextPage(false), nil
	}
	return graphqlutil.NextPageCursor(string(m.monitors[len(m.monitors)-1].ID())), nil
}

//
// Monitor
//
type monitor struct {
	*Resolver
	id              int64
	createdBy       int32
	createdAt       time.Time
	changedBy       int32
	changedAt       time.Time
	description     string
	enabled         bool
	namespaceUserID *int32
	namespaceOrgID  *int32
}

const (
	monitorKind                     = "CodeMonitor"
	monitorTriggerQueryKind         = "CodeMonitorTriggerQuery"
	monitorTriggerEventKind         = "CodeMonitorTriggerEvent"
	monitorActionEmailKind          = "CodeMonitorActionEmail"
	monitorActionEmailRecipientKind = "CodeMonitorActionEmailRecipient"
)

func (m *monitor) ID() graphql.ID {
	return relay.MarshalID(monitorKind, m.id)
}

func (m *monitor) CreatedBy(ctx context.Context) (*graphqlbackend.UserResolver, error) {
	return graphqlbackend.UserByIDInt32(ctx, m.createdBy)
}

func (m *monitor) CreatedAt() graphqlbackend.DateTime {
	return graphqlbackend.DateTime{Time: m.createdAt}
}

func (m *monitor) Description() string {
	return m.description
}

func (m *monitor) Enabled() bool {
	return m.enabled
}

func (m *monitor) Owner(ctx context.Context) (n graphqlbackend.NamespaceResolver, err error) {
	if m.namespaceOrgID == nil {
		n.Namespace, err = graphqlbackend.UserByIDInt32(ctx, *m.namespaceUserID)
	} else {
		n.Namespace, err = graphqlbackend.OrgByIDInt32(ctx, *m.namespaceOrgID)
	}
	return n, err
}

func (m *monitor) Trigger(ctx context.Context) (graphqlbackend.MonitorTrigger, error) {
	t, err := m.store.TriggerQueryByMonitorIDInt64(ctx, m.id)
	if err != nil {
		return nil, err
	}
	return &monitorTrigger{&monitorQuery{m.Resolver, t}}, nil
}

func (m *monitor) Actions(ctx context.Context, args *graphqlbackend.ListActionArgs) (graphqlbackend.MonitorActionConnectionResolver, error) {
	return m.actionConnectionResolverWithTriggerID(ctx, nil, m.id, args)
}

func (r *Resolver) actionConnectionResolverWithTriggerID(ctx context.Context, triggerEventID *int, monitorID int64, args *graphqlbackend.ListActionArgs) (c graphqlbackend.MonitorActionConnectionResolver, err error) {
	// For now, we only support emails as actions. Once we add other actions such as
	// webhooks, we have to query those tables here too.
	var q *sqlf.Query
	q, err = r.store.ReadActionEmailQuery(ctx, monitorID, args)
	if err != nil {
		return nil, err
	}
	rows, err := r.store.Query(ctx, q)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	es, err := cm.ScanEmails(rows)
	if err != nil {
		return nil, err
	}
	actions := make([]graphqlbackend.MonitorAction, 0, len(es))
	for _, e := range es {
		actions = append(actions, &action{
			email: &monitorEmail{
				Resolver:       r,
				MonitorEmail:   e,
				triggerEventID: triggerEventID,
			},
		})
	}
	return &monitorActionConnection{actions: actions}, nil
}

//
// MonitorTrigger <<UNION>>
//
type monitorTrigger struct {
	query graphqlbackend.MonitorQueryResolver
}

func (t *monitorTrigger) ToMonitorQuery() (graphqlbackend.MonitorQueryResolver, bool) {
	return t.query, t.query != nil
}

//
// Query
//
type monitorQuery struct {
	*Resolver
	*cm.MonitorQuery
}

func (q *monitorQuery) ID() graphql.ID {
	return relay.MarshalID(monitorTriggerQueryKind, q.Id)
}

func (q *monitorQuery) Query() string {
	return q.QueryString
}

func (q *monitorQuery) Events(ctx context.Context, args *graphqlbackend.ListEventsArgs) (graphqlbackend.MonitorTriggerEventConnectionResolver, error) {
	es, err := q.Resolver.store.GetEventsForQueryIDInt64(ctx, q.Id, args)
	if err != nil {
		return nil, err
	}
	events := make([]graphqlbackend.MonitorTriggerEventResolver, 0, len(es))
	for _, e := range es {
		events = append(events, graphqlbackend.MonitorTriggerEventResolver(&monitorTriggerEvent{
			Resolver:    q.Resolver,
			monitorID:   q.Monitor,
			TriggerJobs: e,
		}))
	}
	return &monitorTriggerEventConnection{q.Resolver, events}, nil
}

//
// MonitorTriggerEventConnection
//
type monitorTriggerEventConnection struct {
	*Resolver
	events []graphqlbackend.MonitorTriggerEventResolver
}

func (a *monitorTriggerEventConnection) Nodes(ctx context.Context) ([]graphqlbackend.MonitorTriggerEventResolver, error) {
	return a.events, nil
}

func (a *monitorTriggerEventConnection) TotalCount(ctx context.Context) (int32, error) {
	return int32(len(a.events)), nil
}

func (a *monitorTriggerEventConnection) PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error) {
	if len(a.events) == 0 {
		return graphqlutil.HasNextPage(false), nil
	}
	return graphqlutil.NextPageCursor(string(a.events[len(a.events)-1].ID())), nil
}

//
// MonitorTriggerEvent
//
type monitorTriggerEvent struct {
	*Resolver
	*cm.TriggerJobs
	monitorID int64
}

func (m *monitorTriggerEvent) ID() graphql.ID {
	return relay.MarshalID(monitorTriggerEventKind, m.Id)
}

// stateToStatus maps the state of the dbworker job to the public GraphQL status of
// events.
var stateToStatus = map[string]string{
	"completed":  "SUCCESS",
	"queued":     "PENDING",
	"processing": "PENDING",
	"errored":    "ERROR",
	"failed":     "ERROR",
}

func (m *monitorTriggerEvent) Status() (string, error) {
	if v, ok := stateToStatus[m.State]; ok {
		return v, nil
	}
	return "", fmt.Errorf("unknown status: %s", m.State)
}

func (m *monitorTriggerEvent) Message() *string {
	return m.FailureMessage
}

func (m *monitorTriggerEvent) Timestamp() (graphqlbackend.DateTime, error) {
	if m.FinishedAt == nil {
		return graphqlbackend.DateTime{Time: m.store.Now()}, nil
	}
	return graphqlbackend.DateTime{Time: *m.FinishedAt}, nil
}

func (m *monitorTriggerEvent) Actions(ctx context.Context, args *graphqlbackend.ListActionArgs) (graphqlbackend.MonitorActionConnectionResolver, error) {
	return m.actionConnectionResolverWithTriggerID(ctx, &m.Id, m.monitorID, args)
}

// ActionConnection
//
type monitorActionConnection struct {
	actions []graphqlbackend.MonitorAction
}

func (a *monitorActionConnection) Nodes(ctx context.Context) ([]graphqlbackend.MonitorAction, error) {
	return a.actions, nil
}

func (a *monitorActionConnection) TotalCount(ctx context.Context) (int32, error) {
	return int32(len(a.actions)), nil
}

func (a *monitorActionConnection) PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error) {
	if len(a.actions) == 0 {
		return graphqlutil.HasNextPage(false), nil
	}
	last := a.actions[len(a.actions)-1]
	if email, ok := last.ToMonitorEmail(); ok {
		return graphqlutil.NextPageCursor(string(email.ID())), nil
	}
	return nil, fmt.Errorf("we only support email actions for now")
}

//
// Action <<UNION>>
//
type action struct {
	email graphqlbackend.MonitorEmailResolver
}

func (a *action) ToMonitorEmail() (graphqlbackend.MonitorEmailResolver, bool) {
	return a.email, a.email != nil
}

//
// Email
//
type monitorEmail struct {
	*Resolver
	*cm.MonitorEmail

	// If triggerEventID == nil, all events of this action will be returned.
	// Otherwise, only those events of this action which are related to the specified
	// trigger event will be returned.
	triggerEventID *int
}

func (m *monitorEmail) Recipients(ctx context.Context, args *graphqlbackend.ListRecipientsArgs) (c graphqlbackend.MonitorActionEmailRecipientsConnectionResolver, err error) {
	var ms []*cm.Recipient
	ms, err = m.store.RecipientsForEmailIDInt64(ctx, m.Id, args)
	if err != nil {
		return nil, err
	}
	var ns []graphqlbackend.NamespaceResolver
	for _, r := range ms {
		n := graphqlbackend.NamespaceResolver{}
		if r.NamespaceOrgID == nil {
			n.Namespace, err = graphqlbackend.UserByIDInt32(ctx, *r.NamespaceUserID)
		} else {
			n.Namespace, err = graphqlbackend.OrgByIDInt32(ctx, *r.NamespaceOrgID)
		}
		if err != nil {
			return nil, err
		}
		ns = append(ns, n)
	}

	// Since recipients can either be a user or an org it would be very tedious to
	// use the user-id or org-id of the last entry as a cursor for the next page. It
	// is easier to just use the id of the recipients table.
	var nextPageCursor string
	if len(ms) > 0 {
		nextPageCursor = string(relay.MarshalID(monitorActionEmailRecipientKind, ms[len(ms)-1].ID))
	}

	var total int32
	total, err = m.store.TotalCountRecipients(ctx, m.Id)
	return &monitorActionEmailRecipientsConnection{ns, nextPageCursor, total}, nil
}

func (m *monitorEmail) Enabled() bool {
	return m.MonitorEmail.Enabled
}

func (m *monitorEmail) Priority() string {
	return m.MonitorEmail.Priority
}

func (m *monitorEmail) Header() string {
	return m.MonitorEmail.Header
}

func (m *monitorEmail) ID() graphql.ID {
	return relay.MarshalID(monitorActionEmailKind, m.Id)
}

func (m *monitorEmail) Events(ctx context.Context, args *graphqlbackend.ListEventsArgs) (graphqlbackend.MonitorActionEventConnectionResolver, error) {
	return &monitorActionEventConnection{}, nil
}

//
// MonitorActionEmailRecipientConnection
//
type monitorActionEmailRecipientsConnection struct {
	recipients     []graphqlbackend.NamespaceResolver
	nextPageCursor string
	totalCount     int32
}

func (a *monitorActionEmailRecipientsConnection) Nodes(ctx context.Context) ([]graphqlbackend.NamespaceResolver, error) {
	return a.recipients, nil
}

func (a *monitorActionEmailRecipientsConnection) TotalCount(ctx context.Context) (int32, error) {
	return a.totalCount, nil
}

func (a *monitorActionEmailRecipientsConnection) PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error) {
	if len(a.recipients) == 0 {
		return graphqlutil.HasNextPage(false), nil
	}
	return graphqlutil.NextPageCursor(a.nextPageCursor), nil
}

//
// MonitorActionEventConnection
//
type monitorActionEventConnection struct {
}

func (a *monitorActionEventConnection) Nodes(ctx context.Context) ([]graphqlbackend.MonitorActionEventResolver, error) {
	notImplemented := "message not implemented"
	return []graphqlbackend.MonitorActionEventResolver{
			&monitorActionEvent{id: "314", status: "SUCCESS", timestamp: graphqlbackend.DateTime{Time: time.Now()}},
			&monitorActionEvent{id: "315", status: "ERROR", message: &notImplemented, timestamp: graphqlbackend.DateTime{Time: time.Now()}},
		},
		nil
}

func (a *monitorActionEventConnection) TotalCount(ctx context.Context) (int32, error) {
	return 1, nil
}

func (a *monitorActionEventConnection) PageInfo(ctx context.Context) (*graphqlutil.PageInfo, error) {
	return graphqlutil.HasNextPage(false), nil
}

//
// MonitorEvent
//
type monitorActionEvent struct {
	id        graphql.ID
	status    string
	message   *string
	timestamp graphqlbackend.DateTime
}

func (m *monitorActionEvent) ID() graphql.ID {
	return m.id
}

func (m *monitorActionEvent) Status() string {
	return m.status
}

func (m *monitorActionEvent) Message() *string {
	return m.message
}

func (m *monitorActionEvent) Timestamp() graphqlbackend.DateTime {
	return m.timestamp
}

func unmarshalAfter(after *string) (int64, error) {
	var a int64
	if after == nil {
		a = 0
	} else {
		err := relay.UnmarshalSpec(graphql.ID(*after), &a)
		if err != nil {
			return -1, err
		}
	}
	return a, nil
}
