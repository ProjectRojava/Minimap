//! Demo data (spec 24): a realistic, small company's worth of work for manual testing and for
//! the snapshot tests of the schedule, impact analysis, overview and weekly review.
//!
//! [`seed`] fills an **empty** database in one transaction. Everything is relative to the
//! `today` it is given (dates a few weeks either side, the week's events on the Monday of
//! `today`'s week), so a fixed `today` gives the same dataset every time. Titles, dates,
//! statuses, estimates and links are fixed; only the generated ids differ between runs.
//!
//! Contents: 2 objectives, 3 projects (EU Region is at risk), 40 tasks (cross-project `blocks`),
//! 8 people (me and 7 others, one overloaded) in 2 nested teams with reporting lines, 3 notes
//! (one 1:1 with mentions and a checklist), 5 decisions (one replaced by another) and 3
//! waiting-ons (one stale, one resolved this week).
//!
//! The history is made to read like a real week: the creation activity is dated three weeks
//! back, and this week's events (a task finished, due dates moved later, a task newly blocked,
//! a decision made, a waiting-on resolved) are dated `today`.

use std::collections::HashMap;

use minimap_core::this_week::monday_of;
use minimap_types::{
    mention_token, AssigneeChoice, CreateDecision, CreateNote, CreateObjective, CreatePerson,
    CreateProject, CreateTask, CreateTeam, CreateWaitingOn, DecisionStatus, DemoSummary, EdgeType,
    NewEdge, NodeRef, NodeType, NoteKind, ObjectiveStatus, Patch, ProjectStatus, TaskStatus,
    UpdateTask, UpdateWaitingOn,
};
use rusqlite::{params, Connection, Transaction};
use time::{Date, Duration, OffsetDateTime, Time};
use uuid::Uuid;

use crate::{
    convert::{id_s, ts_s},
    decisions, edges,
    error::{Result, StoreError},
    notes, objectives, people, projects, snapshot, tasks, teams, waiting_on,
};

/// A weekday `n` calendar days from `today`: a date that falls on a weekend moves to the next
/// Monday (for the future) or the previous Friday (for the past).
fn day(today: Date, n: i64) -> Date {
    let d = today + Duration::days(n);
    let from_monday = d.weekday().number_days_from_monday();
    match (from_monday, n >= 0) {
        (5, true) => d + Duration::days(2),
        (6, true) => d + Duration::days(1),
        (5, false) => d - Duration::days(1),
        (6, false) => d - Duration::days(2),
        _ => d,
    }
}

fn at(date: Date, hour: u8) -> OffsetDateTime {
    OffsetDateTime::new_utc(date, Time::from_hms(hour, 0, 0).unwrap_or(Time::MIDNIGHT))
}

/// Project keys.
const EU: &str = "eu";
const PF: &str = "pf";
const SEC: &str = "sec";

struct Task {
    key: &'static str,
    title: &'static str,
    project: Option<&'static str>,
    who: &'static str,
    status: TaskStatus,
    estimate: Option<f64>,
    /// Calendar days from today.
    start: Option<i64>,
    due: Option<i64>,
    priority: u8,
    /// `allocation_pct` of the assignment when it is not the whole of the person's time.
    allocation: Option<u32>,
}

#[allow(clippy::too_many_arguments)]
const fn t(
    key: &'static str,
    title: &'static str,
    project: Option<&'static str>,
    who: &'static str,
    status: TaskStatus,
    estimate: Option<f64>,
    start: Option<i64>,
    due: Option<i64>,
    priority: u8,
) -> Task {
    Task {
        key,
        title,
        project,
        who,
        status,
        estimate,
        start,
        due,
        priority,
        allocation: None,
    }
}

use TaskStatus::{Cancelled, Done, InProgress, Todo};

/// The 40 tasks, one per line. `e3` and `e13` change status during "this week" (see
/// [`this_week`]).
#[rustfmt::skip]
fn task_table() -> Vec<Task> {
    vec![
        // ---- EU Region (at risk: late by a couple of working days, one overdue, one blocked)
        t("e1", "Choose the EU cloud region", Some(EU), "priya", Done, Some(2.0), Some(-21), Some(-14), 2),
        t("e2", "Provision EU network and clusters", Some(EU), "tomas", InProgress, Some(6.0), Some(-4), Some(7), 1),
        t("e3", "Data residency design", Some(EU), "maya", InProgress, Some(3.0), Some(-16), Some(-9), 2),
        t("e4", "Replicate the customer data pipeline", Some(EU), "tomas", Todo, Some(8.0), None, Some(21), 1),
        t("e5", "Regional auth and tenancy", Some(EU), "maya", Todo, Some(5.0), None, Some(18), 2),
        t("e6", "EU CDN and DNS", Some(EU), "tomas", Todo, Some(3.0), None, Some(14), 3),
        t("e7", "Localised billing and VAT", Some(EU), "elena", Todo, Some(6.0), None, Some(16), 2),
        t("e8", "EU status page and monitoring", Some(EU), "jin", Todo, Some(4.0), None, Some(15), 3),
        t("e9", "Load test the EU stack", Some(EU), "maya", Todo, Some(4.0), None, Some(25), 2),
        t("e10", "Go-live checklist", Some(EU), "priya", Todo, None, None, Some(28), 1),
        t("e11", "Customer communications plan", Some(EU), "sam", InProgress, Some(2.0), Some(-9), Some(-3), 2),
        t("e12", "EU pricing", Some(EU), "sam", Todo, None, None, Some(10), 2),
        t("e13", "EU support runbook", Some(EU), "elena", Todo, Some(3.0), None, Some(12), 3),
        t("e14", "Beta customer onboarding", Some(EU), "sam", Todo, None, None, Some(30), 3),
        // ---- Platform Cost Reduction (healthy; Tomás carries too much of it)
        t("p1", "Baseline cloud spend report", Some(PF), "jin", Done, Some(2.0), Some(-28), Some(-21), 3),
        t("p2", "Right-size the compute fleet", Some(PF), "tomas", InProgress, Some(5.0), Some(-3), Some(8), 2),
        t("p3", "Move cold storage to a cheaper tier", Some(PF), "jin", Todo, Some(3.0), None, Some(20), 3),
        t("p4", "Gateway: design rate limiting", Some(PF), "maya", InProgress, Some(3.0), Some(-2), Some(5), 2),
        t("p5", "Gateway: build the routing layer", Some(PF), "maya", Todo, Some(6.0), None, Some(17), 2),
        t("p6", "Gateway: migrate the first five services", Some(PF), "tomas", Todo, Some(7.0), None, Some(35), 2),
        t("p7", "Autoscaling policy", Some(PF), "tomas", Todo, Some(4.0), None, Some(12), 2),
        t("p8", "Reserved-instance purchase plan", Some(PF), "priya", Todo, Some(2.0), None, Some(30), 3),
        t("p9", "Cost dashboard for teams", Some(PF), "elena", Todo, Some(4.0), Some(14), Some(24), 4),
        t("p10", "Decommission the legacy staging cluster", Some(PF), "jin", Todo, Some(2.0), Some(10), Some(40), 4),
        t("p11", "Remove unused load balancers", Some(PF), "tomas", Todo, Some(2.0), None, Some(9), 3),
        t("p12", "Savings review with finance", Some(PF), "me", Todo, Some(1.0), None, Some(45), 2),
        t("p13", "Spot-instance pilot", Some(PF), "tomas", Cancelled, Some(4.0), None, None, 4),
        t("p14", "Weekly cost report automation", Some(PF), "jin", Done, Some(2.0), Some(-8), Some(-2), 3),
        // ---- Security and Compliance
        t("s1", "Threat model for the EU stack", Some(SEC), "raj", Done, Some(3.0), Some(-20), Some(-14), 2),
        t("s2", "Choose a penetration-test vendor", Some(SEC), "raj", InProgress, Some(3.0), Some(-2), Some(3), 2),
        t("s3", "External penetration test", Some(SEC), "raj", Todo, Some(8.0), None, Some(21), 1),
        t("s4", "Security review sign-off", Some(SEC), "raj", Todo, Some(2.0), None, Some(24), 1),
        t("s5", "GDPR impact assessment", Some(SEC), "priya", Todo, Some(4.0), Some(7), Some(20), 2),
        // Half of Maya's time, to show an allocation below 100%.
        Task { allocation: Some(50), ..t("s6", "SOC 2 gap assessment", Some(SEC), "maya", Todo, Some(5.0), Some(21), Some(35), 3) },
        t("s7", "Quarterly access review", Some(SEC), "priya", Todo, Some(2.0), None, Some(14), 3),
        t("s8", "Rotate service credentials", Some(SEC), "jin", Todo, Some(2.0), Some(7), Some(14), 3),
        t("s9", "Security training roll-out", Some(SEC), "sam", Todo, Some(3.0), Some(14), Some(40), 4),
        t("s10", "Incident response drill", Some(SEC), "priya", Todo, Some(2.0), Some(10), Some(30), 3),
        // ---- Inbox
        t("i1", "Book the leadership offsite", None, "me", Todo, Some(1.0), None, Some(10), 4),
        t("i2", "Review the Q2 headcount plan", None, "me", Todo, Some(1.0), None, Some(6), 2),
    ]
}

/// (blocker, blocked, lag in working days). Several cross projects.
#[rustfmt::skip]
const BLOCKS: &[(&str, &str, u32)] = &[
    ("e2", "e4", 2), ("e2", "e5", 0), ("e2", "e6", 0), ("e2", "e8", 0), ("e2", "e13", 0),
    ("e3", "e4", 0), ("e4", "e9", 0), ("e5", "e9", 0), ("e6", "e9", 0), ("e9", "e10", 0),
    ("e9", "e14", 0),
    // across projects
    ("s4", "e10", 0), ("e5", "s4", 0), ("p5", "e5", 0), ("p7", "e9", 0),
    // within projects
    ("p1", "p3", 0), ("p1", "p9", 0), ("p4", "p5", 0), ("p5", "p6", 0), ("p6", "p12", 0),
    ("s2", "s3", 0), ("s3", "s4", 1), ("s5", "s4", 0),
];

/// Done tasks and when they were finished (calendar days from today; `None` = the Monday of
/// this week). The ones still open at the start of the week are finished in [`this_week`].
const FINISHED: &[(&str, Option<i64>)] = &[
    ("e1", Some(-14)),
    ("s1", Some(-14)),
    ("p1", Some(-21)),
    ("p14", None),
];

struct Seeder<'a> {
    tx: &'a Transaction<'a>,
    today: Date,
    people: HashMap<&'static str, Uuid>,
    tasks: HashMap<&'static str, Uuid>,
    projects: HashMap<&'static str, Uuid>,
}

impl Seeder<'_> {
    fn link(
        &self,
        edge_type: EdgeType,
        from: NodeRef,
        to: NodeRef,
        attrs: serde_json::Value,
    ) -> Result<()> {
        edges::add_in_tx(
            self.tx,
            NewEdge {
                edge_type,
                from,
                to,
                attrs,
            },
        )?;
        Ok(())
    }

    fn person_ref(&self, key: &str) -> NodeRef {
        NodeRef::new(NodeType::Person, self.people[key])
    }

    fn project_ref(&self, key: &str) -> NodeRef {
        NodeRef::new(NodeType::Project, self.projects[key])
    }

    fn task_ref(&self, key: &str) -> NodeRef {
        NodeRef::new(NodeType::Task, self.tasks[key])
    }

    fn mention(&self, label: &str, node: NodeRef) -> String {
        mention_token(label, node.id)
    }
}

/// Adds the demo data to an empty database (nothing but, possibly, first run's "me").
///
/// All or nothing: one transaction. `today` anchors every date; pass [`crate::today`] for the
/// real thing or a fixed date for tests.
pub fn seed(conn: &mut Connection, today: Date) -> Result<DemoSummary> {
    if !snapshot::is_pristine(conn)? {
        return Err(StoreError::Invalid(
            "Demo data can only be added to an empty database, and this one already has data. \
             Start with a fresh data folder (or restore an empty backup) to try it."
                .into(),
        ));
    }
    let me = people::ensure_self(conn, "Me")?;
    let tx = conn.transaction()?;
    let monday = monday_of(today);
    let d = |n: i64| day(today, n);

    let mut s = Seeder {
        tx: &tx,
        today,
        people: HashMap::from([("me", me.id)]),
        tasks: HashMap::new(),
        projects: HashMap::new(),
    };

    // ------------------------------------------------------------------ people
    let person = |name: &str, role: &str, hours: f64| CreatePerson {
        name: name.into(),
        role_title: role.into(),
        email: Some(format!(
            "{}@example.com",
            name.split_whitespace()
                .next()
                .unwrap_or("someone")
                .to_lowercase()
        )),
        weekly_capacity_hours: Some(hours),
        is_self: false,
        notes: String::new(),
    };
    for (key, name, role, hours) in [
        ("priya", "Priya Nair", "Engineering Manager, Platform", 40.0),
        ("raj", "Raj Patel", "Security Lead", 40.0),
        ("maya", "Maya Chen", "Staff Engineer", 40.0),
        ("tomas", "Tomás Alvarez", "Backend Engineer", 40.0),
        ("elena", "Elena Rossi", "Frontend Engineer", 40.0),
        ("jin", "Jin Park", "Data Engineer", 40.0),
        ("sam", "Sam Okafor", "Product Manager", 40.0),
    ] {
        let p = people::create_in_tx(&tx, person(name, role, hours))?;
        s.people.insert(key, p.id);
    }

    // ------------------------------------------------------------------- teams
    let engineering = teams::create_in_tx(
        &tx,
        CreateTeam {
            name: "Engineering".into(),
            description: "Everyone who builds and runs the product.".into(),
            parent_team_id: None,
        },
    )?;
    let platform = teams::create_in_tx(
        &tx,
        CreateTeam {
            name: "Platform".into(),
            description: "Infrastructure, the API gateway and the EU region.".into(),
            parent_team_id: Some(engineering.id),
        },
    )?;
    let team = |id: Uuid| NodeRef::new(NodeType::Team, id);
    for (who, of, role) in [
        ("me", engineering.id, "lead"),
        ("raj", engineering.id, "member"),
        ("sam", engineering.id, "member"),
        ("priya", platform.id, "lead"),
        ("maya", platform.id, "member"),
        ("tomas", platform.id, "member"),
        ("elena", platform.id, "member"),
        ("jin", platform.id, "member"),
    ] {
        s.link(
            EdgeType::MemberOf,
            s.person_ref(who),
            team(of),
            serde_json::json!({ "role": role }),
        )?;
    }
    for (who, manager) in [
        ("priya", "me"),
        ("raj", "me"),
        ("sam", "me"),
        ("maya", "priya"),
        ("tomas", "priya"),
        ("elena", "priya"),
        ("jin", "maya"),
    ] {
        s.link(
            EdgeType::ReportsTo,
            s.person_ref(who),
            s.person_ref(manager),
            serde_json::json!({}),
        )?;
    }

    // -------------------------------------------------------------- objectives
    let launch = objectives::create_in_tx(
        &tx,
        CreateObjective {
            title: "Launch in the EU".into(),
            description: "Serve European customers from a European region by the end of the quarter, with the security and data-residency sign-offs they ask for.".into(),
            target_date: Some(d(75)),
            status: Some(ObjectiveStatus::AtRisk),
            priority: Some(1),
        },
    )?;
    let costs = objectives::create_in_tx(
        &tx,
        CreateObjective {
            title: "Cut platform costs by 20%".into(),
            description: "Bring the monthly cloud bill down a fifth without hurting reliability."
                .into(),
            target_date: Some(d(120)),
            status: Some(ObjectiveStatus::OnTrack),
            priority: Some(2),
        },
    )?;
    let objective = |id: Uuid| NodeRef::new(NodeType::Objective, id);

    // ---------------------------------------------------------------- projects
    for (key, title, description, owner, start, target, priority) in [
        (
            EU,
            "EU Region",
            "Stand up the product in a European region.",
            "priya",
            -28,
            26,
            1,
        ),
        (
            PF,
            "Platform Cost Reduction",
            "Right-size, consolidate and put an API gateway in front of the services.",
            "maya",
            -35,
            60,
            2,
        ),
        (
            SEC,
            "Security and Compliance",
            "The reviews, tests and assessments the EU launch and SOC 2 need.",
            "raj",
            -30,
            45,
            2,
        ),
    ] {
        let p = projects::create_in_tx(
            &tx,
            CreateProject {
                title: title.into(),
                slug: None,
                description: description.into(),
                owner_person_id: Some(s.people[owner]),
                start_date: Some(d(start)),
                target_date: Some(d(target)),
                status: Some(ProjectStatus::Active),
                priority: Some(priority),
            },
        )?;
        s.projects.insert(key, p.id);
    }
    for (project, objective_node, weight) in [
        (EU, objective(launch.id), 1.0),
        (SEC, objective(launch.id), 0.5),
        (PF, objective(costs.id), 1.0),
    ] {
        s.link(
            EdgeType::ContributesTo,
            s.project_ref(project),
            objective_node,
            serde_json::json!({ "weight": weight }),
        )?;
    }
    s.link(
        EdgeType::DependsOn,
        s.project_ref(EU),
        s.project_ref(SEC),
        serde_json::json!({ "note": "The security sign-off gates go-live." }),
    )?;

    // ------------------------------------------------------------------- tasks
    for spec in task_table() {
        let assignee = s.people[spec.who];
        let task = tasks::create_in_tx(
            &tx,
            CreateTask {
                title: spec.title.into(),
                assignee: if spec.allocation.is_some() {
                    AssigneeChoice::Nobody
                } else {
                    AssigneeChoice::Person(assignee)
                },
                description: String::new(),
                project_id: spec.project.map(|p| s.projects[p]),
                status: Some(spec.status),
                estimate_days: spec.estimate,
                start_date: spec.start.map(d),
                due_date: spec.due.map(d),
                priority: Some(spec.priority),
            },
        )?;
        if let Some(pct) = spec.allocation {
            s.link(
                EdgeType::AssignedTo,
                NodeRef::new(NodeType::Task, task.id),
                s.person_ref(spec.who),
                serde_json::json!({ "allocation_pct": pct }),
            )?;
        }
        s.tasks.insert(spec.key, task.id);
    }
    for (from, to, lag) in BLOCKS {
        let attrs = if *lag > 0 {
            serde_json::json!({ "lag_days": lag })
        } else {
            serde_json::json!({})
        };
        s.link(EdgeType::Blocks, s.task_ref(from), s.task_ref(to), attrs)?;
    }

    // ------------------------------------------------------------------- notes
    let priya = s.person_ref("priya");
    let tomas = s.person_ref("tomas");
    let elena = s.person_ref("elena");
    let raj = s.person_ref("raj");
    let eu = s.project_ref(EU);
    let sec = s.project_ref(SEC);
    let one_on_one = format!(
        "## Agenda\n\n- {eu} is about two days behind: the data pipeline waits on the clusters.\n- {tomas} has too much on; move the CDN and DNS work to {elena}?\n- Go-live date: hold or move?\n\n## Next\n\n- [ ] Rebalance {tomas}'s tasks\n- [ ] Ask {raj} for a sign-off date\n- [x] Share the cost baseline with finance\n",
        eu = s.mention("EU Region", eu),
        tomas = s.mention("Tomás Alvarez", tomas),
        elena = s.mention("Elena Rossi", elena),
        raj = s.mention("Raj Patel", raj),
    );
    notes::create_in_tx(
        &tx,
        CreateNote {
            title: "1:1 with Priya".into(),
            body: one_on_one,
            note_date: Some(d(0)),
            kind: Some(NoteKind::OneOnOne),
        },
    )?;
    let leadership = format!(
        "Attendees: {priya}, {raj}.\n\nThe {eu} go-live depends on the {sec} work. Sign-off is the long pole; the penetration test cannot start until the vendor is chosen.\n\nDecided to keep the date and add people to the sign-off path rather than move it.\n",
        priya = s.mention("Priya Nair", priya),
        raj = s.mention("Raj Patel", raj),
        eu = s.mention("EU Region", eu),
        sec = s.mention("Security and Compliance", sec),
    );
    notes::create_in_tx(
        &tx,
        CreateNote {
            title: "Leadership sync: EU launch risk".into(),
            body: leadership,
            note_date: Some(d(-5)),
            kind: Some(NoteKind::Meeting),
        },
    )?;
    notes::create_in_tx(
        &tx,
        CreateNote {
            title: "Gateway: build or buy?".into(),
            body: "Compared two vendors and a small in-house router.\n\n- Vendors: faster to start, per-request pricing grows with traffic.\n- In-house: more work now, flat cost, we already know the services.\n\nLeaning in-house; see the decision.\n".into(),
            note_date: Some(d(-12)),
            kind: Some(NoteKind::General),
        },
    )?;

    // --------------------------------------------------------------- decisions
    let decision = |title: &str, context: &str, what: &str, why: &str, on: Option<i64>, status| {
        CreateDecision {
            title: title.into(),
            context: context.into(),
            decision: what.into(),
            rationale: why.into(),
            decided_on: on.map(d),
            status: Some(status),
        }
    };
    let frankfurt = decisions::create_in_tx(
        &tx,
        decision(
            "Frankfurt as the primary EU region",
            "Customers want their data in the EU; two regions were short-listed.",
            "Use Frankfurt as the primary region and Dublin as the later second one.",
            "Lowest latency to most of the customers we are talking to, and the services we need are all available there.",
            Some(-20),
            DecisionStatus::Decided,
        ),
    )?;
    let gateway = decisions::create_in_tx(
        &tx,
        decision(
            "Build the API gateway in-house",
            "Vendor gateways price per request; we expect traffic to grow.",
            "Build a small routing layer ourselves.",
            "Flat cost, and it keeps the migration of the first services in our hands.",
            Some(-12),
            DecisionStatus::Decided,
        ),
    )?;
    let managed = decisions::create_in_tx(
        &tx,
        decision(
            "Run EU data on managed Postgres",
            "We need a database for the EU stack and little time.",
            "Use the cloud provider's managed Postgres.",
            "Least operational work.",
            Some(-18),
            DecisionStatus::Decided,
        ),
    )?;
    let pause = decisions::create_in_tx(
        &tx,
        decision(
            "Pause the spot-instance pilot",
            "The pilot saved little and caused two incidents.",
            "Stop the pilot and revisit after the right-sizing work.",
            "Right-sizing gives most of the saving with none of the risk.",
            None,
            DecisionStatus::Proposed,
        ),
    )?;
    for (decision_id, target) in [
        (frankfurt.id, eu),
        (gateway.id, s.project_ref(PF)),
        (managed.id, eu),
        (pause.id, s.project_ref(PF)),
    ] {
        s.link(
            EdgeType::Affects,
            NodeRef::new(NodeType::Decision, decision_id),
            target,
            serde_json::json!({}),
        )?;
    }

    // ------------------------------------------------------------- waiting-ons
    let waiting = |what: &str, who: &str, asked: i64, expected: Option<i64>| CreateWaitingOn {
        description: what.into(),
        person_id: s.people[who],
        asked_on: Some(d(asked)),
        expected_by: expected.map(d),
        follow_up_on: None,
    };
    let signoff = waiting_on::create_in_tx(
        &tx,
        waiting("Security review sign-off", "raj", -12, Some(-2)),
    )?;
    let pricing = waiting_on::create_in_tx(
        &tx,
        waiting("EU pricing approval from finance", "sam", -3, Some(4)),
    )?;
    let assets = waiting_on::create_in_tx(
        &tx,
        waiting("Brand assets for the EU launch page", "elena", -8, Some(-1)),
    )?;
    for (w, about) in [
        (signoff.id, s.task_ref("s4")),
        (pricing.id, s.task_ref("e12")),
        (assets.id, eu),
    ] {
        s.link(
            EdgeType::About,
            NodeRef::new(NodeType::WaitingOn, w),
            about,
            serde_json::json!({}),
        )?;
    }

    // Everything so far is the past: date its history three weeks back, in order.
    let split: i64 = tx.query_row("SELECT COALESCE(MAX(rowid), 0) FROM activity", [], |r| {
        r.get(0)
    })?;

    // ------------------------------------------------------------- this week
    this_week(&s, monday, managed.id, assets.id)?;

    backdate(&tx, today, monday, split)?;
    for (key, when) in FINISHED {
        let finished = match when {
            Some(n) => at(d(*n), 17),
            None => at(monday, 15),
        };
        tx.execute(
            "UPDATE tasks SET completed_at = ?2 WHERE id = ?1",
            params![id_s(s.tasks[key]), ts_s(finished)],
        )?;
    }
    // The task finished at the start of today's session, after the week began.
    tx.execute(
        "UPDATE tasks SET completed_at = ?2 WHERE id = ?1",
        params![id_s(s.tasks["e3"]), ts_s(at(today, 9))],
    )?;

    let count = |table: &str| -> Result<u32> {
        Ok(tx.query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE archived_at IS NULL"),
            [],
            |r| r.get(0),
        )?)
    };
    let summary = DemoSummary {
        objectives: count("objectives")?,
        projects: count("projects")?,
        tasks: count("tasks")?,
        people: count("people")?,
        teams: count("teams")?,
        notes: count("notes")?,
        decisions: count("decisions")?,
        waiting_ons: count("waiting_on")?,
        links: count("edges")?,
    };
    tx.commit()?;
    Ok(summary)
}

/// What happened this week: a task finished, due dates moved later, a task newly blocked, a
/// better decision that replaces an older one, a waiting-on resolved.
fn this_week(s: &Seeder, monday: Date, managed_postgres: Uuid, assets: Uuid) -> Result<()> {
    let tx = s.tx;
    let task = |key: &str, patch: UpdateTask| -> Result<()> {
        tasks::update_in_tx(tx, s.tasks[key], patch)?;
        Ok(())
    };
    task(
        "e3",
        UpdateTask {
            status: Some(TaskStatus::Done),
            ..Default::default()
        },
    )?;
    // Two due dates moved later (a real slip: the pipeline waits on the clusters, sign-off on
    // the pen test).
    task(
        "e4",
        UpdateTask {
            due_date: Patch::Set(day(s.today, 26)),
            ..Default::default()
        },
    )?;
    task(
        "s4",
        UpdateTask {
            due_date: Patch::Set(day(s.today, 31)),
            ..Default::default()
        },
    )?;
    // Newly blocked: it waits on the clusters.
    task(
        "e13",
        UpdateTask {
            status: Some(TaskStatus::Blocked),
            ..Default::default()
        },
    )?;
    // A better decision, made this week, replaces the managed-database one.
    let self_hosted = decisions::create_in_tx(
        tx,
        CreateDecision {
            title: "Run EU data on self-managed Postgres".into(),
            context: "The managed service can't promise the data stays in the region during failover.".into(),
            decision: "Run Postgres ourselves in Frankfurt, with replicas in the same region.".into(),
            rationale: "Data residency is a hard requirement for the EU customers; the extra operations work is acceptable.".into(),
            decided_on: Some(monday),
            status: Some(DecisionStatus::Decided),
        },
    )?;
    s.link(
        EdgeType::Affects,
        NodeRef::new(NodeType::Decision, self_hosted.id),
        s.project_ref(EU),
        serde_json::json!({}),
    )?;
    decisions::supersede_in_tx(tx, self_hosted.id, managed_postgres)?;
    waiting_on::update_in_tx(
        tx,
        assets,
        UpdateWaitingOn {
            resolved_on: Patch::Set(monday),
            ..Default::default()
        },
    )?;
    Ok(())
}

/// Dates the history: activity up to `split` (rowid) three weeks before this Monday, in order;
/// this week's events at 09:00 on `today`; nodes created three weeks back (so lists sort the way
/// they were made) and last changed then.
fn backdate(tx: &Transaction, today: Date, monday: Date, split: i64) -> Result<()> {
    let past = at(monday - Duration::days(21), 9);
    let now = at(today, 9);
    let mut ids: Vec<i64> = Vec::new();
    {
        let mut stmt = tx.prepare("SELECT rowid FROM activity ORDER BY rowid")?;
        for row in stmt.query_map([], |r| r.get::<_, i64>(0))? {
            ids.push(row?);
        }
    }
    for rowid in ids {
        let when = if rowid <= split {
            past + Duration::milliseconds(rowid)
        } else {
            now + Duration::milliseconds(rowid - split)
        };
        tx.execute(
            "UPDATE activity SET at = ?2 WHERE rowid = ?1",
            params![rowid, ts_s(when)],
        )?;
    }
    for table in [
        "objectives",
        "projects",
        "tasks",
        "teams",
        "notes",
        "decisions",
        "waiting_on",
    ] {
        let mut ids: Vec<i64> = Vec::new();
        {
            let mut stmt = tx.prepare(&format!("SELECT rowid FROM {table} ORDER BY rowid"))?;
            for row in stmt.query_map([], |r| r.get::<_, i64>(0))? {
                ids.push(row?);
            }
        }
        for rowid in ids {
            let when = ts_s(past + Duration::milliseconds(rowid));
            tx.execute(
                &format!("UPDATE {table} SET created_at = ?2, updated_at = ?2 WHERE rowid = ?1"),
                params![rowid, when],
            )?;
        }
    }
    // The people made here (not first run's "me").
    let mut ids: Vec<i64> = Vec::new();
    {
        let mut stmt = tx.prepare("SELECT rowid FROM people WHERE is_self = 0 ORDER BY rowid")?;
        for row in stmt.query_map([], |r| r.get::<_, i64>(0))? {
            ids.push(row?);
        }
    }
    for rowid in ids {
        let when = ts_s(past + Duration::milliseconds(rowid));
        tx.execute(
            "UPDATE people SET created_at = ?2, updated_at = ?2 WHERE rowid = ?1",
            params![rowid, when],
        )?;
    }
    Ok(())
}
