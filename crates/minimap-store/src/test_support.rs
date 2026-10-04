//! Helpers shared by the store's unit tests.

use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

use minimap_types::{AssigneeChoice, CreatePerson, CreateTask};
use rusqlite::Connection;

use crate::{people, tasks};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A fresh empty folder under the system temp dir.
pub(crate) fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "minimap-store-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

pub(crate) fn add_task(conn: &mut Connection, title: &str) {
    tasks::create(
        conn,
        CreateTask {
            title: title.into(),
            assignee: AssigneeChoice::Nobody,
            description: format!("About {title}"),
            project_id: None,
            status: None,
            estimate_days: Some(2.0),
            start_date: None,
            due_date: None,
            priority: None,
        },
    )
    .unwrap();
}

pub(crate) fn add_person(conn: &mut Connection, name: &str) {
    people::create(
        conn,
        CreatePerson {
            name: name.into(),
            role_title: String::new(),
            email: None,
            weekly_capacity_hours: None,
            is_self: false,
            notes: String::new(),
        },
    )
    .unwrap();
}

/// Everything a user could see, as text: equal strings = identical data.
pub(crate) fn fingerprint(conn: &Connection) -> String {
    let mut out = String::new();
    for table in [
        "objectives",
        "projects",
        "tasks",
        "people",
        "teams",
        "notes",
        "decisions",
        "waiting_on",
        "edges",
        "activity",
        "settings",
    ] {
        // This installation's own backup settings are kept across a restore on purpose.
        let filter = if table == "settings" {
            "WHERE key NOT IN ('backup_folder', 'auto_backup')"
        } else {
            ""
        };
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM {table} {filter} ORDER BY 1"))
            .unwrap();
        let cols = stmt.column_count();
        let rows: Vec<String> = stmt
            .query_map([], |r| {
                (0..cols)
                    .map(|i| {
                        r.get::<_, rusqlite::types::Value>(i)
                            .map(|v| format!("{v:?}"))
                    })
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map(|v| v.join("|"))
            })
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        out.push_str(&format!("{table}:\n{}\n", rows.join("\n")));
    }
    out
}
