-- Node tables, edges and activity log (CLAUDE.md section 4).
-- Ids are uuid v7 TEXT; dates are YYYY-MM-DD; timestamps are ISO-8601 UTC.

CREATE TABLE objectives (
  id          TEXT PRIMARY KEY,
  title       TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  target_date TEXT,
  status      TEXT NOT NULL CHECK (status IN ('on_track','at_risk','off_track','done')),
  priority    INTEGER NOT NULL CHECK (priority BETWEEN 1 AND 5),
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  archived_at TEXT
);

CREATE TABLE people (
  id                    TEXT PRIMARY KEY,
  name                  TEXT NOT NULL,
  role_title            TEXT NOT NULL DEFAULT '',
  email                 TEXT,
  weekly_capacity_hours REAL NOT NULL DEFAULT 40 CHECK (weekly_capacity_hours > 0),
  is_self               INTEGER NOT NULL DEFAULT 0 CHECK (is_self IN (0, 1)),
  notes                 TEXT NOT NULL DEFAULT '',
  created_at            TEXT NOT NULL,
  updated_at            TEXT NOT NULL,
  archived_at           TEXT
);
-- Exactly one self person.
CREATE UNIQUE INDEX idx_people_single_self ON people(is_self) WHERE is_self = 1;

CREATE TABLE teams (
  id             TEXT PRIMARY KEY,
  name           TEXT NOT NULL,
  description    TEXT NOT NULL DEFAULT '',
  parent_team_id TEXT REFERENCES teams(id),
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL,
  archived_at    TEXT
);

CREATE TABLE projects (
  id              TEXT PRIMARY KEY,
  title           TEXT NOT NULL,
  description     TEXT NOT NULL DEFAULT '',
  owner_person_id TEXT REFERENCES people(id),
  start_date      TEXT,
  target_date     TEXT,
  status          TEXT NOT NULL CHECK (status IN ('planned','active','paused','done','cancelled')),
  priority        INTEGER NOT NULL CHECK (priority BETWEEN 1 AND 5),
  created_at      TEXT NOT NULL,
  updated_at      TEXT NOT NULL,
  archived_at     TEXT
);

CREATE TABLE tasks (
  id            TEXT PRIMARY KEY,
  title         TEXT NOT NULL,
  description   TEXT NOT NULL DEFAULT '',
  project_id    TEXT REFERENCES projects(id),
  status        TEXT NOT NULL CHECK (status IN ('todo','in_progress','blocked','done','cancelled')),
  estimate_days REAL CHECK (estimate_days IS NULL OR estimate_days >= 0),
  start_date    TEXT,
  due_date      TEXT,
  completed_at  TEXT,
  priority      INTEGER NOT NULL CHECK (priority BETWEEN 1 AND 5),
  created_at    TEXT NOT NULL,
  updated_at    TEXT NOT NULL,
  archived_at   TEXT
);
CREATE INDEX idx_tasks_project ON tasks(project_id);

CREATE TABLE notes (
  id          TEXT PRIMARY KEY,
  title       TEXT NOT NULL,
  body        TEXT NOT NULL DEFAULT '',
  note_date   TEXT NOT NULL,
  kind        TEXT NOT NULL CHECK (kind IN ('one_on_one','meeting','general')),
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  archived_at TEXT
);

CREATE TABLE decisions (
  id          TEXT PRIMARY KEY,
  title       TEXT NOT NULL,
  context     TEXT NOT NULL DEFAULT '',
  decision    TEXT NOT NULL DEFAULT '',
  rationale   TEXT NOT NULL DEFAULT '',
  decided_on  TEXT,
  status      TEXT NOT NULL CHECK (status IN ('proposed','decided','superseded')),
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  archived_at TEXT
);

CREATE TABLE waiting_on (
  id          TEXT PRIMARY KEY,
  description TEXT NOT NULL,
  person_id   TEXT NOT NULL REFERENCES people(id),
  asked_on    TEXT NOT NULL,
  expected_by TEXT,
  resolved_on TEXT,
  created_at  TEXT NOT NULL,
  updated_at  TEXT NOT NULL,
  archived_at TEXT
);

-- Edges reference nodes polymorphically, so there are no foreign keys;
-- endpoint existence is checked in code.
CREATE TABLE edges (
  id          TEXT PRIMARY KEY,
  edge_type   TEXT NOT NULL,
  from_type   TEXT NOT NULL,
  from_id     TEXT NOT NULL,
  to_type     TEXT NOT NULL,
  to_id       TEXT NOT NULL,
  attrs       TEXT NOT NULL DEFAULT '{}',
  created_at  TEXT NOT NULL,
  archived_at TEXT,
  UNIQUE (edge_type, from_id, to_id)
);
CREATE INDEX idx_edges_from ON edges(from_id, edge_type);
CREATE INDEX idx_edges_to   ON edges(to_id, edge_type);

-- History survives hard deletes, so no foreign key on node_id.
CREATE TABLE activity (
  id        TEXT PRIMARY KEY,
  at        TEXT NOT NULL,
  node_type TEXT NOT NULL,
  node_id   TEXT NOT NULL,
  action    TEXT NOT NULL,
  diff      TEXT NOT NULL
);
CREATE INDEX idx_activity_node ON activity(node_id, at);
CREATE INDEX idx_activity_at   ON activity(at);
