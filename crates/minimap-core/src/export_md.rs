//! Markdown for the full data export (spec 26): one file per project with its tasks, the inbox,
//! and each note as a `.md` file, plus a README that says what is in the folder. Pure: the
//! command layer writes the files.
//!
//! The JSON is the complete export; this is the readable one. It follows the same data: archived
//! projects and notes are included and marked, links come from the active edges.

use std::collections::{HashMap, HashSet};

use minimap_types::{
    DataExport, Decision, EdgeType, Note, NoteKind, Objective, Person, Project, Task, TaskStatus,
    TaskType, Uuid, WaitingOn,
};

use crate::{notes::mentions_as_names, report::escape, slug::slugify};

/// A file to write, by path relative to the export folder (forward slashes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdFile {
    pub path: String,
    pub text: String,
}

/// A path as a Markdown link target: spaces and the characters that would end the link are
/// percent-encoded.
pub fn encode_link(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '(' => out.push_str("%28"),
            ')' => out.push_str("%29"),
            '%' => out.push_str("%25"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            c => out.push(c),
        }
    }
    out
}

/// Turns `(attachment:<id>)` link targets in a note body into the exported file's path
/// (`paths` is relative to the export folder, `up` gets from the note's folder to it).
/// Attachments that weren't exported keep their `attachment:` target.
pub fn rewrite_attachments(body: &str, up: &str, paths: &HashMap<Uuid, String>) -> String {
    const OPEN: &str = "(attachment:";
    let mut out = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(at) = rest.find(OPEN) {
        out.push_str(&rest[..at]);
        let after = &rest[at + OPEN.len()..];
        let id = after
            .get(..36)
            .filter(|_| after.as_bytes().get(36) == Some(&b')'))
            .and_then(|s| s.parse::<Uuid>().ok());
        match id.and_then(|id| paths.get(&id)) {
            Some(path) => {
                out.push('(');
                out.push_str(up);
                out.push_str(&encode_link(path));
                out.push(')');
                rest = &after[37..];
            }
            None => {
                out.push_str(OPEN);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// A file name stem that isn't taken yet (`name`, `name-2`, ...).
fn unique(base: &str, used: &mut HashSet<String>) -> String {
    let mut name = base.to_owned();
    let mut n = 2;
    while !used.insert(name.clone()) {
        name = format!("{base}-{n}");
        n += 1;
    }
    name
}

fn days(n: f64) -> String {
    let n = (n * 100.0).round() / 100.0;
    if n.fract() == 0.0 {
        format!("{n:.0}d")
    } else {
        format!("{n}d")
    }
}

fn words(s: &str) -> String {
    s.replace('_', " ")
}

struct Lookup<'a> {
    people: HashMap<Uuid, &'a Person>,
    tasks: HashMap<Uuid, &'a Task>,
    projects: HashMap<Uuid, &'a Project>,
    objectives: HashMap<Uuid, &'a Objective>,
    decisions: HashMap<Uuid, &'a Decision>,
    waiting: HashMap<Uuid, &'a WaitingOn>,
    /// Task types by id.
    types: HashMap<&'a str, &'a TaskType>,
    /// Active edges of one type: (from, to, attrs).
    edges: HashMap<EdgeType, Vec<(Uuid, Uuid, &'a serde_json::Value)>>,
}

impl<'a> Lookup<'a> {
    fn new(data: &'a DataExport) -> Self {
        let mut edges: HashMap<EdgeType, Vec<(Uuid, Uuid, &serde_json::Value)>> = HashMap::new();
        for e in data.edges.iter().filter(|e| e.archived_at.is_none()) {
            edges
                .entry(e.edge_type)
                .or_default()
                .push((e.from_id, e.to_id, &e.attrs));
        }
        Self {
            people: data.people.iter().map(|p| (p.id, p)).collect(),
            tasks: data.tasks.iter().map(|t| (t.id, t)).collect(),
            projects: data.projects.iter().map(|p| (p.id, p)).collect(),
            objectives: data.objectives.iter().map(|o| (o.id, o)).collect(),
            decisions: data.decisions.iter().map(|d| (d.id, d)).collect(),
            waiting: data.waiting_on.iter().map(|w| (w.id, w)).collect(),
            types: data.task_types.iter().map(|t| (t.id.as_str(), t)).collect(),
            edges,
        }
    }

    fn edges(&self, t: EdgeType) -> &[(Uuid, Uuid, &'a serde_json::Value)] {
        self.edges.get(&t).map_or(&[], Vec::as_slice)
    }

    fn assignee(&self, task: Uuid) -> Option<&'a Person> {
        self.edges(EdgeType::AssignedTo)
            .iter()
            .find(|(from, ..)| *from == task)
            .and_then(|(_, to, _)| self.people.get(to).copied())
    }

    fn blockers(&self, task: Uuid) -> Vec<&'a Task> {
        let mut list: Vec<&Task> = self
            .edges(EdgeType::Blocks)
            .iter()
            .filter(|(_, to, _)| *to == task)
            .filter_map(|(from, ..)| self.tasks.get(from).copied())
            .filter(|t| !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled))
            .collect();
        list.sort_by(|a, b| a.title.cmp(&b.title));
        list
    }
}

fn is_open(t: &Task) -> bool {
    !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled)
}

fn task_line(t: &Task, l: &Lookup) -> String {
    let title = escape(&t.title);
    let mut line = match t.status {
        TaskStatus::Done => format!("- [x] {title}"),
        TaskStatus::Cancelled => format!("- [ ] ~~{title}~~"),
        _ => format!("- [ ] {title}"),
    };
    let mut bits: Vec<String> = Vec::new();
    match t.status {
        TaskStatus::InProgress | TaskStatus::Blocked | TaskStatus::Cancelled => {
            bits.push(words(t.status.as_str()));
        }
        TaskStatus::Done => bits.push(match t.completed_at {
            Some(at) => format!("done {}", at.date()),
            None => "done".to_owned(),
        }),
        TaskStatus::Todo => {}
    }
    if let Some(kind) = t.task_type.as_deref().and_then(|id| l.types.get(id)) {
        bits.push(escape(&kind.name));
    }
    if let Some(p) = l.assignee(t.id) {
        bits.push(escape(&p.name));
    }
    match (t.due_date, t.start_minute.filter(|_| t.is_meeting())) {
        // A meeting reads as the day and the time it is at (spec 38).
        (Some(d), Some(m)) => bits.push(format!(
            "meeting {d} {}",
            minimap_types::fmt_range(m, t.meeting_minutes())
        )),
        (Some(d), None) => bits.push(format!("due {d}")),
        _ => {}
    }
    if let Some(e) = t.estimate_days {
        bits.push(days(e));
    }
    if t.priority <= 2 {
        bits.push(format!("P{}", t.priority));
    }
    let blockers = l.blockers(t.id);
    if is_open(t) && !blockers.is_empty() {
        bits.push(format!(
            "blocked by {}",
            blockers
                .iter()
                .map(|b| escape(&b.title))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if let Some(rule) = &t.recurrence {
        bits.push(format!("repeats {}", rule.describe()));
    }
    for link in &t.links {
        bits.push(format!(
            "[{}]({})",
            escape(&link.label()),
            link.url.replace('(', "%28").replace(')', "%29")
        ));
    }
    if t.archived_at.is_some() {
        bits.push("archived".to_owned());
    }
    if !bits.is_empty() {
        line.push_str(" — ");
        line.push_str(&bits.join(" · "));
    }
    line
}

/// Open tasks by due date (undated last), then the finished ones by title.
fn task_sections(mut tasks: Vec<&Task>, l: &Lookup) -> String {
    tasks.sort_by(|a, b| {
        (!is_open(a), a.due_date.is_none(), a.due_date, &a.title).cmp(&(
            !is_open(b),
            b.due_date.is_none(),
            b.due_date,
            &b.title,
        ))
    });
    let (open, finished): (Vec<&Task>, Vec<&Task>) = tasks.into_iter().partition(|t| is_open(t));
    let mut out = String::new();
    for (heading, list) in [("Open", open), ("Finished", finished)] {
        if list.is_empty() {
            continue;
        }
        out.push_str(&format!("### {heading}\n\n"));
        for t in list {
            out.push_str(&task_line(t, l));
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

fn project_file(p: &Project, data: &DataExport, l: &Lookup) -> String {
    let mut out = format!("# {}\n\n", escape(&p.title));
    let mut facts = vec![
        format!("**Status:** {}", words(p.status.as_str())),
        format!("**Priority:** {}", p.priority),
    ];
    if let Some(o) = p.owner_person_id.and_then(|id| l.people.get(&id)) {
        facts.push(format!("**Owner:** {}", escape(&o.name)));
    }
    match (p.start_date, p.target_date) {
        (Some(s), Some(t)) => facts.push(format!("**Dates:** {s} → {t}")),
        (Some(s), None) => facts.push(format!("**Started:** {s}")),
        (None, Some(t)) => facts.push(format!("**Target:** {t}")),
        (None, None) => {}
    }
    facts.push(format!("**Handle:** `{}`", p.slug));
    if p.archived_at.is_some() {
        facts.push("**Archived**".to_owned());
    }
    for f in facts {
        out.push_str(&format!("- {f}\n"));
    }
    out.push('\n');
    if !p.description.trim().is_empty() {
        out.push_str(p.description.trim());
        out.push_str("\n\n");
    }

    let section = |heading: &str, lines: Vec<String>, out: &mut String| {
        if !lines.is_empty() {
            out.push_str(&format!("## {heading}\n\n"));
            for line in lines {
                out.push_str(&format!("- {line}\n"));
            }
            out.push('\n');
        }
    };
    section(
        "Objectives",
        l.edges(EdgeType::ContributesTo)
            .iter()
            .filter(|(from, ..)| *from == p.id)
            .filter_map(|(_, to, attrs)| {
                let o = l.objectives.get(to)?;
                let weight = attrs
                    .get("weight")
                    .and_then(|w| w.as_f64())
                    .map_or(String::new(), |w| format!(" (weight {w})"));
                Some(format!("{}{weight}", escape(&o.title)))
            })
            .collect(),
        &mut out,
    );
    let project_links = |forward: bool| -> Vec<String> {
        l.edges(EdgeType::DependsOn)
            .iter()
            .filter_map(|(from, to, attrs)| {
                let (mine, other) = if forward { (from, to) } else { (to, from) };
                if *mine != p.id {
                    return None;
                }
                let other = l.projects.get(other)?;
                let note = attrs
                    .get("note")
                    .and_then(|n| n.as_str())
                    .filter(|n| !n.trim().is_empty())
                    .map_or(String::new(), |n| format!(": {}", escape(n)));
                Some(format!("{}{note}", escape(&other.title)))
            })
            .collect()
    };
    section("Depends on", project_links(true), &mut out);
    section("Needed by", project_links(false), &mut out);

    let tasks: Vec<&Task> = data
        .tasks
        .iter()
        .filter(|t| t.project_id == Some(p.id))
        .collect();
    if !tasks.is_empty() {
        out.push_str("## Tasks\n\n");
        out.push_str(&task_sections(tasks, l));
    }

    section(
        "Decisions",
        l.edges(EdgeType::Affects)
            .iter()
            .filter(|(_, to, _)| *to == p.id)
            .filter_map(|(from, ..)| l.decisions.get(from))
            .map(|d| {
                let when = d.decided_on.map_or(String::new(), |on| format!(", {on}"));
                let what = if d.decision.trim().is_empty() {
                    String::new()
                } else {
                    format!(": {}", escape(&d.decision))
                };
                format!(
                    "**{}** ({}{when}){what}",
                    escape(&d.title),
                    words(d.status.as_str())
                )
            })
            .collect(),
        &mut out,
    );
    section(
        "Waiting on",
        l.edges(EdgeType::About)
            .iter()
            .filter(|(_, to, _)| *to == p.id)
            .filter_map(|(from, ..)| l.waiting.get(from))
            .filter_map(|w| {
                let who = l.people.get(&w.person_id)?;
                let state = match w.resolved_on {
                    Some(on) => format!("resolved {on}"),
                    None => match w.expected_by {
                        Some(by) => format!("asked {}, expected {by}", w.asked_on),
                        None => format!("asked {}", w.asked_on),
                    },
                };
                Some(format!(
                    "{}: {} ({state})",
                    escape(&who.name),
                    escape(&w.description)
                ))
            })
            .collect(),
        &mut out,
    );
    out.trim_end().to_owned() + "\n"
}

fn inbox_file(data: &DataExport, l: &Lookup) -> Option<String> {
    let tasks: Vec<&Task> = data
        .tasks
        .iter()
        .filter(|t| t.project_id.is_none_or(|id| !l.projects.contains_key(&id)))
        .collect();
    if tasks.is_empty() {
        return None;
    }
    let mut out = String::from("# Inbox\n\nTasks that belong to no project.\n\n");
    out.push_str(&task_sections(tasks, l));
    Some(out.trim_end().to_owned() + "\n")
}

fn kind_word(k: NoteKind) -> &'static str {
    match k {
        NoteKind::OneOnOne => "1:1",
        NoteKind::Meeting => "meeting",
        NoteKind::General => "note",
    }
}

fn note_file(n: &Note, paths: &HashMap<Uuid, String>) -> String {
    let mut meta = format!("{} · {}", kind_word(n.kind), n.note_date);
    if let Some(rule) = &n.recurrence {
        meta.push_str(&format!(" · repeats {}", rule.describe()));
    }
    if n.archived_at.is_some() {
        meta.push_str(" · archived");
    }
    let body = rewrite_attachments(&mentions_as_names(&n.body), "../", paths);
    format!(
        "# {}\n\n*{meta}*\n\n{}\n",
        escape(&n.title),
        body.trim_end()
    )
}

/// The Markdown files of an export: `projects/<name>.md`, `inbox.md` (when there are tasks
/// without a project) and `notes/<date>-<name>.md`. `attachments` maps an attachment's id to
/// its path in the export folder, for the notes that show or link it.
pub fn markdown(data: &DataExport, attachments: &HashMap<Uuid, String>) -> Vec<MdFile> {
    let l = Lookup::new(data);
    let mut files = Vec::new();
    let mut used = HashSet::new();
    for p in &data.projects {
        let name = unique(&slugify(&p.title), &mut used);
        files.push(MdFile {
            path: format!("projects/{name}.md"),
            text: project_file(p, data, &l),
        });
    }
    if let Some(text) = inbox_file(data, &l) {
        files.push(MdFile {
            path: "inbox.md".into(),
            text,
        });
    }
    let mut used = HashSet::new();
    for n in &data.notes {
        let name = unique(&format!("{}-{}", n.note_date, slugify(&n.title)), &mut used);
        files.push(MdFile {
            path: format!("notes/{name}.md"),
            text: note_file(n, attachments),
        });
    }
    files
}

/// `README.md` of an export: what the folder holds and how the files fit together.
pub fn readme(markdown: bool) -> String {
    let mut out = String::from(
        "# Minimap export\n\n\
         Everything from your Minimap, in open formats. Nothing here needs Minimap to read.\n\n\
         ## The data (JSON)\n\n\
         - `manifest.json`: what this export is (format and version, app and schema version, when it was made, how many records each file holds)\n\
         - `objectives.json`, `projects.json`, `tasks.json`, `people.json`, `teams.json`, `notes.json`, `decisions.json`, `waiting_on.json`: one file per kind of item, each a list. Archived items are included and have `archived_at` set\n\
         - `edges.json`: every link between items (`blocks`, `depends_on`, `assigned_to`, ...), with `from_id` and `to_id` pointing at item ids; removed links have `archived_at` set\n\
         - `attachments.json`: the files attached to items (which item, name, size, checksum); the files themselves are in `attachments/<id>/`\n\
         - `task_types.json`: your list of task types; a task's `task_type` is the `id` of one of them\n\
         - `activity.json`: the full history of changes, oldest first. Each entry has a `diff` of `{field: [old, new]}`\n\n\
         Ids are UUIDs, dates are `YYYY-MM-DD` and timestamps are RFC 3339 in UTC.\n\n\
         Note bodies are Markdown. A mention of an item looks like `@[Name](node:<id>)` and a picture or file like `![name](attachment:<id>)`.\n\n\
         This export is plain text: it is not encrypted, whatever protects your Minimap data.\n",
    );
    if markdown {
        out.push_str(
            "\n## The readable version (Markdown)\n\n\
             - `projects/`: one file per project with its details, objectives, dependencies, tasks, decisions and waiting-ons\n\
             - `inbox.md`: tasks that belong to no project\n\
             - `notes/`: one file per note, named by date and title, with mentions as `@Name`\n",
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{
        DecisionStatus, Edge, NodeType, ObjectiveStatus, ProjectStatus, WaitingOn,
    };
    use time::{macros::date, OffsetDateTime};

    const T0: OffsetDateTime = OffsetDateTime::UNIX_EPOCH;

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn project(n: u128, title: &str) -> Project {
        Project {
            id: id(n),
            title: title.into(),
            slug: slugify(title),
            description: String::new(),
            owner_person_id: None,
            start_date: None,
            target_date: None,
            status: ProjectStatus::Active,
            priority: 3,
            created_at: T0,
            updated_at: T0,
            archived_at: None,
        }
    }

    fn task(n: u128, title: &str, project: Option<u128>, status: TaskStatus) -> Task {
        Task {
            links: Vec::new(),
            task_type: None,
            focus: None,
            start_minute: None,
            length_minutes: None,
            id: id(n),
            title: title.into(),
            description: String::new(),
            project_id: project.map(id),
            status,
            estimate_days: None,
            start_date: None,
            due_date: None,
            completed_at: None,
            priority: 3,
            recurrence: None,
            created_at: T0,
            updated_at: T0,
            archived_at: None,
        }
    }

    fn person(n: u128, name: &str) -> Person {
        Person {
            id: id(n),
            name: name.into(),
            role_title: String::new(),
            email: None,
            weekly_capacity_hours: 40.0,
            is_self: false,
            notes: String::new(),
            created_at: T0,
            updated_at: T0,
            archived_at: None,
        }
    }

    fn edge(
        n: u128,
        t: EdgeType,
        from: (NodeType, u128),
        to: (NodeType, u128),
        attrs: serde_json::Value,
    ) -> Edge {
        Edge {
            id: id(9000 + n),
            edge_type: t,
            from_type: from.0,
            from_id: id(from.1),
            to_type: to.0,
            to_id: id(to.1),
            attrs,
            created_at: T0,
            archived_at: None,
        }
    }

    fn note(n: u128, title: &str, body: &str) -> Note {
        Note {
            id: id(n),
            title: title.into(),
            body: body.into(),
            note_date: date!(2027 - 03 - 03),
            kind: NoteKind::OneOnOne,
            recurrence: None,
            created_at: T0,
            updated_at: T0,
            archived_at: None,
        }
    }

    fn file<'a>(files: &'a [MdFile], path: &str) -> &'a str {
        &files
            .iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| {
                panic!(
                    "no {path}: {:?}",
                    files.iter().map(|f| &f.path).collect::<Vec<_>>()
                )
            })
            .text
    }

    fn data() -> DataExport {
        let mut eu = project(1, "EU Region");
        eu.owner_person_id = Some(id(20));
        eu.start_date = Some(date!(2027 - 02 - 01));
        eu.target_date = Some(date!(2027 - 03 - 29));
        eu.description = "Stand up the product in Europe.".into();
        let sec = project(2, "Security");
        let mut done = task(12, "Pick a region", Some(1), TaskStatus::Done);
        done.completed_at = Some(date!(2027 - 02 - 10).midnight().assume_utc());
        let mut due = task(11, "Build clusters", Some(1), TaskStatus::InProgress);
        due.due_date = Some(date!(2027 - 03 - 05));
        due.estimate_days = Some(6.5);
        due.priority = 1;
        due.task_type = Some("decision".into());
        let blocked = task(13, "Go live", Some(1), TaskStatus::Todo);
        let objective = Objective {
            ongoing: false,
            review_every_days: None,
            last_reviewed_on: None,
            id: id(30),
            title: "Launch in the EU".into(),
            description: String::new(),
            target_date: None,
            status: ObjectiveStatus::OnTrack,
            priority: 1,
            created_at: T0,
            updated_at: T0,
            archived_at: None,
        };
        let decision = Decision {
            id: id(40),
            title: "Frankfurt".into(),
            context: String::new(),
            decision: "Use Frankfurt.".into(),
            rationale: String::new(),
            decided_on: Some(date!(2027 - 02 - 12)),
            status: DecisionStatus::Decided,
            created_at: T0,
            updated_at: T0,
            archived_at: None,
        };
        let waiting = WaitingOn {
            id: id(50),
            description: "Sign-off".into(),
            person_id: id(21),
            asked_on: date!(2027 - 02 - 20),
            expected_by: Some(date!(2027 - 03 - 01)),
            follow_up_on: None,
            resolved_on: None,
            created_at: T0,
            updated_at: T0,
            archived_at: None,
        };
        DataExport {
            objectives: vec![objective],
            projects: vec![eu, sec],
            tasks: vec![
                done,
                due,
                blocked,
                task(14, "Book the offsite", None, TaskStatus::Todo),
            ],
            people: vec![person(20, "Priya Nair"), person(21, "Raj Patel")],
            decisions: vec![decision],
            waiting_on: vec![waiting],
            notes: vec![note(
                60,
                "1:1 with Priya",
                "Talked about @[Priya Nair](node:00000000-0000-0000-0000-000000000014).",
            )],
            edges: vec![
                edge(
                    1,
                    EdgeType::ContributesTo,
                    (NodeType::Project, 1),
                    (NodeType::Objective, 30),
                    serde_json::json!({"weight": 0.5}),
                ),
                edge(
                    2,
                    EdgeType::DependsOn,
                    (NodeType::Project, 1),
                    (NodeType::Project, 2),
                    serde_json::json!({"note": "Sign-off first"}),
                ),
                edge(
                    3,
                    EdgeType::AssignedTo,
                    (NodeType::Task, 11),
                    (NodeType::Person, 20),
                    serde_json::json!({}),
                ),
                edge(
                    4,
                    EdgeType::Blocks,
                    (NodeType::Task, 11),
                    (NodeType::Task, 13),
                    serde_json::json!({}),
                ),
                edge(
                    5,
                    EdgeType::Affects,
                    (NodeType::Decision, 40),
                    (NodeType::Project, 1),
                    serde_json::json!({}),
                ),
                edge(
                    6,
                    EdgeType::About,
                    (NodeType::WaitingOn, 50),
                    (NodeType::Project, 1),
                    serde_json::json!({}),
                ),
            ],
            task_types: minimap_types::default_task_types(),
            ..Default::default()
        }
    }

    #[test]
    fn a_project_file_has_its_facts_links_tasks_decisions_and_waiting_ons() {
        let files = markdown(&data(), &HashMap::new());
        let eu = file(&files, "projects/eu-region.md");
        assert_eq!(
            eu,
            "# EU Region\n\n\
             - **Status:** active\n\
             - **Priority:** 3\n\
             - **Owner:** Priya Nair\n\
             - **Dates:** 2027-02-01 → 2027-03-29\n\
             - **Handle:** `eu-region`\n\n\
             Stand up the product in Europe.\n\n\
             ## Objectives\n\n\
             - Launch in the EU (weight 0.5)\n\n\
             ## Depends on\n\n\
             - Security: Sign-off first\n\n\
             ## Tasks\n\n\
             ### Open\n\n\
             - [ ] Build clusters — in progress · Decision · Priya Nair · due 2027-03-05 · 6.5d · P1\n\
             - [ ] Go live — blocked by Build clusters\n\n\
             ### Finished\n\n\
             - [x] Pick a region — done 2027-02-10\n\n\
             ## Decisions\n\n\
             - **Frankfurt** (decided, 2027-02-12): Use Frankfurt.\n\n\
             ## Waiting on\n\n\
             - Raj Patel: Sign-off (asked 2027-02-20, expected 2027-03-01)\n"
        );
        // The other side of the dependency.
        let sec = file(&files, "projects/security.md");
        assert!(
            sec.contains("## Needed by\n\n- EU Region: Sign-off first"),
            "{sec}"
        );
        assert!(
            !sec.contains("## Tasks"),
            "a project without tasks has no tasks section"
        );
    }

    #[test]
    fn a_meeting_reads_as_its_day_and_time() {
        let mut d = data();
        d.tasks[1].task_type = Some(minimap_types::MEETING_TYPE.into());
        d.tasks[1].due_date = Some(date!(2027 - 03 - 05));
        d.tasks[1].start_minute = Some(10 * 60 + 30);
        let files = markdown(&d, &HashMap::new());
        let text = file(&files, "projects/eu-region.md");
        assert!(text.contains("meeting 2027-03-05 10:30–11:30"), "{text}");
    }

    #[test]
    fn repeating_tasks_and_notes_say_so() {
        use minimap_types::Cadence;
        let mut d = data();
        d.tasks[1].recurrence = Some(
            Cadence::Weekly {
                every: 2,
                weekday: 0,
            }
            .into(),
        );
        d.notes[0].recurrence = Some(Cadence::Monthly { day: 1 }.into());
        let files = markdown(&d, &HashMap::new());
        assert!(
            file(&files, "projects/eu-region.md")
                .contains("6.5d · P1 · repeats every 2 weeks on Monday"),
            "{}",
            file(&files, "projects/eu-region.md")
        );
        assert!(file(&files, "notes/2027-03-03-1-1-with-priya.md")
            .contains("*1:1 · 2027-03-03 · repeats monthly on the 1st*"));
    }

    #[test]
    fn a_tasks_reference_links_are_markdown_links() {
        use minimap_types::RefLink;
        let mut d = data();
        d.tasks[1].links = vec![
            RefLink {
                title: "Design [v2]".into(),
                url: "https://docs.google.com/document/d/1".into(),
            },
            RefLink {
                title: String::new(),
                url: "https://example.com/a_(b)".into(),
            },
        ];
        let files = markdown(&d, &HashMap::new());
        let eu = file(&files, "projects/eu-region.md");
        assert!(
            eu.contains(
                "P1 · [Design \\[v2\\]](https://docs.google.com/document/d/1) · [example.com/a\\_(b)](https://example.com/a_%28b%29)"
            ),
            "{eu}"
        );
    }

    #[test]
    fn tasks_without_a_project_are_in_the_inbox() {
        let files = markdown(&data(), &HashMap::new());
        assert_eq!(
            file(&files, "inbox.md"),
            "# Inbox\n\nTasks that belong to no project.\n\n### Open\n\n- [ ] Book the offsite\n"
        );
        // No inbox file when there is nothing in it.
        let mut d = data();
        d.tasks.retain(|t| t.project_id.is_some());
        assert!(markdown(&d, &HashMap::new())
            .iter()
            .all(|f| f.path != "inbox.md"));
    }

    #[test]
    fn archived_and_cancelled_things_are_marked_not_dropped() {
        let mut d = data();
        d.projects[1].archived_at = Some(T0);
        d.tasks[2].status = TaskStatus::Cancelled;
        d.tasks[3].archived_at = Some(T0);
        let files = markdown(&d, &HashMap::new());
        assert!(file(&files, "projects/security.md").contains("- **Archived**"));
        assert!(file(&files, "projects/eu-region.md").contains("- [ ] ~~Go live~~ — cancelled"));
        assert!(file(&files, "inbox.md").contains("Book the offsite — archived"));
    }

    #[test]
    fn file_names_are_safe_and_never_collide() {
        let mut d = data();
        d.projects.push(project(3, "EU Region"));
        d.projects.push(project(4, "../../etc/passwd: ???"));
        d.notes.push(note(61, "1:1 with Priya", "second"));
        let files = markdown(&d, &HashMap::new());
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"projects/eu-region.md"));
        assert!(paths.contains(&"projects/eu-region-2.md"));
        assert!(paths.contains(&"projects/etc-passwd.md"), "{paths:?}");
        assert!(paths.contains(&"notes/2027-03-03-1-1-with-priya.md"));
        assert!(paths.contains(&"notes/2027-03-03-1-1-with-priya-2.md"));
        for p in &paths {
            assert!(!p.contains(".."), "{p}");
            assert!(p.matches('/').count() <= 1, "{p}");
        }
        let mut sorted = paths.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), paths.len());
    }

    #[test]
    fn a_note_reads_on_its_own() {
        let files = markdown(&data(), &HashMap::new());
        assert_eq!(
            file(&files, "notes/2027-03-03-1-1-with-priya.md"),
            "# 1:1 with Priya\n\n*1:1 · 2027-03-03*\n\nTalked about @Priya Nair.\n"
        );
    }

    #[test]
    fn user_text_cannot_break_the_markdown() {
        let mut d = data();
        d.projects[0].title = "A *bold*\n# move".into();
        d.tasks[1].title = "[link](x) | `code`".into();
        let files = markdown(&d, &HashMap::new());
        let eu = &files[0].text;
        assert!(eu.starts_with("# A \\*bold\\* # move\n"), "{eu}");
        assert!(eu.contains("- [ ] \\[link\\](x) \\| \\`code\\`"), "{eu}");
    }

    #[test]
    fn attachment_links_point_at_the_exported_files() {
        let pic = id(70);
        let gone = id(71);
        let body = format!(
            "![chart](attachment:{pic}) and [spec](attachment:{gone}) and ![x](attachment:not-an-id)"
        );
        let paths = HashMap::from([(pic, format!("attachments/{pic}/my chart (v2).png"))]);
        let out = rewrite_attachments(&body, "../", &paths);
        assert_eq!(
            out,
            format!(
                "![chart](../attachments/{pic}/my%20chart%20%28v2%29.png) and [spec](attachment:{gone}) and ![x](attachment:not-an-id)"
            )
        );
        // Nothing to rewrite: untouched.
        assert_eq!(
            rewrite_attachments("plain (attachment: text", "../", &paths),
            "plain (attachment: text"
        );
    }

    #[test]
    fn the_readme_mentions_markdown_only_when_it_was_written() {
        assert!(!readme(false).contains("projects/"));
        let with = readme(true);
        assert!(
            with.contains("`projects/`")
                && with.contains("`notes/`")
                && with.contains("`inbox.md`")
        );
        assert!(readme(false).contains("not encrypted"));
        assert!(readme(false).contains("manifest.json") && readme(false).contains("activity.json"));
    }

    #[test]
    fn a_whole_empty_export_is_fine() {
        assert!(markdown(&DataExport::default(), &HashMap::new()).is_empty());
    }
}
