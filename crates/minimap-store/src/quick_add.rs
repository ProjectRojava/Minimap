//! Commits a quick-add plan: everything it creates, and every link, in one transaction.
//! Parsing and resolving names is `minimap-core::quick_add`.

use minimap_types::{
    mention_token, AssigneeChoice, CreateDecision, CreateNote, CreateObjective, CreatePerson,
    CreateProject, CreateTask, CreateWaitingOn, DirectoryEntry, EdgeType, NewEdge, NodeRef,
    NodeType, QuickAssignee, QuickMain, QuickPlan, QuickResult, Ref, TaskStatus,
};
use rusqlite::{Connection, Transaction};
use serde_json::json;
use uuid::Uuid;

use crate::{
    decisions, edges,
    error::{Result, StoreError},
    nodes, notes, objectives, people, projects, tasks, waiting_on,
};

/// Active nodes a quick-add line may refer to: people, projects (with handles), objectives and
/// open tasks.
pub fn directory(conn: &Connection) -> Result<Vec<DirectoryEntry>> {
    let mut out = Vec::new();
    for p in people::list(conn, false)? {
        out.push(DirectoryEntry {
            node: NodeRef::new(NodeType::Person, p.id),
            label: p.name,
            handle: None,
            is_self: p.is_self,
        });
    }
    for p in projects::list(conn, false)? {
        out.push(DirectoryEntry {
            node: NodeRef::new(NodeType::Project, p.id),
            label: p.title,
            handle: Some(p.slug),
            is_self: false,
        });
    }
    for o in objectives::list(conn, false)? {
        out.push(DirectoryEntry {
            node: NodeRef::new(NodeType::Objective, o.id),
            label: o.title,
            handle: None,
            is_self: false,
        });
    }
    for t in tasks::list(conn, false)? {
        if matches!(
            t.status,
            TaskStatus::Todo | TaskStatus::InProgress | TaskStatus::Blocked
        ) {
            out.push(DirectoryEntry {
                node: NodeRef::new(NodeType::Task, t.id),
                label: t.title,
                handle: None,
                is_self: false,
            });
        }
    }
    Ok(out)
}

/// Creates what the plan asks for, all or nothing.
pub fn commit(conn: &mut Connection, plan: QuickPlan) -> Result<QuickResult> {
    let tx = conn.transaction()?;
    let mut made: Vec<NodeRef> = Vec::with_capacity(plan.new_nodes.len());
    for n in plan.new_nodes {
        made.push(create_new(&tx, n.node_type, n.name)?);
    }
    let resolve = |r: &Ref| -> Result<NodeRef> {
        match r {
            Ref::Existing(n) => Ok(*n),
            Ref::New(i) => made
                .get(*i as usize)
                .copied()
                .ok_or_else(|| StoreError::Invalid("the plan refers to a missing new node".into())),
        }
    };
    let link = |edge_type: EdgeType, from: NodeRef, to: NodeRef| -> Result<()> {
        edges::add_in_tx(
            &tx,
            NewEdge {
                edge_type,
                from,
                to,
                attrs: json!({}),
            },
        )
        .map(|_| ())
    };

    let primary: NodeRef = match plan.main {
        QuickMain::Task {
            title,
            priority,
            start_date,
            due_date,
            estimate_days,
            project,
            assignee,
            blocks,
            objectives,
            recurrence,
        } => {
            let assignee = match assignee {
                QuickAssignee::Default => AssigneeChoice::Me,
                QuickAssignee::Person(r) => AssigneeChoice::Person(resolve(&r)?.id),
            };
            let project_id = project.as_ref().map(resolve).transpose()?.map(|n| n.id);
            let t = tasks::create_in_tx(
                &tx,
                CreateTask {
                    links: Vec::new(),
                    title,
                    assignee,
                    description: String::new(),
                    project_id,
                    status: None,
                    estimate_days,
                    start_date,
                    due_date,
                    priority,
                    recurrence,
                },
            )?;
            let me = NodeRef::new(NodeType::Task, t.id);
            for b in &blocks {
                link(EdgeType::Blocks, me, resolve(b)?)?;
            }
            for o in &objectives {
                link(EdgeType::ContributesTo, me, resolve(o)?)?;
            }
            me
        }
        QuickMain::Project {
            title,
            priority,
            start_date,
            target_date,
            owner,
            objectives,
        } => {
            let owner_person_id = owner.as_ref().map(resolve).transpose()?.map(|n| n.id);
            let p = projects::create_in_tx(
                &tx,
                CreateProject {
                    title,
                    slug: None,
                    description: String::new(),
                    owner_person_id,
                    start_date,
                    target_date,
                    status: None,
                    priority,
                },
            )?;
            let me = NodeRef::new(NodeType::Project, p.id);
            for o in &objectives {
                link(EdgeType::ContributesTo, me, resolve(o)?)?;
            }
            me
        }
        QuickMain::Wait {
            description,
            person,
            expected_by,
            about,
        } => {
            let w = waiting_on::create_in_tx(
                &tx,
                CreateWaitingOn {
                    description,
                    person_id: resolve(&person)?.id,
                    asked_on: None,
                    expected_by,
                    follow_up_on: None,
                },
            )?;
            let me = NodeRef::new(NodeType::WaitingOn, w.id);
            if let Some(a) = &about {
                link(EdgeType::About, me, resolve(a)?)?;
            }
            me
        }
        QuickMain::Note {
            title,
            kind,
            note_date,
            mentions,
            recurrence,
        } => {
            // Mentions in the body become `mentions` links when the note is created.
            let mut tokens = Vec::with_capacity(mentions.len());
            for m in &mentions {
                tokens.push(mention_token(&m.label, resolve(&m.target)?.id));
            }
            let body = tokens.join(" ");
            // A repeating note's next ones start like this one (so a 1:1 keeps its person).
            let recurrence = recurrence.map(|mut rule| {
                if rule.template.is_none() && !body.is_empty() {
                    rule.template = Some(body.clone());
                }
                rule
            });
            let n = notes::create_in_tx(
                &tx,
                CreateNote {
                    title,
                    body,
                    note_date,
                    kind: Some(kind),
                    recurrence,
                },
            )?;
            NodeRef::new(NodeType::Note, n.id)
        }
        QuickMain::Decision {
            title,
            status,
            decided_on,
            affects,
        } => {
            let d = decisions::create_in_tx(
                &tx,
                CreateDecision {
                    title,
                    context: String::new(),
                    decision: String::new(),
                    rationale: String::new(),
                    decided_on,
                    status,
                },
            )?;
            let me = NodeRef::new(NodeType::Decision, d.id);
            for a in &affects {
                link(EdgeType::Affects, me, resolve(a)?)?;
            }
            me
        }
    };

    let node = nodes::summary(&tx, primary)?;
    let created = made
        .iter()
        .map(|n| nodes::summary(&tx, *n))
        .collect::<Result<Vec<_>>>()?;
    tx.commit()?;
    Ok(QuickResult { node, created })
}

fn create_new(tx: &Transaction, node_type: NodeType, name: String) -> Result<NodeRef> {
    let id: Uuid = match node_type {
        NodeType::Person => {
            people::create_in_tx(
                tx,
                CreatePerson {
                    name,
                    role_title: String::new(),
                    email: None,
                    weekly_capacity_hours: None,
                    is_self: false,
                    notes: String::new(),
                },
            )?
            .id
        }
        NodeType::Project => {
            projects::create_in_tx(
                tx,
                CreateProject {
                    title: name,
                    slug: None,
                    description: String::new(),
                    owner_person_id: None,
                    start_date: None,
                    target_date: None,
                    status: None,
                    priority: None,
                },
            )?
            .id
        }
        NodeType::Objective => {
            objectives::create_in_tx(
                tx,
                CreateObjective {
                    title: name,
                    description: String::new(),
                    target_date: None,
                    status: None,
                    priority: None,
                },
            )?
            .id
        }
        NodeType::Task => {
            tasks::create_in_tx(
                tx,
                CreateTask {
                    links: Vec::new(),
                    title: name,
                    assignee: AssigneeChoice::Me,
                    description: String::new(),
                    project_id: None,
                    status: None,
                    estimate_days: None,
                    start_date: None,
                    due_date: None,
                    priority: None,
                    recurrence: None,
                },
            )?
            .id
        }
        other => {
            return Err(StoreError::Invalid(format!(
                "a {other} can't be created from a reference"
            )))
        }
    };
    Ok(NodeRef::new(node_type, id))
}
