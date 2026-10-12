//! Fog of war (spec 43): confirming that an item is still accurate. The rules that turn dates into
//! fog are in `minimap_core::fog`; the reads it needs are `views::last_link_changes` and
//! `views::last_mentions`.

use minimap_types::{Date, NodeRef, NodeType, Patch, UpdateObjective, UpdateProject, UpdateTask};
use rusqlite::Connection;

use crate::{
    error::{Result, StoreError},
    objectives, projects, tasks,
};

/// Says that a task, project or objective is still accurate as of `today`. It is a logged update
/// of `confirmed_on` (undoable), and it is not an edit: `updated_at` stays as it was, so undoing
/// it brings the fog back as it was. An archived item, or any other kind of node, is refused.
pub fn confirm(conn: &mut Connection, node: NodeRef, today: Date) -> Result<()> {
    let set = Patch::Set(today);
    match node.node_type {
        NodeType::Task => {
            if tasks::get(conn, node.id)?.archived_at.is_some() {
                return Err(archived());
            }
            tasks::update(
                conn,
                node.id,
                UpdateTask {
                    confirmed_on: set,
                    ..Default::default()
                },
            )?;
        }
        NodeType::Project => {
            if projects::get(conn, node.id)?.archived_at.is_some() {
                return Err(archived());
            }
            projects::update(
                conn,
                node.id,
                UpdateProject {
                    confirmed_on: set,
                    ..Default::default()
                },
            )?;
        }
        NodeType::Objective => {
            if objectives::get(conn, node.id)?.archived_at.is_some() {
                return Err(archived());
            }
            objectives::update(
                conn,
                node.id,
                UpdateObjective {
                    confirmed_on: set,
                    ..Default::default()
                },
            )?;
        }
        _ => {
            return Err(StoreError::Invalid(
                "only a task, a project or an objective can be confirmed".into(),
            ))
        }
    }
    Ok(())
}

fn archived() -> StoreError {
    StoreError::Invalid("an archived item can't be confirmed".into())
}
