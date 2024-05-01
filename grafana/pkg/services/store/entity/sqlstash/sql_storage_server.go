package sqlstash

import (
	"context"
	"database/sql"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math/rand"
	"slices"
	"strings"
	"time"

	"github.com/bwmarrin/snowflake"
	"github.com/google/uuid"
	"google.golang.org/protobuf/proto"

	folder "github.com/grafana/grafana/pkg/apis/folder/v0alpha1"
	"github.com/grafana/grafana/pkg/infra/appcontext"
	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/infra/tracing"
	"github.com/grafana/grafana/pkg/services/sqlstore/migrator"
	"github.com/grafana/grafana/pkg/services/sqlstore/session"
	"github.com/grafana/grafana/pkg/services/store"
	"github.com/grafana/grafana/pkg/services/store/entity"
	"github.com/grafana/grafana/pkg/services/store/entity/db"
	"github.com/prometheus/client_golang/prometheus"
	"go.opentelemetry.io/otel/attribute"
	"go.opentelemetry.io/otel/trace"
)

const entityTable = "entity"
const entityHistoryTable = "entity_history"

var (
	errorUserNotFoundInContext     = errors.New("can not find user in context")
	errorNextPageTokenNotSupported = errors.New("nextPageToken not yet supported")
	errorEntityAlreadyExists       = errors.New("entity already exists")
)

// Make sure we implement correct interfaces
var _ entity.EntityStoreServer = &sqlEntityServer{}

func ProvideSQLEntityServer(db db.EntityDBInterface, tracer tracing.Tracer /*, cfg *setting.Cfg */) (SqlEntityServer, error) {
	ctx, cancel := context.WithCancel(context.Background())

	entityServer := &sqlEntityServer{
		db:     db,
		log:    log.New("sql-entity-server"),
		ctx:    ctx,
		cancel: cancel,
		tracer: tracer,
	}

	if err := prometheus.Register(NewStorageMetrics()); err != nil {
		entityServer.log.Warn("error registering storage server metrics", "error", err)
	}

	return entityServer, nil
}

type SqlEntityServer interface {
	entity.EntityStoreServer

	Init() error
	Stop()
}

type sqlEntityServer struct {
	log         log.Logger
	db          db.EntityDBInterface // needed to keep xorm engine in scope
	sess        *session.SessionDB
	dialect     migrator.Dialect
	snowflake   *snowflake.Node
	broadcaster Broadcaster[*entity.EntityWatchResponse]
	ctx         context.Context
	cancel      context.CancelFunc
	stream      chan *entity.EntityWatchResponse
	tracer      tracing.Tracer
}

func (s *sqlEntityServer) Init() error {
	if s.sess != nil {
		return nil
	}

	if s.db == nil {
		return errors.New("missing db")
	}

	err := s.db.Init()
	if err != nil {
		return err
	}

	sess, err := s.db.GetSession()
	if err != nil {
		return err
	}

	engine, err := s.db.GetEngine()
	if err != nil {
		return err
	}

	s.sess = sess
	s.dialect = migrator.NewDialect(engine.DriverName())

	// initialize snowflake generator
	s.snowflake, err = snowflake.NewNode(rand.Int63n(1024))
	if err != nil {
		return err
	}

	// set up the broadcaster
	s.broadcaster, err = NewBroadcaster(s.ctx, func(stream chan *entity.EntityWatchResponse) error {
		s.stream = stream

		// start the poller
		go s.poller(stream)

		return nil
	})
	if err != nil {
		return err
	}

	return nil
}

func (s *sqlEntityServer) IsHealthy(ctx context.Context, r *entity.HealthCheckRequest) (*entity.HealthCheckResponse, error) {
	sess, err := s.db.GetSession()
	if err != nil {
		return nil, err
	}
	_, err = sess.Query(ctx, "SELECT 1")
	if err != nil {
		return nil, err
	}

	return &entity.HealthCheckResponse{Status: entity.HealthCheckResponse_SERVING}, nil
}

func (s *sqlEntityServer) Stop() {
	s.cancel()
}

type FieldSelectRequest interface {
	GetWithBody() bool
	GetWithStatus() bool
}

func (s *sqlEntityServer) getReadFields(r FieldSelectRequest) []string {
	fields := []string{
		"guid",
		"key",
		"namespace", "group", "group_version", "resource", "name", "folder",
		"resource_version", "size", "etag", "errors", // errors are always returned
		"created_at", "created_by",
		"updated_at", "updated_by",
		"origin", "origin_key", "origin_ts",
		"meta",
		"title", "slug", "description", "labels", "fields",
		"message",
		"action",
	}

	if r.GetWithBody() {
		fields = append(fields, `body`)
	}
	if r.GetWithStatus() {
		fields = append(fields, "status")
	}

	return fields
}

func (s *sqlEntityServer) getReadSelect(r FieldSelectRequest) (string, error) {
	if err := s.Init(); err != nil {
		return "", err
	}

	fields := s.getReadFields(r)

	quotedFields := make([]string, len(fields))
	for i, f := range fields {
		quotedFields[i] = s.dialect.Quote(f)
	}
	return "SELECT " + strings.Join(quotedFields, ","), nil
}

func readEntity(rows *sql.Rows, r FieldSelectRequest) (*entity.Entity, error) {
	raw := &entity.Entity{
		Origin: &entity.EntityOriginInfo{},
	}

	errors := ""
	labels := ""
	fields := ""

	args := []any{
		&raw.Guid,
		&raw.Key,
		&raw.Namespace, &raw.Group, &raw.GroupVersion, &raw.Resource, &raw.Name, &raw.Folder,
		&raw.ResourceVersion, &raw.Size, &raw.ETag, &errors,
		&raw.CreatedAt, &raw.CreatedBy,
		&raw.UpdatedAt, &raw.UpdatedBy,
		&raw.Origin.Source, &raw.Origin.Key, &raw.Origin.Time,
		&raw.Meta,
		&raw.Title, &raw.Slug, &raw.Description, &labels, &fields,
		&raw.Message,
		&raw.Action,
	}
	if r.GetWithBody() {
		args = append(args, &raw.Body)
	}
	if r.GetWithStatus() {
		args = append(args, &raw.Status)
	}

	err := rows.Scan(args...)
	if err != nil {
		return nil, err
	}

	// unmarshal json labels
	if labels != "" {
		if err := json.Unmarshal([]byte(labels), &raw.Labels); err != nil {
			return nil, err
		}
	}

	// set empty body, meta or status to nil
	if raw.Body != nil && len(raw.Body) == 0 {
		raw.Body = nil
	}
	if raw.Meta != nil && len(raw.Meta) == 0 {
		raw.Meta = nil
	}
	if raw.Status != nil && len(raw.Status) == 0 {
		raw.Status = nil
	}

	return raw, nil
}

func (s *sqlEntityServer) Read(ctx context.Context, r *entity.ReadEntityRequest) (*entity.Entity, error) {
	ctx, span := s.tracer.Start(ctx, "storage_server.Read")
	defer span.End()
	ctxLogger := s.log.FromContext(log.WithContextualAttributes(ctx, []any{"method", "read"}))

	if err := s.Init(); err != nil {
		ctxLogger.Error("init error", "error", err)
		return nil, err
	}

	res, err := s.read(ctx, s.sess, r)
	if err != nil {
		ctxLogger.Error("read error", "error", err)
	}
	return res, err
}

func (s *sqlEntityServer) read(ctx context.Context, tx session.SessionQuerier, r *entity.ReadEntityRequest) (*entity.Entity, error) {
	ctx, span := s.tracer.Start(ctx, "storage_server.read")
	defer span.End()

	table := entityTable
	where := []string{}
	args := []any{}

	if r.Key == "" {
		return nil, fmt.Errorf("missing key")
	}

	key, err := entity.ParseKey(r.Key)
	if err != nil {
		return nil, err
	}

	where = append(where, s.dialect.Quote("namespace")+"=?", s.dialect.Quote("group")+"=?", s.dialect.Quote("resource")+"=?", s.dialect.Quote("name")+"=?")
	args = append(args, key.Namespace, key.Group, key.Resource, key.Name)

	if r.ResourceVersion != 0 {
		table = entityHistoryTable
		where = append(where, s.dialect.Quote("resource_version")+">=?")
		args = append(args, r.ResourceVersion)
	}

	query, err := s.getReadSelect(r)
	if err != nil {
		return nil, err
	}

	if false { // TODO, MYSQL/PosgreSQL can lock the row " FOR UPDATE"
		query += " FOR UPDATE"
	}

	query += " FROM " + table +
		" WHERE " + strings.Join(where, " AND ")

	if r.ResourceVersion != 0 {
		query += " ORDER BY resource_version DESC"
	}
	query += " LIMIT 1"

	s.log.Debug("read", "query", query, "args", args)

	rows, err := tx.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer func() { _ = rows.Close() }()

	if !rows.Next() {
		return &entity.Entity{}, nil
	}

	return readEntity(rows, r)
}

//nolint:gocyclo
func (s *sqlEntityServer) Create(ctx context.Context, r *entity.CreateEntityRequest) (*entity.CreateEntityResponse, error) {
	ctx, span := s.tracer.Start(ctx, "storage_server.Create")
	defer span.End()
	ctxLogger := s.log.FromContext(log.WithContextualAttributes(ctx, []any{"method", "create"}))

	if err := s.Init(); err != nil {
		ctxLogger.Error("init error", "error", err)
		return nil, err
	}

	createdAt := r.Entity.CreatedAt
	if createdAt < 1000 {
		createdAt = time.Now().UnixMilli()
	}

	createdBy := r.Entity.CreatedBy
	if createdBy == "" {
		modifier, err := appcontext.User(ctx)
		if err != nil {
			ctxLogger.Error("error getting user from ctx", "error", err)
			return nil, err
		}
		if modifier == nil {
			ctxLogger.Error("could not find user in context", "error", errorUserNotFoundInContext)
			return nil, err
		}
		createdBy = store.GetUserIDString(modifier)
	}

	updatedAt := r.Entity.UpdatedAt
	updatedBy := r.Entity.UpdatedBy

	rsp := &entity.CreateEntityResponse{
		Entity: &entity.Entity{},
		Status: entity.CreateEntityResponse_CREATED, // Will be changed if not true
	}

	err := s.sess.WithTransaction(ctx, func(tx *session.SessionTx) error {
		current, err := s.read(ctx, tx, &entity.ReadEntityRequest{
			Key:        r.Entity.Key,
			WithBody:   true,
			WithStatus: true,
		})
		if err != nil {
			return err
		}

		// if we found an existing entity
		if current.Guid != "" {
			ctxLogger.Error("entity already exists", "error", errorEntityAlreadyExists)
			return errorEntityAlreadyExists
		}

		// generate guid for new entity
		current.Guid = uuid.New().String()

		// set created at/by
		current.CreatedAt = createdAt
		current.CreatedBy = createdBy

		// parse provided key
		key, err := entity.ParseKey(r.Entity.Key)
		if err != nil {
			ctxLogger.Error("error parsing key", "error", err)
			return err
		}

		current.Key = r.Entity.Key
		current.Namespace = key.Namespace
		current.Group = key.Group
		current.GroupVersion = r.Entity.GroupVersion
		current.Resource = key.Resource
		current.Name = key.Name

		if r.Entity.Folder != "" {
			current.Folder = r.Entity.Folder
		}
		if r.Entity.Slug != "" {
			current.Slug = r.Entity.Slug
		}

		if r.Entity.Body != nil {
			current.Body = r.Entity.Body
			current.Size = int64(len(current.Body))
		}

		if r.Entity.Meta != nil {
			current.Meta = r.Entity.Meta
		}

		if r.Entity.Status != nil {
			current.Status = r.Entity.Status
		}

		etag := createContentsHash(current.Body, current.Meta, current.Status)
		current.ETag = etag

		current.UpdatedAt = updatedAt
		current.UpdatedBy = updatedBy

		if r.Entity.Title != "" {
			current.Title = r.Entity.Title
		}
		if r.Entity.Description != "" {
			current.Description = r.Entity.Description
		}

		labels, err := json.Marshal(r.Entity.Labels)
		if err != nil {
			ctxLogger.Error("error marshalling labels", "msg", err.Error())
			return err
		}
		current.Labels = r.Entity.Labels

		fields, err := json.Marshal(r.Entity.Fields)
		if err != nil {
			ctxLogger.Error("error marshalling fields", "msg", err.Error())
			return err
		}
		current.Fields = r.Entity.Fields

		errors, err := json.Marshal(r.Entity.Errors)
		if err != nil {
			ctxLogger.Error("error marshalling errors", "msg", err.Error())
			return err
		}
		current.Errors = r.Entity.Errors

		if current.Origin == nil {
			current.Origin = &entity.EntityOriginInfo{}
		}

		if r.Entity.Origin != nil {
			if r.Entity.Origin.Source != "" {
				current.Origin.Source = r.Entity.Origin.Source
			}
			if r.Entity.Origin.Key != "" {
				current.Origin.Key = r.Entity.Origin.Key
			}
			if r.Entity.Origin.Time > 0 {
				current.Origin.Time = r.Entity.Origin.Time
			}
		}

		// Set the comment on this write
		if r.Entity.Message != "" {
			current.Message = r.Entity.Message
		}

		// Update resource version
		current.ResourceVersion = s.snowflake.Generate().Int64()

		current.Action = entity.Entity_CREATED

		values := map[string]any{
			"guid":             current.Guid,
			"key":              current.Key,
			"namespace":        current.Namespace,
			"group":            current.Group,
			"resource":         current.Resource,
			"name":             current.Name,
			"created_at":       current.CreatedAt,
			"created_by":       current.CreatedBy,
			"group_version":    current.GroupVersion,
			"folder":           current.Folder,
			"slug":             current.Slug,
			"updated_at":       current.UpdatedAt,
			"updated_by":       current.UpdatedBy,
			"body":             current.Body,
			"meta":             current.Meta,
			"status":           current.Status,
			"size":             current.Size,
			"etag":             current.ETag,
			"resource_version": current.ResourceVersion,
			"title":            current.Title,
			"description":      current.Description,
			"labels":           labels,
			"fields":           fields,
			"errors":           errors,
			"origin":           current.Origin.Source,
			"origin_key":       current.Origin.Key,
			"origin_ts":        current.Origin.Time,
			"message":          current.Message,
			"action":           current.Action,
		}

		// 1. Add row to the `entity_history` values
		if err = s.insert(ctx, tx, entityHistoryTable, values); err != nil {
			ctxLogger.Error("insert entity_history error", "error", err)
			return err
		}

		// 2. Add row to the main `entity` table
		if err = s.insert(ctx, tx, entityTable, values); err != nil {
			ctxLogger.Error("insert entity error", "error", err)
			return err
		}

		switch current.Group {
		case folder.GROUP:
			switch current.Resource {
			case folder.RESOURCE:
				err = s.updateFolderTree(ctx, tx, current.Namespace)
				if err != nil {
					ctxLogger.Error("error updating folder tree", "error", err.Error())
					return err
				}
			}
		}

		rsp.Entity = current

		return s.setLabels(ctx, tx, current.Guid, current.Labels)
	})
	if err != nil {
		ctxLogger.Error("error creating entity", "msg", err.Error())
		rsp.Status = entity.CreateEntityResponse_ERROR
	}

	evt := &entity.EntityWatchResponse{
		Timestamp: time.Now().UnixMilli(),
		Entity:    rsp.Entity,
	}
	s.stream <- evt

	return rsp, err
}

//nolint:gocyclo
func (s *sqlEntityServer) Update(ctx context.Context, r *entity.UpdateEntityRequest) (*entity.UpdateEntityResponse, error) {
	ctx, span := s.tracer.Start(ctx, "storage_server.Update")
	defer span.End()
	ctxLogger := s.log.FromContext(log.WithContextualAttributes(ctx, []any{"method", "update"}))

	if err := s.Init(); err != nil {
		ctxLogger.Error("init error", "error", err)
		return nil, err
	}

	updatedAt := r.Entity.UpdatedAt
	if updatedAt < 1000 {
		updatedAt = time.Now().UnixMilli()
	}

	updatedBy := r.Entity.UpdatedBy
	if updatedBy == "" {
		modifier, err := appcontext.User(ctx)
		if err != nil {
			ctxLogger.Error("error getting user from ctx", "error", err)
			return nil, err
		}
		if modifier == nil {
			ctxLogger.Error("could not find user in context", "error", errorUserNotFoundInContext)
			return nil, errorUserNotFoundInContext
		}
		updatedBy = store.GetUserIDString(modifier)
	}

	rsp := &entity.UpdateEntityResponse{
		Entity: &entity.Entity{},
		Status: entity.UpdateEntityResponse_UPDATED, // Will be changed if not true
	}

	var previous *entity.Entity
	var err error

	err = s.sess.WithTransaction(ctx, func(tx *session.SessionTx) error {
		previous, err = s.read(ctx, tx, &entity.ReadEntityRequest{
			Key:        r.Entity.Key,
			WithBody:   true,
			WithStatus: true,
		})
		if err != nil {
			return err
		}

		// Optimistic locking
		if r.PreviousVersion > 0 && r.PreviousVersion != previous.ResourceVersion {
			StorageServerMetrics.OptimisticLockFailed.WithLabelValues("update").Inc()
			return fmt.Errorf("optimistic lock failed")
		}

		// if we didn't find an existing entity
		if previous.Guid == "" {
			return fmt.Errorf("entity not found")
		}

		rsp.Entity.Guid = previous.Guid

		// Clear the refs
		if _, err := tx.Exec(ctx, "DELETE FROM entity_ref WHERE guid=?", rsp.Entity.Guid); err != nil {
			return err
		}

		updated := proto.Clone(previous).(*entity.Entity)

		if r.Entity.GroupVersion != "" {
			updated.GroupVersion = r.Entity.GroupVersion
		}

		if r.Entity.Folder != "" {
			updated.Folder = r.Entity.Folder
		}
		if r.Entity.Slug != "" {
			updated.Slug = r.Entity.Slug
		}

		if r.Entity.Body != nil {
			updated.Body = r.Entity.Body
			updated.Size = int64(len(updated.Body))
		}

		if r.Entity.Meta != nil {
			updated.Meta = r.Entity.Meta
		}

		if r.Entity.Status != nil {
			updated.Status = r.Entity.Status
		}

		etag := createContentsHash(updated.Body, updated.Meta, updated.Status)
		updated.ETag = etag

		updated.UpdatedAt = updatedAt
		updated.UpdatedBy = updatedBy

		if r.Entity.Title != "" {
			updated.Title = r.Entity.Title
		}
		if r.Entity.Description != "" {
			updated.Description = r.Entity.Description
		}

		labels, err := json.Marshal(r.Entity.Labels)
		if err != nil {
			ctxLogger.Error("error marshalling labels", "msg", err.Error())
			return err
		}
		updated.Labels = r.Entity.Labels

		fields, err := json.Marshal(r.Entity.Fields)
		if err != nil {
			ctxLogger.Error("error marshalling fields", "msg", err.Error())
			return err
		}
		updated.Fields = r.Entity.Fields

		errors, err := json.Marshal(r.Entity.Errors)
		if err != nil {
			ctxLogger.Error("error marshalling errors", "msg", err.Error())
			return err
		}
		updated.Errors = r.Entity.Errors

		if updated.Origin == nil {
			updated.Origin = &entity.EntityOriginInfo{}
		}

		if r.Entity.Origin != nil {
			if r.Entity.Origin.Source != "" {
				updated.Origin.Source = r.Entity.Origin.Source
			}
			if r.Entity.Origin.Key != "" {
				updated.Origin.Key = r.Entity.Origin.Key
			}
			if r.Entity.Origin.Time > 0 {
				updated.Origin.Time = r.Entity.Origin.Time
			}
		}

		// Set the comment on this write
		if r.Entity.Message != "" {
			updated.Message = r.Entity.Message
		}

		// Update resource version
		updated.ResourceVersion = s.snowflake.Generate().Int64()

		updated.Action = entity.Entity_UPDATED

		values := map[string]any{
			// below are only set in history table
			"guid":       updated.Guid,
			"key":        updated.Key,
			"namespace":  updated.Namespace,
			"group":      updated.Group,
			"resource":   updated.Resource,
			"name":       updated.Name,
			"created_at": updated.CreatedAt,
			"created_by": updated.CreatedBy,
			// below are updated
			"group_version":    updated.GroupVersion,
			"folder":           updated.Folder,
			"slug":             updated.Slug,
			"updated_at":       updated.UpdatedAt,
			"updated_by":       updated.UpdatedBy,
			"body":             updated.Body,
			"meta":             updated.Meta,
			"status":           updated.Status,
			"size":             updated.Size,
			"etag":             updated.ETag,
			"resource_version": updated.ResourceVersion,
			"title":            updated.Title,
			"description":      updated.Description,
			"labels":           labels,
			"fields":           fields,
			"errors":           errors,
			"origin":           updated.Origin.Source,
			"origin_key":       updated.Origin.Key,
			"origin_ts":        updated.Origin.Time,
			"message":          updated.Message,
			"action":           updated.Action,
		}

		// 1. Add the `entity_history` values
		if err := s.insert(ctx, tx, entityHistoryTable, values); err != nil {
			return err
		}

		// 2. update the main `entity` table

		// remove values that are only set at insert
		delete(values, "guid")
		delete(values, "key")
		delete(values, "namespace")
		delete(values, "group")
		delete(values, "resource")
		delete(values, "name")
		delete(values, "created_at")
		delete(values, "created_by")

		err = s.update(
			ctx,
			tx,
			entityTable,
			values,
			map[string]any{
				"guid": updated.Guid,
			},
		)
		if err != nil {
			ctxLogger.Error("error updating entity", "error", err.Error())
			return err
		}

		switch updated.Group {
		case folder.GROUP:
			switch updated.Resource {
			case folder.RESOURCE:
				err = s.updateFolderTree(ctx, tx, updated.Namespace)
				if err != nil {
					ctxLogger.Error("error updating folder tree", "msg", err.Error())
					return err
				}
			}
		}

		rsp.Entity = updated

		return s.setLabels(ctx, tx, updated.Guid, updated.Labels)
	})
	if err != nil {
		ctxLogger.Error("error updating entity", "msg", err.Error())
		rsp.Status = entity.UpdateEntityResponse_ERROR
	}

	evt := &entity.EntityWatchResponse{
		Timestamp: time.Now().UnixMilli(),
		Entity:    rsp.Entity,
		Previous:  previous,
	}

	s.stream <- evt

	return rsp, err
}

func (s *sqlEntityServer) setLabels(ctx context.Context, tx *session.SessionTx, guid string, labels map[string]string) error {
	ctx, span := s.tracer.Start(ctx, "storage_server.setLabels")
	defer span.End()

	s.log.Debug("setLabels", "guid", guid, "labels", labels)

	// Clear the old labels
	if _, err := tx.Exec(ctx, "DELETE FROM entity_labels WHERE guid=?", guid); err != nil {
		return err
	}

	// Add the new labels
	for k, v := range labels {
		query, args, err := s.dialect.InsertQuery(
			"entity_labels",
			map[string]any{
				"guid":  guid,
				"label": k,
				"value": v,
			},
		)
		if err != nil {
			return err
		}

		_, err = tx.Exec(ctx, query, args...)
		if err != nil {
			return err
		}
	}

	return nil
}

func (s *sqlEntityServer) Delete(ctx context.Context, r *entity.DeleteEntityRequest) (*entity.DeleteEntityResponse, error) {
	ctx, span := s.tracer.Start(ctx, "storage_server.Delete")
	defer span.End()
	ctxLogger := s.log.FromContext(log.WithContextualAttributes(ctx, []any{"method", "delete"}))

	if err := s.Init(); err != nil {
		ctxLogger.Error("init error", "error", err)
		return nil, err
	}

	rsp := &entity.DeleteEntityResponse{}

	var previous *entity.Entity
	var updated *entity.Entity

	err := s.sess.WithTransaction(ctx, func(tx *session.SessionTx) error {
		var err error
		previous, err = s.Read(ctx, &entity.ReadEntityRequest{
			Key:        r.Key,
			WithBody:   true,
			WithStatus: true,
		})
		if err != nil {
			if errors.Is(err, sql.ErrNoRows) {
				rsp.Status = entity.DeleteEntityResponse_NOTFOUND
			} else {
				rsp.Status = entity.DeleteEntityResponse_ERROR
			}
			return err
		}

		if previous.Guid == "" {
			rsp.Status = entity.DeleteEntityResponse_NOTFOUND
			return nil
		}

		if r.PreviousVersion > 0 && r.PreviousVersion != previous.ResourceVersion {
			rsp.Status = entity.DeleteEntityResponse_ERROR
			StorageServerMetrics.OptimisticLockFailed.WithLabelValues("delete").Inc()
			return fmt.Errorf("optimistic lock failed")
		}

		updated, err = s.doDelete(ctx, tx, previous)
		if err != nil {
			rsp.Status = entity.DeleteEntityResponse_ERROR
			return err
		}

		rsp.Status = entity.DeleteEntityResponse_DELETED
		return nil
	})
	if err != nil {
		ctxLogger.Error("delete error", "error", err)
	}

	if rsp.Status == entity.DeleteEntityResponse_DELETED {
		// k8s expects us to return the entity as it was before the deletion, but with the updated RV
		rsp.Entity = proto.Clone(previous).(*entity.Entity)
		rsp.Entity.ResourceVersion = updated.ResourceVersion

		evt := &entity.EntityWatchResponse{
			Timestamp: time.Now().UnixMilli(),
			Entity:    updated,
			Previous:  previous,
		}
		s.stream <- evt
	} else {
		rsp.Entity = previous
	}

	return rsp, err
}

func (s *sqlEntityServer) doDelete(ctx context.Context, tx *session.SessionTx, ent *entity.Entity) (*entity.Entity, error) {
	ctx, span := s.tracer.Start(ctx, "storage_server.doDelete")
	defer span.End()
	ctxLogger := s.log.FromContext(ctx)

	updated := proto.Clone(ent).(*entity.Entity)

	// Update resource version
	updated.ResourceVersion = s.snowflake.Generate().Int64()

	updated.Action = entity.Entity_DELETED

	// Get updated by
	modifier, err := appcontext.User(ctx)
	if err != nil {
		return nil, err
	}
	if modifier == nil {
		return nil, fmt.Errorf("can not find user in context")
	}

	labels, err := json.Marshal(updated.Labels)
	if err != nil {
		ctxLogger.Error("error marshalling labels", "msg", err.Error())
		return nil, err
	}

	fields, err := json.Marshal(updated.Fields)
	if err != nil {
		ctxLogger.Error("error marshalling fields", "msg", err.Error())
		return nil, err
	}

	errors, err := json.Marshal(updated.Errors)
	if err != nil {
		ctxLogger.Error("error marshalling errors", "msg", err.Error())
		return nil, err
	}

	if updated.Origin == nil {
		updated.Origin = &entity.EntityOriginInfo{}
	}

	updated.UpdatedAt = time.Now().UnixMilli()
	updated.UpdatedBy = store.GetUserIDString(modifier)

	values := map[string]any{
		// below are only set in history table
		"guid":       updated.Guid,
		"key":        updated.Key,
		"namespace":  updated.Namespace,
		"group":      updated.Group,
		"resource":   updated.Resource,
		"name":       updated.Name,
		"created_at": updated.CreatedAt,
		"created_by": updated.CreatedBy,
		// below are updated
		"group_version":    updated.GroupVersion,
		"folder":           updated.Folder,
		"slug":             updated.Slug,
		"updated_at":       updated.UpdatedAt,
		"updated_by":       updated.UpdatedBy,
		"body":             updated.Body,
		"meta":             updated.Meta,
		"status":           updated.Status,
		"size":             updated.Size,
		"etag":             updated.ETag,
		"resource_version": updated.ResourceVersion,
		"title":            updated.Title,
		"description":      updated.Description,
		"labels":           labels,
		"fields":           fields,
		"errors":           errors,
		"origin":           updated.Origin.Source,
		"origin_key":       updated.Origin.Key,
		"origin_ts":        updated.Origin.Time,
		"message":          updated.Message,
		"action":           updated.Action,
	}

	// 1. Add the `entity_history` values
	if err := s.insert(ctx, tx, entityHistoryTable, values); err != nil {
		return nil, err
	}

	if err = s.exec(ctx, tx, "DELETE FROM entity WHERE guid=?", updated.Guid); err != nil {
		return nil, err
	}
	if err = s.exec(ctx, tx, "DELETE FROM entity_labels WHERE guid=?", updated.Guid); err != nil {
		return nil, err
	}
	if err = s.exec(ctx, tx, "DELETE FROM entity_ref WHERE guid=?", updated.Guid); err != nil {
		return nil, err
	}

	switch updated.Group {
	case folder.GROUP:
		switch updated.Resource {
		case folder.RESOURCE:
			err = s.updateFolderTree(ctx, tx, updated.Namespace)
			if err != nil {
				s.log.Error("error updating folder tree", "msg", err.Error())
				return nil, err
			}
		}
	}

	return updated, nil
}

func (s *sqlEntityServer) History(ctx context.Context, r *entity.EntityHistoryRequest) (*entity.EntityHistoryResponse, error) {
	ctx, span := s.tracer.Start(ctx, "storage_server.History")
	defer span.End()
	ctxLogger := s.log.FromContext(log.WithContextualAttributes(ctx, []any{"method", "history"}))

	if err := s.Init(); err != nil {
		ctxLogger.Error("init error", "error", err)
		return nil, err
	}

	user, err := appcontext.User(ctx)
	if err != nil {
		ctxLogger.Error("error getting user from ctx", "error", err)
		return nil, err
	}
	if user == nil {
		ctxLogger.Error("could not find user in context", "error", errorUserNotFoundInContext)
		return nil, errorUserNotFoundInContext
	}

	res, err := s.history(ctx, r)
	if err != nil {
		ctxLogger.Error("history error", "error", err)
	}
	return res, err
}

func (s *sqlEntityServer) history(ctx context.Context, r *entity.EntityHistoryRequest) (*entity.EntityHistoryResponse, error) {
	ctx, span := s.tracer.Start(ctx, "storage_server.history")
	defer span.End()

	var limit int64 = 100
	if r.Limit > 0 && r.Limit < 100 {
		limit = r.Limit
	}

	entityQuery := selectQuery{
		dialect:  s.dialect,
		from:     entityHistoryTable, // the table
		limit:    r.Limit,
		oneExtra: true, // request one more than the limit (and show next token if it exists)
	}

	fields := s.getReadFields(r)
	entityQuery.AddFields(fields...)

	if r.Key != "" {
		key, err := entity.ParseKey(r.Key)
		if err != nil {
			return nil, err
		}

		if key.Name == "" {
			return nil, fmt.Errorf("missing name")
		}

		args := []any{key.Group, key.Resource}
		whereclause := "(" + s.dialect.Quote("group") + "=? AND " + s.dialect.Quote("resource") + "=?"
		if key.Namespace != "" {
			args = append(args, key.Namespace)
			whereclause += " AND " + s.dialect.Quote("namespace") + "=?"
		}
		args = append(args, key.Name)
		whereclause += " AND " + s.dialect.Quote("name") + "=?)"

		entityQuery.AddWhere(whereclause, args...)
	} else if r.Guid != "" {
		entityQuery.AddWhere(s.dialect.Quote("guid")+"=?", r.Guid)
	} else {
		return nil, fmt.Errorf("no key or guid specified")
	}

	if r.Before > 0 {
		entityQuery.AddWhere(s.dialect.Quote("resource_version")+"<?", r.Before)
	}

	// if we have a page token, use that to specify the first record
	continueToken, err := GetContinueToken(r)
	if err != nil {
		return nil, err
	}
	if continueToken != nil {
		entityQuery.offset = continueToken.StartOffset
	}

	for _, sort := range r.Sort {
		sortBy, err := ParseSortBy(sort)
		if err != nil {
			return nil, err
		}
		entityQuery.AddOrderBy(sortBy.Field, sortBy.Direction)
	}
	entityQuery.AddOrderBy("resource_version", Ascending)

	query, args := entityQuery.ToQuery()

	s.log.Debug("history", "query", query, "args", args)

	rows, err := s.query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer func() { _ = rows.Close() }()

	rsp := &entity.EntityHistoryResponse{
		Key:             r.Key,
		ResourceVersion: s.snowflake.Generate().Int64(),
	}
	for rows.Next() {
		v, err := readEntity(rows, r)
		if err != nil {
			return nil, err
		}

		// found more than requested
		if int64(len(rsp.Versions)) >= limit {
			continueToken := &ContinueToken{
				Sort:        r.Sort,
				StartOffset: entityQuery.offset + entityQuery.limit,
			}
			rsp.NextPageToken = continueToken.String()
			break
		}

		rsp.Versions = append(rsp.Versions, v)
	}
	return rsp, err
}

type ContinueRequest interface {
	GetNextPageToken() string
	GetSort() []string
}

type ContinueToken struct {
	Sort            []string `json:"s"`
	StartOffset     int64    `json:"o"`
	ResourceVersion int64    `json:"v"`
	RecordCnt       int64    `json:"c"`
}

func (c *ContinueToken) String() string {
	b, _ := json.Marshal(c)
	return base64.StdEncoding.EncodeToString(b)
}

func GetContinueToken(r ContinueRequest) (*ContinueToken, error) {
	if r.GetNextPageToken() == "" {
		return nil, nil
	}

	continueVal, err := base64.StdEncoding.DecodeString(r.GetNextPageToken())
	if err != nil {
		return nil, fmt.Errorf("error decoding continue token")
	}

	t := &ContinueToken{}
	err = json.Unmarshal(continueVal, t)
	if err != nil {
		return nil, err
	}

	if !slices.Equal(t.Sort, r.GetSort()) {
		return nil, fmt.Errorf("sort order changed")
	}

	return t, nil
}

var sortByFields = []string{
	"guid",
	"key",
	"namespace", "group", "group_version", "resource", "name", "folder",
	"resource_version", "size", "etag",
	"created_at", "created_by",
	"updated_at", "updated_by",
	"origin", "origin_key", "origin_ts",
	"title", "slug", "description",
}

type SortBy struct {
	Field     string
	Direction Direction
}

func ParseSortBy(sort string) (*SortBy, error) {
	sortBy := &SortBy{
		Field:     "guid",
		Direction: Ascending,
	}

	if strings.HasSuffix(sort, "_desc") {
		sortBy.Field = sort[:len(sort)-5]
		sortBy.Direction = Descending
	} else {
		sortBy.Field = sort
	}

	if !slices.Contains(sortByFields, sortBy.Field) {
		return nil, fmt.Errorf("invalid sort field '%s', valid fields: %v", sortBy.Field, sortByFields)
	}

	return sortBy, nil
}

//nolint:gocyclo
func (s *sqlEntityServer) List(ctx context.Context, r *entity.EntityListRequest) (*entity.EntityListResponse, error) {
	ctx, span := s.tracer.Start(ctx, "storage_server.List")
	defer span.End()
	ctxLogger := s.log.FromContext(log.WithContextualAttributes(ctx, []any{"method", "list"}))

	if err := s.Init(); err != nil {
		ctxLogger.Error("init error", "error", err)
		return nil, err
	}

	user, err := appcontext.User(ctx)
	if err != nil {
		ctxLogger.Error("error getting user from ctx", "error", err)
		return nil, err
	}
	if user == nil {
		ctxLogger.Error("could not find user in context", "error", errorUserNotFoundInContext)
		return nil, errorUserNotFoundInContext
	}

	fields := s.getReadFields(r)

	// main query we will use to retrieve entities
	entityQuery := NewSelectQuery(s.dialect, entityTable)
	entityQuery.AddFields(fields...)
	entityQuery.SetLimit(r.Limit)
	entityQuery.SetOneExtra()

	// query to retrieve the max resource version and entity count
	rvMaxQuery := NewSelectQuery(s.dialect, entityTable)
	rvMaxQuery.AddRawFields("coalesce(max(resource_version),0) as rv", "count(guid) as cnt")

	// subquery to get latest resource version for each entity
	// when we need to query from entity_history
	rvSubQuery := NewSelectQuery(s.dialect, entityHistoryTable)
	rvSubQuery.AddFields("guid")
	rvSubQuery.AddRawFields("max(resource_version) as max_rv")

	// if we are looking for deleted entities, we list "deleted" entries from the entity_history table
	if r.Deleted {
		entityQuery.from = entityHistoryTable
		entityQuery.AddWhere("action", entity.Entity_DELETED)

		rvMaxQuery.from = entityHistoryTable
		rvMaxQuery.AddWhere("action", entity.Entity_DELETED)
	}

	// TODO fix this
	// entityQuery.addWhere("namespace", user.OrgID)

	if len(r.Group) > 0 {
		entityQuery.AddWhereIn("group", ToAnyList(r.Group))
		rvMaxQuery.AddWhereIn("group", ToAnyList(r.Group))
		rvSubQuery.AddWhereIn("group", ToAnyList(r.Group))
	}

	if len(r.Resource) > 0 {
		entityQuery.AddWhereIn("resource", ToAnyList(r.Resource))
		rvMaxQuery.AddWhereIn("resource", ToAnyList(r.Resource))
		rvSubQuery.AddWhereIn("resource", ToAnyList(r.Resource))
	}

	if len(r.Key) > 0 {
		where := []string{}
		args := []any{}
		for _, k := range r.Key {
			key, err := entity.ParseKey(k)
			if err != nil {
				return nil, err
			}

			args = append(args, key.Group, key.Resource)
			whereclause := "(t." + s.dialect.Quote("group") + "=? AND t." + s.dialect.Quote("resource") + "=?"
			if key.Namespace != "" {
				args = append(args, key.Namespace)
				whereclause += " AND t." + s.dialect.Quote("namespace") + "=?"
			}
			if key.Name != "" {
				args = append(args, key.Name)
				whereclause += " AND t." + s.dialect.Quote("name") + "=?"
			}
			whereclause += ")"

			where = append(where, whereclause)
		}

		entityQuery.AddWhere("("+strings.Join(where, " OR ")+")", args...)
		rvMaxQuery.AddWhere("("+strings.Join(where, " OR ")+")", args...)
		rvSubQuery.AddWhere("("+strings.Join(where, " OR ")+")", args...)
	}

	if len(r.OriginKeys) > 0 {
		entityQuery.AddWhereIn("origin_key", ToAnyList(r.OriginKeys))
		rvMaxQuery.AddWhereIn("origin_key", ToAnyList(r.OriginKeys))
		rvSubQuery.AddWhereIn("origin_key", ToAnyList(r.OriginKeys))
	}

	// get the maximum resource version and count of entities
	type RVMaxRow struct {
		Rv  int64 `db:"rv"`
		Cnt int64 `db:"cnt"`
	}
	rvMaxRow := &RVMaxRow{}
	query, args := rvMaxQuery.ToQuery()

	err = s.sess.Get(ctx, rvMaxRow, query, args...)
	if err != nil {
		if !errors.Is(err, sql.ErrNoRows) {
			ctxLogger.Error("error running rvMaxQuery", "error", err)
			return nil, err
		}
	}

	ctxLogger.Debug("getting max rv", "maxRv", rvMaxRow.Rv, "cnt", rvMaxRow.Cnt, "query", query, "args", args)

	// if we have a page token, use that to specify the first record
	continueToken, err := GetContinueToken(r)
	if err != nil {
		ctxLogger.Error("error getting continue token", "error", err)
		return nil, err
	}
	if continueToken != nil {
		entityQuery.offset = continueToken.StartOffset
		if continueToken.ResourceVersion > 0 {
			if r.Deleted {
				// if we're continuing, we need to list only revisions that are older than the given resource version
				entityQuery.AddWhere("resource_version <= ?", continueToken.ResourceVersion)
			} else {
				// cap versions considered by the per resource max version subquery
				rvSubQuery.AddWhere("resource_version <= ?", continueToken.ResourceVersion)
			}
		}

		if (continueToken.ResourceVersion > 0 && continueToken.ResourceVersion != rvMaxRow.Rv) || (continueToken.RecordCnt > 0 && continueToken.RecordCnt != rvMaxRow.Cnt) {
			entityQuery.From(entityHistoryTable)
			entityQuery.AddWhere("t.action != ?", entity.Entity_DELETED)

			rvSubQuery.AddGroupBy("guid")
			query, args = rvSubQuery.ToQuery()
			entityQuery.AddJoin("INNER JOIN ("+query+") rv ON rv.guid = t.guid AND rv.max_rv = t.resource_version", args...)
		}
	} else {
		continueToken = &ContinueToken{
			Sort:            r.Sort,
			StartOffset:     0,
			ResourceVersion: rvMaxRow.Rv,
			RecordCnt:       rvMaxRow.Cnt,
		}

		if continueToken.ResourceVersion == 0 {
			// we use a snowflake as a fallback resource version
			continueToken.ResourceVersion = s.snowflake.Generate().Int64()
		}
	}

	// initialize the result
	rsp := &entity.EntityListResponse{
		ResourceVersion: continueToken.ResourceVersion,
	}

	// Folder guid
	if r.Folder != "" {
		entityQuery.AddWhere("folder", r.Folder)
	}

	if len(r.Labels) > 0 {
		// if we are looking for deleted entities, we need to use the labels column
		if entityQuery.from == entityHistoryTable {
			for labelKey, labelValue := range r.Labels {
				entityQuery.AddWhereJsonContainsKV("labels", labelKey, labelValue)
			}
			// for active entities, we can use the entity_labels table
		} else {
			var args []any
			var conditions []string
			for labelKey, labelValue := range r.Labels {
				args = append(args, labelKey)
				args = append(args, labelValue)
				conditions = append(conditions, "(label = ? AND value = ?)")
			}
			query := "SELECT guid FROM entity_labels" +
				" WHERE (" + strings.Join(conditions, " OR ") + ")" +
				" GROUP BY guid" +
				" HAVING COUNT(label) = ?"
			args = append(args, len(r.Labels))

			entityQuery.AddWhereInSubquery("guid", query, args)
		}
	}

	for _, sort := range r.Sort {
		sortBy, err := ParseSortBy(sort)
		if err != nil {
			return nil, err
		}
		entityQuery.AddOrderBy(sortBy.Field, sortBy.Direction)
	}
	entityQuery.AddOrderBy("guid", Ascending)

	query, args = entityQuery.ToQuery()

	ctxLogger.Debug("listing", "query", query, "args", args)

	rows, err := s.query(ctx, query, args...)
	if err != nil {
		ctxLogger.Error("error running list query", "error", err)
		return nil, err
	}
	defer func() { _ = rows.Close() }()
	for rows.Next() {
		result, err := readEntity(rows, r)
		if err != nil {
			ctxLogger.Error("error reading rows to entity", "error", err)
			return rsp, err
		}

		// found more than requested
		if entityQuery.limit > 0 && int64(len(rsp.Results)) >= entityQuery.limit {
			continueToken.StartOffset = entityQuery.offset + entityQuery.limit
			rsp.NextPageToken = continueToken.String()
			break
		}

		rsp.Results = append(rsp.Results, result)
	}
	span.AddEvent("processed rows", trace.WithAttributes(attribute.Int("row_count", len(rsp.Results))))

	return rsp, err
}

func (s *sqlEntityServer) Watch(w entity.EntityStore_WatchServer) error {
	ctxLogger := s.log.FromContext(log.WithContextualAttributes(w.Context(), []any{"method", "watch"}))

	if err := s.Init(); err != nil {
		ctxLogger.Error("init error", "error", err)
		return err
	}

	user, err := appcontext.User(w.Context())
	if err != nil {
		ctxLogger.Error("error getting user from ctx", "error", err)
		return err
	}
	if user == nil {
		ctxLogger.Error("could not find user in context", "error", errorUserNotFoundInContext)
		return errorUserNotFoundInContext
	}

	r, err := w.Recv()
	if err != nil {
		ctxLogger.Error("recv error", "error", err)
		return err
	}

	// collect and send any historical events
	if r.SendInitialEvents {
		r.Since, err = s.watchInit(w.Context(), r, w)
		if err != nil {
			ctxLogger.Error("watch init error", "err", err)
			return err
		}
	} else if r.Since == 0 {
		r.Since = s.snowflake.Generate().Int64()
	}

	// subscribe to new events
	err = s.watch(r, w)
	if err != nil {
		ctxLogger.Error("watch error", "err", err)
		return err
	}

	return nil
}

// watchInit is a helper function to send the initial set of entities to the client
func (s *sqlEntityServer) watchInit(ctx context.Context, r *entity.EntityWatchRequest, w entity.EntityStore_WatchServer) (int64, error) {
	ctx, span := s.tracer.Start(ctx, "storage_server.watchInit")
	defer span.End()
	ctxLogger := s.log.FromContext(log.WithContextualAttributes(ctx, []any{"method", "watchInit"}))

	lastRv := r.Since

	fields := s.getReadFields(r)

	entityQuery := selectQuery{
		dialect:  s.dialect,
		from:     entityTable, // the table
		limit:    1000,        // r.Limit,
		oneExtra: true,        // request one more than the limit (and show next token if it exists)
	}

	entityQuery.AddFields(fields...)

	// TODO fix this
	// entityQuery.addWhere("namespace", user.OrgID)

	if len(r.Resource) > 0 {
		entityQuery.AddWhereIn("resource", ToAnyList(r.Resource))
	}

	if len(r.Key) > 0 {
		where := []string{}
		args := []any{}
		for _, k := range r.Key {
			key, err := entity.ParseKey(k)
			if err != nil {
				ctxLogger.Error("error parsing key", "error", err, "key", k)
				return lastRv, err
			}

			args = append(args, key.Group, key.Resource)
			whereclause := "(" + s.dialect.Quote("group") + "=? AND " + s.dialect.Quote("resource") + "=?"
			if key.Namespace != "" {
				args = append(args, key.Namespace)
				whereclause += " AND " + s.dialect.Quote("namespace") + "=?"
			}
			if key.Name != "" {
				args = append(args, key.Name)
				whereclause += " AND " + s.dialect.Quote("name") + "=?"
			}
			whereclause += ")"

			where = append(where, whereclause)
		}

		entityQuery.AddWhere("("+strings.Join(where, " OR ")+")", args...)
	}

	// Folder guid
	if r.Folder != "" {
		entityQuery.AddWhere("folder", r.Folder)
	}

	if len(r.Labels) > 0 {
		if entityQuery.from != entityTable {
			for labelKey, labelValue := range r.Labels {
				entityQuery.AddWhereJsonContainsKV("labels", labelKey, labelValue)
			}
		} else {
			var args []any
			var conditions []string
			for labelKey, labelValue := range r.Labels {
				args = append(args, labelKey)
				args = append(args, labelValue)
				conditions = append(conditions, "(label = ? AND value = ?)")
			}
			query := "SELECT guid FROM entity_labels" +
				" WHERE (" + strings.Join(conditions, " OR ") + ")" +
				" GROUP BY guid" +
				" HAVING COUNT(label) = ?"
			args = append(args, len(r.Labels))

			entityQuery.AddWhereInSubquery("guid", query, args)
		}
	}

	entityQuery.AddOrderBy("resource_version", Ascending)

	var err error

	for hasmore := true; hasmore; {
		err = func() error {
			query, args := entityQuery.ToQuery()

			ctxLogger.Debug("watch init", "query", query, "args", args)

			rows, err := s.query(ctx, query, args...)
			if err != nil {
				return err
			}
			defer func() { _ = rows.Close() }()

			found := int64(0)

			for rows.Next() {
				found++
				if found > entityQuery.limit {
					entityQuery.offset += entityQuery.limit
					return nil
				}

				result, err := readEntity(rows, r)
				if err != nil {
					return err
				}

				if result.ResourceVersion > lastRv {
					lastRv = result.ResourceVersion
				}

				resp := &entity.EntityWatchResponse{
					Timestamp: time.Now().UnixMilli(),
					Entity:    result,
				}

				ctxLogger.Debug("sending init event", "guid", result.Guid, "action", result.Action, "rv", result.ResourceVersion)

				err = w.Send(resp)
				if err != nil {
					return err
				}
			}

			hasmore = false
			return nil
		}()
		if err != nil {
			ctxLogger.Error("watchInit error", "error", err)
			return lastRv, err
		}
	}

	// send a bookmark event
	if r.AllowWatchBookmarks {
		resp := &entity.EntityWatchResponse{
			Timestamp: time.Now().UnixMilli(),
			Entity: &entity.Entity{
				Action:          entity.Entity_BOOKMARK,
				ResourceVersion: lastRv,
			},
		}
		err = w.Send(resp)
		if err != nil {
			ctxLogger.Error("error sending bookmark event", "error", err)
			return lastRv, err
		}
	}

	return lastRv, nil
}

func (s *sqlEntityServer) poller(stream chan *entity.EntityWatchResponse) {
	var err error
	since := s.snowflake.Generate().Int64()

	interval := 1 * time.Second

	t := time.NewTicker(interval)
	defer t.Stop()

	for {
		select {
		case <-s.ctx.Done():
			return
		case <-t.C:
			since, err = s.poll(since, stream)
			if err != nil {
				s.log.Error("watch error", "err", err)
			}
			t.Reset(interval)
		}
	}
}

func (s *sqlEntityServer) poll(since int64, out chan *entity.EntityWatchResponse) (int64, error) {
	ctx, span := s.tracer.Start(s.ctx, "storage_server.poll")
	defer span.End()
	ctxLogger := s.log.FromContext(log.WithContextualAttributes(ctx, []any{"method", "poll"}))

	rr := &entity.ReadEntityRequest{
		WithBody:   true,
		WithStatus: true,
	}

	fields := s.getReadFields(rr)

	for hasmore := true; hasmore; {
		err := func() error {
			entityQuery := selectQuery{
				dialect: s.dialect,
				from:    entityHistoryTable, // the table
				limit:   100,                // r.Limit,
				// offset:   0,
				oneExtra: true, // request one more than the limit (and show next token if it exists)
				orderBy:  []string{"resource_version"},
			}

			entityQuery.AddFields(fields...)
			entityQuery.AddWhere("resource_version > ?", since)

			query, args := entityQuery.ToQuery()

			rows, err := s.query(ctx, query, args...)
			if err != nil {
				return err
			}
			defer func() { _ = rows.Close() }()

			found := int64(0)
			for rows.Next() {
				// check if the context is done
				if ctx.Err() != nil {
					hasmore = false
					return nil
				}

				found++
				if found > entityQuery.limit {
					return nil
				}

				updated, err := readEntity(rows, rr)
				if err != nil {
					ctxLogger.Error("poll error readEntity", "error", err)
					return err
				}

				if updated.ResourceVersion > since {
					since = updated.ResourceVersion
				}

				result := &entity.EntityWatchResponse{
					Timestamp: time.Now().UnixMilli(),
					Entity:    updated,
				}

				if updated.Action == entity.Entity_UPDATED || updated.Action == entity.Entity_DELETED {
					rr := &entity.EntityHistoryRequest{
						Guid:       updated.Guid,
						Before:     updated.ResourceVersion,
						Limit:      1,
						Sort:       []string{"resource_version_desc"},
						WithBody:   rr.WithBody,
						WithStatus: rr.WithStatus,
					}
					history, err := s.history(ctx, rr)
					if err != nil {
						ctxLogger.Error("error reading previous entity", "guid", updated.Guid, "err", err)
						return err
					}

					result.Previous = history.Versions[0]
				}

				ctxLogger.Debug("sending poll result", "guid", updated.Guid, "action", updated.Action, "rv", updated.ResourceVersion)
				out <- result
			}

			hasmore = false
			return nil
		}()
		if err != nil {
			ctxLogger.Error("poll error", "error", err)
			return since, err
		}
	}

	return since, nil
}

func watchMatches(r *entity.EntityWatchRequest, result *entity.Entity) bool {
	if result == nil {
		return false
	}

	// Folder guid
	if r.Folder != "" && r.Folder != result.Folder {
		return false
	}

	// must match at least one resource if specified
	if len(r.Resource) > 0 {
		matched := false
		for _, res := range r.Resource {
			if res == result.Resource {
				matched = true
				break
			}
		}
		if !matched {
			return false
		}
	}

	// must match at least one key if specified
	if len(r.Key) > 0 {
		matched := false
		for _, k := range r.Key {
			key, err := entity.ParseKey(k)
			if err != nil {
				return false
			}

			if key.Group == result.Group && key.Resource == result.Resource && (key.Namespace == "" || key.Namespace == result.Namespace) && (key.Name == "" || key.Name == result.Name) {
				matched = true
				break
			}
		}
		if !matched {
			return false
		}
	}

	// must match all specified label/value pairs
	if len(r.Labels) > 0 {
		for labelKey, labelValue := range r.Labels {
			if result.Labels[labelKey] != labelValue {
				return false
			}
		}
	}

	return true
}

// watch is a helper to get the next set of entities and send them to the client
func (s *sqlEntityServer) watch(r *entity.EntityWatchRequest, w entity.EntityStore_WatchServer) error {
	s.log.Debug("watch started", "since", r.Since)

	evts, err := s.broadcaster.Subscribe(w.Context())
	if err != nil {
		return err
	}

	stop := make(chan struct{})
	since := r.Since

	go func() {
		for {
			r, err := w.Recv()
			if errors.Is(err, io.EOF) {
				s.log.Debug("watch client closed stream")
				stop <- struct{}{}
				return
			}
			if err != nil {
				s.log.Error("error receiving message", "err", err)
				stop <- struct{}{}
				return
			}
			if r.Action == entity.EntityWatchRequest_STOP {
				s.log.Debug("watch stop requested")
				stop <- struct{}{}
				return
			}
			// handle any other message types
			s.log.Debug("watch received unexpected message", "action", r.Action)
		}
	}()

	for {
		select {
		// stop signal
		case <-stop:
			s.log.Debug("watch stopped")
			return nil
		// context canceled
		case <-w.Context().Done():
			s.log.Debug("watch context done")
			return nil
		// got a raw result from the broadcaster
		case result, ok := <-evts:
			if !ok {
				s.log.Debug("watch events closed")
				return nil
			}

			// Invalid result or resource version too old
			if result == nil || result.Entity == nil || result.Entity.ResourceVersion <= since {
				break
			}

			since = result.Entity.ResourceVersion

			resp, err := s.watchEvent(r, result)
			if err != nil {
				break
			}
			if resp == nil {
				break
			}

			err = w.Send(resp)
			if err != nil {
				s.log.Error("error sending watch event", "err", err)
				return err
			}
		}
	}
}

func (s *sqlEntityServer) watchEvent(r *entity.EntityWatchRequest, result *entity.EntityWatchResponse) (*entity.EntityWatchResponse, error) {
	// if this is an update or a delete, check the current or previous version matches
	if result.Previous != nil {
		// if neither the previous nor the current result match our watch params, skip it
		if !watchMatches(r, result.Entity) && !watchMatches(r, result.Previous) {
			s.log.Debug("watch result not matched", "guid", result.Entity.Guid, "action", result.Entity.Action, "rv", result.Entity.ResourceVersion)
			return nil, nil
		}
	} else {
		// if result doesn't match our watch params, skip it
		if !watchMatches(r, result.Entity) {
			s.log.Debug("watch result not matched", "guid", result.Entity.Guid, "action", result.Entity.Action, "rv", result.Entity.ResourceVersion)
			return nil, nil
		}
	}

	// remove the body and status if not requested
	if !r.WithBody {
		result.Entity.Body = nil
		if result.Previous != nil {
			result.Previous.Body = nil
		}
	}
	if !r.WithStatus {
		result.Entity.Status = nil
		if result.Previous != nil {
			result.Previous.Status = nil
		}
	}

	s.log.Debug("sending watch result", "guid", result.Entity.Guid, "action", result.Entity.Action, "rv", result.Entity.ResourceVersion)
	return result, nil
}

func (s *sqlEntityServer) FindReferences(ctx context.Context, r *entity.ReferenceRequest) (*entity.EntityListResponse, error) {
	ctx, span := s.tracer.Start(ctx, "storage_server.FindReferences")
	defer span.End()
	ctxLogger := s.log.FromContext(log.WithContextualAttributes(ctx, []any{"method", "findReferences"}))

	if err := s.Init(); err != nil {
		ctxLogger.Error("init error", "error", err)
		return nil, err
	}

	user, err := appcontext.User(ctx)
	if err != nil {
		ctxLogger.Error("error getting user from ctx", "error", err)
		return nil, err
	}
	if user == nil {
		ctxLogger.Error("could not find user in context", "error", errorUserNotFoundInContext)
		return nil, errorUserNotFoundInContext
	}

	if r.NextPageToken != "" {
		ctxLogger.Error("nextPageToken not yet supported", "error", errorNextPageTokenNotSupported)
		return nil, errorNextPageTokenNotSupported
	}

	fields := []string{
		"e.guid", "e.guid",
		"e.namespace", "e.group", "e.group_version", "e.resource", "e.name",
		"e.resource_version", "e.folder", "e.slug", "e.errors", // errors are always returned
		"e.size", "e.updated_at", "e.updated_by",
		"e.title", "e.description", "e.meta",
	}

	sql := "SELECT " + strings.Join(fields, ",") +
		" FROM entity_ref AS er JOIN entity AS e ON er.guid = e.guid" +
		" WHERE er.namespace=? AND er.group=? AND er.resource=? AND er.resolved_to=?"

	rows, err := s.query(ctx, sql, r.Namespace, r.Group, r.Resource, r.Name)
	if err != nil {
		ctxLogger.Error("query error", "error", err)
		return nil, err
	}
	defer func() { _ = rows.Close() }()
	token := ""
	rsp := &entity.EntityListResponse{}
	for rows.Next() {
		result := &entity.Entity{}

		args := []any{
			&token, &result.Guid,
			&result.Namespace, &result.Group, &result.GroupVersion, &result.Resource, &result.Name,
			&result.ResourceVersion, &result.Folder, &result.Slug, &result.Errors,
			&result.Size, &result.UpdatedAt, &result.UpdatedBy,
			&result.Title, &result.Description, &result.Meta,
		}

		err = rows.Scan(args...)
		if err != nil {
			ctxLogger.Error("error scanning rows", "error", err)
			return rsp, err
		}

		// // found one more than requested
		// if int64(len(rsp.Results)) >= entityQuery.limit {
		// 	// TODO? should this encode start+offset?
		// 	rsp.NextPageToken = token
		// 	break
		// }

		rsp.Results = append(rsp.Results, result)
	}

	return rsp, err
}

func (s *sqlEntityServer) query(ctx context.Context, query string, args ...any) (*sql.Rows, error) {
	ctx, span := s.tracer.Start(ctx, "storage_server.query", trace.WithAttributes(attribute.String("query", query)))
	defer span.End()

	rows, err := s.sess.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	return rows, nil
}

func (s *sqlEntityServer) exec(ctx context.Context, tx *session.SessionTx, statement string, args ...any) error {
	ctx, span := s.tracer.Start(ctx, "storage_server.exec", trace.WithAttributes(attribute.String("statement", statement)))
	defer span.End()

	_, err := tx.Exec(ctx, statement, args...)
	return err
}

func (s *sqlEntityServer) insert(ctx context.Context, tx *session.SessionTx, table string, values map[string]any) error {
	ctx, span := s.tracer.Start(ctx, "storage_server.insert", trace.WithAttributes(attribute.String("table", table)))
	defer span.End()

	err := s.dialect.Insert(ctx, tx, table, values)
	return err
}

func (s *sqlEntityServer) update(ctx context.Context, tx *session.SessionTx, table string, row map[string]any, where map[string]any) error {
	ctx, span := s.tracer.Start(ctx, "storage_server.db_update", trace.WithAttributes(attribute.String("table", table)))
	defer span.End()

	err := s.dialect.Update(
		ctx,
		tx,
		table,
		row,
		where,
	)
	return err
}
