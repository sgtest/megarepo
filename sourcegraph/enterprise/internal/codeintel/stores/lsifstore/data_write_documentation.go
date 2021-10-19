package lsifstore

import (
	"context"
	"database/sql"
	"fmt"
	stdlog "log"
	"runtime/debug"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"unicode"
	"unicode/utf8"

	"github.com/cockroachdb/errors"
	"github.com/inconshreveable/log15"
	"github.com/keegancsmith/sqlf"
	"github.com/opentracing/opentracing-go/log"

	"github.com/sourcegraph/sourcegraph/enterprise/internal/codeintel/stores/dbstore"
	"github.com/sourcegraph/sourcegraph/internal/conf"
	"github.com/sourcegraph/sourcegraph/internal/database/basestore"
	"github.com/sourcegraph/sourcegraph/internal/database/batch"
	"github.com/sourcegraph/sourcegraph/internal/observation"
	"github.com/sourcegraph/sourcegraph/internal/types"
	"github.com/sourcegraph/sourcegraph/lib/codeintel/lsif/protocol"
	"github.com/sourcegraph/sourcegraph/lib/codeintel/precise"
)

// WriteDocumentationPages is called (transactionally) from the precise-code-intel-worker.
func (s *Store) WriteDocumentationPages(ctx context.Context, upload dbstore.Upload, repo *types.Repo, isDefaultBranch bool, documentationPages chan *precise.DocumentationPageData) (err error) {
	ctx, traceLog, endObservation := s.operations.writeDocumentationPages.WithAndLogger(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("bundleID", upload.ID),
		log.String("repo", upload.RepositoryName),
		log.String("commit", upload.Commit),
		log.String("root", upload.Root),
	}})
	defer endObservation(1, observation.Args{})

	defer func() {
		if err := recover(); err != nil {
			stack := debug.Stack()
			stdlog.Printf("API docs panic: %v\n%s", err, stack)
			traceLog(log.String("API docs panic error", fmt.Sprint(err)))
			traceLog(log.String("API docs panic stack", string(stack)))
		}
	}()

	return s.doWriteDocumentationPages(ctx, upload, repo, isDefaultBranch, documentationPages, traceLog)
}

func (s *Store) doWriteDocumentationPages(ctx context.Context, upload dbstore.Upload, repo *types.Repo, isDefaultBranch bool, documentationPages chan *precise.DocumentationPageData, traceLog observation.TraceLogger) (err error) {
	tx, err := s.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Done(err) }()

	// Create temporary table symmetric to lsif_data_documentation_pages without the dump id
	if err := tx.Exec(ctx, sqlf.Sprintf(writeDocumentationPagesTemporaryTableQuery)); err != nil {
		return err
	}

	var count uint32
	var pages []*precise.DocumentationPageData
	inserter := func(inserter *batch.Inserter) error {
		for page := range documentationPages {
			pages = append(pages, page)
			data, err := s.serializer.MarshalDocumentationPageData(page)
			if err != nil {
				return err
			}

			if err := inserter.Insert(ctx, page.Tree.PathID, data); err != nil {
				return err
			}

			atomic.AddUint32(&count, 1)
		}
		return nil
	}

	// Bulk insert all the unique column values into the temporary table
	if err := withBatchInserter(
		ctx,
		tx.Handle().DB(),
		"t_lsif_data_documentation_pages",
		[]string{"path_id", "data"},
		inserter,
	); err != nil {
		return err
	}
	traceLog(log.Int("numResultChunkRecords", int(count)))

	// Note: If someone disables API docs search indexing, uploads during that time will not be
	// indexed even if it is turned back on. Only future uploads would be.
	if conf.APIDocsSearchIndexingEnabled() {
		// Perform search indexing for API docs pages.
		if err := tx.WriteDocumentationSearch(ctx, upload, repo, isDefaultBranch, pages); err != nil {
			return errors.Wrap(err, "WriteDocumentationSearch")
		}
	}

	// Insert the values from the temporary table into the target table. We select a
	// parameterized dump id here since it is the same for all rows in this operation.
	return tx.Exec(ctx, sqlf.Sprintf(writeDocumentationPagesInsertQuery, upload.ID))
}

const writeDocumentationPagesTemporaryTableQuery = `
-- source: enterprise/internal/codeintel/stores/lsifstore/data_write_documentation.go:WriteDocumentationPages
CREATE TEMPORARY TABLE t_lsif_data_documentation_pages (
	path_id TEXT NOT NULL,
	data bytea NOT NULL
) ON COMMIT DROP
`

const writeDocumentationPagesInsertQuery = `
-- source: enterprise/internal/codeintel/stores/lsifstore/data_write_documentation.go:WriteDocumentationPages
INSERT INTO lsif_data_documentation_pages (dump_id, path_id, data, search_indexed)
SELECT %s, source.path_id, source.data, 'true'
FROM t_lsif_data_documentation_pages source
`

// WriteDocumentationPathInfo is called (transactionally) from the precise-code-intel-worker.
func (s *Store) WriteDocumentationPathInfo(ctx context.Context, bundleID int, documentationPathInfo chan *precise.DocumentationPathInfoData) (err error) {
	ctx, traceLog, endObservation := s.operations.writeDocumentationPathInfo.WithAndLogger(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("bundleID", bundleID),
	}})
	defer endObservation(1, observation.Args{})

	tx, err := s.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Done(err) }()

	// Create temporary table symmetric to lsif_data_documentation_path_info without the dump id
	if err := tx.Exec(ctx, sqlf.Sprintf(writeDocumentationPathInfoTemporaryTableQuery)); err != nil {
		return err
	}

	var count uint32
	inserter := func(inserter *batch.Inserter) error {
		for v := range documentationPathInfo {
			data, err := s.serializer.MarshalDocumentationPathInfoData(v)
			if err != nil {
				return err
			}

			if err := inserter.Insert(ctx, v.PathID, data); err != nil {
				return err
			}

			atomic.AddUint32(&count, 1)
		}
		return nil
	}

	// Bulk insert all the unique column values into the temporary table
	if err := withBatchInserter(
		ctx,
		tx.Handle().DB(),
		"t_lsif_data_documentation_path_info",
		[]string{"path_id", "data"},
		inserter,
	); err != nil {
		return err
	}
	traceLog(log.Int("numResultChunkRecords", int(count)))

	// Insert the values from the temporary table into the target table. We select a
	// parameterized dump id here since it is the same for all rows in this operation.
	return tx.Exec(ctx, sqlf.Sprintf(writeDocumentationPathInfoInsertQuery, bundleID))
}

const writeDocumentationPathInfoTemporaryTableQuery = `
-- source: enterprise/internal/codeintel/stores/lsifstore/data_write_documentation.go:WriteDocumentationPathInfo
CREATE TEMPORARY TABLE t_lsif_data_documentation_path_info (
	path_id TEXT NOT NULL,
	data bytea NOT NULL
) ON COMMIT DROP
`

const writeDocumentationPathInfoInsertQuery = `
-- source: enterprise/internal/codeintel/stores/lsifstore/data_write_documentation.go:WriteDocumentationPathInfo
INSERT INTO lsif_data_documentation_path_info (dump_id, path_id, data)
SELECT %s, source.path_id, source.data
FROM t_lsif_data_documentation_path_info source
`

// WriteDocumentationMappings is called (transactionally) from the precise-code-intel-worker.
func (s *Store) WriteDocumentationMappings(ctx context.Context, bundleID int, mappings chan precise.DocumentationMapping) (err error) {
	ctx, traceLog, endObservation := s.operations.writeDocumentationMappings.WithAndLogger(ctx, &err, observation.Args{LogFields: []log.Field{
		log.Int("bundleID", bundleID),
	}})
	defer endObservation(1, observation.Args{})

	tx, err := s.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Done(err) }()

	// Create temporary table symmetric to lsif_data_documentation_mappings without the dump id
	if err := tx.Exec(ctx, sqlf.Sprintf(writeDocumentationMappingsTemporaryTableQuery)); err != nil {
		return err
	}

	var count uint32
	inserter := func(inserter *batch.Inserter) error {
		for mapping := range mappings {
			if err := inserter.Insert(ctx, mapping.PathID, mapping.ResultID, mapping.FilePath); err != nil {
				return err
			}
			atomic.AddUint32(&count, 1)
		}
		return nil
	}

	// Bulk insert all the unique column values into the temporary table
	if err := withBatchInserter(
		ctx,
		tx.Handle().DB(),
		"t_lsif_data_documentation_mappings",
		[]string{"path_id", "result_id", "file_path"},
		inserter,
	); err != nil {
		return err
	}
	traceLog(log.Int("numRecords", int(count)))

	// Insert the values from the temporary table into the target table. We select a
	// parameterized dump id here since it is the same for all rows in this operation.
	return tx.Exec(ctx, sqlf.Sprintf(writeDocumentationMappingsInsertQuery, bundleID))
}

const writeDocumentationMappingsTemporaryTableQuery = `
-- source: enterprise/internal/codeintel/stores/lsifstore/data_write_documentation.go:WriteDocumentationMappings
CREATE TEMPORARY TABLE t_lsif_data_documentation_mappings (
	path_id TEXT NOT NULL,
	result_id integer NOT NULL,
	file_path text
) ON COMMIT DROP
`

const writeDocumentationMappingsInsertQuery = `
-- source: enterprise/internal/codeintel/stores/lsifstore/data_write_documentation.go:WriteDocumentationMappings
INSERT INTO lsif_data_documentation_mappings (dump_id, path_id, result_id, file_path)
SELECT %s, source.path_id, source.result_id, source.file_path
FROM t_lsif_data_documentation_mappings source
`

// WriteDocumentationSearch is called (within a transaction) to write the search index for a given documentation page.
func (s *Store) WriteDocumentationSearch(ctx context.Context, upload dbstore.Upload, repo *types.Repo, isDefaultBranch bool, pages []*precise.DocumentationPageData) (err error) {
	if !isDefaultBranch {
		// We do not index non-default branches for API docs search.
		return nil
	}

	ctx, endObservation := s.operations.writeDocumentationSearch.With(ctx, &err, observation.Args{LogFields: []log.Field{
		log.String("repo", upload.RepositoryName),
		log.Int("bundleID", upload.ID),
		log.Int("pages", len(pages)),
	}})
	defer endObservation(1, observation.Args{})

	tableSuffix := "public"
	if repo.Private {
		tableSuffix = "private"
	}

	// This will not always produce a proper language name, e.g. if an indexer is not named after
	// the language or is not in "lsif-$LANGUAGE" format. That's OK: in that case, the "language"
	// is the indexer name which is likely good enough since we use fuzzy search / partial text matching
	// over it.
	languageName := strings.ToLower(strings.TrimPrefix(upload.Indexer, "lsif-"))

	tx, err := s.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Done(err) }()

	repositoryNameID, err := tx.upsertRepositoryName(ctx, upload.RepositoryName, tableSuffix)
	if err != nil {
		return err
	}

	languageNameID, err := tx.upsertLanguageName(ctx, languageName, tableSuffix)
	if err != nil {
		return err
	}

	tagIDs, err := tx.upsertTags(ctx, gatherTags(pages), tableSuffix)
	if err != nil {
		return err
	}

	if err := tx.replaceSearchRecords(ctx, upload, repositoryNameID, languageNameID, pages, tagIDs, tableSuffix); err != nil {
		return err
	}

	// Truncate the search index size if it exceeds our configured limit now.
	for _, suffix := range []string{"public", "private"} {
		if err := tx.truncateDocumentationSearchIndexSize(ctx, suffix); err != nil {
			return errors.Wrap(err, "truncating documentation search index size")
		}
	}

	return nil
}

func (s *Store) upsertRepositoryName(ctx context.Context, name, tableSuffix string) (int, error) {
	id, _, err := basestore.ScanFirstInt(s.Query(ctx, sqlf.Sprintf(
		strings.ReplaceAll(upsertRepositoryNameQuery, "$SUFFIX", tableSuffix),
		name,
		textSearchVector(name),
		textSearchVector(reverse(name)),
	)))

	return id, err
}

const upsertRepositoryNameQuery = `
-- source: enterprise/internal/codeintel/stores/lsifstore/data_write_documentation.go:upsertRepositoryName
INSERT INTO lsif_data_docs_search_repo_names_$SUFFIX (repo_name, tsv, reverse_tsv)
VALUES (%s, %s, %s)
-- Make no-op DO UPDATE to force RETURNING to fire on unchanged rows
ON CONFLICT (repo_name) DO UPDATE SET repo_name = EXCLUDED.repo_name
RETURNING id
`

func (s *Store) upsertLanguageName(ctx context.Context, name, tableSuffix string) (int, error) {
	id, _, err := basestore.ScanFirstInt(s.Query(ctx, sqlf.Sprintf(
		strings.ReplaceAll(upsertLanguageNameQuery, "$SUFFIX", tableSuffix),
		name,
		textSearchVector(name),
	)))

	return id, err
}

const upsertLanguageNameQuery = `
-- source: enterprise/internal/codeintel/stores/lsifstore/data_write_documentation.go:upsertLanguageName
INSERT INTO lsif_data_docs_search_lang_names_$SUFFIX (lang_name, tsv)
VALUES (%s, %s)
-- Make no-op DO UPDATE to force RETURNING to fire on unchanged rows
ON CONFLICT (lang_name) DO UPDATE SET lang_name = EXClUDED.lang_name
RETURNING id
`

func (s *Store) upsertTags(ctx context.Context, tagMap map[string]struct{}, tableSuffix string) (map[string]int, error) {
	inserter := func(inserter *batch.Inserter) error {
		for tags := range tagMap {
			if err := inserter.Insert(ctx, tags, textSearchVector(tags)); err != nil {
				return err
			}
		}

		return nil
	}

	tagIDs := make(map[string]int, len(tagMap))
	returningScanner := func(rows *sql.Rows) error {
		var tagID int
		var tags string
		if err := rows.Scan(&tagID, &tags); err != nil {
			return err
		}

		// Correlate id and tags of each upserted row
		tagIDs[tags] = tagID
		return nil
	}

	if err := batch.WithInserterWithReturn(
		ctx,
		s.Handle().DB(),
		"lsif_data_docs_search_tags_"+tableSuffix,
		[]string{"tags", "tsv"},
		// Make no-op DO UPDATE to force RETURNING to fire on unchanged rows
		"ON CONFLICT (tags) DO UPDATE SET tags = EXCLUDED.tags",
		// Return id and tags on each row so we can correlate ids, either
		// existing or new, for each row we sent to be inserted.
		[]string{"id", "tags"},
		returningScanner,
		inserter,
	); err != nil {
		return nil, err
	}

	return tagIDs, nil
}

func (s *Store) replaceSearchRecords(
	ctx context.Context,
	upload dbstore.Upload,
	repositoryNameID int,
	languageNameID int,
	pages []*precise.DocumentationPageData,
	tagIDs map[string]int,
	tableSuffix string,
) error {
	tx, err := s.Transact(ctx)
	if err != nil {
		return err
	}
	defer func() { err = tx.Done(err) }()

	// Create temporary table symmetric to lsif_data_docs_search_$SUFFIX without the fields that would have
	// the same value for the same upload. We'll insert these shared values all at once to save on query
	// bandwidth.
	if err := tx.Exec(ctx, sqlf.Sprintf(strings.ReplaceAll(insertSearchRecordsTemporaryTableQuery, "$SUFFIX", tableSuffix))); err != nil {
		return err
	}

	inserter := func(inserter *batch.Inserter) error {
		handler := func(node *precise.DocumentationNode) error {
			if node.Documentation.SearchKey == "" {
				return nil
			}

			detail := truncate(node.Detail.String(), 5*1024) // 5 KiB - just for sanity
			label := truncate(node.Label.String(), 256)      // 256 bytes, enough for ~100 characters in all languages
			tagsID := tagIDs[normalizeTags(node.Documentation.Tags)]

			if err := inserter.Insert(
				ctx,
				node.PathID, // path_id
				detail,      // detail
				tagsID,      // tags_id

				node.Documentation.SearchKey,                            // search_key
				textSearchVector(node.Documentation.SearchKey),          // search_key_tsv
				textSearchVector(reverse(node.Documentation.SearchKey)), // search_key_reverse_tsv

				label,                            // label
				textSearchVector(label),          // label_tsv
				textSearchVector(reverse(label)), // label_reverse_tsv
			); err != nil {
				return err
			}

			return nil
		}

		for _, page := range pages {
			if err := walkDocumentationNode(page.Tree, handler); err != nil {
				return err
			}
		}

		return nil
	}

	// Bulk insert all the unique column values into the temporary table
	if err := batch.WithInserter(
		ctx,
		tx.Handle().DB(),
		"t_lsif_data_docs_search_"+tableSuffix,
		[]string{
			"path_id",
			"detail",
			"tags_id",
			"search_key",
			"search_key_tsv",
			"search_key_reverse_tsv",
			"label",
			"label_tsv",
			"label_reverse_tsv",
		},
		inserter,
	); err != nil {
		return err
	}

	// Insert the values from the temporary table into the target table. Here we insert
	// the value that are the same for every row instead of sending them on each of the
	// batched insert calls.
	if err := tx.Exec(ctx, sqlf.Sprintf(
		strings.ReplaceAll(insertSearchRecordsInsertQuery, "$SUFFIX", tableSuffix),
		// For insertion
		upload.RepositoryID, // repo_id
		upload.ID,           // dump_id
		upload.Root,         // dump_root
		repositoryNameID,    // repo_name_id
		languageNameID,      // lang_name_id
		// For deletion
		upload.RepositoryID, // repo_id
		upload.Root,         // dump_root
		languageNameID,      // lang_name_id
		upload.ID,           // dump_id
	)); err != nil {
		return err
	}

	return nil
}

const insertSearchRecordsTemporaryTableQuery = `
-- source: enterprise/internal/codeintel/stores/lsifstore/data_write_documentation.go:insertSearchRecords
CREATE TEMPORARY TABLE t_lsif_data_docs_search_$SUFFIX (
	path_id TEXT NOT NULL,
	detail TEXT NOT NULL,
	tags_id INTEGER NOT NULL,
	search_key TEXT NOT NULL,
	search_key_tsv TSVECTOR NOT NULL,
	search_key_reverse_tsv TSVECTOR NOT NULL,
	label TEXT NOT NULL,
	label_tsv TSVECTOR NOT NULL,
	label_reverse_tsv TSVECTOR NOT NULL
) ON COMMIT DROP
`

const insertSearchRecordsInsertQuery = `
-- source: enterprise/internal/codeintel/stores/lsifstore/data_write_documentation.go:insertSearchRecords
WITH
ins AS (
	INSERT INTO lsif_data_docs_search_$SUFFIX (
		repo_id,
		dump_id,
		dump_root,
		repo_name_id,
		lang_name_id,
		path_id,
		detail,
		tags_id,
		search_key,
		search_key_tsv,
		search_key_reverse_tsv,
		label,
		label_tsv,
		label_reverse_tsv
	)
	SELECT
		%s, -- repo_id
		%s, -- dump_id
		%s, -- dump_root
		%s, -- repo_name_id
		%s, -- lang_name_id
		source.path_id,
		source.detail,
		source.tags_id,
		source.search_key,
		source.search_key_tsv,
		source.search_key_reverse_tsv,
		source.label,
		source.label_tsv,
		source.label_reverse_tsv
	FROM t_lsif_data_docs_search_$SUFFIX source
	RETURNING 1
),
del AS (
	DELETE FROM lsif_data_docs_search_$SUFFIX
	WHERE
		repo_id = %s AND
		dump_root = %s AND
		lang_name_id = %s AND
		dump_id != %s
	RETURNING 1
)
SELECT
	(SELECT COUNT(*) FROM ins) AS num_ins,
	(SELECT COUNT(*) FROM del) AS num_del
`

func walkDocumentationNode(node *precise.DocumentationNode, f func(node *precise.DocumentationNode) error) error {
	if err := f(node); err != nil {
		return err
	}

	for _, child := range node.Children {
		if child.Node != nil {
			if err := walkDocumentationNode(child.Node, f); err != nil {
				return err
			}
		}
	}

	return nil
}

func gatherTags(pages []*precise.DocumentationPageData) map[string]struct{} {
	tagMap := map[string]struct{}{}
	for _, page := range pages {
		_ = walkDocumentationNode(page.Tree, func(node *precise.DocumentationNode) error {
			if node.Documentation.SearchKey != "" {
				tagMap[normalizeTags(node.Documentation.Tags)] = struct{}{}
			}

			return nil
		})
	}

	return tagMap
}

func normalizeTags(tags []protocol.Tag) string {
	s := make([]string, 0, len(tags))
	for _, tag := range tags {
		s = append(s, string(tag))
	}

	return strings.Join(s, " ")
}

var (
	lastTruncationWarningMu   sync.Mutex
	lastTruncationWarningTime time.Time
)

// truncateDocumentationSearchIndexSize is called (within a transaction) to truncate the
// documentation search index size according to the site config apidocs.search-index-limit-factor.
func (s *Store) truncateDocumentationSearchIndexSize(ctx context.Context, tableSuffix string) error {
	totalRows, exists, err := basestore.ScanFirstInt64(s.Query(ctx, sqlf.Sprintf(
		strings.ReplaceAll(countDocumentationSearchRowsQuery, "$SUFFIX", tableSuffix),
	)))
	if !exists {
		return fmt.Errorf("failed to count table size")
	}
	if err != nil {
		return errors.Wrap(err, "counting table size")
	}

	searchIndexLimitFactor := conf.Get().ApidocsSearchIndexSizeLimitFactor
	if searchIndexLimitFactor <= 0 {
		searchIndexLimitFactor = 1.0
	}
	searchIndexRowsLimit := int64(searchIndexLimitFactor * 250_000_000)
	rowsToDelete := totalRows - searchIndexRowsLimit
	if rowsToDelete <= 0 {
		return nil
	}

	lastTruncationWarningMu.Lock()
	if lastTruncationWarningTime.IsZero() || time.Since(lastTruncationWarningTime) > 5*time.Minute {
		lastTruncationWarningTime = time.Now()
		log15.Warn(
			"API docs search index size exceeded configured limit, truncating index",
			"apidocs.search-index-limit-factor", searchIndexLimitFactor,
			"rows_limit", searchIndexRowsLimit,
			"total_rows", totalRows,
			"deleting", rowsToDelete,
		)
	}
	lastTruncationWarningMu.Unlock()

	// Delete the first (oldest) N rows
	if err := s.Exec(ctx, sqlf.Sprintf(
		strings.ReplaceAll(truncateDocumentationSearchRowsQuery, "$SUFFIX", tableSuffix),
		rowsToDelete,
	)); err != nil {
		return errors.Wrap(err, "truncating search index rows")
	}
	return nil
}

const countDocumentationSearchRowsQuery = `
-- source: enterprise/internal/codeintel/stores/lsifstore/data_write_documentation.go:truncateDocumentationSearchIndexSize
SELECT count::bigint FROM lsif_data_apidocs_num_search_results_$SUFFIX
`

const truncateDocumentationSearchRowsQuery = `
-- source: enterprise/internal/codeintel/stores/lsifstore/data_write_documentation.go:truncateDocumentationSearchIndexSize
WITH candidates AS (
	SELECT id FROM lsif_data_docs_search_$SUFFIX

	-- Lock these rows in a deterministic order so that we don't deadlock with other processes
	-- updating the lsif_data_docs_search_* tables.
	ORDER BY id FOR UPDATE
	LIMIT %s
)
DELETE FROM lsif_data_docs_search_$SUFFIX
WHERE id IN (SELECT id FROM candidates)
RETURNING id
`

// truncate truncates a string to limitBytes, taking into account multi-byte runes. If the string
// is truncated, an ellipsis "…" is added to the end.
func truncate(s string, limitBytes int) string {
	runes := []rune(s)
	bytes := 0
	for i, r := range runes {
		if bytes+len(string(r)) >= limitBytes {
			return string(runes[:i-1]) + "…"
		}
		bytes += len(string(r))
	}
	return s
}

// textSearchVector constructs an ordered tsvector from the given string.
//
// Postgres' built in to_tsvector configurations (`simple`, `english`, etc.) work well for human
// language search but for multiple reasons produces tsvectors that are not appropriate for our
// use case of code search.
//
// By default, tsvectors perform word deduplication and normalization of words (Rat -> rat for
// example.) They also get sorted alphabetically:
//
// ```
// SELECT 'a fat cat sat on a mat and ate a fat rat'::tsvector;
//                       tsvector
// ----------------------------------------------------
//  'a' 'and' 'ate' 'cat' 'fat' 'mat' 'on' 'rat' 'sat'
// ```
//
// In the context of general document search, this doesn't matter. But in our context of API docs
// search, the order in which words (in the general computing sense) appear matters. For example,
// when searching `mux.router` it's important we match (package mux, symbol router) and not
// (package router, symbol mux).
//
// Another critical reason to_tsvector's configurations are not suitable for codes search is that
// they explicitly drop most punctuation (excluding periods) and don't split words between periods:
//
// ```
// select to_tsvector('foo::bar mux.Router const Foo* = Bar<T<X>>');
//                  to_tsvector
// ----------------------------------------------
//  'bar':2,6 'const':4 'foo':1,5 'mux.router':3
// ```
//
// Luckily, Postgres allows one to construct tsvectors manually by providing a sorted list of lexemes
// with optional integer _positions_ and weights, or:
//
// ```
// SELECT $$'foo':1 '::':2 'bar':3 'mux':4 'Router':5 'const':6 'Foo':7 '*':8 '=':9 'Bar':10 '<':11 'T':12 '<':13 'X':14 '>':15 '>':16$$::tsvector;
//                                                       tsvector
// --------------------------------------------------------------------------------------------------------------------
//  '*':8 '::':2 '<':11,13 '=':9 '>':15,16 'Bar':10 'Foo':7 'Router':5 'T':12 'X':14 'bar':3 'const':6 'foo':1 'mux':4
// ```
//
// Ordered, case-sensitive, punctuation-inclusive tsquery matches against the tsvector are then possible.
//
// For example, a query for `bufio.` would then match a tsvector ("bufio", ".", "Reader", ".", "writeBuf"):
//
// ```
// SELECT $$'bufio':1 '.':2 'Reader':3 '.':4 'writeBuf':5$$::tsvector @@ tsquery_phrase($$'bufio'$$::tsquery, $$'.'$$::tsquery) AS matches;
//  matches
// ---------
//  t
// ```
//
func textSearchVector(s string) string {
	// We need to emit a string in the Postgres tsvector format, roughly:
	//
	//     lexeme1:1 lexeme2:2 lexeme3:3
	//
	lexemes := lexemes(s)
	pairs := make([]string, 0, len(lexemes))
	for i, lexeme := range lexemes {
		pairs = append(pairs, fmt.Sprintf("%s:%v", lexeme, i+1))
	}
	return strings.Join(pairs, " ")
}

// lexemes splits the string into lexemes, each will be any contiguous section of Unicode digits,
// numbers, and letters. All other unicode runes, such as punctuation, are considered their own
// individual lexemes - and spaces are removed and considered boundaries.
func lexemes(s string) []string {
	var (
		lexemes       []string
		currentLexeme []rune
	)
	for _, r := range s {
		if unicode.IsDigit(r) || unicode.IsNumber(r) || unicode.IsLetter(r) {
			currentLexeme = append(currentLexeme, r)
			continue
		}
		if len(currentLexeme) > 0 {
			lexemes = append(lexemes, string(currentLexeme))
			currentLexeme = currentLexeme[:0]
		}
		if !unicode.IsSpace(r) {
			lexemes = append(lexemes, string(r))
		}
	}
	if len(currentLexeme) > 0 {
		lexemes = append(lexemes, string(currentLexeme))
	}
	return lexemes
}

// reverses a UTF-8 string accounting for Unicode and combining characters. This is not a part of
// the Go standard library or any of the golang.org/x packages. Note that reversing a slice of
// runes is not enough (would not handle combining characters.)
//
// See http://rosettacode.org/wiki/Reverse_a_string#Go
func reverse(s string) string {
	if s == "" {
		return ""
	}
	p := []rune(s)
	r := make([]rune, len(p))
	start := len(r)
	for i := 0; i < len(p); {
		// quietly skip invalid UTF-8
		if p[i] == utf8.RuneError {
			i++
			continue
		}
		j := i + 1
		for j < len(p) && (unicode.Is(unicode.Mn, p[j]) ||
			unicode.Is(unicode.Me, p[j]) || unicode.Is(unicode.Mc, p[j])) {
			j++
		}
		for k := j - 1; k >= i; k-- {
			start--
			r[start] = p[k]
		}
		i = j
	}
	return (string(r[start:]))
}
