//! Read models for the people and teams screens. Read-only; writes go through the repos.

use std::collections::{HashMap, HashSet};

use minimap_core::subtasks::{
    expand_blocks, next_step, sequence as subtask_sequence, status_hint as subtask_status_hint,
    Hierarchy,
};
use minimap_types::{
    Contribution, DecisionItem, Edge, EdgeType, LinkedNode, Membership, NodeRef, NodeSummary,
    NodeType, NoteItem, ObjectiveDetail, ObjectiveRow, PersonArchivePreview, PersonDetail,
    PersonRow, ProjectArchivePreview, ProjectDetail, ProjectRow, ProjectTask, StatusHint, Subtask,
    SubtaskProgress, TaskDetail, TaskRow, TaskStatus, TeamDetail, TeamRow, WaitingOnItem,
};
use rusqlite::Connection;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::{
    convert::*, decisions, edges, error::Result, nodes, notes, objectives, people, projects, tasks,
    teams, waiting_on,
};

const ACTIVE_TASK: &str = "t.status IN ('todo','in_progress','blocked')";

fn parse_id(s: String) -> Uuid {
    // Ids are written by us; a malformed one would have failed the row mapping already.
    Uuid::parse_str(&s).unwrap_or_default()
}

fn role_of(attrs: &serde_json::Value) -> String {
    attrs
        .get("role")
        .and_then(|r| r.as_str())
        .unwrap_or("member")
        .to_owned()
}

/// Active tasks assigned to each person.
fn active_task_counts(conn: &Connection) -> Result<HashMap<Uuid, u32>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT e.to_id, COUNT(*) FROM edges e JOIN tasks t ON t.id = e.from_id
         WHERE e.edge_type = 'assigned_to' AND e.archived_at IS NULL
           AND t.archived_at IS NULL AND {ACTIVE_TASK}
         GROUP BY e.to_id"
    ))?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?;
    Ok(rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|(id, n)| (parse_id(id), n))
        .collect())
}

fn open_waiting_on_counts(conn: &Connection) -> Result<HashMap<Uuid, u32>> {
    let mut stmt = conn.prepare(
        "SELECT person_id, COUNT(*) FROM waiting_on
         WHERE archived_at IS NULL AND resolved_on IS NULL GROUP BY person_id",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?;
    Ok(rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|(id, n)| (parse_id(id), n))
        .collect())
}

/// Active people with their teams and workload, ordered by name.
pub fn people_rows(conn: &Connection) -> Result<Vec<PersonRow>> {
    let tasks = active_task_counts(conn)?;
    let waiting = open_waiting_on_counts(conn)?;

    let all_teams: HashMap<Uuid, NodeSummary> = teams::list(conn, false)?
        .into_iter()
        .map(|t| {
            (
                t.id,
                NodeSummary {
                    node: NodeRef::new(NodeType::Team, t.id),
                    label: t.name,
                    archived: false,
                },
            )
        })
        .collect();
    let mut by_person: HashMap<Uuid, Vec<NodeSummary>> = HashMap::new();
    for e in edges::list_active(conn)? {
        if e.edge_type == EdgeType::MemberOf {
            if let Some(team) = all_teams.get(&e.to_id) {
                by_person.entry(e.from_id).or_default().push(team.clone());
            }
        }
    }

    let mut rows: Vec<PersonRow> = people::list(conn, false)?
        .into_iter()
        .map(|person| {
            let mut teams = by_person.remove(&person.id).unwrap_or_default();
            teams.sort_by_key(|t| t.label.to_lowercase());
            PersonRow {
                active_task_count: tasks.get(&person.id).copied().unwrap_or(0),
                open_waiting_on_count: waiting.get(&person.id).copied().unwrap_or(0),
                teams,
                person,
            }
        })
        .collect();
    rows.sort_by_key(|r| r.person.name.to_lowercase());
    Ok(rows)
}

pub fn person_detail(conn: &Connection, id: Uuid) -> Result<PersonDetail> {
    let person = people::get(conn, id)?;
    let mut memberships = Vec::new();
    let mut manager = None;
    let mut reports = Vec::new();
    for link in edges::links_for_node(conn, id)? {
        match (link.edge.edge_type, link.outgoing) {
            (EdgeType::MemberOf, true) => memberships.push(Membership {
                edge_id: link.edge.id,
                role: role_of(&link.edge.attrs),
                node: link.other,
            }),
            (EdgeType::ReportsTo, true) => {
                manager = Some(LinkedNode {
                    edge_id: link.edge.id,
                    node: link.other,
                })
            }
            (EdgeType::ReportsTo, false) => reports.push(link.other),
            _ => {}
        }
    }
    let waiting_ons = waiting_on::list(conn, false)?
        .into_iter()
        .filter(|w| w.person_id == id && w.resolved_on.is_none())
        .collect();
    let active_task_count = active_task_counts(conn)?.get(&id).copied().unwrap_or(0);
    Ok(PersonDetail {
        person,
        memberships,
        manager,
        reports,
        waiting_ons,
        active_task_count,
    })
}

/// Active tasks assigned to a person (what archiving them would leave unassigned).
pub fn person_archive_preview(conn: &Connection, id: Uuid) -> Result<PersonArchivePreview> {
    people::get(conn, id)?; // NotFound for unknown ids
    let mut stmt = conn.prepare(&format!(
        "SELECT t.id, t.title FROM edges e JOIN tasks t ON t.id = e.from_id
         WHERE e.edge_type = 'assigned_to' AND e.to_id = ?1 AND e.archived_at IS NULL
           AND t.archived_at IS NULL AND {ACTIVE_TASK}
         ORDER BY t.id"
    ))?;
    let rows = stmt.query_map([id_s(id)], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    let assigned_tasks = rows
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|(tid, title)| NodeSummary {
            node: NodeRef::new(NodeType::Task, parse_id(tid)),
            label: title,
            archived: false,
        })
        .collect();
    Ok(PersonArchivePreview { assigned_tasks })
}

/// Active teams as a tree: parents before their children, siblings by name.
/// A team whose parent is archived or missing is shown at the top level.
pub fn team_rows(conn: &Connection) -> Result<Vec<TeamRow>> {
    let all = teams::list(conn, false)?;
    let ids: std::collections::HashSet<Uuid> = all.iter().map(|t| t.id).collect();

    let mut members: HashMap<Uuid, u32> = HashMap::new();
    let person_ids: std::collections::HashSet<Uuid> = people::list(conn, false)?
        .into_iter()
        .map(|p| p.id)
        .collect();
    for e in edges::list_active(conn)? {
        if e.edge_type == EdgeType::MemberOf && person_ids.contains(&e.from_id) {
            *members.entry(e.to_id).or_default() += 1;
        }
    }

    let mut children: HashMap<Option<Uuid>, Vec<&minimap_types::Team>> = HashMap::new();
    for t in &all {
        let parent = t.parent_team_id.filter(|p| ids.contains(p));
        children.entry(parent).or_default().push(t);
    }
    for list in children.values_mut() {
        list.sort_by_key(|t| t.name.to_lowercase());
    }

    fn walk(
        parent: Option<Uuid>,
        depth: u32,
        children: &HashMap<Option<Uuid>, Vec<&minimap_types::Team>>,
        members: &HashMap<Uuid, u32>,
        out: &mut Vec<TeamRow>,
    ) {
        for t in children.get(&parent).into_iter().flatten() {
            out.push(TeamRow {
                team: (*t).clone(),
                depth,
                member_count: members.get(&t.id).copied().unwrap_or(0),
            });
            walk(Some(t.id), depth + 1, children, members, out);
        }
    }
    let mut out = Vec::new();
    walk(None, 0, &children, &members, &mut out);
    Ok(out)
}

pub fn team_detail(conn: &Connection, id: Uuid) -> Result<TeamDetail> {
    let team = teams::get(conn, id)?;
    let parent = team
        .parent_team_id
        .map(|p| nodes::summary(conn, NodeRef::new(NodeType::Team, p)))
        .transpose()?;
    let mut children: Vec<NodeSummary> = teams::list(conn, false)?
        .into_iter()
        .filter(|t| t.parent_team_id == Some(id))
        .map(|t| NodeSummary {
            node: NodeRef::new(NodeType::Team, t.id),
            label: t.name,
            archived: false,
        })
        .collect();
    children.sort_by_key(|c| c.label.to_lowercase());
    let mut members: Vec<Membership> = edges::links_for_node(conn, id)?
        .into_iter()
        .filter(|l| l.edge.edge_type == EdgeType::MemberOf && !l.outgoing && !l.other.archived)
        .map(|l| Membership {
            edge_id: l.edge.id,
            role: role_of(&l.edge.attrs),
            node: l.other,
        })
        .collect();
    members.sort_by_key(|m| m.node.label.to_lowercase());
    Ok(TeamDetail {
        team,
        parent,
        children,
        members,
    })
}

/// Active objectives with how many projects/tasks contribute to each, in storage order
/// (arranging and sorting is `minimap-core::objectives::arrange`).
pub fn objective_rows(conn: &Connection) -> Result<Vec<ObjectiveRow>> {
    // Archiving a project or task archives its edges, so active edges mean active contributors.
    let mut stmt = conn.prepare(
        "SELECT to_id, COUNT(*) FROM edges
         WHERE edge_type = 'contributes_to' AND archived_at IS NULL GROUP BY to_id",
    )?;
    let counts: HashMap<Uuid, u32> = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|(id, n)| (parse_id(id), n))
        .collect();
    Ok(objectives::list(conn, false)?
        .into_iter()
        .map(|objective| ObjectiveRow {
            contribution_count: counts.get(&objective.id).copied().unwrap_or(0),
            objective,
        })
        .collect())
}

/// Status of a project, task or objective, as text.
fn status_of(conn: &Connection, node: NodeRef) -> Result<String> {
    let table = match node.node_type {
        NodeType::Project => "projects",
        NodeType::Task => "tasks",
        NodeType::Objective => "objectives",
        _ => return Ok(String::new()),
    };
    Ok(conn.query_row(
        &format!("SELECT status FROM {table} WHERE id = ?1"),
        [id_s(node.id)],
        |r| r.get(0),
    )?)
}

/// An objective and the projects/tasks contributing to it (projects first, then by name).
pub fn objective_detail(conn: &Connection, id: Uuid) -> Result<ObjectiveDetail> {
    let objective = objectives::get(conn, id)?;
    let mut contributions = Vec::new();
    for link in edges::links_for_node(conn, id)? {
        if link.edge.edge_type != EdgeType::ContributesTo || link.outgoing {
            continue;
        }
        contributions.push(Contribution {
            edge_id: link.edge.id,
            status: status_of(conn, link.other.node)?,
            weight: link.edge.attrs.get("weight").and_then(|w| w.as_f64()),
            node: link.other,
        });
    }
    contributions.sort_by_key(|c| {
        (
            c.node.node.node_type != NodeType::Project,
            c.node.label.to_lowercase(),
        )
    });
    Ok(ObjectiveDetail {
        objective,
        contributions,
    })
}

fn summary_of(node_type: NodeType, id: Uuid, label: String) -> NodeSummary {
    NodeSummary {
        node: NodeRef::new(node_type, id),
        label,
        archived: false,
    }
}

/// Active projects with owner, objectives and task progress, in storage order
/// (filtering, ordering and grouping is `minimap-core::projects`).
pub fn project_rows(conn: &Connection) -> Result<Vec<ProjectRow>> {
    let people: HashMap<Uuid, String> = people::list(conn, true)?
        .into_iter()
        .map(|p| (p.id, p.name))
        .collect();
    let objective_names: HashMap<Uuid, String> = objectives::list(conn, false)?
        .into_iter()
        .map(|o| (o.id, o.title))
        .collect();
    let mut by_project: HashMap<Uuid, Vec<NodeSummary>> = HashMap::new();
    for e in edges::list_active_of_type(conn, EdgeType::ContributesTo)? {
        if e.from_type == NodeType::Project {
            if let Some(name) = objective_names.get(&e.to_id) {
                by_project.entry(e.from_id).or_default().push(summary_of(
                    NodeType::Objective,
                    e.to_id,
                    name.clone(),
                ));
            }
        }
    }
    let mut stmt = conn.prepare(
        "SELECT project_id, COUNT(*), SUM(status = 'done') FROM tasks
         WHERE project_id IS NOT NULL AND archived_at IS NULL GROUP BY project_id",
    )?;
    let counts: HashMap<Uuid, (u32, u32)> = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, u32>(1)?,
                r.get::<_, u32>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|(id, total, done)| (parse_id(id), (total, done)))
        .collect();

    Ok(projects::list(conn, false)?
        .into_iter()
        .map(|project| {
            let (task_count, done_task_count) = counts.get(&project.id).copied().unwrap_or((0, 0));
            let mut objectives = by_project.remove(&project.id).unwrap_or_default();
            objectives.sort_by_key(|o| o.label.to_lowercase());
            ProjectRow {
                owner: project.owner_person_id.and_then(|o| {
                    people
                        .get(&o)
                        .map(|n| summary_of(NodeType::Person, o, n.clone()))
                }),
                objectives,
                task_count,
                done_task_count,
                project,
            }
        })
        .collect())
}

/// Active tasks of a project, oldest first.
fn project_tasks(conn: &Connection, id: Uuid) -> Result<Vec<ProjectTask>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, status, due_date FROM tasks
         WHERE project_id = ?1 AND archived_at IS NULL ORDER BY id",
    )?;
    let rows = stmt.query_map([id_s(id)], |r| {
        Ok(ProjectTask {
            node: summary_of(NodeType::Task, col_uuid(r, 0)?, r.get(1)?),
            status: col_enum(r, 2)?,
            due_date: col_date_opt(r, 3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn project_detail(conn: &Connection, id: Uuid) -> Result<ProjectDetail> {
    let project = projects::get(conn, id)?;
    let owner = project
        .owner_person_id
        .map(|o| nodes::summary(conn, NodeRef::new(NodeType::Person, o)))
        .transpose()?;
    let mut objectives = Vec::new();
    let mut depends_on = Vec::new();
    let mut needed_by = Vec::new();
    for link in edges::links_for_node(conn, id)? {
        match (link.edge.edge_type, link.outgoing) {
            (EdgeType::ContributesTo, true) => objectives.push(Contribution {
                edge_id: link.edge.id,
                status: status_of(conn, link.other.node)?,
                weight: link.edge.attrs.get("weight").and_then(|w| w.as_f64()),
                node: link.other,
            }),
            (EdgeType::DependsOn, true) => depends_on.push(LinkedNode {
                edge_id: link.edge.id,
                node: link.other,
            }),
            (EdgeType::DependsOn, false) => needed_by.push(link.other),
            _ => {}
        }
    }
    objectives.sort_by_key(|c| c.node.label.to_lowercase());
    depends_on.sort_by_key(|d| d.node.label.to_lowercase());
    needed_by.sort_by_key(|n| n.label.to_lowercase());
    Ok(ProjectDetail {
        owner,
        objectives,
        depends_on,
        needed_by,
        tasks: project_tasks(conn, id)?,
        project,
    })
}

/// The active tasks that archiving the project would affect.
pub fn project_archive_preview(conn: &Connection, id: Uuid) -> Result<ProjectArchivePreview> {
    projects::get(conn, id)?; // NotFound for unknown ids
    Ok(ProjectArchivePreview {
        tasks: project_tasks(conn, id)?
            .into_iter()
            .map(|t| t.node)
            .collect(),
    })
}

/// Active tasks with their project and assignee, in storage order
/// (filtering and ordering is `minimap-core::tasks`).
pub fn task_rows(conn: &Connection) -> Result<Vec<TaskRow>> {
    let projects: HashMap<Uuid, String> = projects::list(conn, true)?
        .into_iter()
        .map(|p| (p.id, p.title))
        .collect();
    let people: HashMap<Uuid, String> = people::list(conn, true)?
        .into_iter()
        .map(|p| (p.id, p.name))
        .collect();
    let mut assignee: HashMap<Uuid, Uuid> = HashMap::new();
    for e in edges::list_active_of_type(conn, EdgeType::AssignedTo)? {
        assignee.insert(e.from_id, e.to_id);
    }
    // Parent of each task, and each parent's subtasks (status of every child by id).
    let all = tasks::list(conn, false)?;
    let by_id: HashMap<Uuid, (&str, TaskStatus)> = all
        .iter()
        .map(|t| (t.id, (t.title.as_str(), t.status)))
        .collect();
    let mut parent: HashMap<Uuid, Uuid> = HashMap::new();
    let mut progress: HashMap<Uuid, SubtaskProgress> = HashMap::new();
    for e in edges::list_active_of_type(conn, EdgeType::SubtaskOf)? {
        let (Some(_), Some((_, child_status))) = (by_id.get(&e.to_id), by_id.get(&e.from_id))
        else {
            continue;
        };
        parent.insert(e.from_id, e.to_id);
        if *child_status != TaskStatus::Cancelled {
            let p = progress.entry(e.to_id).or_default();
            p.total += 1;
            p.done += u32::from(*child_status == TaskStatus::Done);
        }
    }
    let parents: HashMap<Uuid, NodeSummary> = parent
        .iter()
        .filter_map(|(child, p)| {
            by_id
                .get(p)
                .map(|(title, _)| (*child, summary_of(NodeType::Task, *p, (*title).to_owned())))
        })
        .collect();
    Ok(all
        .into_iter()
        .map(|task| TaskRow {
            parent: parents.get(&task.id).cloned(),
            subtasks: progress.get(&task.id).copied().unwrap_or_default(),
            project: task.project_id.and_then(|p| {
                projects
                    .get(&p)
                    .map(|n| summary_of(NodeType::Project, p, n.clone()))
            }),
            assignee: assignee.get(&task.id).and_then(|p| {
                people
                    .get(p)
                    .map(|n| summary_of(NodeType::Person, *p, n.clone()))
            }),
            task,
        })
        .collect())
}

pub fn task_detail(conn: &Connection, id: Uuid) -> Result<TaskDetail> {
    let task = tasks::get(conn, id)?;
    let project = task
        .project_id
        .map(|p| nodes::summary(conn, NodeRef::new(NodeType::Project, p)))
        .transpose()?;
    let assignee = edges::list_for_node(conn, id, false)?
        .into_iter()
        .find(|e| e.edge_type == EdgeType::AssignedTo && e.from_id == id)
        .map(|e| nodes::summary(conn, e.to()))
        .transpose()?;
    let mut parent = None;
    let mut children: Vec<(OffsetDateTime, Uuid)> = Vec::new();
    for e in edges::list_for_node(conn, id, false)? {
        if e.edge_type != EdgeType::SubtaskOf {
            continue;
        }
        if e.from_id == id {
            parent = Some(nodes::summary(conn, e.to())?);
        } else {
            children.push((tasks::get(conn, e.from_id)?.created_at, e.from_id));
        }
    }
    children.sort();
    let child_ids: Vec<Uuid> = children.iter().map(|c| c.1).collect();

    // The order the work goes in, and what each step waits for (spec 29).
    let all_tasks = tasks::list(conn, false)?;
    let all_edges = edges::list_active(conn)?;
    let hierarchy = Hierarchy::new(&all_tasks, &all_edges);
    let blocks: Vec<&Edge> = all_edges
        .iter()
        .filter(|e| e.edge_type == EdgeType::Blocks)
        .collect();
    let pairs: Vec<(Uuid, Uuid)> = blocks.iter().map(|e| (e.from_id, e.to_id)).collect();
    let order = subtask_sequence(&child_ids, &pairs);
    let open: HashSet<Uuid> = all_tasks
        .iter()
        .filter(|t| !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled))
        .map(|t| t.id)
        .collect();
    let leaf_blocks = expand_blocks(
        &hierarchy,
        &pairs.iter().map(|&(a, b)| (a, b, 0.0)).collect::<Vec<_>>(),
    );
    let waiting = |child: Uuid| {
        let leaves = hierarchy.leaves_under(child);
        leaf_blocks
            .iter()
            .any(|(from, to, _)| leaves.contains(to) && open.contains(from))
    };
    let next = next_step(&order, |c| open.contains(&c), waiting);

    let mut subtasks = Vec::new();
    let mut statuses = Vec::new();
    for child_id in order {
        let child = tasks::get(conn, child_id)?;
        statuses.push(child.status);
        let assignee = edges::list_for_node(conn, child.id, false)?
            .into_iter()
            .find(|a| a.edge_type == EdgeType::AssignedTo && a.from_id == child.id)
            .map(|a| nodes::summary(conn, a.to()))
            .transpose()?;
        let mut after = Vec::new();
        for e in blocks
            .iter()
            .filter(|e| e.to_id == child.id && child_ids.contains(&e.from_id))
        {
            after.push(LinkedNode {
                edge_id: e.id,
                node: nodes::summary(conn, e.from())?,
            });
        }
        subtasks.push(Subtask {
            node: nodes::summary(conn, NodeRef::new(NodeType::Task, child.id))?,
            status: child.status,
            due_date: child.due_date,
            assignee,
            after,
            waiting: open.contains(&child.id) && waiting(child.id),
            next: next == Some(child.id),
        });
    }
    let status_hint = subtask_status_hint(task.status, &statuses).map(|h| StatusHint {
        status: h.status,
        text: h.text.to_owned(),
    });
    Ok(TaskDetail {
        task,
        project,
        assignee,
        parent,
        subtasks,
        status_hint,
    })
}

/// Active waiting-ons with who they are from and what they are about, in storage order
/// (ages, stale flags, filtering and ordering is `minimap-core::waiting_on`).
pub fn waiting_on_items(conn: &Connection) -> Result<Vec<WaitingOnItem>> {
    let people: HashMap<Uuid, NodeSummary> = people::list(conn, true)?
        .into_iter()
        .map(|p| {
            (
                p.id,
                NodeSummary {
                    node: NodeRef::new(NodeType::Person, p.id),
                    label: p.name,
                    archived: p.archived_at.is_some(),
                },
            )
        })
        .collect();
    let mut about: HashMap<Uuid, NodeSummary> = HashMap::new();
    for e in edges::list_active_of_type(conn, EdgeType::About)? {
        if e.from_type == NodeType::WaitingOn {
            about.insert(e.from_id, nodes::summary(conn, e.to())?);
        }
    }
    Ok(waiting_on::list(conn, false)?
        .into_iter()
        .filter_map(|waiting| {
            // Waiting-ons always reference an existing person (foreign key).
            people
                .get(&waiting.person_id)
                .cloned()
                .map(|person| WaitingOnItem {
                    about: about.get(&waiting.id).cloned(),
                    person,
                    waiting,
                })
        })
        .collect())
}

fn mentions_by_note(conn: &Connection) -> Result<HashMap<Uuid, Vec<NodeSummary>>> {
    let mut out: HashMap<Uuid, Vec<NodeSummary>> = HashMap::new();
    for e in edges::list_active_of_type(conn, EdgeType::Mentions)? {
        if e.from_type == NodeType::Note {
            out.entry(e.from_id)
                .or_default()
                .push(nodes::summary(conn, e.to())?);
        }
    }
    for list in out.values_mut() {
        list.sort_by_key(|n| n.label.to_lowercase());
    }
    Ok(out)
}

/// Active notes (with bodies) and what each mentions, in storage order
/// (filtering and ordering is `minimap-core::notes`).
pub fn note_items(conn: &Connection) -> Result<Vec<NoteItem>> {
    let mut mentions = mentions_by_note(conn)?;
    Ok(notes::list(conn, false)?
        .into_iter()
        .map(|note| NoteItem {
            mentions: mentions.remove(&note.id).unwrap_or_default(),
            note,
        })
        .collect())
}

pub fn note_item(conn: &Connection, id: Uuid) -> Result<NoteItem> {
    let note = notes::get(conn, id)?;
    let mentions = mentions_by_note(conn)?.remove(&id).unwrap_or_default();
    Ok(NoteItem { note, mentions })
}

/// Active decisions with what each affects and what replaced it, in storage order
/// (filtering and ordering is `minimap-core::decisions`).
pub fn decision_items(conn: &Connection) -> Result<Vec<DecisionItem>> {
    let mut affects: HashMap<Uuid, Vec<NodeSummary>> = HashMap::new();
    for e in edges::list_active_of_type(conn, EdgeType::Affects)? {
        if e.from_type == NodeType::Decision {
            affects
                .entry(e.from_id)
                .or_default()
                .push(nodes::summary(conn, e.to())?);
        }
    }
    for list in affects.values_mut() {
        list.sort_by_key(|n| n.label.to_lowercase());
    }
    // Edges come back oldest first, so the last link wins when a decision was replaced twice.
    let mut replaced_by: HashMap<Uuid, NodeSummary> = HashMap::new();
    for e in edges::list_active_of_type(conn, EdgeType::Supersedes)? {
        replaced_by.insert(e.to_id, nodes::summary(conn, e.from())?);
    }
    Ok(decisions::list(conn, false)?
        .into_iter()
        .map(|decision| DecisionItem {
            affects: affects.remove(&decision.id).unwrap_or_default(),
            superseded_by: replaced_by.remove(&decision.id),
            decision,
        })
        .collect())
}

/// Open tasks that block each open task (from active `blocks` links), for "blocked by ...".
pub fn open_blockers(conn: &Connection) -> Result<HashMap<Uuid, Vec<NodeSummary>>> {
    let open: HashMap<Uuid, String> = tasks::list(conn, false)?
        .into_iter()
        .filter(|t| {
            !matches!(
                t.status,
                minimap_types::TaskStatus::Done | minimap_types::TaskStatus::Cancelled
            )
        })
        .map(|t| (t.id, t.title))
        .collect();
    let mut out: HashMap<Uuid, Vec<NodeSummary>> = HashMap::new();
    for e in edges::list_active_of_type(conn, EdgeType::Blocks)? {
        if let (Some(title), true) = (open.get(&e.from_id), open.contains_key(&e.to_id)) {
            out.entry(e.to_id).or_default().push(NodeSummary {
                node: NodeRef::new(NodeType::Task, e.from_id),
                label: title.clone(),
                archived: false,
            });
        }
    }
    for list in out.values_mut() {
        list.sort_by_key(|n| n.label.to_lowercase());
    }
    Ok(out)
}
