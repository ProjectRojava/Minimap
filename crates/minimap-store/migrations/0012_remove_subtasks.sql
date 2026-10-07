-- Subtasks were removed (ADR-0015): archive the links that still say "subtask_of", so nothing
-- shows or counts them. The edge type stays readable, so old backups and sync snapshots load.
UPDATE edges
   SET archived_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
 WHERE edge_type = 'subtask_of' AND archived_at IS NULL;
