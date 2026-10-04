//! Quick-add (spec 12): the preview the UI shows, the choices it sends back, and the plan the
//! store commits. The grammar itself is in `docs/quick-add-grammar.md`.

use serde::{Deserialize, Serialize};
use time::Date;
use uuid::Uuid;

use crate::{DecisionStatus, NodeRef, NodeSummary, NodeType, NoteKind};

/// What a line of quick-add creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickKind {
    Task,
    Project,
    Wait,
    Note,
    Decision,
}

/// A node the parser may match a name against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectoryEntry {
    pub node: NodeRef,
    pub label: String,
    /// A project's handle (`api-launch`).
    pub handle: Option<String>,
    pub is_self: bool,
}

/// What the user decided about one reference that wasn't clear (`QuickRef::key`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickChoice {
    pub key: String,
    pub pick: QuickPick,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickPick {
    /// This existing node.
    Node(Uuid),
    /// Make a new one with the name as typed.
    Create,
    /// Leave it out (what that means is `skip` on the pending reference).
    Skip,
}

/// How one `@name`, `#project` or `key:name` reference stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefState {
    Resolved {
        node: NodeSummary,
    },
    /// Will be created when committed.
    Created {
        name: String,
    },
    /// Left out by choice; `effect` says what happens instead.
    Skipped {
        effect: String,
    },
    /// Needs an answer before it can be committed. No options = nothing matched.
    Pending {
        options: Vec<NodeSummary>,
        /// The options are guesses at a misspelling, not matches.
        guess: bool,
        can_create: bool,
        /// What leaving it out would mean; `None` when the reference is required.
        skip: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickRef {
    /// Identifies the reference in `QuickChoice`s (role and typed text).
    pub key: String,
    /// "Assignee", "Project", "Blocks"...
    pub label: String,
    /// The text as typed, without its `@`/`#`.
    pub query: String,
    pub state: RefState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickDetail {
    pub label: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickPreview {
    pub kind: QuickKind,
    /// The task or project title, the wait's description, the note or decision title.
    pub title: String,
    pub details: Vec<QuickDetail>,
    pub refs: Vec<QuickRef>,
    /// Things wrong with the line itself; nothing can be committed while there are any.
    pub problems: Vec<String>,
    /// No problems and no reference waiting for an answer.
    pub ready: bool,
}

// ------------------------------------------------------------------ the plan

/// A node to reference: one that exists, or the n-th of `QuickPlan::new_nodes`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ref {
    Existing(NodeRef),
    New(u32),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewNode {
    /// Person, project, objective or task.
    pub node_type: NodeType,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickAssignee {
    /// The self person, like any new task.
    Default,
    Person(Ref),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteMention {
    pub label: String,
    pub target: Ref,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuickMain {
    Task {
        title: String,
        priority: Option<u8>,
        start_date: Option<Date>,
        due_date: Option<Date>,
        estimate_days: Option<f64>,
        project: Option<Ref>,
        assignee: QuickAssignee,
        /// Tasks this one blocks.
        blocks: Vec<Ref>,
        objectives: Vec<Ref>,
    },
    Project {
        title: String,
        priority: Option<u8>,
        start_date: Option<Date>,
        target_date: Option<Date>,
        owner: Option<Ref>,
        objectives: Vec<Ref>,
    },
    Wait {
        description: String,
        person: Ref,
        expected_by: Option<Date>,
        about: Option<Ref>,
    },
    Note {
        title: String,
        kind: NoteKind,
        note_date: Option<Date>,
        mentions: Vec<NoteMention>,
    },
    Decision {
        title: String,
        status: Option<DecisionStatus>,
        decided_on: Option<Date>,
        affects: Vec<Ref>,
    },
}

/// Everything one quick-add writes, committed in one transaction: first the nodes the line
/// asked to create (people, projects, objectives, tasks), then the main one, then its links.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickPlan {
    pub new_nodes: Vec<NewNode>,
    pub main: QuickMain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuickResult {
    /// The node the line was about; the UI opens it.
    pub node: NodeSummary,
    /// Extra nodes created for unresolved references.
    pub created: Vec<NodeSummary>,
}
