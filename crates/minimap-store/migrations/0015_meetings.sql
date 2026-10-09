-- Meetings (spec 38): the start of a meeting as a minute of the day on the user's clock (0-1439),
-- and its length in minutes. NULL on tasks that are not meetings (the length NULL on a meeting
-- means the default hour). The day is the task's `due_date`.
ALTER TABLE tasks ADD COLUMN start_minute INTEGER;
ALTER TABLE tasks ADD COLUMN length_minutes INTEGER;
