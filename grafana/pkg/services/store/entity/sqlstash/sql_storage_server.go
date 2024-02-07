package sqlstash

import (
	"context"
	"database/sql"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"math/rand"
	"slices"
	"strings"
	"time"

	"github.com/bwmarrin/snowflake"
	"github.com/google/uuid"

	folder "github.com/grafana/grafana/pkg/apis/folder/v0alpha1"
	"github.com/grafana/grafana/pkg/infra/appcontext"
	"github.com/grafana/grafana/pkg/infra/log"
	"github.com/grafana/grafana/pkg/services/sqlstore/migrator"
	"github.com/grafana/grafana/pkg/services/sqlstore/session"
	"github.com/grafana/grafana/pkg/services/store"
	"github.com/grafana/grafana/pkg/services/store/entity"
	"github.com/grafana/grafana/pkg/services/store/entity/db"
)

// Make sure we implement both store + admin
var _ entity.EntityStoreServer = &sqlEntityServer{}

func ProvideSQLEntityServer(db db.EntityDBInterface /*, cfg *setting.Cfg */) (entity.EntityStoreServer, error) {
	snode, err := snowflake.NewNode(rand.Int63n(1024))
	if err != nil {
		return nil, err
	}

	entityServer := &sqlEntityServer{
		db:        db,
		log:       log.New("sql-entity-server"),
		snowflake: snode,
	}

	return entityServer, nil
}

type sqlEntityServer struct {
	log       log.Logger
	db        db.EntityDBInterface // needed to keep xorm engine in scope
	sess      *session.SessionDB
	dialect   migrator.Dialect
	snowflake *snowflake.Node
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
	return nil
}

func (s *sqlEntityServer) getReadFields(r *entity.ReadEntityRequest) []string {
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
	}

	if r.WithBody {
		fields = append(fields, `body`)
	}
	if r.WithStatus {
		fields = append(fields, "status")
	}

	return fields
}

func (s *sqlEntityServer) getReadSelect(r *entity.ReadEntityRequest) (string, error) {
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

func (s *sqlEntityServer) rowToEntity(ctx context.Context, rows *sql.Rows, r *entity.ReadEntityRequest) (*entity.Entity, error) {
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
	}
	if r.WithBody {
		args = append(args, &raw.Body)
	}
	if r.WithStatus {
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
	if err := s.Init(); err != nil {
		return nil, err
	}

	return s.read(ctx, s.sess, r)
}

func (s *sqlEntityServer) read(ctx context.Context, tx session.SessionQuerier, r *entity.ReadEntityRequest) (*entity.Entity, error) {
	table := "entity"
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
		table = "entity_history"
		where = append(where, s.dialect.Quote("resource_version")+"=?")
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

	s.log.Debug("read", "query", query, "args", args)

	rows, err := tx.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer func() { _ = rows.Close() }()

	if !rows.Next() {
		return &entity.Entity{}, nil
	}

	return s.rowToEntity(ctx, rows, r)
}

func (s *sqlEntityServer) BatchRead(ctx context.Context, b *entity.BatchReadEntityRequest) (*entity.BatchReadEntityResponse, error) {
	if len(b.Batch) < 1 {
		return nil, fmt.Errorf("missing querires")
	}

	first := b.Batch[0]
	args := []any{}
	constraints := []string{}

	for _, r := range b.Batch {
		if r.WithBody != first.WithBody || r.WithStatus != first.WithStatus {
			return nil, fmt.Errorf("requests must want the same things")
		}

		if r.Key == "" {
			return nil, fmt.Errorf("missing key")
		}

		constraints = append(constraints, s.dialect.Quote("key")+"=?")
		args = append(args, r.Key)

		if r.ResourceVersion != 0 {
			return nil, fmt.Errorf("version not supported for batch read (yet?)")
		}
	}

	req := b.Batch[0]
	query, err := s.getReadSelect(req)
	if err != nil {
		return nil, err
	}

	query += " FROM entity" +
		" WHERE (" + strings.Join(constraints, " OR ") + ")"
	rows, err := s.sess.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer func() { _ = rows.Close() }()

	// TODO? make sure the results are in order?
	rsp := &entity.BatchReadEntityResponse{}
	for rows.Next() {
		r, err := s.rowToEntity(ctx, rows, req)
		if err != nil {
			return nil, err
		}
		rsp.Results = append(rsp.Results, r)
	}
	return rsp, nil
}

//nolint:gocyclo
func (s *sqlEntityServer) Create(ctx context.Context, r *entity.CreateEntityRequest) (*entity.CreateEntityResponse, error) {
	if err := s.Init(); err != nil {
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
			return nil, err
		}
		if modifier == nil {
			return nil, fmt.Errorf("can not find user in context")
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
			return fmt.Errorf("entity already exists")
		}

		// generate guid for new entity
		current.Guid = uuid.New().String()

		// set created at/by
		current.CreatedAt = createdAt
		current.CreatedBy = createdBy

		// parse provided key
		key, err := entity.ParseKey(r.Entity.Key)
		if err != nil {
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
			s.log.Error("error marshalling labels", "msg", err.Error())
			return err
		}
		current.Labels = r.Entity.Labels

		fields, err := json.Marshal(r.Entity.Fields)
		if err != nil {
			s.log.Error("error marshalling fields", "msg", err.Error())
			return err
		}
		current.Fields = r.Entity.Fields

		errors, err := json.Marshal(r.Entity.Errors)
		if err != nil {
			s.log.Error("error marshalling errors", "msg", err.Error())
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

		// Update version
		current.ResourceVersion = s.snowflake.Generate().Int64()

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
		}

		// 1. Add row to the `entity_history` values
		if err := s.dialect.Insert(ctx, tx, "entity_history", values); err != nil {
			s.log.Error("error inserting entity history", "msg", err.Error())
			return err
		}

		// 2. Add row to the main `entity` table
		if err := s.dialect.Insert(ctx, tx, "entity", values); err != nil {
			s.log.Error("error inserting entity", "msg", err.Error())
			return err
		}

		switch current.Group {
		case folder.GROUP:
			switch current.Resource {
			case folder.RESOURCE:
				err = s.updateFolderTree(ctx, tx, current.Namespace)
				if err != nil {
					s.log.Error("error updating folder tree", "msg", err.Error())
					return err
				}
			}
		}

		rsp.Entity = current

		return s.setLabels(ctx, tx, current.Guid, current.Labels)
	})
	if err != nil {
		s.log.Error("error creating entity", "msg", err.Error())
		rsp.Status = entity.CreateEntityResponse_ERROR
	}

	return rsp, err
}

//nolint:gocyclo
func (s *sqlEntityServer) Update(ctx context.Context, r *entity.UpdateEntityRequest) (*entity.UpdateEntityResponse, error) {
	if err := s.Init(); err != nil {
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
			return nil, err
		}
		if modifier == nil {
			return nil, fmt.Errorf("can not find user in context")
		}
		updatedBy = store.GetUserIDString(modifier)
	}

	rsp := &entity.UpdateEntityResponse{
		Entity: &entity.Entity{},
		Status: entity.UpdateEntityResponse_UPDATED, // Will be changed if not true
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

		// Optimistic locking
		if r.PreviousVersion > 0 && r.PreviousVersion != current.ResourceVersion {
			return fmt.Errorf("optimistic lock failed")
		}

		// if we didn't find an existing entity
		if current.Guid == "" {
			return fmt.Errorf("entity not found")
		}

		rsp.Entity.Guid = current.Guid

		// Clear the refs
		if _, err := tx.Exec(ctx, "DELETE FROM entity_ref WHERE guid=?", rsp.Entity.Guid); err != nil {
			return err
		}

		if r.Entity.GroupVersion != "" {
			current.GroupVersion = r.Entity.GroupVersion
		}

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
			s.log.Error("error marshalling labels", "msg", err.Error())
			return err
		}
		current.Labels = r.Entity.Labels

		fields, err := json.Marshal(r.Entity.Fields)
		if err != nil {
			s.log.Error("error marshalling fields", "msg", err.Error())
			return err
		}
		current.Fields = r.Entity.Fields

		errors, err := json.Marshal(r.Entity.Errors)
		if err != nil {
			s.log.Error("error marshalling errors", "msg", err.Error())
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

		// Update version
		current.ResourceVersion = s.snowflake.Generate().Int64()

		values := map[string]any{
			// below are only set in history table
			"guid":       current.Guid,
			"key":        current.Key,
			"namespace":  current.Namespace,
			"group":      current.Group,
			"resource":   current.Resource,
			"name":       current.Name,
			"created_at": current.CreatedAt,
			"created_by": current.CreatedBy,
			// below are updated
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
		}

		// 1. Add the `entity_history` values
		if err := s.dialect.Insert(ctx, tx, "entity_history", values); err != nil {
			s.log.Error("error inserting entity history", "msg", err.Error())
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

		err = s.dialect.Update(
			ctx,
			tx,
			"entity",
			values,
			map[string]any{
				"guid": current.Guid,
			},
		)
		if err != nil {
			s.log.Error("error updating entity", "msg", err.Error())
			return err
		}

		switch current.Group {
		case folder.GROUP:
			switch current.Resource {
			case folder.RESOURCE:
				err = s.updateFolderTree(ctx, tx, current.Namespace)
				if err != nil {
					s.log.Error("error updating folder tree", "msg", err.Error())
					return err
				}
			}
		}

		rsp.Entity = current

		return s.setLabels(ctx, tx, current.Guid, current.Labels)
	})
	if err != nil {
		s.log.Error("error updating entity", "msg", err.Error())
		rsp.Status = entity.UpdateEntityResponse_ERROR
	}

	return rsp, err
}

func (s *sqlEntityServer) setLabels(ctx context.Context, tx *session.SessionTx, guid string, labels map[string]string) error {
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
	if err := s.Init(); err != nil {
		return nil, err
	}

	rsp := &entity.DeleteEntityResponse{}

	err := s.sess.WithTransaction(ctx, func(tx *session.SessionTx) error {
		var err error
		rsp.Entity, err = s.Read(ctx, &entity.ReadEntityRequest{
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

		if r.PreviousVersion > 0 && r.PreviousVersion != rsp.Entity.ResourceVersion {
			rsp.Status = entity.DeleteEntityResponse_ERROR
			return fmt.Errorf("optimistic lock failed")
		}

		err = s.doDelete(ctx, tx, rsp.Entity)
		if err != nil {
			rsp.Status = entity.DeleteEntityResponse_ERROR
			return err
		}

		rsp.Status = entity.DeleteEntityResponse_DELETED
		return nil
	})

	return rsp, err
}

func (s *sqlEntityServer) doDelete(ctx context.Context, tx *session.SessionTx, ent *entity.Entity) error {
	_, err := tx.Exec(ctx, "DELETE FROM entity WHERE guid=?", ent.Guid)
	if err != nil {
		return err
	}

	// TODO: keep history? would need current version bump, and the "write" would have to get from history
	_, err = tx.Exec(ctx, "DELETE FROM entity_history WHERE guid=?", ent.Guid)
	if err != nil {
		return err
	}
	_, err = tx.Exec(ctx, "DELETE FROM entity_labels WHERE guid=?", ent.Guid)
	if err != nil {
		return err
	}
	_, err = tx.Exec(ctx, "DELETE FROM entity_ref WHERE guid=?", ent.Guid)
	if err != nil {
		return err
	}

	switch ent.Group {
	case folder.GROUP:
		switch ent.Resource {
		case folder.RESOURCE:
			err = s.updateFolderTree(ctx, tx, ent.Namespace)
			if err != nil {
				s.log.Error("error updating folder tree", "msg", err.Error())
				return err
			}
		}
	}

	return nil
}

func (s *sqlEntityServer) History(ctx context.Context, r *entity.EntityHistoryRequest) (*entity.EntityHistoryResponse, error) {
	if err := s.Init(); err != nil {
		return nil, err
	}

	var limit int64 = 100
	if r.Limit > 0 && r.Limit < 100 {
		limit = r.Limit
	}

	rr := &entity.ReadEntityRequest{
		Key:        r.Key,
		WithBody:   true,
		WithStatus: true,
	}

	query, err := s.getReadSelect(rr)
	if err != nil {
		return nil, err
	}

	if r.Key == "" {
		return nil, fmt.Errorf("missing key")
	}

	key, err := entity.ParseKey(r.Key)
	if err != nil {
		return nil, err
	}

	where := []string{}
	args := []any{}

	where = append(where, s.dialect.Quote("namespace")+"=?", s.dialect.Quote("group")+"=?", s.dialect.Quote("resource")+"=?", s.dialect.Quote("name")+"=?")
	args = append(args, key.Namespace, key.Group, key.Resource, key.Name)

	if r.NextPageToken != "" {
		if true {
			return nil, fmt.Errorf("tokens not yet supported")
		}
		where = append(where, "version <= ?")
		args = append(args, r.NextPageToken)
	}

	query += " FROM entity_history" +
		" WHERE " + strings.Join(where, " AND ") +
		" ORDER BY resource_version DESC" +
		// select 1 more than we need to see if there is a next page
		" LIMIT " + fmt.Sprint(limit+1)

	rows, err := s.sess.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer func() { _ = rows.Close() }()

	rsp := &entity.EntityHistoryResponse{
		Key: r.Key,
	}
	for rows.Next() {
		v, err := s.rowToEntity(ctx, rows, rr)
		if err != nil {
			return nil, err
		}

		// found more than requested
		if int64(len(rsp.Versions)) >= limit {
			rsp.NextPageToken = fmt.Sprintf("rv:%d", v.ResourceVersion)
			break
		}

		rsp.Versions = append(rsp.Versions, v)
	}
	return rsp, err
}

type ContinueToken struct {
	Sort        []string `json:"s"`
	StartOffset int64    `json:"o"`
}

func (c *ContinueToken) String() string {
	b, _ := json.Marshal(c)
	return base64.StdEncoding.EncodeToString(b)
}

func GetContinueToken(r *entity.EntityListRequest) (*ContinueToken, error) {
	if r.NextPageToken == "" {
		return nil, nil
	}

	continueVal, err := base64.StdEncoding.DecodeString(r.NextPageToken)
	if err != nil {
		return nil, fmt.Errorf("error decoding continue token")
	}

	t := &ContinueToken{}
	err = json.Unmarshal(continueVal, t)
	if err != nil {
		return nil, err
	}

	if !slices.Equal(t.Sort, r.Sort) {
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

func (s *sqlEntityServer) List(ctx context.Context, r *entity.EntityListRequest) (*entity.EntityListResponse, error) {
	if err := s.Init(); err != nil {
		return nil, err
	}

	user, err := appcontext.User(ctx)
	if err != nil {
		return nil, err
	}
	if user == nil {
		return nil, fmt.Errorf("missing user in context")
	}

	rr := &entity.ReadEntityRequest{
		WithBody:   r.WithBody,
		WithStatus: r.WithStatus,
	}

	fields := s.getReadFields(rr)

	entityQuery := selectQuery{
		dialect:  s.dialect,
		fields:   fields,
		from:     "entity", // the table
		args:     []any{},
		limit:    r.Limit,
		offset:   0,
		oneExtra: true, // request one more than the limit (and show next token if it exists)
	}

	// TODO fix this
	// entityQuery.addWhere("namespace", user.OrgID)

	if len(r.Group) > 0 {
		entityQuery.addWhereIn("group", r.Group)
	}

	if len(r.Resource) > 0 {
		entityQuery.addWhereIn("resource", r.Resource)
	}

	if len(r.Key) > 0 {
		where := []string{}
		args := []any{}
		for _, k := range r.Key {
			key, err := entity.ParseKey(k)
			if err != nil {
				return nil, err
			}

			args = append(args, key.Namespace, key.Group, key.Resource)
			whereclause := "(" + s.dialect.Quote("namespace") + "=? AND " + s.dialect.Quote("group") + "=? AND " + s.dialect.Quote("resource") + "=?"
			if key.Name != "" {
				args = append(args, key.Name)
				whereclause += " AND " + s.dialect.Quote("name") + "=?"
			}
			whereclause += ")"

			where = append(where, whereclause)
		}

		entityQuery.addWhere("("+strings.Join(where, " OR ")+")", args...)
	}

	// Folder guid
	if r.Folder != "" {
		entityQuery.addWhere("folder", r.Folder)
	}

	// if we have a page token, use that to specify the first record
	continueToken, err := GetContinueToken(r)
	if err != nil {
		return nil, err
	}
	if continueToken != nil {
		entityQuery.offset = continueToken.StartOffset
	}

	if len(r.Labels) > 0 {
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

		entityQuery.addWhereInSubquery("guid", query, args)
	}
	for _, sort := range r.Sort {
		sortBy, err := ParseSortBy(sort)
		if err != nil {
			return nil, err
		}
		entityQuery.addOrderBy(sortBy.Field, sortBy.Direction)
	}
	entityQuery.addOrderBy("guid", Ascending)

	query, args := entityQuery.toQuery()

	s.log.Debug("listing", "query", query, "args", args)

	rows, err := s.sess.Query(ctx, query, args...)
	if err != nil {
		return nil, err
	}
	defer func() { _ = rows.Close() }()
	rsp := &entity.EntityListResponse{}
	for rows.Next() {
		result, err := s.rowToEntity(ctx, rows, rr)
		if err != nil {
			return rsp, err
		}

		// found more than requested
		if int64(len(rsp.Results)) >= entityQuery.limit {
			continueToken := &ContinueToken{
				Sort:        r.Sort,
				StartOffset: entityQuery.offset + entityQuery.limit,
			}
			rsp.NextPageToken = continueToken.String()
			break
		}

		rsp.Results = append(rsp.Results, result)
	}

	return rsp, err
}

func (s *sqlEntityServer) Watch(*entity.EntityWatchRequest, entity.EntityStore_WatchServer) error {
	if err := s.Init(); err != nil {
		return err
	}

	return fmt.Errorf("unimplemented")
}

func (s *sqlEntityServer) FindReferences(ctx context.Context, r *entity.ReferenceRequest) (*entity.EntityListResponse, error) {
	if err := s.Init(); err != nil {
		return nil, err
	}

	user, err := appcontext.User(ctx)
	if err != nil {
		return nil, err
	}
	if user == nil {
		return nil, fmt.Errorf("missing user in context")
	}

	if r.NextPageToken != "" {
		return nil, fmt.Errorf("not yet supported")
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

	rows, err := s.sess.Query(ctx, sql, r.Namespace, r.Group, r.Resource, r.Name)
	if err != nil {
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
