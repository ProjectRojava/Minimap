//! Zoom (spec 52, docs/product-design/09-semantic-zoom.md). Thin: the rules are
//! `minimap_core::zoom`. Nothing is stored.

use minimap_core::zoom::{path_of, ZoomInput};
use minimap_store::Connection;
use minimap_types::{AppError, NodeRef, NodeSummary, NodeType, ZoomStep};
use tauri::State;

use crate::{error::store_error, state::AppState};

/// The way from the whole portfolio down to `node`: World first, the node last, one level deeper
/// at every step (a missing objective or project is skipped).
#[tauri::command]
pub async fn get_zoom_path(
    state: State<'_, AppState>,
    node: NodeRef,
) -> Result<Vec<ZoomStep>, AppError> {
    state.run(move |conn| zoom_path_impl(conn, node)).await
}

pub(crate) fn zoom_path_impl(conn: &Connection, node: NodeRef) -> Result<Vec<ZoomStep>, AppError> {
    // The objectives, projects and tasks name themselves; any other kind is named here.
    let others: Vec<NodeSummary> = match node.node_type {
        NodeType::Objective | NodeType::Project | NodeType::Task => Vec::new(),
        _ => vec![minimap_store::nodes::summary(conn, node).map_err(store_error)?],
    };
    // Archived ones are included so an archived item can still name itself; the rules skip them
    // as steps above something else.
    let tasks = minimap_store::tasks::list(conn, true).map_err(store_error)?;
    let projects = minimap_store::projects::list(conn, true).map_err(store_error)?;
    let objectives = minimap_store::objectives::list(conn, true).map_err(store_error)?;
    let edges = minimap_store::edges::list_active(conn).map_err(store_error)?;
    Ok(path_of(
        node,
        &ZoomInput {
            tasks: &tasks,
            projects: &projects,
            objectives: &objectives,
            edges: &edges,
            others: &others,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::Level;

    fn labels(path: &[ZoomStep]) -> Vec<&str> {
        path.iter().map(|s| s.label()).collect()
    }

    #[test]
    fn an_eu_region_task_zooms_out_through_its_project_and_objective_in_the_demo_data() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        minimap_store::demo::seed(&mut conn, time::macros::date!(2027 - 03 - 03)).unwrap();
        let task = minimap_store::tasks::list(&conn, false)
            .unwrap()
            .into_iter()
            .find(|t| t.title == "Provision EU network and clusters")
            .unwrap();
        let path = zoom_path_impl(&conn, NodeRef::new(NodeType::Task, task.id)).unwrap();
        assert_eq!(
            labels(&path),
            [
                "World",
                "Launch in the EU",
                "EU Region",
                "Provision EU network and clusters"
            ]
        );
        let levels: Vec<_> = path.iter().map(|s| s.level).collect();
        assert_eq!(
            levels,
            [Level::World, Level::Objective, Level::Project, Level::Item]
        );
    }

    #[test]
    fn a_note_in_the_demo_data_is_named_and_sits_below_what_it_mentions() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        minimap_store::demo::seed(&mut conn, time::macros::date!(2027 - 03 - 03)).unwrap();
        let notes = minimap_store::notes::list(&conn, false).unwrap();
        for note in notes {
            let path = zoom_path_impl(&conn, NodeRef::new(NodeType::Note, note.id)).unwrap();
            assert_eq!(path.first().map(|s| s.level), Some(Level::World));
            let last = path.last().unwrap();
            assert_eq!(last.level, Level::Item);
            assert_eq!(last.label(), note.title);
        }
    }

    #[test]
    fn an_unknown_note_is_not_found() {
        let conn = minimap_store::open_in_memory().unwrap();
        let err = zoom_path_impl(
            &conn,
            NodeRef::new(NodeType::Note, minimap_types::Uuid::from_u128(1)),
        )
        .unwrap_err();
        assert_eq!(err.code, "not_found");
    }
}
