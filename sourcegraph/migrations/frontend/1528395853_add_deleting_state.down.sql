BEGIN;

DROP VIEW lsif_dumps_with_repository_name;
DROP VIEW lsif_dumps;

CREATE VIEW lsif_dumps AS SELECT u.id,
    u.commit,
    u.root,
    u.uploaded_at,
    u.state,
    u.failure_message,
    u.started_at,
    u.finished_at,
    u.repository_id,
    u.indexer,
    u.num_parts,
    u.uploaded_parts,
    u.process_after,
    u.num_resets,
    u.upload_size,
    u.num_failures,
    u.associated_index_id,
    u.finished_at AS processed_at
FROM lsif_uploads u WHERE u.state = 'completed'::text;

CREATE VIEW lsif_dumps_with_repository_name AS SELECT u.id,
    u.commit,
    u.root,
    u.uploaded_at,
    u.state,
    u.failure_message,
    u.started_at,
    u.finished_at,
    u.repository_id,
    u.indexer,
    u.num_parts,
    u.uploaded_parts,
    u.process_after,
    u.num_resets,
    u.upload_size,
    u.num_failures,
    u.associated_index_id,
    u.processed_at,
    r.name AS repository_name
FROM lsif_dumps u JOIN repo r ON r.id = u.repository_id WHERE r.deleted_at IS NULL;

COMMIT;
