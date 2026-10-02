-- Initial schema. Node tables, edges and activity arrive in M1.
CREATE TABLE app_meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
INSERT INTO app_meta (key, value) VALUES ('created_by', 'minimap');
