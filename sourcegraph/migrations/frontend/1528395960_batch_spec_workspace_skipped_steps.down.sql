BEGIN;

ALTER TABLE batch_spec_workspaces DROP COLUMN IF EXISTS skipped_steps;

COMMIT;
