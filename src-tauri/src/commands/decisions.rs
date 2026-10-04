use minimap_core::decisions::arrange;
use minimap_types::{
    AppError, CreateDecision, Decision, DecisionFilter, DecisionRow, NodeRef, NodeType,
    UpdateDecision, Uuid,
};
use tauri::State;

use crate::{error::store_error, state::AppState};

/// Decisions newest first, optionally filtered by status, affected node, dates and text.
#[tauri::command]
pub async fn list_decisions(
    state: State<'_, AppState>,
    filter_by: DecisionFilter,
) -> Result<Vec<DecisionRow>, AppError> {
    state
        .run(move |conn| {
            let items = minimap_store::views::decision_items(conn).map_err(store_error)?;
            Ok(arrange(items, &filter_by))
        })
        .await
}

#[tauri::command]
pub async fn get_decision(state: State<'_, AppState>, id: Uuid) -> Result<Decision, AppError> {
    state
        .run(move |conn| minimap_store::decisions::get(conn, id).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn create_decision(
    state: State<'_, AppState>,
    input: CreateDecision,
) -> Result<Decision, AppError> {
    state
        .run(move |conn| minimap_store::decisions::create(conn, input).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn update_decision(
    state: State<'_, AppState>,
    id: Uuid,
    patch: UpdateDecision,
) -> Result<Decision, AppError> {
    state
        .run(move |conn| minimap_store::decisions::update(conn, id, patch).map_err(store_error))
        .await
}

#[tauri::command]
pub async fn archive_decision(state: State<'_, AppState>, id: Uuid) -> Result<(), AppError> {
    state
        .run(move |conn| {
            minimap_store::nodes::archive(conn, NodeRef::new(NodeType::Decision, id))
                .map_err(store_error)
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::edges::add_edge_impl;
    use minimap_store::Connection;
    use minimap_types::{DecisionStatus, EdgeType, NewEdge};

    fn decision(conn: &mut Connection, title: &str) -> Uuid {
        minimap_store::decisions::create(
            conn,
            CreateDecision {
                title: title.into(),
                context: String::new(),
                decision: String::new(),
                rationale: String::new(),
                decided_on: None,
                status: Some(DecisionStatus::Decided),
            },
        )
        .unwrap()
        .id
    }

    fn supersedes(new: Uuid, old: Uuid) -> NewEdge {
        NewEdge {
            edge_type: EdgeType::Supersedes,
            from: NodeRef::new(NodeType::Decision, new),
            to: NodeRef::new(NodeType::Decision, old),
            attrs: serde_json::json!({}),
        }
    }

    #[test]
    fn superseding_marks_the_old_decision_and_lists_its_replacement() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let old = decision(&mut conn, "Use Mongo");
        let new = decision(&mut conn, "Use Postgres");
        add_edge_impl(&mut conn, supersedes(new, old)).unwrap();

        let items = minimap_store::views::decision_items(&conn).unwrap();
        let rows = arrange(items, &DecisionFilter::default());
        let old_row = rows.iter().find(|r| r.id == old).unwrap();
        assert_eq!(old_row.status, DecisionStatus::Superseded);
        assert_eq!(
            old_row.superseded_by.as_ref().unwrap().label,
            "Use Postgres"
        );
        let new_row = rows.iter().find(|r| r.id == new).unwrap();
        assert_eq!(new_row.status, DecisionStatus::Decided);
        assert!(new_row.superseded_by.is_none());
    }

    #[test]
    fn superseding_loops_and_wrong_types_are_rejected_and_change_nothing() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let a = decision(&mut conn, "A");
        let b = decision(&mut conn, "B");
        add_edge_impl(&mut conn, supersedes(b, a)).unwrap();

        let err = add_edge_impl(&mut conn, supersedes(a, b)).unwrap_err();
        assert_eq!(err.code, "cycle");
        assert!(
            err.message.contains("A") && err.message.contains("B"),
            "{}",
            err.message
        );
        // B is still the live one.
        assert_eq!(
            minimap_store::decisions::get(&conn, b).unwrap().status,
            DecisionStatus::Decided
        );

        let mut wrong = supersedes(b, a);
        wrong.to = NodeRef::new(NodeType::Note, a);
        assert_eq!(
            add_edge_impl(&mut conn, wrong).unwrap_err().code,
            "invalid_edge"
        );
    }

    #[test]
    fn linking_twice_is_a_duplicate_and_leaves_the_status_alone() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let old = decision(&mut conn, "Old");
        let new = decision(&mut conn, "New");
        add_edge_impl(&mut conn, supersedes(new, old)).unwrap();
        minimap_store::decisions::update(
            &mut conn,
            old,
            UpdateDecision {
                status: Some(DecisionStatus::Decided),
                ..Default::default()
            },
        )
        .unwrap();
        let err = add_edge_impl(&mut conn, supersedes(new, old)).unwrap_err();
        assert_eq!(err.code, "duplicate");
        assert_eq!(
            minimap_store::decisions::get(&conn, old).unwrap().status,
            DecisionStatus::Decided
        );
    }
}
