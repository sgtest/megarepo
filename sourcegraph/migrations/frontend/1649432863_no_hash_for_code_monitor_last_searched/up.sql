-- Perform migration here.
--
-- See /migrations/README.md. Highlights:
--  * Make migrations idempotent (use IF EXISTS)
--  * Make migrations backwards-compatible (old readers/writers must continue to work)
--  * If you are using CREATE INDEX CONCURRENTLY, then make sure that only one statement
--    is defined per file, and that each such statement is NOT wrapped in a transaction.
--    Each such migration must also declare "createIndexConcurrently: true" in their
--    associated metadata.yaml file.
--  * If you are modifying Postgres extensions, you must also declare "privileged: true"
--    in the associated metadata.yaml file.

DELETE FROM cm_last_searched;
ALTER TABLE cm_last_searched
    DROP CONSTRAINT IF EXISTS cm_last_searched_pkey,
    DROP COLUMN IF EXISTS args_hash,
    ADD COLUMN IF NOT EXISTS repo_id INTEGER NOT NULL REFERENCES repo(id) ON DELETE CASCADE,
    ADD PRIMARY KEY (monitor_id, repo_id);
