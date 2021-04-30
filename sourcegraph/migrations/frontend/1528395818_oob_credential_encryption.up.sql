BEGIN;

INSERT INTO out_of_band_migrations (id, team, component, description, introduced, non_destructive)
VALUES (
    9,                                          -- This must be consistent across all Sourcegraph instances
    'batch-changes',                            -- Team owning migration
    'frontend-db.user-credentials',             -- Component being migrated
    'Encrypt batch changes user credentials',   -- Description
    '3.28.0',                                   -- The next minor release
    false                                       -- Can be read with previous version without down migration
)
ON CONFLICT DO NOTHING;

ALTER TABLE
    user_credentials
ADD COLUMN IF NOT EXISTS
    credential_enc BYTEA NULL,
ADD COLUMN IF NOT EXISTS
    ssh_migration_applied BOOLEAN NOT NULL DEFAULT FALSE,
ALTER COLUMN
    credential DROP NOT NULL,
DROP CONSTRAINT IF EXISTS
    user_credentials_there_can_be_only_one,
ADD CONSTRAINT
    user_credentials_there_can_be_only_one
    CHECK
    (num_nonnulls(credential, credential_enc) = 1);

-- Calculate the ssh_migration_applied field using the same algorithm as the
-- previous version of the SSH migrator.
UPDATE
    user_credentials
SET
    ssh_migration_applied = TRUE
WHERE
    credential IS NOT NULL
    AND domain = 'batches'
    AND (credential::json->'Type')::text NOT IN (
        'BasicAuth',
        'OAuthBearerToken'
    );

-- Create an index on credential_enc, since we want to quickly check its null
-- state when calculating the progress of the OOB migration. Note that we can't
-- apply an index to the actual field because it may be (and in many cases
-- probably is) beyond the limit for a B-tree index.
CREATE INDEX IF NOT EXISTS
    user_credentials_credential_enc_idx
ON
    user_credentials ((credential_enc IS NULL));

COMMIT;
