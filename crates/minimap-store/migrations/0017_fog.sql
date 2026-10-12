-- Fog of war (spec 43): the day someone said "still accurate" without changing anything.
-- ISO date, NULL = never confirmed.
ALTER TABLE tasks ADD COLUMN confirmed_on TEXT;
ALTER TABLE projects ADD COLUMN confirmed_on TEXT;
ALTER TABLE objectives ADD COLUMN confirmed_on TEXT;
