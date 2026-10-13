-- Pings (docs/product-design/05-pings.md, ADR-0020): a marker on any item with a day and an
-- optional line of text. A small table that points at its target (no foreign key: targets are
-- polymorphic, like `edges` and `attachments`); it is not a node type.
CREATE TABLE pings (
  id          TEXT PRIMARY KEY,           -- uuid v7
  target_type TEXT NOT NULL,              -- node type name, as in edges.from_type
  target_id   TEXT NOT NULL,
  note        TEXT NOT NULL DEFAULT '',
  on_date     TEXT NOT NULL,              -- YYYY-MM-DD
  cleared_on  TEXT NULL,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  archived_at TEXT NULL
);
CREATE INDEX idx_pings_target ON pings(target_id) WHERE archived_at IS NULL AND cleared_on IS NULL;
CREATE INDEX idx_pings_on ON pings(on_date) WHERE archived_at IS NULL AND cleared_on IS NULL;
