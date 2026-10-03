use minimap_core::waiting_on::arrange;
use minimap_store::Connection;
use minimap_types::{
    AppError, CreateWaitingOn, NodeRef, NodeType, Patch, UpdateWaitingOn, Uuid, WaitingOn,
    WaitingOnFilter, WaitingOnRow,
};
use tauri::State;
use time::{Date, Duration};

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

fn check_person_active(conn: &Connection, person: Uuid) -> Result<(), AppError> {
    let s = minimap_store::nodes::summary(conn, NodeRef::new(NodeType::Person, person))
        .map_err(store_error)?;
    if s.archived {
        return Err(app_error("invalid", "that person is archived"));
    }
    Ok(())
}

/// Open waiting-ons oldest first, with age, stale and snoozed flags (the stale threshold is
/// a setting). Snoozed and resolved ones are left out unless the filter asks for them.
#[tauri::command]
pub async fn get_waiting_on(
    state: State<'_, AppState>,
    filter_by: WaitingOnFilter,
) -> Result<Vec<WaitingOnRow>, AppError> {
    state
        .run(move |conn| list_impl(conn, &filter_by, minimap_store::today()))
        .await
}

pub(crate) fn list_impl(
    conn: &Connection,
    filter: &WaitingOnFilter,
    today: Date,
) -> Result<Vec<WaitingOnRow>, AppError> {
    let stale_days = minimap_store::settings::get(conn)
        .map_err(store_error)?
        .stale_waiting_days;
    let items = minimap_store::views::waiting_on_items(conn).map_err(store_error)?;
    Ok(arrange(items, filter, today, stale_days))
}

/// One waiting-on as the list shows it (whatever its state), for the detail pane.
#[tauri::command]
pub async fn get_waiting_on_detail(
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<WaitingOnRow, AppError> {
    state
        .run(move |conn| {
            let all = WaitingOnFilter {
                person_id: None,
                include_resolved: true,
                include_snoozed: true,
            };
            list_impl(conn, &all, minimap_store::today())?
                .into_iter()
                .find(|r| r.waiting.id == id)
                .ok_or_else(|| app_error("not_found", "that waiting-on no longer exists"))
        })
        .await
}

#[tauri::command]
pub async fn create_waiting_on(
    state: State<'_, AppState>,
    input: CreateWaitingOn,
) -> Result<WaitingOn, AppError> {
    state
        .run(move |conn| {
            check_person_active(conn, input.person_id)?;
            minimap_store::waiting_on::create(conn, input).map_err(store_error)
        })
        .await
}

#[tauri::command]
pub async fn update_waiting_on(
    state: State<'_, AppState>,
    id: Uuid,
    patch: UpdateWaitingOn,
) -> Result<WaitingOn, AppError> {
    state
        .run(move |conn| {
            if let Some(p) = patch.person_id {
                check_person_active(conn, p)?;
            }
            minimap_store::waiting_on::update(conn, id, patch).map_err(store_error)
        })
        .await
}

/// Marks it resolved today (already resolved: unchanged).
#[tauri::command]
pub async fn resolve_waiting_on(
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<WaitingOn, AppError> {
    state
        .run(move |conn| resolve_impl(conn, id, minimap_store::today()))
        .await
}

pub(crate) fn resolve_impl(
    conn: &mut Connection,
    id: Uuid,
    today: Date,
) -> Result<WaitingOn, AppError> {
    let patch = UpdateWaitingOn {
        resolved_on: Patch::Set(today),
        ..Default::default()
    };
    let current = minimap_store::waiting_on::get(conn, id).map_err(store_error)?;
    if current.resolved_on.is_some() {
        return Ok(current);
    }
    minimap_store::waiting_on::update(conn, id, patch).map_err(store_error)
}

#[tauri::command]
pub async fn reopen_waiting_on(
    state: State<'_, AppState>,
    id: Uuid,
) -> Result<WaitingOn, AppError> {
    state
        .run(move |conn| {
            let patch = UpdateWaitingOn {
                resolved_on: Patch::Clear,
                ..Default::default()
            };
            minimap_store::waiting_on::update(conn, id, patch).map_err(store_error)
        })
        .await
}

/// Hides it for `days` days (resurfaces on that date); `None` ends the snooze.
#[tauri::command]
pub async fn snooze_waiting_on(
    state: State<'_, AppState>,
    id: Uuid,
    days: Option<u32>,
) -> Result<WaitingOn, AppError> {
    state
        .run(move |conn| snooze_impl(conn, id, days, minimap_store::today()))
        .await
}

pub(crate) fn snooze_impl(
    conn: &mut Connection,
    id: Uuid,
    days: Option<u32>,
    today: Date,
) -> Result<WaitingOn, AppError> {
    let follow_up_on = match days {
        Some(d) => Patch::Set(today + Duration::days(i64::from(d))),
        None => Patch::Clear,
    };
    let patch = UpdateWaitingOn {
        follow_up_on,
        ..Default::default()
    };
    minimap_store::waiting_on::update(conn, id, patch).map_err(store_error)
}

#[tauri::command]
pub async fn archive_waiting_on(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    state
        .run(move |conn| {
            minimap_store::nodes::archive(conn, NodeRef::new(NodeType::WaitingOn, id))
                .map_err(store_error)
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{CreatePerson, UpdateSettings};
    use time::macros::date;

    const TODAY: Date = date!(2027 - 03 - 15);

    fn setup() -> (Connection, Uuid) {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let raj = minimap_store::people::create(
            &mut conn,
            CreatePerson {
                name: "Raj".into(),
                role_title: String::new(),
                email: None,
                weekly_capacity_hours: None,
                is_self: false,
                notes: String::new(),
            },
        )
        .unwrap();
        (conn, raj.id)
    }

    fn make(conn: &mut Connection, person: Uuid, text: &str, asked: Date) -> WaitingOn {
        minimap_store::waiting_on::create(
            conn,
            CreateWaitingOn {
                description: text.into(),
                person_id: person,
                asked_on: Some(asked),
                expected_by: None,
                follow_up_on: None,
            },
        )
        .unwrap()
    }

    fn texts(rows: &[WaitingOnRow]) -> Vec<&str> {
        rows.iter()
            .map(|r| r.waiting.description.as_str())
            .collect()
    }

    #[test]
    fn resolve_reopen_and_snooze_round_trip() {
        let (mut conn, raj) = setup();
        let w = make(&mut conn, raj, "sign-off", TODAY - Duration::days(3));

        let resolved = resolve_impl(&mut conn, w.id, TODAY).unwrap();
        assert_eq!(resolved.resolved_on, Some(TODAY));
        // Resolving again keeps the original date and writes nothing.
        let count = minimap_store::activity::count(&conn).unwrap();
        let again = resolve_impl(&mut conn, w.id, TODAY + Duration::days(5)).unwrap();
        assert_eq!(again.resolved_on, Some(TODAY));
        assert_eq!(minimap_store::activity::count(&conn).unwrap(), count);

        let open = WaitingOnFilter::default();
        assert!(list_impl(&conn, &open, TODAY).unwrap().is_empty());
        let all = WaitingOnFilter {
            include_resolved: true,
            ..Default::default()
        };
        assert_eq!(
            texts(&list_impl(&conn, &all, TODAY).unwrap()),
            vec!["sign-off"]
        );

        // Reopen (command body, via the store) brings it back.
        minimap_store::waiting_on::update(
            &mut conn,
            w.id,
            UpdateWaitingOn {
                resolved_on: Patch::Clear,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(list_impl(&conn, &open, TODAY).unwrap().len(), 1);

        // Snooze 3 days: hidden today and the day after, back on the follow-up date.
        let snoozed = snooze_impl(&mut conn, w.id, Some(3), TODAY).unwrap();
        assert_eq!(snoozed.follow_up_on, Some(TODAY + Duration::days(3)));
        assert!(list_impl(&conn, &open, TODAY).unwrap().is_empty());
        assert!(list_impl(&conn, &open, TODAY + Duration::days(2))
            .unwrap()
            .is_empty());
        assert_eq!(
            list_impl(&conn, &open, TODAY + Duration::days(3))
                .unwrap()
                .len(),
            1
        );
        let with_snoozed = WaitingOnFilter {
            include_snoozed: true,
            ..Default::default()
        };
        assert!(list_impl(&conn, &with_snoozed, TODAY).unwrap()[0].snoozed);
        // Ending the snooze.
        snooze_impl(&mut conn, w.id, None, TODAY).unwrap();
        assert_eq!(list_impl(&conn, &open, TODAY).unwrap().len(), 1);
    }

    #[test]
    fn stale_follows_the_setting() {
        let (mut conn, raj) = setup();
        make(&mut conn, raj, "ten days", TODAY - Duration::days(10));
        make(&mut conn, raj, "two days", TODAY - Duration::days(2));
        let open = WaitingOnFilter::default();
        let stale = |conn: &Connection| -> Vec<bool> {
            list_impl(conn, &open, TODAY)
                .unwrap()
                .iter()
                .map(|r| r.stale)
                .collect()
        };
        assert_eq!(stale(&conn), vec![true, false]); // default threshold 7 days
        minimap_store::settings::update(
            &mut conn,
            UpdateSettings {
                stale_waiting_days: Some(14),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(stale(&conn), vec![false, false]);
        minimap_store::settings::update(
            &mut conn,
            UpdateSettings {
                stale_waiting_days: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(stale(&conn), vec![true, true]);
    }

    #[test]
    fn archived_people_cannot_be_waited_on() {
        let (mut conn, raj) = setup();
        minimap_store::nodes::archive(&mut conn, NodeRef::new(NodeType::Person, raj)).unwrap();
        assert_eq!(check_person_active(&conn, raj).unwrap_err().code, "invalid");
        assert_eq!(
            check_person_active(&conn, Uuid::now_v7()).unwrap_err().code,
            "not_found"
        );
    }
}
