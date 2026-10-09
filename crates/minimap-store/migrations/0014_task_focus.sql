-- Focus on tasks (spec 37): a JSON object, `{}` = in focus until taken out, `{"until":"YYYY-MM-DD"}` =
-- in focus through that day. NULL = not in focus.
ALTER TABLE tasks ADD COLUMN focus TEXT;
