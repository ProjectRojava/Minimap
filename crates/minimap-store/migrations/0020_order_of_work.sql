-- Order of work (spec 60, ADR-0024): the task's step in its assignee's order (1 = first).
-- NULL = not in an order.
ALTER TABLE tasks ADD COLUMN order_step INTEGER;
