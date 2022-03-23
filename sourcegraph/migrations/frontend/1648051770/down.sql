-- Undo the changes made in the up migration

ALTER TABLE feature_flag_overrides
    DROP CONSTRAINT feature_flag_overrides_flag_name_fkey;

ALTER TABLE feature_flag_overrides
    ADD CONSTRAINT feature_flag_overrides_flag_name_fkey
    FOREIGN KEY (flag_name)
    REFERENCES feature_flags(flag_name)
    ON DELETE CASCADE;

