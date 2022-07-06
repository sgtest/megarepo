package lsifstore

import (
	"context"
	"sort"
	"strconv"
	"strings"

	"github.com/keegancsmith/sqlf"
	"github.com/opentracing/opentracing-go/log"

	"github.com/sourcegraph/sourcegraph/internal/observation"
)

var tableNames = []string{
	"lsif_data_metadata",
	"lsif_data_documents",
	"lsif_data_documents_schema_versions",
	"lsif_data_result_chunks",
	"lsif_data_definitions",
	"lsif_data_definitions_schema_versions",
	"lsif_data_references",
	"lsif_data_references_schema_versions",
	"lsif_data_implementations",
	"lsif_data_implementations_schema_versions",
}

// DeleteLsifDataByUploadIds deletes LSIF data by UploadIds from the lsif database.
func (s *store) DeleteLsifDataByUploadIds(ctx context.Context, bundleIDs ...int) (err error) {
	ctx, trace, endObservation := s.operations.deleteLsifDataByUploadIds.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("numBundleIDs", len(bundleIDs)),
		log.String("bundleIDs", intsToString(bundleIDs)),
	}})
	defer endObservation(1, observation.Args{})

	if len(bundleIDs) == 0 {
		return nil
	}

	// Ensure ids are sorted so that we take row locks during the
	// DELETE query in a determinstic order. This should prevent
	// deadlocks with other queries that mass update the same table.
	sort.Ints(bundleIDs)

	var ids []*sqlf.Query
	for _, bundleID := range bundleIDs {
		ids = append(ids, sqlf.Sprintf("%d", bundleID))
	}

	tx, err := s.db.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() {
		err = tx.Done(err)
	}()

	for _, tableName := range tableNames {
		trace.Log(log.String("tableName", tableName))

		query := sqlf.Sprintf(deleteQuery, sqlf.Sprintf(tableName), sqlf.Join(ids, ","))
		if err := tx.Exec(ctx, query); err != nil {
			return err
		}
	}

	return nil
}

const deleteQuery = `
-- source: internal/codeintel/uploads/internal/lsifstore/lsifstore_delete.go:DeleteLsifDataByUploadIds
DELETE FROM %s WHERE dump_id IN (%s)
`

func intsToString(vs []int) string {
	strs := make([]string, 0, len(vs))
	for _, v := range vs {
		strs = append(strs, strconv.Itoa(v))
	}

	return strings.Join(strs, ", ")
}
