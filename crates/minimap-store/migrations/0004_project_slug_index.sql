-- Unique among active projects; archiving frees the handle.
CREATE UNIQUE INDEX idx_projects_slug_active ON projects(slug) WHERE archived_at IS NULL;
