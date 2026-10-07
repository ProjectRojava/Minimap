-- Ongoing objectives (spec 30): an objective with no end (keep the systems healthy), judged by
-- its work and a review rhythm instead of a target date.
ALTER TABLE objectives ADD COLUMN ongoing INTEGER NOT NULL DEFAULT 0;
ALTER TABLE objectives ADD COLUMN review_every_days INTEGER;
ALTER TABLE objectives ADD COLUMN last_reviewed_on TEXT;
