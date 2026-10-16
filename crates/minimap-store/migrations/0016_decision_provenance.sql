-- Decisions, spec 42: what Minimap filled in by itself, as against what the user set. The status
-- is set automatically when the task of type Decision is finished; the decided-on date is
-- stamped automatically when a decision is decided without one, or by finishing the task.
-- Setting either by hand clears its flag.
ALTER TABLE decisions ADD COLUMN auto_status INTEGER NOT NULL DEFAULT 0;
ALTER TABLE decisions ADD COLUMN auto_decided_on INTEGER NOT NULL DEFAULT 0;
