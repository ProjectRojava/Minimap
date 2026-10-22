-- Backburner (spec 59): the day a task was put on the backburner, ISO date. NULL = not on it.
ALTER TABLE tasks ADD COLUMN backburner_since TEXT;
