-- Reference links on tasks (spec 28): a JSON list of {"title","url"}, in the order added.
ALTER TABLE tasks ADD COLUMN links TEXT NOT NULL DEFAULT '[]';
