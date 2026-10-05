-- Multi-device sync and attachments (spec 22, ADR-0011).
--
-- Merging two devices' copies of the data needs to know which version of a row is newer, and
-- which rows were deleted for good. Edges and settings had no `updated_at`; triggers keep it
-- current so no write path can forget it. While a merge is applying another device's rows it
-- sets `app_meta.local.merging`, which stops the triggers re-stamping what the merge writes.

ALTER TABLE edges ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';
UPDATE edges SET updated_at =
  CASE WHEN archived_at IS NOT NULL AND archived_at > created_at THEN archived_at ELSE created_at END;

CREATE TRIGGER edges_stamp_insert AFTER INSERT ON edges
WHEN new.updated_at = ''
BEGIN
  UPDATE edges SET updated_at = new.created_at WHERE id = new.id;
END;

CREATE TRIGGER edges_stamp_update AFTER UPDATE ON edges
WHEN new.updated_at = old.updated_at
 AND NOT EXISTS (SELECT 1 FROM app_meta WHERE key = 'local.merging')
BEGIN
  UPDATE edges SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = new.id;
END;

ALTER TABLE settings ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';
UPDATE settings SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now');

CREATE TRIGGER settings_stamp_insert AFTER INSERT ON settings
WHEN new.updated_at = ''
BEGIN
  UPDATE settings SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE key = new.key;
  DELETE FROM tombstones WHERE kind = 'setting' AND id = new.key;
END;

CREATE TRIGGER settings_stamp_update AFTER UPDATE OF value ON settings
WHEN new.updated_at = old.updated_at
 AND NOT EXISTS (SELECT 1 FROM app_meta WHERE key = 'local.merging')
BEGIN
  UPDATE settings SET updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE key = new.key;
END;

-- A hard delete leaves a tombstone so the deletion reaches the other devices and the item
-- never comes back. `kind` is a node type, 'attachment' or 'setting'; `id` is the row's id
-- (the key, for a setting). Recovering an item from a checkpoint sets `lifted_at`: the
-- tombstone counts only while it is newer than its lifting, and both columns merge by taking
-- the later value, so a recovery reaches the other devices and a later delete beats it again.
CREATE TABLE tombstones (
  kind       TEXT NOT NULL,
  id         TEXT NOT NULL,
  deleted_at TEXT NOT NULL,
  lifted_at  TEXT,
  PRIMARY KEY (kind, id)
);

CREATE TRIGGER settings_tombstone AFTER DELETE ON settings
WHEN NOT EXISTS (SELECT 1 FROM app_meta WHERE key = 'local.merging')
BEGIN
  INSERT INTO tombstones (kind, id, deleted_at)
  VALUES ('setting', old.key, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
  ON CONFLICT(kind, id) DO UPDATE SET deleted_at = excluded.deleted_at;
END;

-- Files attached to any item. The bytes live elsewhere (a local cache and Google Drive, named
-- by `sha256`); this row says what the file is and where it is attached. There are no foreign
-- keys: like edges, attachments point at items polymorphically.
CREATE TABLE attachments (
  id          TEXT PRIMARY KEY,
  node_type   TEXT NOT NULL,
  node_id     TEXT NOT NULL,
  sha256      TEXT NOT NULL,
  file_name   TEXT NOT NULL,
  mime_type   TEXT NOT NULL,
  size_bytes  INTEGER NOT NULL CHECK (size_bytes >= 0),
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  archived_at TEXT
);
CREATE INDEX idx_attachments_node ON attachments(node_id);
CREATE INDEX idx_attachments_sha  ON attachments(sha256);
