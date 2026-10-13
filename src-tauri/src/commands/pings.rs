//! Pings (docs/product-design/05-pings.md, ADR-0020): a marker on any item with a day to come
//! back to it. Thin: the rules are `minimap_core::pings`, the writes `minimap_store::pings`.

use minimap_core::{pings::arrange, quick_add::parse_when};
use minimap_store::Connection;
use minimap_types::{
    AppError, CreatePing, Date, NodeRef, Patch, Ping, PingRow, PingsView, UpdatePing, Uuid,
};
use tauri::State;

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

/// A ping's refusals are sentences for the person ("Pick today or later"), not "invalid value: ...".
fn ping_error(e: minimap_store::StoreError) -> AppError {
    match e {
        minimap_store::StoreError::Invalid(message) => app_error("invalid", message),
        other => store_error(other),
    }
}

/// The live pings of items that are not archived: those whose day has come (oldest first) and
/// those still to come (soonest first).
#[tauri::command]
pub async fn list_pings(state: State<'_, AppState>) -> Result<PingsView, AppError> {
    state
        .run(|conn| list_pings_at(conn, minimap_store::today()))
        .await
}

/// [`list_pings`] as of `today` (the tests and the demo snapshots fix it).
pub(crate) fn list_pings_at(conn: &Connection, today: Date) -> Result<PingsView, AppError> {
    let rows = minimap_store::pings::list_live(conn).map_err(ping_error)?;
    Ok(arrange(rows, today))
}

/// Pings an item. `on` is a natural day (`today`, `tomorrow`, `fri`, `next-week`, `+3d`,
/// `2027-03-31`). An item that has a live ping gets that ping changed instead.
#[tauri::command]
pub async fn ping_item(
    state: State<'_, AppState>,
    target: NodeRef,
    note: String,
    on: String,
) -> Result<PingRow, AppError> {
    state
        .run(move |conn| ping_item_at(conn, target, &note, &on, minimap_store::today()))
        .await
}

pub(crate) fn ping_item_at(
    conn: &mut Connection,
    target: NodeRef,
    note: &str,
    on: &str,
    today: Date,
) -> Result<PingRow, AppError> {
    let on_date = parse_when(on, today).map_err(|m| app_error("invalid", m))?;
    let ping = minimap_store::pings::create(
        conn,
        today,
        CreatePing {
            target,
            note: note.to_owned(),
            on_date,
        },
    )
    .map_err(ping_error)?;
    row_of(conn, ping)
}

/// Changes a ping's note and/or day.
#[tauri::command]
pub async fn update_ping(
    state: State<'_, AppState>,
    id: Uuid,
    note: Option<String>,
    on: Option<String>,
) -> Result<PingRow, AppError> {
    state
        .run(move |conn| update_ping_at(conn, id, note, on.as_deref(), minimap_store::today()))
        .await
}

pub(crate) fn update_ping_at(
    conn: &mut Connection,
    id: Uuid,
    note: Option<String>,
    on: Option<&str>,
    today: Date,
) -> Result<PingRow, AppError> {
    let on_date = on
        .map(|text| parse_when(text, today).map_err(|m| app_error("invalid", m)))
        .transpose()?;
    let ping = minimap_store::pings::update(
        conn,
        id,
        today,
        UpdatePing {
            note,
            on_date,
            cleared_on: Patch::Keep,
        },
    )
    .map_err(ping_error)?;
    row_of(conn, ping)
}

/// Clears a ping: it drops off This week. (Ctrl/Cmd+Z brings it back.)
#[tauri::command]
pub async fn clear_ping(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    state
        .run(move |conn| {
            minimap_store::pings::clear(conn, id, minimap_store::today())
                .map(|_| ())
                .map_err(ping_error)
        })
        .await
}

/// Brings a cleared ping back.
#[tauri::command]
pub async fn reopen_ping(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    state
        .run(move |conn| {
            minimap_store::pings::reopen(conn, id, minimap_store::today())
                .map(|_| ())
                .map_err(ping_error)
        })
        .await
}

/// Removes a ping.
#[tauri::command]
pub async fn archive_ping(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    state
        .run(move |conn| minimap_store::pings::archive(conn, id).map_err(ping_error))
        .await
}

/// The live ping on an item, for its pane.
#[tauri::command]
pub async fn get_ping_for(
    state: State<'_, AppState>,
    target_id: Uuid,
) -> Result<Option<Ping>, AppError> {
    state
        .run(move |conn| minimap_store::pings::live_for(conn, target_id).map_err(ping_error))
        .await
}

fn row_of(conn: &Connection, ping: Ping) -> Result<PingRow, AppError> {
    let target = minimap_store::nodes::summary(conn, ping.target).map_err(ping_error)?;
    Ok(PingRow { ping, target })
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{CreateProject, NodeType, ProjectStatus};
    use time::{macros::date, Duration};

    // Wednesday 2027-03-03, as in the demo snapshots.
    const TODAY: Date = date!(2027 - 03 - 03);

    fn project(conn: &mut Connection, title: &str) -> NodeRef {
        let p = minimap_store::projects::create(
            conn,
            CreateProject {
                title: title.into(),
                slug: None,
                description: String::new(),
                owner_person_id: None,
                start_date: None,
                target_date: None,
                status: Some(ProjectStatus::Active),
                priority: None,
            },
        )
        .unwrap();
        NodeRef::new(NodeType::Project, p.id)
    }

    #[test]
    fn a_ping_is_made_from_a_natural_day_and_listed_as_due_or_upcoming() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let eu = project(&mut conn, "EU Region");
        let billing = project(&mut conn, "Billing");
        let row = ping_item_at(&mut conn, eu, "check data residency", "fri", TODAY).unwrap();
        assert_eq!(row.ping.on_date, date!(2027 - 03 - 05));
        assert_eq!(row.target.label, "EU Region");
        ping_item_at(&mut conn, billing, "", "today", TODAY).unwrap();
        let view = list_pings_at(&conn, TODAY).unwrap();
        assert_eq!(view.due.len(), 1);
        assert_eq!(view.due[0].target.label, "Billing");
        assert_eq!(view.upcoming.len(), 1);
        // On Friday both are due.
        let friday = list_pings_at(&conn, TODAY + Duration::days(2)).unwrap();
        assert_eq!(friday.due.len(), 2);
        assert!(friday.upcoming.is_empty());
    }

    #[test]
    fn bad_days_say_what_is_accepted_and_a_past_day_is_refused() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let eu = project(&mut conn, "EU Region");
        let bad = ping_item_at(&mut conn, eu, "", "someday", TODAY).unwrap_err();
        assert_eq!(bad.code, "invalid");
        assert!(bad.message.contains("today, fri, next-wed"));
        let past = ping_item_at(&mut conn, eu, "", "2027-01-01", TODAY).unwrap_err();
        assert_eq!(past.message, "Pick today or later");
    }

    #[test]
    fn pinging_again_changes_the_ping_and_it_can_be_updated() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let eu = project(&mut conn, "EU Region");
        let first = ping_item_at(&mut conn, eu, "a", "tomorrow", TODAY).unwrap();
        let second = ping_item_at(&mut conn, eu, "b", "+3d", TODAY).unwrap();
        assert_eq!(first.ping.id, second.ping.id);
        let changed =
            update_ping_at(&mut conn, first.ping.id, Some("c".into()), None, TODAY).unwrap();
        assert_eq!(changed.ping.note, "c");
        assert_eq!(changed.ping.on_date, second.ping.on_date);
        assert_eq!(list_pings_at(&conn, TODAY).unwrap().upcoming.len(), 1);
    }
}
