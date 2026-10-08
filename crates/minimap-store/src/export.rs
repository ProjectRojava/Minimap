//! Full data export (spec 26): reads everything the user put in the database, archived items
//! included, as the same structs the rest of the app uses. Writing files is the command layer's
//! job; Markdown is core's.

use minimap_types::DataExport;
use rusqlite::Connection;

use crate::{
    activity, attachments, decisions, edges, error::Result, notes, objectives, people, projects,
    tasks, teams, waiting_on,
};

/// Everything, in one consistent read (one transaction's worth of a single connection).
pub fn collect(conn: &Connection) -> Result<DataExport> {
    Ok(DataExport {
        objectives: objectives::list(conn, true)?,
        projects: projects::list(conn, true)?,
        tasks: tasks::list(conn, true)?,
        people: people::list(conn, true)?,
        teams: teams::list(conn, true)?,
        notes: notes::list(conn, true)?,
        decisions: decisions::list(conn, true)?,
        waiting_on: waiting_on::list(conn, true)?,
        edges: edges::list_all(conn)?,
        attachments: attachments::list_records(conn)?,
        activity: activity::list_all(conn)?,
        task_types: crate::settings::task_types(conn)?,
    })
}
