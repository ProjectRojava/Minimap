//! The tasks list (also the inbox): filters, inline editing, keyboard shortcuts,
//! and the new-task form with pasted-list preview.

use std::str::FromStr;

use leptos::{prelude::*, task::spawn_local, web_sys};
use minimap_types::{
    timefmt::parse_date, AppError, AssigneeChoice, CreateTask, NodeRef, NodeType, Patch,
    TaskFilter, TaskRow, TaskStatus, UpdateTask, Uuid,
};
use wasm_bindgen::JsCast;

use crate::{
    api,
    components::{
        approaching::FilteredApproach,
        backburner::{set_backburner, since_title},
        focus::{is_on, set_focus, toggle_choice, FocusStar},
        fog::confirm_row,
        form::{
            date_patch, DateField, SelectField, BUTTON, BUTTON_ON, BUTTON_PRIMARY, BUTTON_SOFT,
            COMPACT_INPUT, INPUT,
        },
        node_row::NodeRow,
        objective_colour::{use_objective_colours, ObjectiveDot},
        orientation::{Kind, MineBanner, Orientation, WhyChip},
        page::{column_head, EmptyState, Hints, PageHeader, Tone, FILTER_BAR},
        task_board::today,
        task_type::{remember_new_type, use_task_types, TypeChip, CHOOSE_TYPE},
        waiting_badge::WaitingChip,
    },
    labels::{deadline_heat, priority_option, task_status_label, task_status_tone},
    state::{finish, finish_task, DataVersion, FinishDecision, ListNav, Selection, Toasts},
};

const COLS: &str =
    "grid w-full items-center gap-2 grid-cols-[6.5rem_3.5rem_minmax(0,1fr)_9rem_8rem_8.5rem]";

/// Next status for the `s` key: to do -> in progress -> done -> to do.
pub fn next_status(s: TaskStatus) -> TaskStatus {
    match s {
        TaskStatus::Todo | TaskStatus::Blocked | TaskStatus::Cancelled => {
            if s == TaskStatus::Todo {
                TaskStatus::InProgress
            } else {
                TaskStatus::Todo
            }
        }
        TaskStatus::InProgress => TaskStatus::Done,
        TaskStatus::Done => TaskStatus::Todo,
    }
}

fn focus_element(id: &str) {
    if let Some(el) = document()
        .get_element_by_id(id)
        .and_then(|e| e.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = el.focus();
    }
}

fn status_options() -> Vec<(String, String)> {
    TaskStatus::ALL
        .iter()
        .map(|s| (s.as_str().to_owned(), task_status_label(*s).to_owned()))
        .collect()
}

fn any_status_options() -> Vec<(String, String)> {
    std::iter::once((String::new(), "Any status".to_owned()))
        .chain(status_options())
        .collect()
}

fn priority_options() -> Vec<(String, String)> {
    (1..=5u8)
        .map(|p| (p.to_string(), priority_option(p)))
        .collect()
}

/// How the Kanban board orders the cards of every column (one setting for all of them, kept on
/// this computer).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoardSort {
    /// Open columns by deadline (soonest first), Done newest finished first.
    #[default]
    Default,
    /// Earliest deadline first; tasks without one last.
    DueSoonest,
    /// Latest deadline first; tasks without one last.
    DueLatest,
    /// P1 first, then earliest deadline.
    Priority,
}

impl BoardSort {
    pub const ALL: [BoardSort; 4] = [
        BoardSort::Default,
        BoardSort::DueSoonest,
        BoardSort::DueLatest,
        BoardSort::Priority,
    ];

    pub fn id(self) -> &'static str {
        match self {
            BoardSort::Default => "default",
            BoardSort::DueSoonest => "due_soonest",
            BoardSort::DueLatest => "due_latest",
            BoardSort::Priority => "priority",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            BoardSort::Default => "Sort: default",
            BoardSort::DueSoonest => "Sort: due date, soonest",
            BoardSort::DueLatest => "Sort: due date, latest",
            BoardSort::Priority => "Sort: priority",
        }
    }

    /// The dropdown's options.
    pub fn options() -> Vec<(String, String)> {
        Self::ALL
            .into_iter()
            .map(|s| (s.id().to_owned(), s.label().to_owned()))
            .collect()
    }
}

const BOARD_SORT_KEY: &str = "minimap.board-sort";

/// The remembered order (a convenience of this computer: nothing breaks without it).
fn load_board_sort() -> BoardSort {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(BOARD_SORT_KEY).ok().flatten())
        .and_then(|id| BoardSort::from_id(&id))
        .unwrap_or_default()
}

fn save_board_sort(sort: BoardSort) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(BOARD_SORT_KEY, sort.id());
    }
}

const FILTERS_KEY: &str = "minimap.task-filters";

/// The dropdown filters as remembered on this computer (not the search text: it is typed for one
/// look, and a forgotten word would hide tasks for no visible reason).
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Remembered {
    #[serde(default)]
    status: String,
    #[serde(default)]
    project: String,
    #[serde(default)]
    assignee: String,
    #[serde(default)]
    task_type: String,
    #[serde(default)]
    due_from: String,
    #[serde(default)]
    due_to: String,
    #[serde(default)]
    show_closed: bool,
    #[serde(default)]
    show_parked: bool,
}

fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

/// The filters of the Tasks screen. They live outside the list and the board so switching
/// between the two keeps what you searched for.
#[derive(Clone, Copy)]
pub struct TaskFilters {
    pub text: RwSignal<String>,
    pub status: RwSignal<String>,
    pub project: RwSignal<String>,
    pub assignee: RwSignal<String>,
    pub due_from: RwSignal<String>,
    pub due_to: RwSignal<String>,
    /// A task type's id; empty = any type.
    pub task_type: RwSignal<String>,
    /// List: show done tasks. Board: show the Cancelled column.
    pub show_closed: RwSignal<bool>,
    /// List: show tasks on the backburner (spec 59). The board has them in a column.
    pub show_parked: RwSignal<bool>,
    /// The board's card order (every column).
    pub board_sort: RwSignal<BoardSort>,
}

impl TaskFilters {
    pub fn new() -> Self {
        Self {
            text: RwSignal::new(String::new()),
            status: RwSignal::new(String::new()),
            project: RwSignal::new(String::new()),
            assignee: RwSignal::new(String::new()),
            due_from: RwSignal::new(String::new()),
            due_to: RwSignal::new(String::new()),
            task_type: RwSignal::new(String::new()),
            show_closed: RwSignal::new(false),
            show_parked: RwSignal::new(false),
            board_sort: RwSignal::new(load_board_sort()),
        }
    }

    /// The filters as last left on this computer (a convenience: nothing breaks without it), kept
    /// up to date from now on. They stay until *Reset* puts them back. Call once, in the Tasks
    /// screen: the Inbox keeps its own.
    pub fn remembered() -> Self {
        let filters = Self::new();
        let saved: Remembered = local_storage()
            .and_then(|s| s.get_item(FILTERS_KEY).ok().flatten())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        filters.status.set(saved.status);
        filters.project.set(saved.project);
        filters.assignee.set(saved.assignee);
        filters.task_type.set(saved.task_type);
        filters.due_from.set(saved.due_from);
        filters.due_to.set(saved.due_to);
        filters.show_closed.set(saved.show_closed);
        filters.show_parked.set(saved.show_parked);
        Effect::new(move |_| {
            let now = Remembered {
                status: filters.status.get(),
                project: filters.project.get(),
                assignee: filters.assignee.get(),
                task_type: filters.task_type.get(),
                due_from: filters.due_from.get(),
                due_to: filters.due_to.get(),
                show_closed: filters.show_closed.get(),
                show_parked: filters.show_parked.get(),
            };
            if let (Some(storage), Ok(json)) = (local_storage(), serde_json::to_string(&now)) {
                let _ = storage.set_item(FILTERS_KEY, &json);
            }
        });
        filters
    }

    /// The request for the current filters (reads the signals, so call it inside a resource).
    /// The board asks for finished tasks too and ignores the status filter: its columns are the
    /// statuses.
    pub fn request(&self, inbox: bool, board: bool) -> TaskFilter {
        let t = self.text.get();
        TaskFilter {
            status: if board {
                None
            } else {
                TaskStatus::from_str(&self.status.get()).ok()
            },
            project_id: Uuid::parse_str(&self.project.get()).ok(),
            assignee_id: Uuid::parse_str(&self.assignee.get()).ok(),
            due_from: parse_date(&self.due_from.get()).ok(),
            due_to: parse_date(&self.due_to.get()).ok(),
            text: (!t.trim().is_empty()).then_some(t),
            no_project: inbox,
            include_closed: board || self.show_closed.get(),
            task_type: Some(self.task_type.get()).filter(|t| !t.is_empty()),
            hide_parked: !board && !self.show_parked.get(),
        }
    }
}

impl Default for TaskFilters {
    fn default() -> Self {
        Self::new()
    }
}

/// The filter bar of the Tasks and Inbox screens. On the board there is no status filter (the
/// columns are the statuses) and the checkbox reveals the Cancelled column.
#[component]
pub fn FilterControls(filters: TaskFilters, inbox: bool, board: bool) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let people = LocalResource::new(move || {
        version.track();
        api::list_people()
    });
    let projects = LocalResource::new(move || {
        version.track();
        api::list_node_summaries(NodeType::Project)
    });
    let TaskFilters {
        text,
        status,
        project,
        assignee,
        due_from,
        due_to,
        task_type,
        show_closed,
        show_parked,
        board_sort,
    } = filters;
    let types = use_task_types();
    // Bumped by "Reset" (or by a dropped choice, below) so the dropdowns and date boxes, which keep their own value, start over.
    let epoch = RwSignal::new(0u32);
    // A remembered project or person that no longer exists (archived or deleted since) would
    // filter on something the dropdown cannot show, so it is dropped.
    if !inbox {
        Effect::new(move |_| {
            if let Some(Ok(list)) = projects.get() {
                let wanted = project.get_untracked();
                if !wanted.is_empty() && !list.iter().any(|p| p.node.id.to_string() == wanted) {
                    project.set(String::new());
                    epoch.update(|e| *e += 1);
                }
            }
        });
    }
    Effect::new(move |_| {
        if let Some(Ok(list)) = people.get() {
            let wanted = assignee.get_untracked();
            if !wanted.is_empty() && !list.iter().any(|p| p.person.id.to_string() == wanted) {
                assignee.set(String::new());
                epoch.update(|e| *e += 1);
            }
        }
    });
    let changed = move || {
        changed_count(&Changed {
            text: text.get(),
            status: status.get(),
            project: project.get(),
            assignee: assignee.get(),
            task_type: task_type.get(),
            due_from: due_from.get(),
            due_to: due_to.get(),
            show_closed: show_closed.get(),
            show_parked: show_parked.get(),
            sort: board_sort.get(),
            board,
        })
    };
    let any_changed = move || changed() > 0;
    let reset = move |_| {
        text.set(String::new());
        status.set(String::new());
        project.set(String::new());
        assignee.set(String::new());
        task_type.set(String::new());
        due_from.set(String::new());
        due_to.set(String::new());
        show_closed.set(false);
        show_parked.set(false);
        board_sort.set(BoardSort::Default);
        save_board_sort(BoardSort::Default);
        epoch.update(|e| *e += 1);
    };
    let on = move |active: bool| if active { MARKED } else { "" };
    view! {
        <div class=FILTER_BAR>
            <input class=move || format!("{COMPACT_INPUT} w-44 {}", on(!text.get().trim().is_empty()))
                   type="search" placeholder="Search tasks"
                   prop:value=move || text.get() on:input=move |ev| text.set(event_target_value(&ev)) />
            {(!board).then(|| view! {
                {move || { epoch.track(); view! {
                    <span class=move || on(!status.get().is_empty())>
                        <SelectField compact=true current=status.get_untracked()
                            options=any_status_options() on_change=move |v: String| status.set(v) />
                    </span>
                } }}
            })}
            {(!inbox).then(|| view! {
                {move || {
                    epoch.track();
                    let colours = use_objective_colours();
                    let options: Vec<(String, String)> = std::iter::once((String::new(), "Any project".to_owned()))
                        .chain(match projects.get() {
                            Some(Ok(p)) => p.iter().map(|p| colours.project_choice(p)).collect(),
                            _ => Vec::new(),
                        })
                        .collect();
                    view! {
                        <span class=move || on(!project.get().is_empty())>
                            <SelectField compact=true current=project.get_untracked() options=options
                                         on_change=move |v: String| project.set(v) />
                        </span>
                    }
                }}
            })}
            {move || {
                epoch.track();
                let options: Vec<(String, String)> = std::iter::once((String::new(), "Anyone".to_owned()))
                    .chain(match people.get() {
                        Some(Ok(p)) => p.into_iter().map(|p| (p.person.id.to_string(), p.person.name)).collect(),
                        _ => Vec::new(),
                    })
                    .collect();
                view! {
                    <span class=move || on(!assignee.get().is_empty())>
                        <SelectField compact=true current=assignee.get_untracked() options=options
                                     on_change=move |v: String| assignee.set(v) />
                    </span>
                }
            }}
            {move || {
                epoch.track();
                let options: Vec<(String, String)> = std::iter::once((String::new(), "Any type".to_owned()))
                    .chain(types.all().into_iter().map(|t| {
                        let name = if t.archived { format!("{} (archived)", t.name) } else { t.name };
                        (t.id, name)
                    }))
                    .collect();
                view! {
                    <span class=move || on(!task_type.get().is_empty())>
                        <SelectField compact=true current=task_type.get_untracked() options=options
                                     on_change=move |v: String| task_type.set(v) />
                    </span>
                }
            }}
            <label class=move || format!("flex items-center gap-1 text-[11px] {}",
                    if due_from.get().is_empty() && due_to.get().is_empty() { "text-muted" } else { "text-accent" })>"Due"
                {move || { epoch.track(); view! {
                    <span class=move || on(!due_from.get().is_empty())>
                        <DateField compact=true current=due_from.get_untracked()
                                   on_commit=move |v: String| due_from.set(v) />
                    </span>
                    "to"
                    <span class=move || on(!due_to.get().is_empty())>
                        <DateField compact=true current=due_to.get_untracked()
                                   on_commit=move |v: String| due_to.set(v) />
                    </span>
                } }}
            </label>
            {board.then(|| view! {
                {move || { epoch.track(); view! {
                    <span class=move || on(board_sort.get() != BoardSort::Default)>
                        <SelectField compact=true options=BoardSort::options()
                            current=board_sort.get_untracked().id().to_owned()
                            on_change=move |v: String| if let Some(sort) = BoardSort::from_id(&v) {
                                board_sort.set(sort);
                                save_board_sort(sort);
                            } />
                    </span>
                } }}
            })}
            <label class=move || format!("flex items-center gap-1 text-[11px] {}",
                    if show_closed.get() { "text-accent" } else { "text-muted" })>
                <input type="checkbox" prop:checked=move || show_closed.get()
                       on:change=move |ev| show_closed.set(event_target_checked(&ev)) />
                {if board { "Show cancelled" } else { "Show done" }}
            </label>
            // On the board the Backburner is a column; the list leaves those tasks out until asked.
            {(!board).then(|| view! {
                <label class=move || format!("flex items-center gap-1 text-[11px] {}",
                        if show_parked.get() { "text-accent" } else { "text-muted" })>
                    <input type="checkbox" prop:checked=move || show_parked.get()
                           on:change=move |ev| show_parked.set(event_target_checked(&ev)) />
                    "Show backburner"
                </label>
            })}
            <Show when=any_changed>
                <button class=BUTTON_SOFT
                        title="Put every filter and the sort back to their defaults"
                        on:click=reset>
                    {move || format!("Reset ({} changed)", changed())}
                </button>
            </Show>
        </div>
    }
}

/// The ring round a filter that has been changed from its default.
const MARKED: &str = "rounded-sm ring-1 ring-accent";

/// What the filter bar holds, for counting what differs from the defaults.
struct Changed {
    text: String,
    status: String,
    project: String,
    assignee: String,
    task_type: String,
    due_from: String,
    due_to: String,
    show_closed: bool,
    show_parked: bool,
    sort: BoardSort,
    /// The board has no status filter (its columns are the statuses) but has the sort.
    board: bool,
}

/// How many controls are set to something other than their default (a due range counts once).
fn changed_count(f: &Changed) -> usize {
    [
        !f.text.trim().is_empty(),
        !f.board && !f.status.is_empty(),
        !f.project.is_empty(),
        !f.assignee.is_empty(),
        !f.task_type.is_empty(),
        !f.due_from.is_empty() || !f.due_to.is_empty(),
        f.show_closed,
        !f.board && f.show_parked,
        f.board && f.sort != BoardSort::Default,
    ]
    .into_iter()
    .filter(|on| *on)
    .count()
}

/// The List / Board switch in the header of the Tasks screen (`board` is true on the board).
#[component]
pub fn LayoutToggle(board: RwSignal<bool>) -> impl IntoView {
    let class =
        move |on: bool| format!("{BUTTON} !rounded-none {}", if on { BUTTON_ON } else { "" });
    view! {
        <div class="flex" role="group" aria-label="Layout">
            <button class=move || class(!board.get()) aria-pressed=move || (!board.get()).to_string()
                    on:click=move |_| board.set(false)>"List"</button>
            <button class=move || class(board.get()) aria-pressed=move || board.get().to_string()
                    on:click=move |_| board.set(true)>"Board"</button>
        </div>
    }
}

#[component]
pub fn TaskList(
    inbox: bool,
    /// Filters shared with the board; the inbox makes its own.
    #[prop(optional)]
    filters: Option<TaskFilters>,
    /// Given on the Tasks screen, which can switch to the board.
    #[prop(optional)]
    layout: Option<RwSignal<bool>>,
) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let gate = expect_context::<FinishDecision>();
    let list = expect_context::<ListNav>();

    let filters = filters.unwrap_or_default();
    let project = filters.project;

    let rows = LocalResource::new(move || {
        version.track();
        api::list_tasks(filters.request(inbox, false))
    });
    let people = LocalResource::new(move || {
        version.track();
        api::list_people()
    });
    let projects = LocalResource::new(move || {
        version.track();
        api::list_node_summaries(NodeType::Project)
    });
    // In My view (spec 50) only your tasks and what reaches them (not the inbox: those tasks have
    // no project, and assignment already filters them).
    let orientation = expect_context::<Orientation>();
    let shown = move || {
        rows.get().map(|r| {
            r.map(|mut v| {
                if !inbox {
                    v.retain(|t| orientation.shows(Kind::Task, t.task.id));
                }
                v
            })
        })
    };

    Effect::new(move |_| match shown() {
        Some(Ok(r)) => list.set_items(
            r.iter()
                .map(|t| NodeRef::new(NodeType::Task, t.task.id))
                .collect(),
        ),
        Some(Err(e)) => toasts.error(&e),
        None => {}
    });

    // Row shortcuts on the task under the cursor.
    list.on_row_key(move |key, node| {
        let Some(Ok(all)) = rows.get_untracked() else {
            return;
        };
        let Some(row) = all.into_iter().find(|r| r.task.id == node.id) else {
            return;
        };
        let id = row.task.id;
        let patch = match key.as_str() {
            "x" => Some(UpdateTask {
                status: Some(if row.task.status == TaskStatus::Done {
                    TaskStatus::Todo
                } else {
                    TaskStatus::Done
                }),
                ..Default::default()
            }),
            "s" => Some(UpdateTask {
                status: Some(next_status(row.task.status)),
                ..Default::default()
            }),
            "f" => {
                let on = is_on(row.task.focus.as_ref(), today());
                set_focus(id, toggle_choice(on), toasts, version);
                None
            }
            k @ ("1" | "2" | "3" | "4" | "5") => Some(UpdateTask {
                priority: k.parse().ok(),
                ..Default::default()
            }),
            "c" => {
                confirm_row(node);
                None
            }
            "b" if !row.task.is_meeting() => {
                set_backburner(id, !row.task.is_parked(), toasts, version);
                None
            }
            "d" => {
                focus_element(&format!("task-due-{id}"));
                None
            }
            "a" => {
                focus_element(&format!("task-assignee-{id}"));
                None
            }
            _ => None,
        };
        if let Some(patch) = patch {
            spawn_local(async move {
                finish_task(api::update_task(id, patch).await, id, toasts, version, gate);
            });
        }
    });

    // New task: one title, or a pasted list that is previewed first.
    let adding = RwSignal::new(false);
    list.on_new(move || adding.set(true));
    let draft = RwSignal::new(String::new());
    let preview = RwSignal::new(None::<Vec<String>>);
    // A task needs a project: the one being filtered on, else the one picked in the form.
    let new_project = RwSignal::new(String::new());
    let target_project = move || {
        let filtered = if inbox {
            None
        } else {
            Uuid::parse_str(&project.get_untracked()).ok()
        };
        filtered.or_else(|| Uuid::parse_str(&new_project.get_untracked()).ok())
    };
    let needs_project = move || {
        toasts.error(&AppError {
            code: "invalid".into(),
            message: "Choose a project for the new task first.".into(),
        });
    };
    // A task has a type: the one being filtered on, else the one used last, else asked.
    let new_types = use_task_types();
    let new_type = RwSignal::new(new_types.starting_type(&filters.task_type.get_untracked()));
    let chosen_type = move || {
        let kind = new_type.get_untracked();
        if kind.is_empty() {
            toasts.error(&AppError {
                code: "invalid".into(),
                message: CHOOSE_TYPE.into(),
            });
            return None;
        }
        Some(kind)
    };

    let selection = expect_context::<Selection>();
    let create_one = move |title: String| {
        let Some(project_id) = target_project() else {
            return needs_project();
        };
        let Some(kind) = chosen_type() else {
            return;
        };
        let input = CreateTask {
            links: Vec::new(),
            focus: None,
            backburner_since: None,
            start_minute: None,
            length_minutes: None,
            task_type: Some(kind.clone()),
            title,
            assignee: AssigneeChoice::Me,
            description: String::new(),
            project_id: Some(project_id),
            status: None,
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: None,
            recurrence: None,
        };
        spawn_local(async move {
            // The new task opens on the right, ready to be filled in.
            if let Some(task) = finish(api::create_task(input).await, toasts, version) {
                remember_new_type(&kind);
                draft.set(String::new());
                selection.open(NodeRef::new(NodeType::Task, task.id));
            }
        });
    };
    let submit = move || {
        let t = draft.get_untracked();
        if t.trim().is_empty() {
            return;
        }
        if !t.contains('\n') {
            return create_one(t.trim().to_owned());
        }
        spawn_local(async move {
            match api::parse_task_lines(t).await {
                Ok(lines) if lines.len() > 1 => preview.set(Some(lines)),
                Ok(lines) => {
                    if let Some(only) = lines.into_iter().next() {
                        create_one(only);
                    }
                }
                Err(e) => toasts.error(&e),
            }
        });
    };
    let confirm_bulk = move |_| {
        let Some(titles) = preview.get_untracked() else {
            return;
        };
        let Some(project_id) = target_project() else {
            return needs_project();
        };
        let Some(kind) = chosen_type() else {
            return;
        };
        spawn_local(async move {
            if finish(
                api::create_tasks_bulk(
                    titles,
                    Some(project_id),
                    AssigneeChoice::Me,
                    Some(kind.clone()),
                )
                .await,
                toasts,
                version,
            )
            .is_some()
            {
                remember_new_type(&kind);
                preview.set(None);
                draft.set(String::new());
            }
        });
    };

    view! {
        <div class="flex flex-col h-full">
            <PageHeader icon=if inbox { "inbox" } else { "tasks" } title=if inbox { "Inbox" } else { "Tasks" } subtitle="Work you or your team own, with estimates and due dates"
                        crumbs=crate::components::zoom::project_crumbs(filters.project)>
                <button class=BUTTON_SOFT on:click=move |_| adding.update(|a| *a = !*a)>
                    {move || if adding.get() { "Cancel" } else { "New task" }}
                </button>
                {layout.map(|_| view! { <crate::components::meeting::NewMeetingButton /> })}
                {layout.map(|board| view! { <span class="ml-auto"><LayoutToggle board=board /></span> })}
                <Hints keys=&[("n", "new"), ("j/k", "move"), ("x", "done"), ("s", "status"), ("f", "focus"), ("c", "still accurate"), ("1-5", "priority"), ("d", "due"), ("a", "assignee")] />
            </PageHeader>
            {(!inbox).then(|| view! { <MineBanner /> })}
            <FilterControls filters=filters inbox=inbox board=false />
            {(!inbox).then(|| view! { <FilteredApproach project=filters.project /> })}
            <Show when=move || adding.get()>
                <div class="px-4 py-2 border-b border-line bg-panel space-y-2">
                    // Filtering on a project already says where the task goes; otherwise it is asked.
                    {move || {
                        let filtered = !inbox && !project.get().is_empty();
                        (!filtered).then(|| {
                            let colours = use_objective_colours();
                            let options: Vec<(String, String)> = std::iter::once((String::new(), "Choose a project…".to_owned()))
                                .chain(match projects.get() {
                                    Some(Ok(p)) => p.iter().map(|p| colours.project_choice(p)).collect(),
                                    _ => Vec::new(),
                                })
                                .collect();
                            view! {
                                <SelectField label="Project (required)" options=options
                                             current=new_project.get_untracked()
                                             on_change=move |v: String| new_project.set(v) />
                            }
                        })
                    }}
                    {move || view! {
                        <SelectField label="Type (required)" options=new_types.new_task_options()
                                     current=new_type.get_untracked()
                                     on_change=move |v: String| new_type.set(v) />
                    }}
                    <textarea class=INPUT rows="1" autofocus
                        placeholder="New task (paste a list for several)"
                        prop:value=move || draft.get()
                        on:input=move |ev| { draft.set(event_target_value(&ev)); preview.set(None); }
                        on:keydown=move |ev| {
                            if ev.key() == "Enter" && !ev.shift_key() {
                                ev.prevent_default();
                                submit();
                            }
                        }></textarea>
                    {move || preview.get().map(|lines| {
                        let n = lines.len();
                        view! {
                            <div class="space-y-2">
                                <p class="text-[11px] text-muted">{n} " tasks will be created:"</p>
                                <ol class="max-h-40 list-decimal overflow-y-auto pl-5">
                                    {lines.into_iter().map(|l| view! { <li>{l}</li> }).collect_view()}
                                </ol>
                                <div class="flex gap-2">
                                    <button class=BUTTON_PRIMARY on:click=confirm_bulk>{format!("Create {n} tasks")}</button>
                                    <button class=BUTTON on:click=move |_| preview.set(None)>"Back"</button>
                                </div>
                            </div>
                        }
                    })}
                    <p class="text-[11px] text-muted">"Enter adds a task. Shift+Enter for a new line. Pasting several lines shows a preview first."</p>
                </div>
            </Show>
            <div class=column_head(COLS)>
                <span>"Status"</span><span>"Pri"</span><span>"Task"</span><span>"Project"</span>
                <span>"Assignee"</span><span>"Due"</span>
            </div>
            <div class="flex-1 overflow-y-auto" role="table">
                {move || match (shown(), people.get(), projects.get()) {
                    (Some(Ok(r)), Some(Ok(ps)), Some(Ok(pr))) => {
                        if r.is_empty() {
                            return if inbox {
                                view! {
                                    <EmptyState icon="inbox" title="Inbox zero"
                                        hint="Every open task has a project. Tasks from before projects were required, and ones whose project was archived, land here." />
                                }.into_any()
                            } else {
                                view! {
                                    <EmptyState icon="tasks" title="No tasks match"
                                        hint="Clear the filters to see all tasks, or press n to add one." />
                                }.into_any()
                            };
                        }
                        let people: Vec<(String, String)> = ps.iter().map(|p| (p.person.id.to_string(), p.person.name.clone())).collect();
                        let colours = use_objective_colours();
                        let projects: Vec<(String, String)> = pr.iter().map(|p| colours.project_choice(p)).collect();
                        r.into_iter().enumerate().map(|(i, row)| {
                            view! { <TaskRowView row=row index=i people=people.clone() projects=projects.clone() /> }
                        }).collect_view().into_any()
                    }
                    (Some(Err(_)), _, _) => view! { <p class="p-4 text-muted">"Couldn't load tasks."</p> }.into_any(),
                    _ => view! { <p class="p-4 text-muted">"Loading…"</p> }.into_any(),
                }}
            </div>
        </div>
    }
}

/// One task with its controls inline; every control saves on change, no modal.
#[component]
fn TaskRowView(
    row: TaskRow,
    index: usize,
    people: Vec<(String, String)>,
    projects: Vec<(String, String)>,
) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let gate = expect_context::<FinishDecision>();
    let id = row.task.id;
    let node = NodeRef::new(NodeType::Task, id);
    let save = move |patch: UpdateTask| {
        spawn_local(async move {
            finish_task(api::update_task(id, patch).await, id, toasts, version, gate);
        });
    };

    let waiting = row.waiting.clone();
    let t = row.task;
    // The row wears the colour of the objective its project serves (ADR-0012).
    let colours = use_objective_colours();
    let project_id = t.project_id;
    let edge = Signal::derive(move || colours.first_hue(&colours.of_project(project_id)));
    let open = !matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled);
    let heat = deadline_heat(
        t.due_date,
        crate::components::task_board::today(),
        open && !t.is_meeting() && !t.is_parked(),
    );
    let is_meeting = t.is_meeting();
    let meeting_time = t
        .start_minute
        .filter(|_| is_meeting)
        .map(|m| minimap_types::fmt_range(m, t.meeting_minutes()));
    // "↻" after the title of a task that repeats; hover says how.
    let repeats = t
        .recurrence
        .as_ref()
        .map(|r| format!("Repeats {}", r.describe()));
    let closed = matches!(t.status, TaskStatus::Done | TaskStatus::Cancelled);
    let focus = t.focus;
    let parked = t.is_parked();
    // A project can be changed but not taken away; "No project" is only there for a meeting or a
    // task from before projects were required, which have none.
    let project_options: Vec<(String, String)> = if t.project_id.is_some() && !is_meeting {
        projects.into_iter().collect()
    } else {
        std::iter::once((String::new(), "No project".to_owned()))
            .chain(projects)
            .collect()
    };
    let assignee_options: Vec<(String, String)> =
        std::iter::once((String::new(), "Unassigned".to_owned()))
            .chain(people)
            .collect();
    let title_class = if closed {
        "truncate text-muted line-through"
    } else {
        "truncate font-medium"
    };

    let on_status = move |v: String| {
        if let Ok(s) = TaskStatus::from_str(&v) {
            save(UpdateTask {
                status: Some(s),
                ..Default::default()
            });
        }
    };
    let on_priority = move |v: String| {
        if let Ok(p) = v.parse::<u8>() {
            save(UpdateTask {
                priority: Some(p),
                ..Default::default()
            });
        }
    };
    let on_project = move |v: String| {
        let project_id = match Uuid::parse_str(&v) {
            Ok(p) => Patch::Set(p),
            Err(_) => Patch::Clear,
        };
        save(UpdateTask {
            project_id,
            ..Default::default()
        });
    };
    let on_assignee = move |v: String| {
        let person = Uuid::parse_str(&v).ok();
        spawn_local(async move {
            finish(api::set_assignee(id, person).await, toasts, version);
        });
    };
    let on_due = move |v: String| match date_patch(&v) {
        Ok(p) => save(UpdateTask {
            due_date: p,
            ..Default::default()
        }),
        Err(e) => {
            toasts.error(&e);
            version.bump();
        }
    };
    let status_now = t.status.as_str().to_owned();
    let priority_now = t.priority.to_string();
    let project_now = t.project_id.map(|p| p.to_string()).unwrap_or_default();
    let assignee_now = row
        .assignee
        .map(|a| a.node.id.to_string())
        .unwrap_or_default();
    let due_now = t.due_date.map(|d| d.to_string()).unwrap_or_default();
    let assignee_id = format!("task-assignee-{id}");
    let due_id = format!("task-due-{id}");

    view! {
        <NodeRow node=node index=index hue=edge heat=heat>
            <div class=COLS>
                // Controls must not open the pane when clicked.
                <span on:click=|ev| ev.stop_propagation()>
                    <SelectField compact=true options=status_options() current=status_now on_change=on_status
                        tone=task_status_tone(t.status).text() />
                </span>
                <span on:click=|ev| ev.stop_propagation()>
                    <SelectField compact=true options=priority_options() current=priority_now on_change=on_priority />
                </span>
                <span class="flex min-w-0 items-center gap-1.5">
                    {move || colours.of_project(project_id).into_iter().next().map(|o| view! {
                        <ObjectiveDot objective=o />
                    })}
                    <span class=title_class>{t.title}</span>
                    <WhyChip kind=Kind::Task id=id />
                    <TypeChip id=t.task_type.clone() />
                    <WaitingChip badge=waiting />
                    {repeats.map(|text| view! {
                        <span class=Tone::Neutral.chip() title=text>"↻"</span>
                    })}
                    {meeting_time.map(|range| view! {
                        <span class=Tone::Accent.chip() title="When the meeting is">{range}</span>
                    })}
                    {t.backburner_since.map(|since| view! {
                        <span class=Tone::Neutral.chip() title=since_title(since)>"Backburner"</span>
                    })}
                    {(!closed && !is_meeting && !parked).then(|| view! { <FocusStar task=id focus=focus /> })}
                </span>
                <span on:click=|ev| ev.stop_propagation()>
                    <SelectField compact=true options=project_options current=project_now on_change=on_project />
                </span>
                <span on:click=|ev| ev.stop_propagation()>
                    <SelectField compact=true options=assignee_options id=assignee_id current=assignee_now
                        on_change=on_assignee />
                </span>
                <span on:click=|ev| ev.stop_propagation()>
                    <DateField compact=true id=due_id current=due_now on_commit=on_due />
                </span>
            </div>
        </NodeRow>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn untouched(board: bool) -> Changed {
        Changed {
            text: String::new(),
            status: String::new(),
            project: String::new(),
            assignee: String::new(),
            task_type: String::new(),
            due_from: String::new(),
            due_to: String::new(),
            show_closed: false,
            show_parked: false,
            sort: BoardSort::Default,
            board,
        }
    }

    #[test]
    fn nothing_is_changed_until_a_control_leaves_its_default() {
        assert_eq!(changed_count(&untouched(true)), 0);
        assert_eq!(changed_count(&untouched(false)), 0);
    }

    #[test]
    fn each_changed_control_counts_once_and_a_due_range_counts_as_one() {
        let mut f = untouched(true);
        f.project = "p".into();
        f.text = "  ".into(); // blank text is no search
        assert_eq!(changed_count(&f), 1);
        f.due_from = "2027-03-01".into();
        f.due_to = "2027-03-31".into();
        f.show_closed = true;
        f.sort = BoardSort::Priority;
        f.assignee = "a".into();
        f.task_type = "decision".into();
        f.text = "docs".into();
        assert_eq!(changed_count(&f), 7);
    }

    #[test]
    fn the_status_filter_is_only_on_the_list_and_the_sort_only_on_the_board() {
        let mut f = untouched(true);
        f.status = "todo".into();
        assert_eq!(changed_count(&f), 0, "the board has no status filter");
        f.board = false;
        assert_eq!(changed_count(&f), 1);
        f.sort = BoardSort::DueLatest;
        assert_eq!(changed_count(&f), 1, "the list has no board sort");
    }
}
