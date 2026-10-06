//! Full data export (spec 26): everything in open formats, so nobody is locked in.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    Activity, Decision, Edge, NodeType, Note, Objective, Person, Project, Task, Team, WaitingOn,
};

/// The value of `manifest.json`'s `format`.
pub const EXPORT_FORMAT: &str = "minimap-export";
/// Bumped when the layout of an export changes in a way a reader would have to know about.
pub const EXPORT_FORMAT_VERSION: u32 = 1;

/// What goes in the export folder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    /// One JSON file per kind of item, plus links, history and a manifest (and the attached
    /// files).
    #[default]
    Json,
    /// The same, and also Markdown: one file per project with its tasks, the inbox, and each note
    /// as a `.md` file.
    JsonAndMarkdown,
}

impl ExportFormat {
    pub fn markdown(self) -> bool {
        self == ExportFormat::JsonAndMarkdown
    }
}

/// An attached file's row: what it is and where it is attached. (The bytes are exported as
/// files next to it.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttachmentRecord {
    pub id: Uuid,
    pub node_type: NodeType,
    pub node_id: Uuid,
    pub file_name: String,
    pub mime_type: String,
    pub size_bytes: u64,
    /// Hex SHA-256 of the file's content.
    pub sha256: String,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub archived_at: Option<OffsetDateTime>,
}

/// Everything the database holds that the user put there, archived items included. Each field
/// is written as one JSON file, and the files read back into exactly these structs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DataExport {
    pub objectives: Vec<Objective>,
    pub projects: Vec<Project>,
    pub tasks: Vec<Task>,
    pub people: Vec<Person>,
    pub teams: Vec<Team>,
    pub notes: Vec<Note>,
    pub decisions: Vec<Decision>,
    pub waiting_on: Vec<WaitingOn>,
    /// Every link, removed ones included (`archived_at` says which).
    pub edges: Vec<Edge>,
    pub attachments: Vec<AttachmentRecord>,
    /// The whole history, oldest first.
    pub activity: Vec<Activity>,
}

impl DataExport {
    /// How many records each file holds, by file name without `.json`.
    pub fn counts(&self) -> BTreeMap<String, u32> {
        let n = |len: usize| u32::try_from(len).unwrap_or(u32::MAX);
        BTreeMap::from([
            ("objectives".to_owned(), n(self.objectives.len())),
            ("projects".to_owned(), n(self.projects.len())),
            ("tasks".to_owned(), n(self.tasks.len())),
            ("people".to_owned(), n(self.people.len())),
            ("teams".to_owned(), n(self.teams.len())),
            ("notes".to_owned(), n(self.notes.len())),
            ("decisions".to_owned(), n(self.decisions.len())),
            ("waiting_on".to_owned(), n(self.waiting_on.len())),
            ("edges".to_owned(), n(self.edges.len())),
            ("attachments".to_owned(), n(self.attachments.len())),
            ("activity".to_owned(), n(self.activity.len())),
        ])
    }
}

/// `manifest.json`: what the export is and how to read it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportManifest {
    /// Always [`EXPORT_FORMAT`].
    pub format: String,
    pub format_version: u32,
    pub app: String,
    pub app_version: String,
    /// The database schema the data comes from.
    pub schema_version: u32,
    #[serde(with = "time::serde::rfc3339")]
    pub exported_at: OffsetDateTime,
    /// Archived items and removed links are in the files, marked by `archived_at`.
    pub includes_archived: bool,
    /// Markdown files (`projects/`, `notes/`, `inbox.md`) were written too.
    pub markdown: bool,
    /// Records per file, by file name without `.json`.
    pub counts: BTreeMap<String, u32>,
    /// Attached files written under `attachments/<id>/`.
    pub attachment_files: u32,
    /// Attached files that are on Google Drive but not on this computer, so not in the export.
    pub attachment_files_missing: u32,
}

/// What `export_all` did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportAllResult {
    /// The new folder holding the export (inside the one that was chosen).
    pub folder: String,
    /// Files written.
    pub files: u32,
    pub bytes: u64,
    pub counts: BTreeMap<String, u32>,
    pub markdown_files: u32,
    pub attachment_files: u32,
    pub attachment_files_missing: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_json_only() {
        assert_eq!(ExportFormat::default(), ExportFormat::Json);
        assert!(!ExportFormat::Json.markdown());
        assert!(ExportFormat::JsonAndMarkdown.markdown());
        assert_eq!(
            serde_json::to_string(&ExportFormat::JsonAndMarkdown).unwrap(),
            "\"json_and_markdown\""
        );
    }

    #[test]
    fn counts_name_every_file() {
        let counts = DataExport::default().counts();
        let names: Vec<&str> = counts.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            [
                "activity",
                "attachments",
                "decisions",
                "edges",
                "notes",
                "objectives",
                "people",
                "projects",
                "tasks",
                "teams",
                "waiting_on"
            ]
        );
    }
}
