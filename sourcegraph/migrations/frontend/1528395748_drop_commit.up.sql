BEGIN;

ALTER TABLE lsif_nearest_uploads DROP COLUMN "commit";

COMMIT;
