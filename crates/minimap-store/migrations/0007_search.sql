-- Full-text search (spec 11).
--
-- `search_docs` maps each node to an integer rowid (FTS5 rows are keyed by integer) and carries
-- the node type and archived flag; `search_index` holds the searchable text: `title` (names,
-- titles, handles) and `body` (everything else). Triggers keep both in step with the node
-- tables, so every write path is covered. `mention_text` is a SQL function the store registers
-- on each connection (it turns `@[Name](node:id)` into `@Name`, so ids never reach the index);
-- a connection without it can read the database but not write notes.
--
-- `search_vocab` lists the indexed terms, for typo-tolerant matching.

CREATE TABLE search_docs (
  rowid       INTEGER PRIMARY KEY,
  node_type   TEXT NOT NULL,
  node_id     TEXT NOT NULL UNIQUE,
  archived    INTEGER NOT NULL DEFAULT 0
);

CREATE VIRTUAL TABLE search_index USING fts5(
  title, body,
  tokenize = 'unicode61 remove_diacritics 2'
);

CREATE VIRTUAL TABLE search_vocab USING fts5vocab(search_index, 'row');

-- objectives
CREATE TRIGGER search_objectives_ai AFTER INSERT ON objectives BEGIN
  INSERT INTO search_docs (node_type, node_id, archived)
  VALUES ('objective', new.id, new.archived_at IS NOT NULL);
  INSERT INTO search_index (rowid, title, body)
  VALUES (last_insert_rowid(), new.title, new.description);
END;
CREATE TRIGGER search_objectives_au AFTER UPDATE OF title, description, archived_at ON objectives BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  INSERT INTO search_index (rowid, title, body)
  SELECT rowid, new.title, new.description FROM search_docs WHERE node_id = new.id;
  UPDATE search_docs SET archived = new.archived_at IS NOT NULL WHERE node_id = new.id;
END;
CREATE TRIGGER search_objectives_ad AFTER DELETE ON objectives BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  DELETE FROM search_docs WHERE node_id = old.id;
END;

-- projects
CREATE TRIGGER search_projects_ai AFTER INSERT ON projects BEGIN
  INSERT INTO search_docs (node_type, node_id, archived)
  VALUES ('project', new.id, new.archived_at IS NOT NULL);
  INSERT INTO search_index (rowid, title, body)
  VALUES (last_insert_rowid(), new.title || ' ' || new.slug, new.description);
END;
CREATE TRIGGER search_projects_au AFTER UPDATE OF title, slug, description, archived_at ON projects BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  INSERT INTO search_index (rowid, title, body)
  SELECT rowid, new.title || ' ' || new.slug, new.description FROM search_docs WHERE node_id = new.id;
  UPDATE search_docs SET archived = new.archived_at IS NOT NULL WHERE node_id = new.id;
END;
CREATE TRIGGER search_projects_ad AFTER DELETE ON projects BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  DELETE FROM search_docs WHERE node_id = old.id;
END;

-- tasks
CREATE TRIGGER search_tasks_ai AFTER INSERT ON tasks BEGIN
  INSERT INTO search_docs (node_type, node_id, archived)
  VALUES ('task', new.id, new.archived_at IS NOT NULL);
  INSERT INTO search_index (rowid, title, body)
  VALUES (last_insert_rowid(), new.title, new.description);
END;
CREATE TRIGGER search_tasks_au AFTER UPDATE OF title, description, archived_at ON tasks BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  INSERT INTO search_index (rowid, title, body)
  SELECT rowid, new.title, new.description FROM search_docs WHERE node_id = new.id;
  UPDATE search_docs SET archived = new.archived_at IS NOT NULL WHERE node_id = new.id;
END;
CREATE TRIGGER search_tasks_ad AFTER DELETE ON tasks BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  DELETE FROM search_docs WHERE node_id = old.id;
END;

-- people
CREATE TRIGGER search_people_ai AFTER INSERT ON people BEGIN
  INSERT INTO search_docs (node_type, node_id, archived)
  VALUES ('person', new.id, new.archived_at IS NOT NULL);
  INSERT INTO search_index (rowid, title, body)
  VALUES (last_insert_rowid(), new.name, new.role_title || ' ' || ifnull(new.email, '') || ' ' || new.notes);
END;
CREATE TRIGGER search_people_au AFTER UPDATE OF name, role_title, email, notes, archived_at ON people BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  INSERT INTO search_index (rowid, title, body)
  SELECT rowid, new.name, new.role_title || ' ' || ifnull(new.email, '') || ' ' || new.notes FROM search_docs WHERE node_id = new.id;
  UPDATE search_docs SET archived = new.archived_at IS NOT NULL WHERE node_id = new.id;
END;
CREATE TRIGGER search_people_ad AFTER DELETE ON people BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  DELETE FROM search_docs WHERE node_id = old.id;
END;

-- teams
CREATE TRIGGER search_teams_ai AFTER INSERT ON teams BEGIN
  INSERT INTO search_docs (node_type, node_id, archived)
  VALUES ('team', new.id, new.archived_at IS NOT NULL);
  INSERT INTO search_index (rowid, title, body)
  VALUES (last_insert_rowid(), new.name, new.description);
END;
CREATE TRIGGER search_teams_au AFTER UPDATE OF name, description, archived_at ON teams BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  INSERT INTO search_index (rowid, title, body)
  SELECT rowid, new.name, new.description FROM search_docs WHERE node_id = new.id;
  UPDATE search_docs SET archived = new.archived_at IS NOT NULL WHERE node_id = new.id;
END;
CREATE TRIGGER search_teams_ad AFTER DELETE ON teams BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  DELETE FROM search_docs WHERE node_id = old.id;
END;

-- notes
CREATE TRIGGER search_notes_ai AFTER INSERT ON notes BEGIN
  INSERT INTO search_docs (node_type, node_id, archived)
  VALUES ('note', new.id, new.archived_at IS NOT NULL);
  INSERT INTO search_index (rowid, title, body)
  VALUES (last_insert_rowid(), new.title, mention_text(new.body));
END;
CREATE TRIGGER search_notes_au AFTER UPDATE OF title, body, archived_at ON notes BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  INSERT INTO search_index (rowid, title, body)
  SELECT rowid, new.title, mention_text(new.body) FROM search_docs WHERE node_id = new.id;
  UPDATE search_docs SET archived = new.archived_at IS NOT NULL WHERE node_id = new.id;
END;
CREATE TRIGGER search_notes_ad AFTER DELETE ON notes BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  DELETE FROM search_docs WHERE node_id = old.id;
END;

-- decisions
CREATE TRIGGER search_decisions_ai AFTER INSERT ON decisions BEGIN
  INSERT INTO search_docs (node_type, node_id, archived)
  VALUES ('decision', new.id, new.archived_at IS NOT NULL);
  INSERT INTO search_index (rowid, title, body)
  VALUES (last_insert_rowid(), new.title, new.context || ' ' || new.decision || ' ' || new.rationale);
END;
CREATE TRIGGER search_decisions_au AFTER UPDATE OF title, context, decision, rationale, archived_at ON decisions BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  INSERT INTO search_index (rowid, title, body)
  SELECT rowid, new.title, new.context || ' ' || new.decision || ' ' || new.rationale FROM search_docs WHERE node_id = new.id;
  UPDATE search_docs SET archived = new.archived_at IS NOT NULL WHERE node_id = new.id;
END;
CREATE TRIGGER search_decisions_ad AFTER DELETE ON decisions BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  DELETE FROM search_docs WHERE node_id = old.id;
END;

-- waiting_on
CREATE TRIGGER search_waiting_on_ai AFTER INSERT ON waiting_on BEGIN
  INSERT INTO search_docs (node_type, node_id, archived)
  VALUES ('waiting_on', new.id, new.archived_at IS NOT NULL);
  INSERT INTO search_index (rowid, title, body)
  VALUES (last_insert_rowid(), new.description, '');
END;
CREATE TRIGGER search_waiting_on_au AFTER UPDATE OF description, archived_at ON waiting_on BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  INSERT INTO search_index (rowid, title, body)
  SELECT rowid, new.description, '' FROM search_docs WHERE node_id = new.id;
  UPDATE search_docs SET archived = new.archived_at IS NOT NULL WHERE node_id = new.id;
END;
CREATE TRIGGER search_waiting_on_ad AFTER DELETE ON waiting_on BEGIN
  DELETE FROM search_index WHERE rowid = (SELECT rowid FROM search_docs WHERE node_id = old.id);
  DELETE FROM search_docs WHERE node_id = old.id;
END;

-- Index what already exists.
INSERT INTO search_docs (node_type, node_id, archived)
  SELECT 'objective', id, archived_at IS NOT NULL FROM objectives;
INSERT INTO search_index (rowid, title, body)
  SELECT d.rowid, n.title, n.description
  FROM objectives n JOIN search_docs d ON d.node_id = n.id;

INSERT INTO search_docs (node_type, node_id, archived)
  SELECT 'project', id, archived_at IS NOT NULL FROM projects;
INSERT INTO search_index (rowid, title, body)
  SELECT d.rowid, n.title || ' ' || n.slug, n.description
  FROM projects n JOIN search_docs d ON d.node_id = n.id;

INSERT INTO search_docs (node_type, node_id, archived)
  SELECT 'task', id, archived_at IS NOT NULL FROM tasks;
INSERT INTO search_index (rowid, title, body)
  SELECT d.rowid, n.title, n.description
  FROM tasks n JOIN search_docs d ON d.node_id = n.id;

INSERT INTO search_docs (node_type, node_id, archived)
  SELECT 'person', id, archived_at IS NOT NULL FROM people;
INSERT INTO search_index (rowid, title, body)
  SELECT d.rowid, n.name, n.role_title || ' ' || ifnull(n.email, '') || ' ' || n.notes
  FROM people n JOIN search_docs d ON d.node_id = n.id;

INSERT INTO search_docs (node_type, node_id, archived)
  SELECT 'team', id, archived_at IS NOT NULL FROM teams;
INSERT INTO search_index (rowid, title, body)
  SELECT d.rowid, n.name, n.description
  FROM teams n JOIN search_docs d ON d.node_id = n.id;

INSERT INTO search_docs (node_type, node_id, archived)
  SELECT 'note', id, archived_at IS NOT NULL FROM notes;
INSERT INTO search_index (rowid, title, body)
  SELECT d.rowid, n.title, mention_text(n.body)
  FROM notes n JOIN search_docs d ON d.node_id = n.id;

INSERT INTO search_docs (node_type, node_id, archived)
  SELECT 'decision', id, archived_at IS NOT NULL FROM decisions;
INSERT INTO search_index (rowid, title, body)
  SELECT d.rowid, n.title, n.context || ' ' || n.decision || ' ' || n.rationale
  FROM decisions n JOIN search_docs d ON d.node_id = n.id;

INSERT INTO search_docs (node_type, node_id, archived)
  SELECT 'waiting_on', id, archived_at IS NOT NULL FROM waiting_on;
INSERT INTO search_index (rowid, title, body)
  SELECT d.rowid, n.description, ''
  FROM waiting_on n JOIN search_docs d ON d.node_id = n.id;
