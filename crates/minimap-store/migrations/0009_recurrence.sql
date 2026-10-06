-- Recurring items (spec 27): a repeat rule on tasks and notes, as JSON
-- ({"cadence":{"kind":"weekly","every":1,"weekday":0},"template":null}). NULL = does not repeat.
ALTER TABLE tasks ADD COLUMN recurrence TEXT;
ALTER TABLE notes ADD COLUMN recurrence TEXT;
