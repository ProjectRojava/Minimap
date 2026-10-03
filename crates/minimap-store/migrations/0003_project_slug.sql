-- Short handle for quick-add (#api-launch). Existing rows are backfilled by the
-- migration hook (see lib.rs), then the unique index is added in 0004.
ALTER TABLE projects ADD COLUMN slug TEXT NOT NULL DEFAULT '';
