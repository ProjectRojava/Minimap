use std::collections::HashMap;

use minimap_core::notes::{arrange, checklist, mention_ids, render};
use minimap_store::Connection;
use minimap_types::{
    AppError, CreateNote, NodeType, Note, NoteDetail, NoteFilter, NoteRow, Task, UpdateNote, Uuid,
};
use tauri::State;

use crate::{error::store_error, state::AppState};

/// Notes newest first, optionally filtered by kind, mentioned node, dates and text.
#[tauri::command]
pub async fn list_notes(
    state: State<'_, AppState>,
    filter_by: NoteFilter,
) -> Result<Vec<NoteRow>, AppError> {
    state
        .run(move |conn| {
            let items = minimap_store::views::note_items(conn).map_err(store_error)?;
            Ok(arrange(items, &filter_by))
        })
        .await
}

#[tauri::command]
pub async fn get_note(state: State<'_, AppState>, id: Uuid) -> Result<Note, AppError> {
    state
        .run(move |conn| minimap_store::notes::get(conn, id).map_err(store_error))
        .await
}

/// The note, what it mentions, and its unchecked `[ ]` lines.
#[tauri::command]
pub async fn get_note_detail(state: State<'_, AppState>, id: Uuid) -> Result<NoteDetail, AppError> {
    state.run(move |conn| detail_impl(conn, id)).await
}

pub(crate) fn detail_impl(conn: &Connection, id: Uuid) -> Result<NoteDetail, AppError> {
    let item = minimap_store::views::note_item(conn, id).map_err(store_error)?;
    Ok(NoteDetail {
        checklist: checklist(&item.note.body),
        mentions: item.mentions,
        note: item.note,
    })
}

/// Mentions in the body become links to the mentioned nodes; edits keep them in sync.
#[tauri::command]
pub async fn create_note(state: State<'_, AppState>, input: CreateNote) -> Result<Note, AppError> {
    state
        .run(move |conn| minimap_store::notes::create(conn, input).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn update_note(
    state: State<'_, AppState>,
    id: Uuid,
    patch: UpdateNote,
) -> Result<Note, AppError> {
    state
        .run(move |conn| minimap_store::notes::update(conn, id, patch).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn archive_note(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    state
        .run(move |conn| {
            minimap_store::nodes::archive(conn, minimap_types::NodeRef::new(NodeType::Note, id))
                .map_err(store_error)
        })
        .await
}

/// Markdown to HTML that is safe to show as-is (see `minimap_core::notes::render`).
/// Works on text that may not be saved yet, so the preview matches the editor.
#[tauri::command]
pub async fn render_markdown(state: State<'_, AppState>, body: String) -> Result<String, AppError> {
    state.run(move |conn| render_impl(conn, &body)).await
}

pub(crate) fn render_impl(conn: &Connection, body: &str) -> Result<String, AppError> {
    // Look every mentioned node up once; archived ones count as gone.
    let mut known: HashMap<Uuid, (NodeType, String)> = HashMap::new();
    for id in mention_ids(body) {
        if let Some(node) = minimap_store::nodes::find(conn, id).map_err(store_error)? {
            let summary = minimap_store::nodes::summary(conn, node).map_err(store_error)?;
            if !summary.archived {
                known.insert(id, (node.node_type, summary.label));
            }
        }
    }
    Ok(render(body, &|id| known.get(&id).cloned()))
}

/// Turns one unchecked `[ ]` line into a task (assigned to the person mentioned on the line,
/// otherwise to me) and replaces the line with a link to it. `text` is the line's text as the
/// caller saw it; a changed line is refused rather than converting the wrong thing.
#[tauri::command]
pub async fn convert_checklist_item(
    state: State<'_, AppState>,
    note_id: Uuid,
    line: u32,
    text: String,
) -> Result<Task, AppError> {
    state
        .run(move |conn| {
            minimap_store::notes::convert_checklist_item(conn, note_id, line as usize, &text)
                .map_err(store_error)
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{CreatePerson, UpdatePerson};

    fn person(conn: &mut Connection, name: &str) -> Uuid {
        minimap_store::people::create(
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
        .unwrap()
        .id
    }

    fn note(conn: &mut Connection, body: &str) -> Uuid {
        minimap_store::notes::create(
            conn,
            CreateNote {
                title: "n".into(),
                body: body.into(),
                note_date: None,
                kind: None,
                recurrence: None,
            },
        )
        .unwrap()
        .id
    }

    #[test]
    fn previews_show_current_names_and_hide_archived_nodes() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let priya = person(&mut conn, "Priya");
        let body = format!("Ask @[Priya](node:{priya}) and <b>x</b>");
        let html = render_impl(&conn, &body).unwrap();
        assert!(
            html.contains("class=\"mention\"") && html.contains("@Priya"),
            "{html}"
        );
        assert!(!html.contains("<b>"), "raw html is not interpreted");

        // Renaming shows up in old notes.
        minimap_store::people::update(
            &mut conn,
            priya,
            UpdatePerson {
                name: Some("Priya Shah".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(render_impl(&conn, &body).unwrap().contains("@Priya Shah"));

        // Once archived the mention renders as plain text.
        minimap_store::nodes::archive(
            &mut conn,
            minimap_types::NodeRef::new(NodeType::Person, priya),
        )
        .unwrap();
        let html = render_impl(&conn, &body).unwrap();
        assert!(
            html.contains("mention missing") && !html.contains("data-node-id"),
            "{html}"
        );
    }

    #[test]
    fn detail_lists_mentions_and_open_checklist_items() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let raj = person(&mut conn, "Raj");
        let n = note(
            &mut conn,
            &format!("[ ] Ask @[Raj](node:{raj}) for sign-off\n- [x] done\n- [ ] second"),
        );
        let d = detail_impl(&conn, n).unwrap();
        assert_eq!(
            d.mentions
                .iter()
                .map(|m| m.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Raj"]
        );
        let items: Vec<(u32, &str)> = d
            .checklist
            .iter()
            .map(|c| (c.line, c.text.as_str()))
            .collect();
        assert_eq!(items, vec![(0, "Ask Raj for sign-off"), (2, "second")]);
        assert_eq!(d.checklist[0].mentions[0].id, raj);
    }
}
