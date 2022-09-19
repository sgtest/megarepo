-- We update the `diff_stat_added` and `diff_stat_deleted` to reflect the way git calculate diffs.
-- When calculating diffs, we only care about the added & deleted lines
do $$
begin
    /* if column `diff_stat_changed` exists on `changesets` table */
    IF EXISTS(
        SELECT 1
            FROM information_schema.columns
        WHERE table_schema = 'public'
            AND table_name = 'changesets'
            AND column_name = 'diff_stat_changed'
    ) THEN
        /* update the `diff_stat_added` and `diff_stat_deleted` */
        UPDATE changesets
        SET
            diff_stat_added = diff_stat_added + diff_stat_changed,
            diff_stat_deleted = diff_stat_deleted + diff_stat_changed
        WHERE
            diff_stat_changed != 0;
    END IF;
end$$;

ALTER TABLE IF EXISTS changesets DROP COLUMN IF EXISTS diff_stat_changed;
