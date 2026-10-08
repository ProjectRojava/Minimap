-- Task types (spec 32): the id of an entry in the task-type list kept in Settings. NULL = no type.
ALTER TABLE tasks ADD COLUMN task_type TEXT;
