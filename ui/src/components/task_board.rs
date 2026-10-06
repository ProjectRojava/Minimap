//! The Tasks board: one column per status, cards you drag from column to column to change a
//! task's status. Cards keep the order the list uses (due date, priority, title); the Done
//! column shows the newest finished work first.

use std::{cmp::Reverse, collections::HashMap};

use leptos::{ev, prelude::*, task::spawn_local, web_sys};
use minimap_types::{
    AssigneeChoice, CreateTask, Date, NodeRef, NodeType, TaskRow, TaskStatus, UpdateTask, Uuid,
};
use wasm_bindgen::JsCast;

use crate::{
    api,
    calendar::format_ymd,
    components::{
        date_field::today_ymd,
        form::BUTTON_SOFT,
        objective_colour::{use_objective_colours, ObjectiveChips},
        page::{Hints, Icon, PageHeader, Tone, CHIP},
        task_list::{next_status, FilterControls, LayoutToggle, TaskFilters},
    },
    labels::{estimate_text, priority_short, task_status_label, task_status_tone},
    state::{finish, DataVersion, ListNav, Selection, Toasts},
};

/// Finished tasks shown before "Show older"; a board of everything ever done is no board.
const DONE_VISIBLE: usize = 15;

/// What one column shows.
#[derive(Clone, Debug, PartialEq)]
struct Column {
    status: TaskStatus,
    /// The cards on show, in order.
    rows: Vec<TaskRow>,
    /// How many more cards the column has (older finished tasks).
    hidden: usize,
}

/// The columns, left to right: the three open states, Done, and Cancelled when asked for.
fn statuses(show_cancelled: bool) -> Vec<TaskStatus> {
    let mut all = vec![
        TaskStatus::Todo,
        TaskStatus::InProgress,
        TaskStatus::Blocked,
        TaskStatus::Done,
    ];
    if show_cancelled {
        all.push(TaskStatus::Cancelled);
    }
    all
}

/// Sorts the tasks into columns. `pending` holds moves that are saving: those cards already
/// sit in their new column, so a drop never waits for the database.
fn columns(
    rows: &[TaskRow],
    pending: &HashMap<Uuid, TaskStatus>,
    show_cancelled: bool,
    all_done: bool,
) -> Vec<Column> {
    statuses(show_cancelled)
        .into_iter()
        .map(|status| {
            let mut cards: Vec<TaskRow> = rows
                .iter()
                .filter(|r| pending.get(&r.task.id).copied().unwrap_or(r.task.status) == status)
                .cloned()
                .map(|mut r| {
                    r.task.status = status;
                    r
                })
                .collect();
            let mut hidden = 0;
            if status == TaskStatus::Done {
                // Just finished first (a card dropped here has no completion time yet).
                cards
                    .sort_by_key(|r| (r.task.completed_at.is_some(), Reverse(r.task.completed_at)));
                if !all_done && cards.len() > DONE_VISIBLE {
                    hidden = cards.len() - DONE_VISIBLE;
                    cards.truncate(DONE_VISIBLE);
                }
            }
            Column {
                status,
                rows: cards,
                hidden,
            }
        })
        .collect()
}

/// How a due date stands against today.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Due {
    /// Past, and the task is still open.
    Overdue,
    Today,
    Later,
}

fn due_state(due: Date, today: Option<Date>, status: TaskStatus) -> Due {
    let open = !matches!(status, TaskStatus::Done | TaskStatus::Cancelled);
    match today {
        Some(t) if open && due < t => Due::Overdue,
        Some(t) if open && due == t => Due::Today,
        _ => Due::Later,
    }
}

/// "AB" for "Ada Byron", "P" for "priya": the avatar's letters.
fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

fn today() -> Option<Date> {
    let (y, m, d) = today_ymd();
    minimap_types::timefmt::parse_date(&format_ymd(y, m, d)).ok()
}

/// A coloured rule across the top of a column, from the status's tone.
fn top_rule(tone: Tone) -> &'static str {
    match tone {
        Tone::Accent => "border-t-accent",
        Tone::Success => "border-t-success",
        Tone::Warning => "border-t-warning",
        Tone::Danger => "border-t-danger",
        Tone::Neutral => "border-t-line-strong",
    }
}

/// What a column says when it has no cards.
fn empty_hint(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Todo => "Nothing waiting to start",
        TaskStatus::InProgress => "Drag a task here when you start it",
        TaskStatus::Blocked => "Nothing is stuck",
        TaskStatus::Done => "Finished work lands here",
        TaskStatus::Cancelled => "Nothing cancelled",
    }
}

/// A key that changes when anything a card shows changes, so only those cards redraw.
fn card_key(r: &TaskRow) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        r.task.id,
        r.task.updated_at.unix_timestamp_nanos(),
        r.task.status.as_str(),
        r.assignee.as_ref().map(|a| a.label.as_str()).unwrap_or(""),
        r.project.as_ref().map(|p| p.label.as_str()).unwrap_or(""),
    )
}

/// The Tasks screen as a board.
#[component]
pub fn TaskBoard(filters: TaskFilters, layout: RwSignal<bool>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let list = expect_context::<ListNav>();

    let rows = LocalResource::new(move || {
        version.track();
        api::list_tasks(filters.request(false, true))
    });
    // The last tasks that loaded, so the board never blanks while it refreshes.
    let data = RwSignal::new(None::<Vec<TaskRow>>);
    // Moves that are saving; cleared once the reloaded tasks arrive.
    let pending = RwSignal::new(HashMap::<Uuid, TaskStatus>::new());
    let all_done = RwSignal::new(false);
    let failed = RwSignal::new(false);
    Effect::new(move |_| match rows.get() {
        Some(Ok(r)) => {
            failed.set(false);
            data.set(Some(r));
            pending.update(|p| p.clear());
        }
        Some(Err(e)) => {
            failed.set(true);
            pending.update(|p| p.clear());
            toasts.error(&e);
        }
        None => {}
    });

    let cols = Memo::new(move |_| {
        data.with(|d| {
            d.as_ref()
                .map(|d| pending.with(|p| columns(d, p, filters.show_closed.get(), all_done.get())))
        })
    });
    // Cards in keyboard order (column by column) for j/k.
    let order = Memo::new(move |_| {
        cols.with(|c| {
            c.iter()
                .flatten()
                .flat_map(|c| c.rows.iter().map(|r| r.task.id))
                .collect::<Vec<_>>()
        })
    });
    Effect::new(move |_| {
        list.set_items(
            order
                .get()
                .into_iter()
                .map(|id| NodeRef::new(NodeType::Task, id))
                .collect(),
        )
    });

    // Moves a card: it jumps at once and the change is saved behind it.
    let move_to = move |id: Uuid, status: TaskStatus| {
        pending.update(|p| {
            p.insert(id, status);
        });
        spawn_local(async move {
            let patch = UpdateTask {
                status: Some(status),
                ..Default::default()
            };
            finish(api::update_task(id, patch).await, toasts, version);
        });
    };

    // Keyboard on the card under the cursor: x done, s next status, 1-5 priority.
    list.on_row_key(move |key, node| {
        let Some(row) = data.with_untracked(|d| {
            d.as_ref()
                .and_then(|d| d.iter().find(|r| r.task.id == node.id).cloned())
        }) else {
            return;
        };
        let id = row.task.id;
        let current = pending.with_untracked(|p| p.get(&id).copied().unwrap_or(row.task.status));
        match key.as_str() {
            "x" => move_to(
                id,
                if current == TaskStatus::Done {
                    TaskStatus::Todo
                } else {
                    TaskStatus::Done
                },
            ),
            "s" => move_to(id, next_status(current)),
            k @ ("1" | "2" | "3" | "4" | "5") => {
                let patch = UpdateTask {
                    priority: k.parse().ok(),
                    ..Default::default()
                };
                spawn_local(async move {
                    finish(api::update_task(id, patch).await, toasts, version);
                });
            }
            _ => {}
        }
    });

    // Drag state: the card in the air (id, its column) and the column it is over.
    let dragging = RwSignal::new(None::<(Uuid, TaskStatus)>);
    let over = RwSignal::new(None::<TaskStatus>);
    // The column with an open "add a task" box.
    let adding = RwSignal::new(None::<TaskStatus>);
    list.on_new(move || adding.set(Some(TaskStatus::Todo)));

    let today_date = today();
    let ctx = BoardCtx {
        filters,
        cols,
        dragging,
        over,
        adding,
        all_done,
        move_to: Callback::new(move |(id, status)| move_to(id, status)),
        order,
        today: today_date,
    };

    view! {
        <div class="flex h-full flex-col">
            <PageHeader icon="tasks" title="Tasks" subtitle="Drag cards between columns to change their status">
                <button class=BUTTON_SOFT
                        on:click=move |_| adding.update(|a| *a = if a.is_some() { None } else { Some(TaskStatus::Todo) })>
                    {move || if adding.get().is_some() { "Cancel" } else { "New task" }}
                </button>
                <span class="ml-auto"><LayoutToggle board=layout /></span>
                <Hints keys=&[("n", "new"), ("j/k", "move"), ("x", "done"), ("s", "next status"), ("1-5", "priority")] />
            </PageHeader>
            <FilterControls filters=filters inbox=false board=true />
            {move || if data.with(Option::is_some) {
                view! {
                    <div class="flex min-h-0 flex-1 gap-3 overflow-x-auto p-3">
                        {statuses(true).into_iter().map(|status| view! {
                            <BoardColumn status=status ctx=ctx />
                        }).collect_view()}
                    </div>
                    {move || {
                        let empty = cols.with(|c| c.iter().flatten().all(|c| c.rows.is_empty()));
                        let filtered = filters.text.with(|t| !t.trim().is_empty())
                            || !filters.project.with(String::is_empty)
                            || !filters.assignee.with(String::is_empty)
                            || !filters.due_from.with(String::is_empty)
                            || !filters.due_to.with(String::is_empty);
                        empty.then(|| view! {
                            <p class="pb-3 text-center text-[11px] text-muted">
                                {if filtered { "No tasks match the filters." } else { "No tasks yet. Press n to add one." }}
                            </p>
                        })
                    }}
                }.into_any()
            } else if failed.get() {
                view! { <p class="p-4 text-muted">"Couldn't load tasks."</p> }.into_any()
            } else {
                view! { <p class="p-4 text-muted">"Loading…"</p> }.into_any()
            }}
        </div>
    }
}

/// What the columns and cards share.
#[derive(Clone, Copy)]
struct BoardCtx {
    filters: TaskFilters,
    cols: Memo<Option<Vec<Column>>>,
    dragging: RwSignal<Option<(Uuid, TaskStatus)>>,
    over: RwSignal<Option<TaskStatus>>,
    adding: RwSignal<Option<TaskStatus>>,
    all_done: RwSignal<bool>,
    move_to: Callback<(Uuid, TaskStatus)>,
    order: Memo<Vec<Uuid>>,
    today: Option<Date>,
}

#[component]
fn BoardColumn(status: TaskStatus, ctx: BoardCtx) -> impl IntoView {
    let BoardCtx {
        filters,
        cols,
        dragging,
        over,
        adding,
        all_done,
        move_to,
        ..
    } = ctx;
    let tone = task_status_tone(status);
    let mine = move || {
        cols.with(|c| {
            c.as_ref()
                .and_then(|c| c.iter().find(|c| c.status == status).cloned())
        })
    };
    let count = move || mine().map(|c| c.rows.len() + c.hidden).unwrap_or_default();
    // A card from another column is over this one: the drop would move it here.
    let target =
        move || over.get() == Some(status) && dragging.get().is_some_and(|d| d.1 != status);

    let on_drop = move |ev: ev::DragEvent| {
        ev.prevent_default();
        over.set(None);
        let Some((id, from)) = dragging.get_untracked() else {
            return;
        };
        dragging.set(None);
        if from != status {
            move_to.run((id, status));
        }
    };
    let on_leave = move |ev: ev::DragEvent| {
        // Moving onto a card inside the column also fires dragleave; only a real exit counts.
        let inside = ev
            .related_target()
            .zip(ev.current_target())
            .and_then(|(r, c)| {
                let r = r.dyn_into::<web_sys::Node>().ok()?;
                let c = c.dyn_into::<web_sys::Node>().ok()?;
                Some(c.contains(Some(&r)))
            })
            .unwrap_or(false);
        if !inside {
            over.update(|o| {
                if *o == Some(status) {
                    *o = None;
                }
            });
        }
    };

    // The Cancelled column exists only when asked for.
    let visible = move || status != TaskStatus::Cancelled || filters.show_closed.get();

    view! {
        <Show when=visible>
            <section
                aria-label=task_status_label(status)
                class=move || format!(
                    "flex w-72 min-w-[16rem] shrink-0 flex-col rounded-sm border border-t-2 border-line {} {}",
                    top_rule(tone),
                    if target() { "bg-hover !border-accent border-dashed" } else { "bg-panel" })
                on:dragover=move |ev: ev::DragEvent| {
                    ev.prevent_default();
                    if let Some(dt) = ev.data_transfer() {
                        dt.set_drop_effect("move");
                    }
                    if over.get_untracked() != Some(status) {
                        over.set(Some(status));
                    }
                }
                on:dragleave=on_leave
                on:drop=on_drop
            >
                <header class="flex items-center gap-2 px-3 py-2">
                    <h2 class=format!("text-[11px] font-semibold uppercase tracking-wider {}", tone.text())>
                        {task_status_label(status)}
                    </h2>
                    <span class="rounded-sm border border-line px-1 text-[10px] leading-4 tabular-nums text-muted">
                        {count}
                    </span>
                    <button class="ml-auto rounded-sm px-1 text-[14px] leading-4 text-faint hover:bg-hover hover:text-accent"
                            title=format!("Add a task to {}", task_status_label(status))
                            aria-label=format!("Add a task to {}", task_status_label(status))
                            on:click=move |_| adding.set(Some(status))>"+"</button>
                </header>
                <div class="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-2 pb-2">
                    <Show when=move || adding.get() == Some(status)>
                        <NewCard status=status ctx=ctx />
                    </Show>
                    <For
                        each=move || mine().map(|c| c.rows).unwrap_or_default()
                        key=card_key
                        children=move |row| view! { <TaskCard row=row ctx=ctx /> }
                    />
                    {move || {
                        let empty = mine().is_some_and(|c| c.rows.is_empty());
                        (empty && !target()).then(|| view! {
                            <p class="rounded-sm border border-dashed border-line px-3 py-6 text-center text-[11px] text-faint">
                                {empty_hint(status)}
                            </p>
                        })
                    }}
                    {move || target().then(|| view! {
                        <p class="rounded-sm border border-dashed border-accent/60 bg-accent/10 px-3 py-3 text-center text-[11px] text-accent">
                            {format!("Move to {}", task_status_label(status))}
                        </p>
                    })}
                    {move || {
                        let hidden = mine().map(|c| c.hidden).unwrap_or_default();
                        (status == TaskStatus::Done && (hidden > 0 || all_done.get())).then(|| view! {
                            <button class="rounded-sm py-1 text-[11px] text-muted hover:bg-hover hover:text-accent"
                                    on:click=move |_| all_done.update(|a| *a = !*a)>
                                {if all_done.get() { "Show fewer".to_owned() } else { format!("Show {hidden} older") }}
                            </button>
                        })
                    }}
                </div>
            </section>
        </Show>
    }
}

/// The box that adds a task straight into a column.
#[component]
fn NewCard(status: TaskStatus, ctx: BoardCtx) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let adding = ctx.adding;
    let project = ctx.filters.project;
    let title = RwSignal::new(String::new());
    let input = leptos::prelude::NodeRef::<leptos::html::Input>::new();
    Effect::new(move |_| {
        if let Some(el) = input.get() {
            let _ = el.focus();
        }
    });
    let submit = move || {
        let t = title.get_untracked();
        let t = t.trim();
        if t.is_empty() {
            return;
        }
        let new = CreateTask {
            title: t.to_owned(),
            assignee: AssigneeChoice::Me,
            description: String::new(),
            // A board filtered to a project adds into it.
            project_id: Uuid::parse_str(&project.get_untracked()).ok(),
            status: Some(status),
            estimate_days: None,
            start_date: None,
            due_date: None,
            priority: None,
            recurrence: None,
        };
        spawn_local(async move {
            if finish(api::create_task(new).await, toasts, version).is_some() {
                title.set(String::new());
            }
        });
    };
    view! {
        <div class="rounded-sm border border-accent/50 bg-canvas p-2">
            <input node_ref=input class="w-full bg-transparent outline-none placeholder:text-faint"
                   placeholder="Task title, Enter to add"
                   prop:value=move || title.get()
                   on:input=move |ev| title.set(event_target_value(&ev))
                   on:keydown=move |ev| match ev.key().as_str() {
                       "Enter" => { ev.prevent_default(); submit(); }
                       "Escape" => { ev.stop_propagation(); adding.set(None); }
                       _ => {}
                   } />
            <p class="mt-1 text-[10px] text-faint">"Enter adds and keeps going · Esc closes"</p>
        </div>
    }
}

#[component]
fn TaskCard(row: TaskRow, ctx: BoardCtx) -> impl IntoView {
    let selection = expect_context::<Selection>();
    let list = expect_context::<ListNav>();
    let BoardCtx {
        dragging,
        over,
        order,
        today,
        ..
    } = ctx;
    let t = row.task;
    let (id, status) = (t.id, t.status);
    let node = NodeRef::new(NodeType::Task, id);
    // The card wears the colour of the objective its project serves (ADR-0012).
    let colours = use_objective_colours();
    let project_id = t.project_id;
    let hue = move || colours.first_hue(&colours.of_project(project_id));
    let closed = matches!(status, TaskStatus::Done | TaskStatus::Cancelled);
    let index = move || order.with(|o| o.iter().position(|x| *x == id));
    let is_open = move || selection.0.get() == Some(node);
    let on_cursor = move || list.cursor.get().is_some() && list.cursor.get() == index();
    let in_the_air = move || dragging.get().map(|d| d.0) == Some(id);

    let repeats = t
        .recurrence
        .as_ref()
        .map(|r| format!("Repeats {}", r.describe()));
    let due = t.due_date.map(|d| (d, due_state(d, today, status)));
    let estimate = estimate_text(t.estimate_days);
    let assignee = row.assignee.map(|a| a.label);
    let project = row.project.map(|p| p.label);
    let title_class = if closed {
        "break-words text-muted line-through"
    } else {
        "break-words font-medium"
    };
    // Urgent work (P1, P2) is the warning tone, like everywhere else.
    let priority = t.priority;
    let priority_class = if priority <= 2 && !closed {
        Tone::Warning.chip()
    } else {
        CHIP
    };

    view! {
        <article
            draggable="true"
            style=move || hue().map(|h| format!("--obj-h: {h}")).unwrap_or_default()
            class=move || format!(
                "group cursor-grab select-none rounded-sm border p-2.5 transition-colors active:cursor-grabbing {} {} {}",
                if hue().is_some() { "obj-bar" } else { "" },
                if is_open() { "border-accent/50 bg-active" }
                else if on_cursor() { "border-line-strong bg-hover" }
                else { "border-line bg-canvas hover:border-line-strong hover:bg-hover" },
                if in_the_air() { "opacity-40" } else { "" })
            on:click=move |_| {
                if let Some(i) = index() {
                    list.cursor.set(Some(i));
                }
                selection.open(node);
            }
            on:dragstart=move |ev: ev::DragEvent| {
                // WebKit only starts a drag when it carries data.
                if let Some(dt) = ev.data_transfer() {
                    let _ = dt.set_data("text/plain", &id.to_string());
                    dt.set_effect_allowed("move");
                }
                dragging.set(Some((id, status)));
            }
            on:dragend=move |_| {
                dragging.set(None);
                over.set(None);
            }
        >
            {move || {
                let objectives = colours.of_project(project_id);
                (!objectives.is_empty()).then(|| view! {
                    <div class="mb-1.5"><ObjectiveChips objectives=objectives /></div>
                })
            }}
            <div class="flex items-start gap-1.5">
                <span class=title_class>{t.title}</span>
                {repeats.map(|text| view! {
                    <span class="shrink-0 text-muted" title=text>"↻"</span>
                })}
            </div>
            {project.map(|p| view! {
                <div class="mt-0.5 flex items-center gap-1 truncate text-[11px] text-muted">
                    <Icon name="projects" size="h-3 w-3" />
                    <span class="truncate">{p}</span>
                </div>
            })}
            <div class="mt-2 flex items-center gap-1.5 text-[11px]">
                <span class=priority_class title="Priority (1 is highest)">{priority_short(priority)}</span>
                {due.map(|(d, state)| {
                    let (class, hint) = match state {
                        Due::Overdue => ("text-danger", "Overdue"),
                        Due::Today => ("text-warning", "Due today"),
                        Due::Later => ("text-muted", "Due date"),
                    };
                    view! { <span class=format!("tabular-nums {class}") title=hint>{d.to_string()}</span> }
                })}
                {(!estimate.is_empty()).then(|| view! {
                    <span class="tabular-nums text-muted" title="Estimate">{estimate}</span>
                })}
                {assignee.map(|name| view! {
                    <span class="ml-auto flex h-5 w-5 shrink-0 items-center justify-center rounded-full border border-line-strong bg-panel text-[9px] font-semibold text-muted"
                          title=name.clone()>
                        {initials(&name)}
                    </span>
                })}
            </div>
        </article>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::timefmt::parse_date;
    use std::sync::atomic::{AtomicU64, Ordering};
    use time::{Duration, OffsetDateTime};

    fn date(text: &str) -> Date {
        parse_date(text).unwrap()
    }

    fn row(title: &str, status: TaskStatus) -> TaskRow {
        let at = OffsetDateTime::UNIX_EPOCH;
        TaskRow {
            task: minimap_types::Task {
                id: {
                    static NEXT: AtomicU64 = AtomicU64::new(1);
                    Uuid::from_u128(NEXT.fetch_add(1, Ordering::Relaxed).into())
                },
                title: title.to_owned(),
                description: String::new(),
                project_id: None,
                status,
                estimate_days: None,
                start_date: None,
                due_date: None,
                completed_at: (status == TaskStatus::Done).then_some(at),
                priority: 3,
                recurrence: None,
                created_at: at,
                updated_at: at,
                archived_at: None,
            },
            project: None,
            assignee: None,
        }
    }

    fn titles(c: &Column) -> Vec<&str> {
        c.rows.iter().map(|r| r.task.title.as_str()).collect()
    }

    #[test]
    fn open_work_gets_three_columns_then_done_and_cancelled_on_request() {
        assert_eq!(
            statuses(false),
            [
                TaskStatus::Todo,
                TaskStatus::InProgress,
                TaskStatus::Blocked,
                TaskStatus::Done
            ]
        );
        assert_eq!(statuses(true).last(), Some(&TaskStatus::Cancelled));
    }

    #[test]
    fn cards_keep_the_lists_order_inside_a_column() {
        let rows = [
            row("a", TaskStatus::Todo),
            row("b", TaskStatus::InProgress),
            row("c", TaskStatus::Todo),
        ];
        let cols = columns(&rows, &HashMap::new(), false, false);
        assert_eq!(titles(&cols[0]), ["a", "c"]);
        assert_eq!(titles(&cols[1]), ["b"]);
        assert!(cols[2].rows.is_empty() && cols[3].rows.is_empty());
    }

    #[test]
    fn a_saving_move_already_sits_in_its_new_column() {
        let rows = [row("a", TaskStatus::Todo), row("b", TaskStatus::Todo)];
        let pending = HashMap::from([(rows[0].task.id, TaskStatus::Blocked)]);
        let cols = columns(&rows, &pending, false, false);
        assert_eq!(titles(&cols[0]), ["b"]);
        assert_eq!(titles(&cols[2]), ["a"]);
        assert_eq!(cols[2].rows[0].task.status, TaskStatus::Blocked);
    }

    #[test]
    fn cancelled_tasks_show_only_when_asked_for() {
        let rows = [row("a", TaskStatus::Cancelled)];
        assert_eq!(columns(&rows, &HashMap::new(), false, false).len(), 4);
        let shown = columns(&rows, &HashMap::new(), true, false);
        assert_eq!(titles(&shown[4]), ["a"]);
    }

    #[test]
    fn done_shows_the_newest_first_and_caps_the_old_ones() {
        let mut rows: Vec<TaskRow> = (0..DONE_VISIBLE + 3)
            .map(|i| {
                let mut r = row(&format!("t{i}"), TaskStatus::Done);
                r.task.completed_at = Some(OffsetDateTime::UNIX_EPOCH + Duration::days(i as i64));
                r
            })
            .collect();
        rows.reverse();
        let done = &columns(&rows, &HashMap::new(), false, false)[3];
        assert_eq!(done.rows.len(), DONE_VISIBLE);
        assert_eq!(done.hidden, 3);
        assert_eq!(done.rows[0].task.title, format!("t{}", DONE_VISIBLE + 2));
        let all = &columns(&rows, &HashMap::new(), false, true)[3];
        assert_eq!((all.rows.len(), all.hidden), (DONE_VISIBLE + 3, 0));
    }

    #[test]
    fn a_card_just_dropped_on_done_leads_the_column() {
        let mut old = row("old", TaskStatus::Done);
        old.task.completed_at = Some(OffsetDateTime::UNIX_EPOCH + Duration::days(9));
        let fresh = row("fresh", TaskStatus::InProgress);
        let pending = HashMap::from([(fresh.task.id, TaskStatus::Done)]);
        let done = &columns(&[old, fresh], &pending, false, false)[3];
        assert_eq!(titles(done), ["fresh", "old"]);
    }

    #[test]
    fn only_open_tasks_can_be_overdue() {
        let today = Some(date("2027-03-10"));
        let due = |d, s| due_state(d, today, s);
        assert_eq!(due(date("2027-03-09"), TaskStatus::Todo), Due::Overdue);
        assert_eq!(due(date("2027-03-10"), TaskStatus::Blocked), Due::Today);
        assert_eq!(due(date("2027-03-11"), TaskStatus::Todo), Due::Later);
        assert_eq!(due(date("2027-03-09"), TaskStatus::Done), Due::Later);
        assert_eq!(due(date("2027-03-09"), TaskStatus::Cancelled), Due::Later);
        assert_eq!(
            due_state(date("2027-03-09"), None, TaskStatus::Todo),
            Due::Later
        );
    }

    #[test]
    fn avatars_use_up_to_two_initials() {
        assert_eq!(initials("Ada Byron"), "AB");
        assert_eq!(initials("priya"), "P");
        assert_eq!(initials("  Mary Jane Watson "), "MJ");
        assert_eq!(initials(""), "");
    }

    #[test]
    fn the_s_key_walks_to_do_in_progress_done() {
        assert_eq!(next_status(TaskStatus::Todo), TaskStatus::InProgress);
        assert_eq!(next_status(TaskStatus::InProgress), TaskStatus::Done);
        assert_eq!(next_status(TaskStatus::Done), TaskStatus::Todo);
        assert_eq!(next_status(TaskStatus::Blocked), TaskStatus::Todo);
    }

    #[test]
    fn every_column_has_a_rule_and_a_hint() {
        for s in TaskStatus::ALL.iter().copied() {
            assert!(top_rule(task_status_tone(s)).starts_with("border-t-"));
            assert!(!empty_hint(s).is_empty());
        }
    }
}
