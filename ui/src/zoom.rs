//! Zoom (spec 52, docs/product-design/09-semantic-zoom.md): which screen and which open item each
//! step of a path means, and the keys that move along it. Pure, so it is unit-tested natively;
//! `state::ZoomCtx` does the moving.

use minimap_types::{Level, NodeRef, NodeType, Uuid, ZoomStep};

use crate::nav::list_path;

/// The screen the World step is.
pub const WORLD_ROUTE: &str = "/overview";

/// The Tasks board filtered to a project.
pub fn project_route(project: Uuid) -> String {
    format!("/tasks?project={project}")
}

/// The path part of a route (`/tasks?project=x` -> `/tasks`).
pub fn path_of_route(route: &str) -> &str {
    route.split_once('?').map_or(route, |(path, _)| path)
}

/// The project a route filters the Tasks board to, if it is one: `/tasks?project=<id>`.
pub fn project_of_route(route: &str) -> Option<Uuid> {
    let (path, query) = route.split_once('?')?;
    if path != "/tasks" {
        return None;
    }
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(key, _)| *key == "project")
        .and_then(|(_, value)| Uuid::parse_str(value).ok())
}

/// A location (`pathname` and `search` as the router gives them, the search with or without its
/// `?`) written as one route.
pub fn join_route(pathname: &str, search: &str) -> String {
    let search = search.trim_start_matches('?');
    if search.is_empty() {
        pathname.to_owned()
    } else {
        format!("{pathname}?{search}")
    }
}

/// Where a step "is": the screen to show and the item to open in the detail pane (none closes
/// it).
/// - World: the Overview, nothing open.
/// - Objective: the Objectives screen with the objective open.
/// - Project: the Tasks board filtered to the project, with the project open.
/// - Item: the screen you are on if the item is on it (`visible_here`), else the item's own list
///   screen; the item is open.
pub fn route_for(step: &ZoomStep, current: &str, visible_here: bool) -> (String, Option<NodeRef>) {
    let node = step.node.as_ref().map(|n| n.node);
    match (step.level, node) {
        (Level::World, _) | (_, None) => (WORLD_ROUTE.to_owned(), None),
        (Level::Objective, Some(n)) => ("/objectives".to_owned(), Some(n)),
        (Level::Project, Some(n)) => (project_route(n.id), Some(n)),
        (Level::Item, Some(n)) => {
            let route = if visible_here {
                current.to_owned()
            } else {
                list_path(n.node_type).to_owned()
            };
            (route, Some(n))
        }
    }
}

/// The step one level out from the item a path ends at: the one before the last.
pub fn out_target(path: &[ZoomStep]) -> Option<&ZoomStep> {
    path.len().checked_sub(2).and_then(|i| path.get(i))
}

/// The step Alt+Down goes to when there is nothing to return to: the highlighted row's own step
/// (the last of its path), unless that is what you are already looking at.
pub fn in_target(focus: Option<NodeRef>, cursor_path: &[ZoomStep]) -> Option<&ZoomStep> {
    let last = cursor_path.last()?;
    let node = last.node.as_ref().map(|n| n.node);
    (node != focus).then_some(last)
}

/// What the item to zoom from is: the item open in the detail pane, else the project the screen
/// is filtered to, else nothing (the whole portfolio).
pub fn focus_of(selection: Option<NodeRef>, page_project: Option<Uuid>) -> Option<NodeRef> {
    selection.or_else(|| page_project.map(|id| NodeRef::new(NodeType::Project, id)))
}

/// Which way a key moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoomKey {
    Out,
    In,
}

/// `Alt+ArrowUp` zooms out and `Alt+ArrowDown` zooms in. With any other modifier (Ctrl, Cmd,
/// Shift) the keys are not for zoom: Cmd+Up and Cmd+Down belong to the text and the system.
pub fn zoom_key(key: &str, ctrl: bool, meta: bool, shift: bool, alt: bool) -> Option<ZoomKey> {
    if !alt || ctrl || meta || shift {
        return None;
    }
    match key {
        "ArrowUp" => Some(ZoomKey::Out),
        "ArrowDown" => Some(ZoomKey::In),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::NodeSummary;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn step(level: Level, node_type: NodeType, n: u128, label: &str) -> ZoomStep {
        ZoomStep::of(
            level,
            NodeSummary {
                node: NodeRef::new(node_type, id(n)),
                label: label.into(),
                archived: false,
            },
        )
    }

    fn path() -> Vec<ZoomStep> {
        vec![
            ZoomStep::world(),
            step(Level::Objective, NodeType::Objective, 1, "Launch in the EU"),
            step(Level::Project, NodeType::Project, 2, "EU Region"),
            step(Level::Item, NodeType::Task, 3, "Gateway auth"),
        ]
    }

    #[test]
    fn each_level_is_a_screen_and_an_open_item() {
        let p = path();
        assert_eq!(
            route_for(&p[0], "/tasks", false),
            ("/overview".into(), None)
        );
        assert_eq!(
            route_for(&p[1], "/tasks", false),
            (
                "/objectives".into(),
                Some(NodeRef::new(NodeType::Objective, id(1)))
            )
        );
        assert_eq!(
            route_for(&p[2], "/overview", false),
            (
                format!("/tasks?project={}", id(2)),
                Some(NodeRef::new(NodeType::Project, id(2)))
            )
        );
        // An item stays on the screen it is visible on, else goes to its own list.
        let task = NodeRef::new(NodeType::Task, id(3));
        assert_eq!(
            route_for(&p[3], "/tasks?project=x", true),
            ("/tasks?project=x".into(), Some(task))
        );
        assert_eq!(
            route_for(&p[3], "/overview", false),
            ("/tasks".into(), Some(task))
        );
    }

    #[test]
    fn a_note_goes_to_the_notes_screen_when_it_is_not_in_view() {
        let note = step(Level::Item, NodeType::Note, 9, "1:1");
        assert_eq!(route_for(&note, "/overview", false).0, "/notes");
    }

    #[test]
    fn a_project_route_reads_back() {
        let route = project_route(id(7));
        assert_eq!(project_of_route(&route), Some(id(7)));
        assert_eq!(path_of_route(&route), "/tasks");
        assert_eq!(path_of_route("/objectives"), "/objectives");
        // Only the Tasks board takes the filter; nonsense is nothing.
        assert_eq!(project_of_route("/tasks"), None);
        assert_eq!(project_of_route("/tasks?project=nope"), None);
        assert_eq!(
            project_of_route(&format!("/projects?project={}", id(7))),
            None
        );
        assert_eq!(
            project_of_route(&format!("/tasks?person=x&project={}", id(7))),
            Some(id(7))
        );
    }

    #[test]
    fn a_location_is_joined_with_or_without_the_question_mark() {
        assert_eq!(join_route("/tasks", ""), "/tasks");
        assert_eq!(join_route("/tasks", "project=x"), "/tasks?project=x");
        assert_eq!(join_route("/tasks", "?project=x"), "/tasks?project=x");
    }

    #[test]
    fn zooming_out_goes_one_step_up_the_path() {
        let p = path();
        assert_eq!(out_target(&p).map(|s| s.level), Some(Level::Project));
        assert_eq!(out_target(&p[..3]).map(|s| s.level), Some(Level::Objective));
        assert_eq!(out_target(&p[..2]).map(|s| s.level), Some(Level::World));
        // Already the whole portfolio: nowhere to go.
        assert_eq!(out_target(&p[..1]), None);
        assert_eq!(out_target(&[]), None);
    }

    #[test]
    fn zooming_in_goes_to_the_row_unless_it_is_already_the_focus() {
        let p = path();
        let task = NodeRef::new(NodeType::Task, id(3));
        let project = NodeRef::new(NodeType::Project, id(2));
        assert_eq!(in_target(None, &p).map(|s| s.level), Some(Level::Item));
        assert_eq!(
            in_target(Some(project), &p).map(|s| s.level),
            Some(Level::Item)
        );
        assert_eq!(in_target(Some(task), &p), None);
        assert_eq!(in_target(None, &[]), None);
    }

    #[test]
    fn the_focus_is_the_open_item_else_the_filtered_project() {
        let task = NodeRef::new(NodeType::Task, id(3));
        assert_eq!(focus_of(Some(task), Some(id(2))), Some(task));
        assert_eq!(
            focus_of(None, Some(id(2))),
            Some(NodeRef::new(NodeType::Project, id(2)))
        );
        assert_eq!(focus_of(None, None), None);
    }

    #[test]
    fn only_alt_with_an_up_or_down_arrow_zooms() {
        use ZoomKey::{In, Out};
        assert_eq!(zoom_key("ArrowUp", false, false, false, true), Some(Out));
        assert_eq!(zoom_key("ArrowDown", false, false, false, true), Some(In));
        assert_eq!(zoom_key("ArrowUp", false, false, false, false), None);
        assert_eq!(zoom_key("ArrowUp", true, false, false, true), None);
        assert_eq!(zoom_key("ArrowDown", false, true, false, true), None);
        assert_eq!(zoom_key("ArrowDown", false, false, true, true), None);
        assert_eq!(zoom_key("ArrowLeft", false, false, false, true), None);
    }
}
