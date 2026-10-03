use minimap_core::cycles::find_cycle;
use minimap_types::{
    AppError, CreateTeam, NodeRef, NodeType, Patch, Team, TeamDetail, TeamRow, UpdateTeam, Uuid,
};
use tauri::State;

use super::edges::label;
use crate::{
    error::{app_error, cycle_error, store_error},
    state::AppState,
};

/// A parent must exist and be active.
fn check_parent_active(conn: &minimap_store::Connection, parent: Uuid) -> Result<(), AppError> {
    let s = minimap_store::nodes::summary(conn, NodeRef::new(NodeType::Team, parent))
        .map_err(store_error)?;
    if s.archived {
        return Err(app_error("invalid", "that parent team is archived"));
    }
    Ok(())
}

#[tauri::command]
pub async fn list_teams(state: State<'_, AppState>) -> Result<Vec<TeamRow>, AppError> {
    state
        .run(|conn| minimap_store::views::team_rows(conn).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn get_team(state: State<'_, AppState>, id: Uuid) -> Result<Team, AppError> {
    state
        .run(move |conn| minimap_store::teams::get(conn, id).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn get_team_detail(state: State<'_, AppState>, id: Uuid) -> Result<TeamDetail, AppError> {
    state
        .run(move |conn| minimap_store::views::team_detail(conn, id).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn create_team(state: State<'_, AppState>, input: CreateTeam) -> Result<Team, AppError> {
    state
        .run(move |conn| {
            if let Some(p) = input.parent_team_id {
                check_parent_active(conn, p)?;
            }
            minimap_store::teams::create(conn, input).map_err(store_error)
        })
        .await
}

/// Changing the parent is checked so nesting never loops.
#[tauri::command]
pub async fn update_team(
    state: State<'_, AppState>,
    id: Uuid,
    patch: UpdateTeam,
) -> Result<Team, AppError> {
    state
        .run(move |conn| {
            if let Patch::Set(parent) = patch.parent_team_id {
                check_new_parent(conn, id, parent)?;
            }
            minimap_store::teams::update(conn, id, patch).map_err(store_error)
        })
        .await
}

/// `parent` must be active and must not be `id` or one of its descendants.
pub(crate) fn check_new_parent(
    conn: &minimap_store::Connection,
    id: Uuid,
    parent: Uuid,
) -> Result<(), AppError> {
    check_parent_active(conn, parent)?;
    // Edges point child -> parent; archived teams still count so nothing can loop after a restore.
    let pairs: Vec<(Uuid, Uuid)> = minimap_store::teams::list(conn, true)
        .map_err(store_error)?
        .into_iter()
        .filter_map(|t| t.parent_team_id.map(|p| (t.id, p)))
        .filter(|(child, _)| *child != id)
        .collect();
    if let Some(cycle) = find_cycle(&pairs, id, parent) {
        let labels: Vec<String> = cycle
            .iter()
            .map(|t| label(conn, NodeType::Team, *t))
            .collect();
        return Err(cycle_error("nest this team there", &labels));
    }
    Ok(())
}

/// Archives a team and its memberships. Refused while it still has active sub-teams.
#[tauri::command]
pub async fn archive_team(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    state
        .run(move |conn| {
            let children: Vec<String> = minimap_store::teams::list(conn, false)
                .map_err(store_error)?
                .into_iter()
                .filter(|t| t.parent_team_id == Some(id))
                .map(|t| t.name)
                .collect();
            if !children.is_empty() {
                return Err(app_error(
                    "state",
                    format!(
                        "Move or archive its sub-teams first: {}",
                        children.join(", ")
                    ),
                ));
            }
            minimap_store::nodes::archive(conn, NodeRef::new(NodeType::Team, id))
                .map_err(store_error)
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::CreateTeam;

    fn team(conn: &mut minimap_store::Connection, name: &str, parent: Option<Uuid>) -> Team {
        minimap_store::teams::create(
            conn,
            CreateTeam {
                name: name.into(),
                description: String::new(),
                parent_team_id: parent,
            },
        )
        .unwrap()
    }

    #[test]
    fn nesting_cannot_loop() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let eng = team(&mut conn, "Engineering", None);
        let platform = team(&mut conn, "Platform", Some(eng.id));
        let infra = team(&mut conn, "Infra", Some(platform.id));

        // Under itself, under its child, under its grandchild: all loops.
        for bad in [eng.id, platform.id, infra.id] {
            let err = check_new_parent(&conn, eng.id, bad).unwrap_err();
            assert_eq!(err.code, "cycle", "parent {bad}");
        }
        let err = check_new_parent(&conn, eng.id, infra.id).unwrap_err();
        assert!(
            err.message
                .contains("Engineering → Infra → Platform → Engineering"),
            "{}",
            err.message
        );

        // Moving a leaf sideways or up is fine.
        let sales = team(&mut conn, "Sales", None);
        assert!(check_new_parent(&conn, infra.id, sales.id).is_ok());
        assert!(check_new_parent(&conn, infra.id, eng.id).is_ok());
    }

    #[test]
    fn archived_parent_is_refused() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let (a, b) = (team(&mut conn, "A", None), team(&mut conn, "B", None));
        minimap_store::nodes::archive(&mut conn, NodeRef::new(NodeType::Team, a.id)).unwrap();
        assert_eq!(
            check_new_parent(&conn, b.id, a.id).unwrap_err().code,
            "invalid"
        );
    }
}
