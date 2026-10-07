use minimap_core::{cycles::find_cycle, edge_rules};
use minimap_store::Connection;
use minimap_types::{AppError, Edge, EdgeType, LinkOption, NewEdge, NodeRef, NodeType, Uuid};
use tauri::State;

use crate::{
    error::{cycle_error, rule_error, store_error},
    state::AppState,
};

/// Readable name for a node, falling back to its id.
pub(crate) fn label(conn: &Connection, node_type: NodeType, id: Uuid) -> String {
    minimap_store::nodes::summary(conn, NodeRef::new(node_type, id))
        .map(|s| s.label)
        .unwrap_or_else(|_| id.to_string())
}

/// Matrix, attribute and cycle rules for a new edge.
pub(crate) fn check_new_edge(conn: &Connection, new: &NewEdge) -> Result<(), AppError> {
    edge_rules::validate(new.edge_type, new.from, new.to, &new.attrs).map_err(rule_error)?;
    // `relates_to` has no direction: A related to B and B related to A are the same link.
    if new.edge_type == EdgeType::RelatesTo {
        let reverse = minimap_store::edges::list_active_of_type(conn, EdgeType::RelatesTo)
            .map_err(store_error)?
            .into_iter()
            .any(|e| e.from_id == new.to.id && e.to_id == new.from.id);
        if reverse {
            return Err(store_error(minimap_store::StoreError::DuplicateEdge));
        }
    }
    if !edge_rules::must_be_acyclic(new.edge_type) {
        return Ok(());
    }
    let pairs: Vec<(Uuid, Uuid)> = minimap_store::edges::list_active_of_type(conn, new.edge_type)
        .map_err(store_error)?
        .into_iter()
        .map(|e| (e.from_id, e.to_id))
        .collect();
    if let Some(cycle) = find_cycle(&pairs, new.from.id, new.to.id) {
        let labels: Vec<String> = cycle
            .iter()
            .map(|id| label(conn, new.from.node_type, *id))
            .collect();
        return Err(cycle_error("add this link", &labels));
    }
    Ok(())
}

/// The relations (and their attributes) that can be added from a node of this type.
#[tauri::command]
pub async fn list_link_options(node_type: NodeType) -> Result<Vec<LinkOption>, AppError> {
    Ok(edge_rules::link_options(node_type))
}

#[tauri::command]
pub async fn add_edge(state: State<'_, AppState>, new: NewEdge) -> Result<Edge, AppError> {
    state.run(move |conn| add_edge_impl(conn, new)).await
}

pub(crate) fn add_edge_impl(conn: &mut Connection, new: NewEdge) -> Result<Edge, AppError> {
    check_new_edge(conn, &new)?;
    if new.edge_type == EdgeType::Supersedes {
        // Also marks the older decision as superseded, in the same transaction.
        return minimap_store::decisions::supersede(conn, new.from.id, new.to.id)
            .map_err(store_error);
    }
    minimap_store::edges::add(conn, new).map_err(store_error)
}

/// Changes a link's attributes (e.g. a contribution's weight) after validating them.
#[tauri::command]
pub async fn update_edge_attrs(
    state: State<'_, AppState>,
    edge_id: Uuid,
    attrs: serde_json::Value,
) -> Result<Edge, AppError> {
    state
        .run(move |conn| update_edge_attrs_impl(conn, edge_id, attrs))
        .await
}

pub(crate) fn update_edge_attrs_impl(
    conn: &mut Connection,
    edge_id: Uuid,
    attrs: serde_json::Value,
) -> Result<Edge, AppError> {
    let edge = minimap_store::edges::get(conn, edge_id).map_err(store_error)?;
    edge_rules::validate_attrs(edge.edge_type, &attrs).map_err(rule_error)?;
    minimap_store::edges::update_attrs(conn, edge_id, attrs).map_err(store_error)
}

#[tauri::command]
pub async fn remove_edge(state: State<'_, AppState>, edge_id: Uuid) -> Result<(), AppError> {
    state
        .run(move |conn| minimap_store::edges::remove(conn, edge_id).map_err(store_error))
        .await
}

/// Sets (or clears, with `None`) a person's single manager.
#[tauri::command]
pub async fn set_manager(
    state: State<'_, AppState>,
    person_id: Uuid,
    manager_id: Option<Uuid>,
) -> Result<(), AppError> {
    state
        .run(move |conn| set_manager_impl(conn, person_id, manager_id))
        .await
}

pub(crate) fn set_manager_impl(
    conn: &mut Connection,
    person_id: Uuid,
    manager_id: Option<Uuid>,
) -> Result<(), AppError> {
    let person = NodeRef::new(NodeType::Person, person_id);
    minimap_store::people::get(conn, person_id).map_err(store_error)?;
    let current = minimap_store::edges::list_active_of_type(conn, EdgeType::ReportsTo)
        .map_err(store_error)?
        .into_iter()
        .find(|e| e.from_id == person_id);

    let new = manager_id.map(|m| NewEdge {
        edge_type: EdgeType::ReportsTo,
        from: person,
        to: NodeRef::new(NodeType::Person, m),
        attrs: serde_json::json!({}),
    });
    if let (Some(c), Some(n)) = (&current, &new) {
        if c.to_id == n.to.id {
            return Ok(()); // unchanged
        }
    }
    if let Some(n) = &new {
        check_new_edge(conn, n)?;
    }
    if let Some(c) = current {
        minimap_store::edges::remove(conn, c.id).map_err(store_error)?;
    }
    if let Some(n) = new {
        minimap_store::edges::add(conn, n)
            .map(|_| ())
            .map_err(store_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{CreatePerson, Person};

    fn person(conn: &mut Connection, name: &str) -> Person {
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
    }

    fn manager_of(conn: &Connection, id: Uuid) -> Option<Uuid> {
        minimap_store::edges::list_active_of_type(conn, EdgeType::ReportsTo)
            .unwrap()
            .into_iter()
            .find(|e| e.from_id == id)
            .map(|e| e.to_id)
    }

    #[test]
    fn reports_to_cycle_is_rejected_with_the_path() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let (a, b, c) = (
            person(&mut conn, "Ann"),
            person(&mut conn, "Bob"),
            person(&mut conn, "Cy"),
        );
        set_manager_impl(&mut conn, a.id, Some(b.id)).unwrap(); // Ann reports to Bob
        set_manager_impl(&mut conn, b.id, Some(c.id)).unwrap(); // Bob reports to Cy

        let err = set_manager_impl(&mut conn, c.id, Some(a.id)).unwrap_err();
        assert_eq!(err.code, "cycle");
        assert!(
            err.message.contains("Cy → Ann → Bob → Cy"),
            "{}",
            err.message
        );
        // Nothing changed.
        assert_eq!(manager_of(&conn, c.id), None);
        assert_eq!(manager_of(&conn, a.id), Some(b.id));
    }

    #[test]
    fn self_report_and_wrong_types_are_rejected() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let a = person(&mut conn, "Ann");
        assert_eq!(
            set_manager_impl(&mut conn, a.id, Some(a.id))
                .unwrap_err()
                .code,
            "invalid_edge"
        );
    }

    #[test]
    fn changing_and_clearing_a_manager_replaces_the_edge() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let (a, b, c) = (
            person(&mut conn, "Ann"),
            person(&mut conn, "Bob"),
            person(&mut conn, "Cy"),
        );
        set_manager_impl(&mut conn, a.id, Some(b.id)).unwrap();
        set_manager_impl(&mut conn, a.id, Some(b.id)).unwrap(); // no-op
        set_manager_impl(&mut conn, a.id, Some(c.id)).unwrap();
        assert_eq!(manager_of(&conn, a.id), Some(c.id));
        // Swapping the direction of an existing relationship is not a loop: the old edge is replaced.
        set_manager_impl(&mut conn, c.id, Some(a.id)).unwrap_err(); // a->c exists, c->a would loop
        set_manager_impl(&mut conn, a.id, None).unwrap();
        assert_eq!(manager_of(&conn, a.id), None);
        set_manager_impl(&mut conn, c.id, Some(a.id)).unwrap();
        assert_eq!(manager_of(&conn, c.id), Some(a.id));
    }

    #[test]
    fn replacing_a_manager_drops_the_old_link_from_loop_checks() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let (a, b, c) = (
            person(&mut conn, "Ann"),
            person(&mut conn, "Bob"),
            person(&mut conn, "Cy"),
        );
        set_manager_impl(&mut conn, a.id, Some(b.id)).unwrap();
        // While Ann reports to Bob, Bob reporting to Ann would loop.
        assert_eq!(
            set_manager_impl(&mut conn, b.id, Some(a.id))
                .unwrap_err()
                .code,
            "cycle"
        );
        // Once Ann is re-pointed to Cy, the same request is fine.
        set_manager_impl(&mut conn, a.id, Some(c.id)).unwrap();
        set_manager_impl(&mut conn, b.id, Some(a.id)).unwrap();
        assert_eq!(manager_of(&conn, b.id), Some(a.id));
        assert_eq!(manager_of(&conn, a.id), Some(c.id));
    }

    #[test]
    fn edge_attrs_are_validated_before_saving() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = person(&mut conn, "Ann");
        let team = minimap_store::teams::create(
            &mut conn,
            minimap_types::CreateTeam {
                name: "Platform".into(),
                description: String::new(),
                parent_team_id: None,
            },
        )
        .unwrap();
        let edge = minimap_store::edges::add(
            &mut conn,
            NewEdge {
                edge_type: EdgeType::MemberOf,
                from: NodeRef::new(NodeType::Person, p.id),
                to: NodeRef::new(NodeType::Team, team.id),
                attrs: serde_json::json!({"role": "member"}),
            },
        )
        .unwrap();

        let ok = update_edge_attrs_impl(&mut conn, edge.id, serde_json::json!({"role": "lead"}))
            .unwrap();
        assert_eq!(ok.attrs, serde_json::json!({"role": "lead"}));
        for bad in [
            serde_json::json!({"role": "boss"}),
            serde_json::json!({"weight": 0.5}),
            serde_json::json!([]),
        ] {
            let err = update_edge_attrs_impl(&mut conn, edge.id, bad).unwrap_err();
            assert_eq!(err.code, "invalid_edge");
        }
        // Rejected updates changed nothing.
        assert_eq!(
            minimap_store::edges::get(&conn, edge.id).unwrap().attrs,
            serde_json::json!({"role": "lead"})
        );
        assert_eq!(
            update_edge_attrs_impl(&mut conn, Uuid::now_v7(), serde_json::json!({}))
                .unwrap_err()
                .code,
            "not_found"
        );
    }

    #[test]
    fn membership_attrs_are_validated() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let p = person(&mut conn, "Ann");
        let team = minimap_store::teams::create(
            &mut conn,
            minimap_types::CreateTeam {
                name: "Platform".into(),
                description: String::new(),
                parent_team_id: None,
            },
        )
        .unwrap();
        let edge = |attrs| NewEdge {
            edge_type: EdgeType::MemberOf,
            from: NodeRef::new(NodeType::Person, p.id),
            to: NodeRef::new(NodeType::Team, team.id),
            attrs,
        };
        assert!(check_new_edge(&conn, &edge(serde_json::json!({"role": "lead"}))).is_ok());
        assert_eq!(
            check_new_edge(&conn, &edge(serde_json::json!({"role": "king"})))
                .unwrap_err()
                .code,
            "invalid_edge"
        );
        // A person cannot be a member of a person.
        let wrong = NewEdge {
            edge_type: EdgeType::MemberOf,
            from: NodeRef::new(NodeType::Person, p.id),
            to: NodeRef::new(NodeType::Person, Uuid::now_v7()),
            attrs: serde_json::json!({}),
        };
        assert_eq!(
            check_new_edge(&conn, &wrong).unwrap_err().code,
            "invalid_edge"
        );
    }

    fn task(conn: &mut Connection, title: &str) -> Uuid {
        minimap_store::tasks::create(
            conn,
            minimap_types::CreateTask {
                links: Vec::new(),
                title: title.into(),
                assignee: minimap_types::AssigneeChoice::Nobody,
                description: String::new(),
                project_id: None,
                status: None,
                estimate_days: None,
                start_date: None,
                due_date: None,
                priority: None,
                recurrence: None,
            },
        )
        .unwrap()
        .id
    }

    fn project(conn: &mut Connection, title: &str) -> Uuid {
        minimap_store::projects::create(
            conn,
            minimap_types::CreateProject {
                title: title.into(),
                slug: None,
                description: String::new(),
                owner_person_id: None,
                start_date: None,
                target_date: None,
                status: None,
                priority: None,
            },
        )
        .unwrap()
        .id
    }

    fn link(kind: EdgeType, from: (NodeType, Uuid), to: (NodeType, Uuid)) -> NewEdge {
        NewEdge {
            edge_type: kind,
            from: NodeRef::new(from.0, from.1),
            to: NodeRef::new(to.0, to.1),
            attrs: serde_json::json!({}),
        }
    }

    fn add(conn: &mut Connection, new: NewEdge) -> Result<Edge, AppError> {
        check_new_edge(conn, &new)?;
        minimap_store::edges::add(conn, new).map_err(store_error)
    }

    #[test]
    fn task_loops_are_rejected_with_the_task_titles() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let (deploy, qa, fix) = (
            task(&mut conn, "Deploy"),
            task(&mut conn, "QA sign-off"),
            task(&mut conn, "Fix login"),
        );
        let t = |id| (NodeType::Task, id);
        add(&mut conn, link(EdgeType::Blocks, t(deploy), t(qa))).unwrap();
        add(&mut conn, link(EdgeType::Blocks, t(qa), t(fix))).unwrap();
        let err = add(&mut conn, link(EdgeType::Blocks, t(fix), t(deploy))).unwrap_err();
        assert_eq!(err.code, "cycle");
        assert_eq!(
            err.message,
            "Can't add this link: this would create a loop — Fix login → Deploy → QA sign-off → Fix login"
        );
        // Self-link and wrong types.
        assert_eq!(
            add(&mut conn, link(EdgeType::Blocks, t(deploy), t(deploy)))
                .unwrap_err()
                .code,
            "invalid_edge"
        );
        let p = project(&mut conn, "P");
        assert_eq!(
            add(
                &mut conn,
                link(EdgeType::Blocks, t(deploy), (NodeType::Project, p))
            )
            .unwrap_err()
            .code,
            "invalid_edge"
        );
        // Blocking is across any tasks (no project needed), and a task blocked twice is fine.
        add(&mut conn, link(EdgeType::Blocks, t(deploy), t(fix))).unwrap();
        // Nothing changed after the rejections.
        assert_eq!(
            minimap_store::edges::list_active_of_type(&conn, EdgeType::Blocks)
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn project_dependency_loops_are_rejected() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let (a, b, c) = (
            project(&mut conn, "Alpha"),
            project(&mut conn, "Beta"),
            project(&mut conn, "Gamma"),
        );
        let p = |id| (NodeType::Project, id);
        add(&mut conn, link(EdgeType::DependsOn, p(a), p(b))).unwrap();
        add(&mut conn, link(EdgeType::DependsOn, p(b), p(c))).unwrap();
        let err = add(&mut conn, link(EdgeType::DependsOn, p(c), p(a))).unwrap_err();
        assert_eq!(err.code, "cycle");
        assert!(
            err.message.contains("Gamma → Alpha → Beta → Gamma"),
            "{}",
            err.message
        );
        // A loop through a removed edge is fine again.
        let ab = minimap_store::edges::list_active_of_type(&conn, EdgeType::DependsOn)
            .unwrap()
            .into_iter()
            .find(|e| e.from_id == a)
            .unwrap();
        minimap_store::edges::remove(&mut conn, ab.id).unwrap();
        add(&mut conn, link(EdgeType::DependsOn, p(c), p(a))).unwrap();
    }

    #[test]
    fn relates_to_links_anything_to_anything_once() {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let (t, p) = (task(&mut conn, "T"), project(&mut conn, "P"));
        let (tt, pp) = ((NodeType::Task, t), (NodeType::Project, p));
        let first = add(&mut conn, link(EdgeType::RelatesTo, tt, pp)).unwrap();
        // The same pair in either direction is the same link.
        assert_eq!(
            add(&mut conn, link(EdgeType::RelatesTo, tt, pp))
                .unwrap_err()
                .code,
            "duplicate"
        );
        assert_eq!(
            add(&mut conn, link(EdgeType::RelatesTo, pp, tt))
                .unwrap_err()
                .code,
            "duplicate"
        );
        // Not acyclic-checked, and takes a note.
        let mut with_note = link(
            EdgeType::RelatesTo,
            tt,
            (NodeType::Task, task(&mut conn, "Other")),
        );
        with_note.attrs = serde_json::json!({ "note": "same customer" });
        add(&mut conn, with_note).unwrap();
        // After removing it, the reverse direction can be added.
        minimap_store::edges::remove(&mut conn, first.id).unwrap();
        add(&mut conn, link(EdgeType::RelatesTo, pp, tt)).unwrap();
        // Self-links stay rejected.
        assert_eq!(
            add(&mut conn, link(EdgeType::RelatesTo, tt, tt))
                .unwrap_err()
                .code,
            "invalid_edge"
        );
    }
}
