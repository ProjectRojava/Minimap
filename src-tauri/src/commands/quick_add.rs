use minimap_core::quick_add::{plan, Context, Outcome};
use minimap_store::Connection;
use minimap_types::{AppError, QuickChoice, QuickPreview, QuickResult};
use tauri::State;

use crate::{
    error::{app_error, store_error},
    state::AppState,
};

fn outcome(conn: &Connection, text: &str, choices: &[QuickChoice]) -> Result<Outcome, AppError> {
    let directory = minimap_store::quick_add::directory(conn).map_err(store_error)?;
    let hours_per_day = minimap_store::settings::get(conn)
        .map_err(store_error)?
        .hours_per_day;
    Ok(plan(
        text,
        &Context {
            today: minimap_store::today(),
            hours_per_day,
            directory: &directory,
            choices,
        },
    ))
}

/// What a line of quick-add would do: what it means, what is wrong, and which names need an
/// answer (`choices` carries the answers so far). Writes nothing.
#[tauri::command]
pub async fn parse_quick_add(
    state: State<'_, AppState>,
    text: String,
    choices: Vec<QuickChoice>,
) -> Result<QuickPreview, AppError> {
    state
        .run(move |conn| Ok(outcome(conn, &text, &choices)?.preview))
        .await
}

/// Creates what the line describes (and any new people, projects... it was told to make), all
/// or nothing. Refused while the preview has problems or unanswered names.
#[tauri::command]
pub async fn commit_quick_add(
    state: State<'_, AppState>,
    text: String,
    choices: Vec<QuickChoice>,
) -> Result<QuickResult, AppError> {
    state
        .run(move |conn| commit_impl(conn, &text, &choices))
        .await
}

pub(crate) fn commit_impl(
    conn: &mut Connection,
    text: &str,
    choices: &[QuickChoice],
) -> Result<QuickResult, AppError> {
    let out = outcome(conn, text, choices)?;
    match out.plan {
        Some(plan) => minimap_store::quick_add::commit(conn, plan).map_err(store_error),
        None => Err(app_error(
            "invalid",
            out.preview
                .problems
                .first()
                .map(String::as_str)
                .unwrap_or("Some names still need an answer"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        CreateObjective, CreatePerson, CreateProject, CreateTask, EdgeType, NodeRef, NodeType,
        QuickMain, QuickPick, QuickPlan, Uuid,
    };

    fn person(conn: &mut Connection, name: &str, me: bool) -> Uuid {
        minimap_store::people::create(
            conn,
            CreatePerson {
                name: name.into(),
                role_title: String::new(),
                email: None,
                weekly_capacity_hours: None,
                is_self: me,
                notes: String::new(),
            },
        )
        .unwrap()
        .id
    }

    fn project(conn: &mut Connection, title: &str) -> Uuid {
        minimap_store::projects::create(
            conn,
            CreateProject {
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

    fn task(conn: &mut Connection, title: &str) -> Uuid {
        minimap_store::tasks::create(
            conn,
            CreateTask {
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

    fn seeded() -> (Connection, Uuid, Uuid, Uuid, Uuid) {
        let mut conn = minimap_store::open_in_memory().unwrap();
        let me = person(&mut conn, "Uday Lamba", true);
        let priya = person(&mut conn, "Priya Shah", false);
        let raj = person(&mut conn, "Raj Patel", false);
        let api = project(&mut conn, "API Launch");
        task(&mut conn, "Release 1.2");
        minimap_store::objectives::create(
            &mut conn,
            CreateObjective {
                ongoing: false,
                review_every_days: None,
                title: "Launch EU".into(),
                description: String::new(),
                target_date: None,
                status: None,
                priority: None,
            },
        )
        .unwrap();
        (conn, me, priya, raj, api)
    }

    fn links(conn: &Connection, id: Uuid) -> Vec<(EdgeType, bool, String)> {
        minimap_store::edges::links_for_node(conn, id)
            .unwrap()
            .into_iter()
            .map(|l| (l.edge.edge_type, l.outgoing, l.other.label))
            .collect()
    }

    #[test]
    fn the_task_example_creates_a_linked_task() {
        let (mut conn, _, _, _, _) = seeded();
        let r = commit_impl(
            &mut conn,
            r#"task Fix login timeout @priya #api-launch !2 due:2027-03-05 est:3d blocks:"Release 1.2""#,
            &[],
        )
        .unwrap();
        assert_eq!(r.node.node.node_type, NodeType::Task);
        assert!(r.created.is_empty());
        let t = minimap_store::tasks::get(&conn, r.node.node.id).unwrap();
        assert_eq!(t.title, "Fix login timeout");
        assert_eq!(t.priority, 2);
        assert_eq!(t.estimate_days, Some(3.0));
        assert_eq!(t.due_date.unwrap().to_string(), "2027-03-05");
        let project = minimap_store::projects::get(&conn, t.project_id.unwrap()).unwrap();
        assert_eq!(project.title, "API Launch");
        let mut l = links(&conn, t.id);
        l.sort_by_key(|(e, _, _)| e.as_str());
        assert_eq!(
            l,
            vec![
                (EdgeType::AssignedTo, true, "Priya Shah".to_owned()),
                (EdgeType::Blocks, true, "Release 1.2".to_owned()),
            ]
        );
    }

    #[test]
    fn a_bare_task_goes_to_the_inbox_assigned_to_me() {
        let (mut conn, me, _, _, _) = seeded();
        let r = commit_impl(&mut conn, "Write the plan", &[]).unwrap();
        let t = minimap_store::tasks::get(&conn, r.node.node.id).unwrap();
        assert_eq!(t.project_id, None);
        let assigned = minimap_store::edges::links_for_node(&conn, t.id).unwrap();
        assert_eq!(assigned.len(), 1);
        assert_eq!(assigned[0].other.node.id, me);
    }

    #[test]
    fn the_other_examples_commit() {
        let (mut conn, me, priya, raj, _api) = seeded();

        let r = commit_impl(
            &mut conn,
            r#"project Q1 EU region owner:@me target:2027-03-31 for:"Launch EU""#,
            &[],
        )
        .unwrap();
        let p = minimap_store::projects::get(&conn, r.node.node.id).unwrap();
        assert_eq!(p.owner_person_id, Some(me));
        assert_eq!(p.slug, "q1-eu-region");
        assert_eq!(
            links(&conn, p.id),
            vec![(EdgeType::ContributesTo, true, "Launch EU".to_owned())]
        );

        let r = commit_impl(
            &mut conn,
            r#"wait @raj on "Security review sign-off" by:2027-03-10 #api-launch"#,
            &[],
        )
        .unwrap();
        let w = minimap_store::waiting_on::get(&conn, r.node.node.id).unwrap();
        assert_eq!(
            (w.person_id, w.description.as_str()),
            (raj, "Security review sign-off")
        );
        assert_eq!(w.expected_by.unwrap().to_string(), "2027-03-10");
        assert_eq!(
            links(&conn, w.id),
            vec![(EdgeType::About, true, "API Launch".to_owned())]
        );

        let r = commit_impl(&mut conn, "note 1:1 @priya", &[]).unwrap();
        let n = minimap_store::notes::get(&conn, r.node.node.id).unwrap();
        assert_eq!(n.title, "1:1 with Priya Shah");
        assert!(n.body.contains(&priya.to_string()));
        assert_eq!(
            links(&conn, n.id),
            vec![(EdgeType::Mentions, true, "Priya Shah".to_owned())]
        );

        let r = commit_impl(
            &mut conn,
            r#"decision "Postgres over Mongo" affects:#api-launch"#,
            &[],
        )
        .unwrap();
        let d = minimap_store::decisions::get(&conn, r.node.node.id).unwrap();
        assert_eq!(d.title, "Postgres over Mongo");
        assert_eq!(
            links(&conn, d.id),
            vec![(EdgeType::Affects, true, "API Launch".to_owned())]
        );
    }

    #[test]
    fn unclear_lines_are_refused_and_write_nothing() {
        let (mut conn, ..) = seeded();
        let before = minimap_store::tasks::list(&conn, true).unwrap().len();
        // Ambiguity (two Priyas) and a missing name.
        person(&mut conn, "Priyanka Rao", false);
        for text in [
            "task x @pri",
            "task x @nobody",
            "wait Review",
            "task",
            "task x due:someday",
        ] {
            assert_eq!(
                commit_impl(&mut conn, text, &[]).unwrap_err().code,
                "invalid",
                "{text}"
            );
        }
        assert_eq!(
            minimap_store::tasks::list(&conn, true).unwrap().len(),
            before
        );
    }

    #[test]
    fn answers_create_or_pick_and_new_names_are_made_with_the_node() {
        let (mut conn, ..) = seeded();
        let priyanka = person(&mut conn, "Priyanka Rao", false);
        let choices = vec![
            QuickChoice {
                key: "Assignee:pri".into(),
                pick: QuickPick::Node(priyanka),
            },
            QuickChoice {
                key: "Project:moonshot".into(),
                pick: QuickPick::Create,
            },
        ];
        let r = commit_impl(&mut conn, "task Ship @pri #moonshot", &choices).unwrap();
        assert_eq!(r.created.len(), 1);
        assert_eq!(
            (r.created[0].node.node_type, r.created[0].label.as_str()),
            (NodeType::Project, "moonshot")
        );
        let t = minimap_store::tasks::get(&conn, r.node.node.id).unwrap();
        let made = minimap_store::projects::get(&conn, t.project_id.unwrap()).unwrap();
        assert_eq!(made.title, "moonshot");
        assert_eq!(links(&conn, t.id)[0].2, "Priyanka Rao");
    }

    #[test]
    fn a_failed_commit_creates_nothing_at_all() {
        let (mut conn, ..) = seeded();
        let people_before = minimap_store::people::list(&conn, true).unwrap().len();
        let plan = QuickPlan {
            new_nodes: vec![minimap_types::NewNode {
                node_type: NodeType::Person,
                name: "Sam".into(),
            }],
            // A blank title fails validation after Sam has been created.
            main: QuickMain::Task {
                title: "  ".into(),
                priority: None,
                start_date: None,
                due_date: None,
                estimate_days: None,
                project: None,
                assignee: minimap_types::QuickAssignee::Person(minimap_types::Ref::New(0)),
                blocks: vec![],
                objectives: vec![],
                recurrence: None,
            },
        };
        assert!(minimap_store::quick_add::commit(&mut conn, plan).is_err());
        assert_eq!(
            minimap_store::people::list(&conn, true).unwrap().len(),
            people_before
        );
    }

    #[test]
    fn previews_follow_the_settings_and_the_directory() {
        let (conn, ..) = seeded();
        let out = outcome(&conn, "task x @priya est:4h", &[]).unwrap();
        assert!(out.preview.ready);
        let hours = out
            .preview
            .details
            .iter()
            .find(|d| d.label == "Estimate")
            .unwrap();
        assert_eq!(hours.value, "0.5d"); // default 8 hours per day
                                         // Archived people are not offered.
        let mut conn = conn;
        let priya = minimap_store::people::list(&conn, false)
            .unwrap()
            .into_iter()
            .find(|p| p.name == "Priya Shah")
            .unwrap();
        minimap_store::nodes::archive(&mut conn, NodeRef::new(NodeType::Person, priya.id)).unwrap();
        assert!(!outcome(&conn, "task x @priya", &[]).unwrap().preview.ready);
    }
}
