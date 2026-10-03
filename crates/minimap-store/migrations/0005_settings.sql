-- App settings: one row per key, value stored as JSON text. Defaults live in code.
CREATE TABLE settings (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
