//! The Tasks board: one column per status, cards you drag from column to column to change a
//! task's status. Cards keep the order the list uses (due date, priority, title); the Done
//! column shows the newest finished work first.

use std::{
    cmp::Reverse,
    collections::{HashMap, HashSet},
};

use leptos::{ev, prelude::*, task::spawn_local, web_sys};
use minimap_types::{
    AssigneeChoice, CreateTask, Date, EdgeLink, EdgeType, NodeRef, NodeType, TaskRow, TaskStatus,
    UpdateTask, Uuid,
};
use wasm_bindgen::JsCast;

use crate::{
    api,
    calendar::format_ymd,
    components::{
        card_menu::CardMenu,
        date_field::today_ymd,
        focus::{is_on, set_focus, toggle_choice, FocusStar},
        form::BUTTON_SOFT,
        objective_colour::{assign, use_objective_colours, ObjectiveChips},
        page::{Hints, Icon, PageHeader, Tone, CHIP},
        task_list::{next_status, BoardSort, FilterControls, LayoutToggle, TaskFilters},
        task_type::{
            finish_badge, frame_colour, frame_style, type_for_new_task, use_task_types, TypeChip,
        },
    },
    labels::{
        deadline_heat, estimate_text, heat_strength, priority_short, task_status_label,
        task_status_tone,
    },
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

/// Puts a column's cards in order. Ties fall back to priority, then title, like the list. One
/// setting orders every column; `Default` is by deadline (soonest first) in the open columns and
/// newest first in Done.
fn sort_cards(cards: &mut [TaskRow], sort: BoardSort, status: TaskStatus) {
    let title = |r: &TaskRow| r.task.title.to_lowercase();
    let sort = match (sort, status) {
        (BoardSort::Default, TaskStatus::Done) => None,
        (BoardSort::Default, _) => Some(BoardSort::DueSoonest),
        (other, _) => Some(other),
    };
    match sort {
        Some(BoardSort::DueLatest) => cards.sort_by_cached_key(|r| {
            (
                r.task.due_date.is_none(),
                Reverse(r.task.due_date),
                r.task.start_minute,
                r.task.priority,
                title(r),
            )
        }),
        Some(BoardSort::Priority) => cards.sort_by_cached_key(|r| {
            (
                r.task.priority,
                r.task.due_date.is_none(),
                r.task.due_date,
                r.task.start_minute,
                title(r),
            )
        }),
        Some(_) => cards.sort_by_cached_key(|r| {
            (
                r.task.due_date.is_none(),
                r.task.due_date,
                r.task.start_minute,
                r.task.priority,
                title(r),
            )
        }),
        // Just finished first (a card dropped here has no completion time yet).
        None => {
            cards.sort_by_key(|r| (r.task.completed_at.is_some(), Reverse(r.task.completed_at)))
        }
    }
}

/// Sorts the tasks into columns. `pending` holds moves that are saving: those cards already
/// sit in their new column, so a drop never waits for the database.
fn columns(
    rows: &[TaskRow],
    pending: &HashMap<Uuid, TaskStatus>,
    show_cancelled: bool,
    all_done: bool,
    sort: BoardSort,
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
            sort_cards(&mut cards, sort, status);
            let mut hidden = 0;
            if status == TaskStatus::Done && !all_done && cards.len() > DONE_VISIBLE {
                hidden = cards.len() - DONE_VISIBLE;
                cards.truncate(DONE_VISIBLE);
            }
            Column {
                status,
                rows: cards,
                hidden,
            }
        })
        .collect()
}

/// The due date as shown on a card: its words, classes and tooltip. An open task due within a
/// week is a pill that gets louder as the day nears (amber tint, solid amber for today and
/// tomorrow, solid red once overdue); anything further out, or closed, is the plain date.
#[derive(Debug, PartialEq, Eq)]
struct DuePill {
    text: String,
    class: &'static str,
    hint: String,
}

fn due_pill(due: Date, today: Option<Date>, status: TaskStatus) -> DuePill {
    let open = !matches!(status, TaskStatus::Done | TaskStatus::Cancelled);
    let plain = || DuePill {
        text: due.to_string(),
        class: "tabular-nums text-muted",
        hint: format!("Due date: {due}"),
    };
    let Some(today) = today.filter(|_| open) else {
        return plain();
    };
    let days = (due - today).whole_days();
    let (text, class, hint) = match days {
        d if d < 0 => (
            format!("{}d overdue", -d),
            "rounded-sm border border-danger bg-danger px-1.5 py-0.5 text-[11px] font-semibold leading-4 tabular-nums text-canvas",
            "Overdue",
        ),
        0 => (
            "Today".to_owned(),
            "rounded-sm border border-warning bg-warning px-1.5 py-0.5 text-[11px] font-semibold leading-4 tabular-nums text-canvas",
            "Due today",
        ),
        1 => (
            "Tomorrow".to_owned(),
            "rounded-sm border border-warning bg-warning px-1.5 py-0.5 text-[11px] font-semibold leading-4 tabular-nums text-canvas",
            "Due tomorrow",
        ),
        2..=7 => (
            format!("In {days}d"),
            "rounded-sm border border-warning/50 bg-warning/10 px-1.5 py-0.5 text-[11px] font-medium leading-4 tabular-nums text-warning",
            "Due this week",
        ),
        _ => return plain(),
    };
    DuePill {
        text,
        class,
        hint: format!("{hint}: {due}"),
    }
}

/// The priority pill of a card: P1 solid amber, P2 tinted amber, the rest quiet. Closed tasks
/// are always quiet.
fn priority_pill(priority: u8, closed: bool) -> &'static str {
    match priority {
        _ if closed => CHIP,
        1 => "inline-block rounded-sm border border-warning bg-warning px-1.5 text-[11px] font-bold leading-4 text-canvas",
        2 => "inline-block rounded-sm border border-warning/50 bg-warning/10 px-1.5 text-[11px] font-semibold leading-4 text-warning",
        _ => CHIP,
    }
}

/// The hues of the chain (linked tasks) and the paperclip (files and web links): blue and green,
/// away from red and amber, which mean "late" and "needs attention".
const LINK_HUE: u16 = 200;
const FILE_HUE: u16 = 145;
/// Sub-tasks (what a task is part of): violet, apart from the other two.
const PART_HUE: u16 = 280;

/// The tooltip of a card's paperclip: "2 files, 1 web link", or nothing when it has neither.
fn attachment_hint(files: u32, urls: usize) -> Option<String> {
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    match (files as usize, urls) {
        (0, 0) => None,
        (f, 0) => Some(plural(f, "file", "files")),
        (0, u) => Some(plural(u, "web link", "web links")),
        (f, u) => Some(format!(
            "{}, {}",
            plural(f, "file", "files"),
            plural(u, "web link", "web links")
        )),
    }
}

/// The tooltip of a card's chain mark: how many tasks it blocks, waits for or is related to.
fn link_hint(links: u32) -> String {
    format!(
        "Blocks, waits for or is related to {links} {}. Open the card to see them.",
        if links == 1 { "task" } else { "tasks" }
    )
}

/// The line at the top of the board while a task's links show: what the lines mean.
fn focus_hint(linked: usize) -> String {
    format!(
        "{linked} linked · arrows point from a blocker to the task that waits, dashed lines are related, dotted lines are sub-tasks"
    )
}

/// The words on a parent card's sub-task mark: "2/5 sub-tasks" (done of counted).
fn subtask_label(done: u32, total: u32) -> String {
    format!(
        "{done}/{total} sub-{}",
        if total == 1 { "task" } else { "tasks" }
    )
}

/// The tooltip of a parent card's sub-task mark.
fn subtask_hint(done: u32, total: u32) -> String {
    format!("{done} of {total} sub-tasks done. Open the card to see them.")
}

/// The words on a card's chain mark: "2 linked tasks".
fn link_label(links: u32) -> String {
    format!(
        "{links} linked {}",
        if links == 1 { "task" } else { "tasks" }
    )
}

/// "AB" for "Ada Byron", "P" for "priya": the avatar's letters.
fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

/// How a task on the board is joined to the one that is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LinkKind {
    /// `from` has to finish before `to` can start.
    Blocks,
    /// The two are just related; the line has no direction.
    Relates,
    /// `from` (a sub-task) is part of `to` (its parent). Organisation only: no order, no arrow.
    Part,
}

/// One line on the board: from a task to a task.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BoardLink {
    from: Uuid,
    to: Uuid,
    kind: LinkKind,
}

/// The task links of `task` that the board draws: *blocks* (the blocker points at the task it
/// holds up), *related* and *part of* (a sub-task to its parent), to other tasks only.
fn board_links(task: Uuid, links: &[EdgeLink]) -> Vec<BoardLink> {
    links
        .iter()
        .filter(|l| l.other.node.node_type == NodeType::Task)
        .filter_map(|l| {
            let other = l.other.node.id;
            match (l.edge.edge_type, l.outgoing) {
                (EdgeType::Blocks, true) => Some(BoardLink {
                    from: task,
                    to: other,
                    kind: LinkKind::Blocks,
                }),
                (EdgeType::Blocks, false) => Some(BoardLink {
                    from: other,
                    to: task,
                    kind: LinkKind::Blocks,
                }),
                (EdgeType::RelatesTo, _) => Some(BoardLink {
                    from: task,
                    to: other,
                    kind: LinkKind::Relates,
                }),
                (EdgeType::SubtaskOf, true) => Some(BoardLink {
                    from: task,
                    to: other,
                    kind: LinkKind::Part,
                }),
                (EdgeType::SubtaskOf, false) => Some(BoardLink {
                    from: other,
                    to: task,
                    kind: LinkKind::Part,
                }),
                _ => None,
            }
        })
        .collect()
}

/// The open task and the lines drawn from it.
#[derive(Clone, Debug, Default, PartialEq)]
struct Focus {
    task: Option<Uuid>,
    links: Vec<BoardLink>,
}

impl Focus {
    /// The tasks joined to the open one (it is not among them).
    fn linked(&self) -> HashSet<Uuid> {
        self.links
            .iter()
            .flat_map(|l| [l.from, l.to])
            .filter(|id| Some(*id) != self.task)
            .collect()
    }
}

/// A card's box in the board's own coordinates, and how far down its column is visible (a
/// column scrolls on its own, so a card can sit outside it).
#[derive(Clone, Copy, Debug, PartialEq)]
struct CardBox {
    left: f64,
    right: f64,
    top: f64,
    bottom: f64,
    lane_top: f64,
    lane_bottom: f64,
}

impl CardBox {
    /// Where a line meets the card: its middle, kept inside the visible part of the column.
    fn mid(&self) -> f64 {
        let (lo, hi) = (
            self.lane_top + 6.0,
            (self.lane_bottom - 6.0).max(self.lane_top + 6.0),
        );
        ((self.top + self.bottom) / 2.0).clamp(lo, hi)
    }
}

/// How far a line between two cards of one column bulges out into the gap on their right.
const BULGE: f64 = 16.0;

/// The curve from one card to another: out of the side that faces the other card, into the side
/// that faces this one. Cards of one column are joined round their right edge.
fn arrow_path(from: &CardBox, to: &CardBox) -> String {
    let (y1, y2) = (from.mid(), to.mid());
    if (from.left - to.left).abs() < 1.0 {
        let x = from.right;
        return format!(
            "M{x:.1},{y1:.1} C{c:.1},{y1:.1} {c:.1},{y2:.1} {x:.1},{y2:.1}",
            c = x + BULGE
        );
    }
    let (x1, x2) = if from.right <= to.left {
        (from.right, to.left)
    } else {
        (from.left, to.right)
    };
    let reach = ((x2 - x1).abs() / 2.0).max(30.0) * (x2 - x1).signum();
    format!(
        "M{x1:.1},{y1:.1} C{:.1},{y1:.1} {:.1},{y2:.1} {x2:.1},{y2:.1}",
        x1 + reach,
        x2 - reach
    )
}

/// A line to draw, ready for the SVG.
#[derive(Clone, Debug, PartialEq)]
struct Arrow {
    d: String,
    kind: LinkKind,
}

/// Measures the cards of `links` inside `stage` and works out the lines. Links to cards that
/// are not on the board (filtered out, older finished tasks) are left out.
fn measure_arrows(stage: &web_sys::Element, links: &[BoardLink]) -> Vec<Arrow> {
    let origin = stage.get_bounding_client_rect();
    let find = |id: Uuid| -> Option<CardBox> {
        let card = stage
            .query_selector(&format!("[data-task=\"{id}\"]"))
            .ok()??;
        let lane = card.closest("[data-lane]").ok()??;
        let (c, l) = (
            card.get_bounding_client_rect(),
            lane.get_bounding_client_rect(),
        );
        Some(CardBox {
            left: c.left() - origin.left(),
            right: c.right() - origin.left(),
            top: c.top() - origin.top(),
            bottom: c.bottom() - origin.top(),
            lane_top: l.top() - origin.top(),
            lane_bottom: l.bottom() - origin.top(),
        })
    };
    links
        .iter()
        .filter_map(|l| {
            Some(Arrow {
                d: arrow_path(&find(l.from)?, &find(l.to)?),
                kind: l.kind,
            })
        })
        .collect()
}

pub(crate) fn today() -> Option<Date> {
    let (y, m, d) = today_ymd();
    minimap_types::timefmt::parse_date(&format_ymd(y, m, d)).ok()
}

/// A column's tint (background and border), from the status's tone.
fn column_tint(tone: Tone) -> &'static str {
    match tone {
        Tone::Accent => "bg-accent/10 border-accent/50",
        Tone::Success => "bg-success/10 border-success/50",
        Tone::Warning => "bg-warning/10 border-warning/50",
        Tone::Danger => "bg-danger/10 border-danger/50",
        Tone::Neutral => "bg-panel border-line-strong",
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
/// Whether a click landed on blank board: not on a card, a button, a field or a link.
fn clicked_blank_space(ev: &ev::MouseEvent) -> bool {
    let Some(target) = ev
        .target()
        .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
    else {
        return false;
    };
    let busy = target
        .closest(
            "[data-task], button, a, input, select, textarea, label, [role=listbox], [role=menu]",
        )
        .ok()
        .flatten();
    busy.is_none()
}

fn card_key(r: &TaskRow) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}",
        r.task.id,
        r.task.updated_at.unix_timestamp_nanos(),
        r.task.status.as_str(),
        r.assignee.as_ref().map(|a| a.label.as_str()).unwrap_or(""),
        r.project.as_ref().map(|p| p.label.as_str()).unwrap_or(""),
        r.link_count,
        r.attachment_count,
        r.subtasks_done * 1000 + r.subtask_count,
        r.parent.as_ref().map(|p| p.label.as_str()).unwrap_or(""),
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
            d.as_ref().map(|d| {
                pending.with(|p| {
                    columns(
                        d,
                        p,
                        filters.show_closed.get(),
                        all_done.get(),
                        filters.board_sort.get(),
                    )
                })
            })
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

    // Keyboard on the card under the cursor: x done, s next status, f focus, 1-5 priority.
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
            "f" => {
                let on = is_on(row.task.focus.as_ref(), today());
                set_focus(id, toggle_choice(on), toasts, version);
            }
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

    // Each person's colour: ranked by id like objectives, so it never changes under a filter.
    let people = LocalResource::new(move || {
        version.track();
        api::list_node_summaries(NodeType::Person)
    });
    let hues = Memo::new(
        move |last: Option<&HashMap<Uuid, u16>>| match people.get() {
            Some(Ok(list)) => assign(&list.iter().map(|p| p.node.id).collect::<Vec<_>>()),
            _ => last.cloned().unwrap_or_default(),
        },
    );

    // The open task's links: those cards light up and lines join them.
    let selection = expect_context::<Selection>();
    let selected_task = Memo::new(move |_| {
        selection
            .0
            .get()
            .filter(|n| n.node_type == NodeType::Task)
            .map(|n| n.id)
    });
    // A click on empty board puts the highlights away for the open task (the pane stays open);
    // opening another task, or the same card again, brings them back.
    let dismissed = RwSignal::new(None::<Uuid>);
    Effect::new(move |_| {
        let task = selected_task.get();
        if dismissed.get_untracked() != task {
            dismissed.set(None);
        }
    });
    let open_task = Memo::new(move |_| {
        let task = selected_task.get();
        task.filter(|id| dismissed.get() != Some(*id))
    });
    let fetched = LocalResource::new(move || {
        version.track();
        let task = open_task.get();
        async move {
            let links = match task {
                Some(id) => api::list_edges_for(id)
                    .await
                    .map(|l| board_links(id, &l))
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            Focus { task, links }
        }
    });
    let focus = RwSignal::new(Focus::default());
    Effect::new(move |_| {
        let task = open_task.get();
        match fetched.get() {
            Some(f) if f.task == task => focus.set(f),
            // The answer is for another task: show none until the right one arrives.
            _ => focus.update(|f| {
                if f.task != task {
                    *f = Focus::default();
                }
            }),
        }
    });
    let linked = Memo::new(move |_| focus.with(Focus::linked));

    let today_date = today();
    // A click anywhere but a card or a control (blank space in a column, between columns, the
    // margins) returns the board to normal.
    let on_background_click = move |ev: ev::MouseEvent| {
        if !clicked_blank_space(&ev) {
            return;
        }
        if let Some(id) = selected_task.get_untracked() {
            dismissed.set(Some(id));
        }
    };

    let ctx = BoardCtx {
        hues,
        linked,
        scrolled: RwSignal::new(0),
        filters,
        cols,
        dragging,
        over,
        adding,
        all_done,
        dismissed,
        move_to: Callback::new(move |(id, status)| move_to(id, status)),
        order,
        today: today_date,
    };

    // The lines are measured from the cards once they are on screen, and again when a card
    // moves (a column scrolls, the window is resized).
    let stage = leptos::prelude::NodeRef::<leptos::html::Div>::new();
    let arrows = RwSignal::new(Vec::<Arrow>::new());
    window_event_listener(ev::resize, move |_| ctx.scrolled.update(|n| *n += 1));
    Effect::new(move |_| {
        ctx.scrolled.track();
        cols.track();
        let links = focus.with(|f| f.links.clone());
        request_animation_frame(move || {
            let found = match stage.get_untracked() {
                Some(el) if !links.is_empty() => measure_arrows(&el, &links),
                _ => Vec::new(),
            };
            arrows.set(found);
        });
    });

    view! {
        <div class="flex h-full flex-col" on:click=on_background_click>
            <PageHeader icon="tasks" title="Tasks" subtitle="Drag cards between columns to change their status">
                <button class=BUTTON_SOFT
                        on:click=move |_| adding.update(|a| *a = if a.is_some() { None } else { Some(TaskStatus::Todo) })>
                    {move || if adding.get().is_some() { "Cancel" } else { "New task" }}
                </button>
                <crate::components::meeting::NewMeetingButton />
                <Show when=move || focus.with(|f| !f.links.is_empty())>
                    <span class="truncate text-[11px] text-muted">
                        {move || focus_hint(linked.with(HashSet::len))}
                    </span>
                </Show>
                <span class="ml-auto"><LayoutToggle board=layout /></span>
                <Hints keys=&[("n", "new"), ("j/k", "move"), ("x", "done"), ("s", "next status"), ("f", "focus"), ("1-5", "priority")] />
            </PageHeader>
            <FilterControls filters=filters inbox=false board=true />
            {move || if data.with(Option::is_some) {
                view! {
                    <div class="flex min-h-0 flex-1 overflow-x-auto p-3">
                        <div node_ref=stage class="relative flex min-w-max gap-3">
                            {statuses(true).into_iter().map(|status| view! {
                                <BoardColumn status=status ctx=ctx />
                            }).collect_view()}
                            <LinkLines arrows=arrows />
                        </div>
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
    /// Each person's hue (by id), for the avatar.
    hues: Memo<HashMap<Uuid, u16>>,
    /// The tasks joined to the open one.
    linked: Memo<HashSet<Uuid>>,
    /// Bumped when a column scrolls, so the lines follow their cards.
    scrolled: RwSignal<u32>,
    filters: TaskFilters,
    cols: Memo<Option<Vec<Column>>>,
    dragging: RwSignal<Option<(Uuid, TaskStatus)>>,
    over: RwSignal<Option<TaskStatus>>,
    adding: RwSignal<Option<TaskStatus>>,
    all_done: RwSignal<bool>,
    dismissed: RwSignal<Option<Uuid>>,
    move_to: Callback<(Uuid, TaskStatus)>,
    order: Memo<Vec<Uuid>>,
    today: Option<Date>,
}

/// The lines over the board that join the open task to the tasks it is linked with.
#[component]
fn LinkLines(arrows: RwSignal<Vec<Arrow>>) -> impl IntoView {
    let numbered = move || arrows.get().into_iter().enumerate().collect::<Vec<_>>();
    let draw = |(_, a): (usize, Arrow)| match a.kind {
        LinkKind::Blocks => view! {
            <path d=a.d fill="none" class="stroke-accent" stroke-width="1.5"
                  marker-end="url(#link-head)" />
        }
        .into_any(),
        LinkKind::Relates => view! {
            <path d=a.d fill="none" class="stroke-accent/70" stroke-width="1.5"
                  stroke-dasharray="4 3" />
        }
        .into_any(),
        // Dots, not dashes, and no head: part-of is not an order.
        LinkKind::Part => view! {
            <path d=a.d fill="none" class="stroke-accent/70" stroke-width="2"
                  stroke-linecap="round" stroke-dasharray="0.1 5" />
        }
        .into_any(),
    };
    view! {
        <svg class="pointer-events-none absolute inset-0 z-10 h-full w-full overflow-visible"
             aria-hidden="true">
            <defs>
                <marker id="link-head" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7"
                        markerHeight="7" orient="auto">
                    <path d="M0,1 L10,5 L0,9 z" class="fill-accent" />
                </marker>
            </defs>
            <For each=numbered key=|(i, a)| (*i, a.d.clone()) children=draw />
        </svg>
    }
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
        scrolled,
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
                    "flex w-72 min-w-[16rem] shrink-0 flex-col rounded-sm border {} {}",
                    column_tint(tone),
                    if target() { "!bg-hover !border-accent border-dashed" } else { "" })
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
                <div data-lane="" class="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-2 pb-2"
                     on:scroll=move |_| scrolled.update(|n| *n += 1)>
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
            links: Vec::new(),
            focus: None,
            start_minute: None,
            length_minutes: None,
            // A board filtered to a type adds tasks of that type.
            task_type: type_for_new_task(&ctx.filters.task_type.get_untracked()),
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
        hues,
        linked,
        dismissed,
        ..
    } = ctx;
    let t = row.task;
    let (id, status) = (t.id, t.status);
    let title_label = t.title.clone();
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
    let is_linked = move || linked.with(|l| l.contains(&id));
    // While a task's links show, the cards that have nothing to do with it step back.
    let dimmed = move || linked.with(|l| !l.is_empty() && !l.contains(&id)) && !is_open();

    let repeats = t
        .recurrence
        .as_ref()
        .map(|r| format!("Repeats {}", r.describe()));
    let task_type = t.task_type.clone();
    // The frame for its type (spec 39), if the setting is on.
    let types = use_task_types();
    let frame_of = StoredValue::new(task_type.clone());
    let frame = move || {
        let kind = frame_of.with_value(|id| id.as_deref().and_then(|id| types.find(id)));
        frame_style(kind.as_ref(), types.borders())
    };
    let focus = t.focus;
    let finished = finish_badge(&t);
    let due = t.due_date.map(|d| due_pill(d, today, status));
    // A meeting has a time, not a deadline: it doesn't warm up as the day nears (spec 38).
    let is_meeting = t.is_meeting();
    let heat = deadline_heat(t.due_date, today, !closed && !is_meeting);
    let meeting_time = t.start_minute.filter(|_| is_meeting).map(|m| {
        (
            minimap_types::fmt_range(m, t.meeting_minutes()),
            t.status == TaskStatus::InProgress,
        )
    });
    let estimate = estimate_text(t.estimate_days);
    let link_count = row.link_count;
    let attached = attachment_hint(row.attachment_count, t.links.len());
    let subtasks = (row.subtask_count > 0).then_some((row.subtasks_done, row.subtask_count));
    let parent = row.parent.map(|p| p.label);
    let marks = link_count > 0 || attached.is_some() || subtasks.is_some() || parent.is_some();
    let assignee = row.assignee.map(|a| (a.node.id, a.label));
    let project = row.project.map(|p| p.label);
    let title_class = if closed {
        "break-words text-muted line-through"
    } else {
        "break-words font-medium"
    };
    // Urgent work (P1, P2) is the warning tone, like everywhere else.
    let priority = t.priority;
    let priority_class = priority_pill(priority, closed);

    view! {
        <article
            draggable="true"
            data-task=id.to_string()
            style=move || {
                // The border is the objective's colour (it was the left edge); the type gives the
                // line (spec 39).
                let mut style = frame_colour(hue()).unwrap_or_default();
                style.push_str(&frame().unwrap_or_default());
                if let Some(h) = heat {
                    style.push_str(&format!("--heat: {};", heat_strength(h)));
                }
                style
            }
            class=move || format!(
                "group relative cursor-grab select-none rounded-sm border p-2.5 transition-colors active:cursor-grabbing {} {} {} {}",
                "card-frame",
                // The border is the card's own, so its states are a tint and a ring.
                if is_open() { "bg-active ring-1 ring-accent/60" }
                else if is_linked() { "bg-hover ring-1 ring-accent" }
                else if on_cursor() { "bg-hover ring-1 ring-line-strong" }
                else { "bg-canvas hover:bg-hover" },
                if heat.is_some() && !is_open() && !on_cursor() { "heat" } else { "" },
                if in_the_air() { "opacity-40" } else if dimmed() { "opacity-50" } else { "" })
            on:click=move |_| {
                if let Some(i) = index() {
                    list.cursor.set(Some(i));
                }
                // Clicking the open card again shows its links again.
                dismissed.set(None);
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
            <CardMenu task=id label=title_label />
            {(!closed && !is_meeting).then(|| view! {
                <FocusStar task=id focus=focus place="absolute right-6 top-1" />
            })}
            {move || {
                let objectives = colours.of_project(project_id);
                let kind = task_type.clone();
                (kind.is_some() || !objectives.is_empty()).then(|| view! {
                    <div class="mb-1.5 flex flex-wrap items-center gap-1 pr-10">
                        <TypeChip id=kind />
                        <ObjectiveChips objectives=objectives />
                    </div>
                })
            }}
            <div class="flex items-start gap-1.5 pr-10">
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
            {marks.then(|| view! {
                <div class="mt-1.5 flex flex-wrap items-center gap-1.5">
                    {(link_count > 0).then(|| view! {
                        <span class="hue-chip" style=format!("--obj-h: {LINK_HUE}") title=link_hint(link_count)>
                            <Icon name="chain" size="h-3.5 w-3.5" />
                            <span>{link_label(link_count)}</span>
                        </span>
                    })}
                    {subtasks.map(|(done, total)| view! {
                        <span class="hue-chip" style=format!("--obj-h: {PART_HUE}") title=subtask_hint(done, total)>
                            <Icon name="subtasks" size="h-3.5 w-3.5" />
                            <span>{subtask_label(done, total)}</span>
                        </span>
                    })}
                    {parent.map(|name| {
                        let title = format!("Part of “{name}”");
                        view! {
                            <span class="hue-chip max-w-full" style=format!("--obj-h: {PART_HUE}") title=title>
                                <Icon name="subtasks" size="h-3.5 w-3.5" />
                                <span class="truncate">{name}</span>
                            </span>
                        }
                    })}
                    {attached.clone().map(|text| {
                        let title = format!("Attached: {text}");
                        view! {
                            <span class="hue-chip" style=format!("--obj-h: {FILE_HUE}") title=title>
                                <Icon name="attach" size="h-3.5 w-3.5" />
                                <span>{text}</span>
                            </span>
                        }
                    })}
                </div>
            })}
            <div class="mt-2 flex items-center gap-1.5 text-[11px]">
                <span class=priority_class title="Priority (1 is highest)">{priority_short(priority)}</span>
                {due.map(|pill| view! { <span class=pill.class title=pill.hint>{pill.text}</span> })}
                {meeting_time.map(|(range, live)| view! {
                    <span class=if live { "tabular-nums font-medium text-accent" } else { "tabular-nums text-muted" }
                          title="When the meeting is">{range}</span>
                })}
                {finished.map(|(text, tone)| view! {
                    <span class=tone.text() title="Finished against the due date">{text}</span>
                })}
                {(!estimate.is_empty()).then(|| view! {
                    <span class="tabular-nums text-muted" title="Estimate">{estimate}</span>
                })}
                {assignee.map(|(person, name)| view! {
                    <span class=move || format!(
                              "ml-auto flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-[9px] font-semibold {}",
                              if hues.with(|h| h.contains_key(&person)) { "avatar" }
                              else { "border border-line-strong bg-panel text-muted" })
                          style=move || hues.with(|h| h.get(&person).map(|h| format!("--obj-h: {h}")))
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
                links: Vec::new(),
                task_type: None,
                focus: None,
                start_minute: None,
                length_minutes: None,
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
            link_count: 0,
            attachment_count: 0,
            parent: None,
            subtask_count: 0,
            subtasks_done: 0,
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

    fn dated(title: &str, status: TaskStatus, due: Option<&str>, priority: u8) -> TaskRow {
        let mut r = row(title, status);
        r.task.due_date = due.map(date);
        r.task.priority = priority;
        r
    }

    #[test]
    fn one_setting_orders_every_column_by_deadline_or_priority() {
        let mut rows = Vec::new();
        for status in [TaskStatus::Todo, TaskStatus::Blocked] {
            rows.extend([
                dated("none", status, None, 1),
                dated("late", status, Some("2027-03-20"), 3),
                dated("soon-p3", status, Some("2027-03-02"), 3),
                dated("soon-p1", status, Some("2027-03-02"), 1),
            ]);
        }
        let by = |sort: BoardSort, column: usize| {
            let cols = columns(&rows, &HashMap::new(), false, false, sort);
            titles(&cols[column])
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        for column in [0, 2] {
            // Undated tasks are always last; a tie on the date goes to the higher priority.
            let soonest = ["soon-p1", "soon-p3", "late", "none"];
            assert_eq!(by(BoardSort::Default, column), soonest);
            assert_eq!(by(BoardSort::DueSoonest, column), soonest);
            assert_eq!(
                by(BoardSort::DueLatest, column),
                ["late", "soon-p1", "soon-p3", "none"]
            );
            assert_eq!(
                by(BoardSort::Priority, column),
                ["soon-p1", "none", "soon-p3", "late"]
            );
        }
    }

    #[test]
    fn done_is_newest_finished_by_default_and_follows_the_chosen_order_otherwise() {
        let mut a = row("a", TaskStatus::Done);
        a.task.completed_at = Some(OffsetDateTime::UNIX_EPOCH + Duration::days(5));
        a.task.due_date = Some(date("2027-03-20"));
        let mut b = row("b", TaskStatus::Done);
        b.task.completed_at = Some(OffsetDateTime::UNIX_EPOCH + Duration::days(1));
        b.task.due_date = Some(date("2027-03-02"));
        let rows = [a, b];
        let order = |sort| {
            let cols = columns(&rows, &HashMap::new(), false, false, sort);
            titles(&cols[3])
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert_eq!(order(BoardSort::Default), ["a", "b"]);
        assert_eq!(order(BoardSort::DueSoonest), ["b", "a"]);
        assert_eq!(order(BoardSort::DueLatest), ["a", "b"]);
    }

    #[test]
    fn the_board_orders_have_ids_that_read_back() {
        for sort in BoardSort::ALL {
            assert_eq!(BoardSort::from_id(sort.id()), Some(sort));
        }
        assert_eq!(BoardSort::from_id("nonsense"), None);
        assert_eq!(BoardSort::default(), BoardSort::Default);
        assert_eq!(BoardSort::options().len(), BoardSort::ALL.len());
    }

    #[test]
    fn cards_keep_the_lists_order_inside_a_column() {
        let rows = [
            row("a", TaskStatus::Todo),
            row("b", TaskStatus::InProgress),
            row("c", TaskStatus::Todo),
        ];
        let cols = columns(&rows, &HashMap::new(), false, false, BoardSort::Default);
        assert_eq!(titles(&cols[0]), ["a", "c"]);
        assert_eq!(titles(&cols[1]), ["b"]);
        assert!(cols[2].rows.is_empty() && cols[3].rows.is_empty());
    }

    #[test]
    fn a_saving_move_already_sits_in_its_new_column() {
        let rows = [row("a", TaskStatus::Todo), row("b", TaskStatus::Todo)];
        let pending = HashMap::from([(rows[0].task.id, TaskStatus::Blocked)]);
        let cols = columns(&rows, &pending, false, false, BoardSort::Default);
        assert_eq!(titles(&cols[0]), ["b"]);
        assert_eq!(titles(&cols[2]), ["a"]);
        assert_eq!(cols[2].rows[0].task.status, TaskStatus::Blocked);
    }

    #[test]
    fn cancelled_tasks_show_only_when_asked_for() {
        let rows = [row("a", TaskStatus::Cancelled)];
        assert_eq!(
            columns(&rows, &HashMap::new(), false, false, BoardSort::Default).len(),
            4
        );
        let shown = columns(&rows, &HashMap::new(), true, false, BoardSort::Default);
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
        let done = &columns(&rows, &HashMap::new(), false, false, BoardSort::Default)[3];
        assert_eq!(done.rows.len(), DONE_VISIBLE);
        assert_eq!(done.hidden, 3);
        assert_eq!(done.rows[0].task.title, format!("t{}", DONE_VISIBLE + 2));
        let all = &columns(&rows, &HashMap::new(), false, true, BoardSort::Default)[3];
        assert_eq!((all.rows.len(), all.hidden), (DONE_VISIBLE + 3, 0));
    }

    #[test]
    fn a_card_just_dropped_on_done_leads_the_column() {
        let mut old = row("old", TaskStatus::Done);
        old.task.completed_at = Some(OffsetDateTime::UNIX_EPOCH + Duration::days(9));
        let fresh = row("fresh", TaskStatus::InProgress);
        let pending = HashMap::from([(fresh.task.id, TaskStatus::Done)]);
        let done = &columns(&[old, fresh], &pending, false, false, BoardSort::Default)[3];
        assert_eq!(titles(done), ["fresh", "old"]);
    }

    #[test]
    fn the_due_pill_gets_louder_as_the_day_nears_and_only_for_open_tasks() {
        let today = Some(date("2027-03-10"));
        let pill = |d: &str, s| due_pill(date(d), today, s);
        let overdue = pill("2027-03-08", TaskStatus::Todo);
        assert_eq!(overdue.text, "2d overdue");
        assert!(overdue.class.contains("bg-danger"));
        assert!(overdue.hint.starts_with("Overdue"));
        let now = pill("2027-03-10", TaskStatus::Blocked);
        assert_eq!(now.text, "Today");
        assert!(now.class.contains("bg-warning ") && now.class.contains("text-canvas"));
        assert_eq!(pill("2027-03-11", TaskStatus::Todo).text, "Tomorrow");
        let soon = pill("2027-03-17", TaskStatus::Todo);
        assert_eq!(soon.text, "In 7d");
        assert!(soon.class.contains("bg-warning/10"));
        // Further out, or closed, is just the date.
        for (d, s) in [
            ("2027-03-18", TaskStatus::Todo),
            ("2027-03-08", TaskStatus::Done),
            ("2027-03-08", TaskStatus::Cancelled),
        ] {
            let plain = pill(d, s);
            assert_eq!(
                (plain.text.as_str(), plain.class),
                (d, "tabular-nums text-muted")
            );
        }
        let unknown = due_pill(date("2027-03-09"), None, TaskStatus::Todo);
        assert_eq!(unknown.text, "2027-03-09");
    }

    #[test]
    fn priority_one_is_solid_two_is_tinted_and_closed_tasks_are_quiet() {
        assert!(priority_pill(1, false).contains("bg-warning "));
        assert!(priority_pill(2, false).contains("bg-warning/10"));
        assert_eq!(priority_pill(3, false), CHIP);
        assert_eq!(priority_pill(1, true), CHIP);
    }

    #[test]
    fn the_paperclip_counts_files_and_web_links_and_hides_when_there_are_none() {
        assert_eq!(attachment_hint(0, 0), None);
        assert_eq!(attachment_hint(1, 0).as_deref(), Some("1 file"));
        assert_eq!(attachment_hint(0, 2).as_deref(), Some("2 web links"));
        assert_eq!(
            attachment_hint(3, 1).as_deref(),
            Some("3 files, 1 web link")
        );
        assert_eq!(
            link_hint(1),
            "Blocks, waits for or is related to 1 task. Open the card to see them."
        );
        assert_eq!(
            link_hint(4),
            "Blocks, waits for or is related to 4 tasks. Open the card to see them."
        );
        assert_eq!(link_label(1), "1 linked task");
        assert_eq!(link_label(4), "4 linked tasks");
    }

    #[test]
    fn avatars_use_up_to_two_initials() {
        assert_eq!(initials("Ada Byron"), "AB");
        assert_eq!(initials("priya"), "P");
        assert_eq!(initials("  Mary Jane Watson "), "MJ");
        assert_eq!(initials(""), "");
    }

    fn link(
        task: Uuid,
        other: Uuid,
        edge_type: EdgeType,
        outgoing: bool,
        other_type: NodeType,
    ) -> EdgeLink {
        let at = OffsetDateTime::UNIX_EPOCH;
        let (from, to) = if outgoing {
            (task, other)
        } else {
            (other, task)
        };
        EdgeLink {
            edge: minimap_types::Edge {
                id: Uuid::from_u128(99),
                edge_type,
                from_type: if outgoing { NodeType::Task } else { other_type },
                from_id: from,
                to_type: if outgoing { other_type } else { NodeType::Task },
                to_id: to,
                attrs: serde_json::json!({}),
                created_at: at,
                archived_at: None,
            },
            outgoing,
            other: minimap_types::NodeSummary {
                node: NodeRef::new(other_type, other),
                label: String::new(),
                archived: false,
            },
        }
    }

    #[test]
    fn blocks_point_from_the_blocker_and_related_tasks_have_no_direction() {
        let [me, a, b, c, d, e] = [1, 2, 3, 4, 7, 8].map(Uuid::from_u128);
        let links = [
            link(me, a, EdgeType::Blocks, true, NodeType::Task),
            link(me, b, EdgeType::Blocks, false, NodeType::Task),
            link(me, c, EdgeType::RelatesTo, false, NodeType::Task),
            // Part of: a sub-task points at its parent, whichever end the open task is.
            link(me, d, EdgeType::SubtaskOf, true, NodeType::Task),
            link(me, e, EdgeType::SubtaskOf, false, NodeType::Task),
            // Not tasks, or not task links: nothing to draw.
            link(
                me,
                Uuid::from_u128(5),
                EdgeType::AssignedTo,
                true,
                NodeType::Person,
            ),
            link(
                me,
                Uuid::from_u128(6),
                EdgeType::RelatesTo,
                true,
                NodeType::Project,
            ),
        ];
        let made = board_links(me, &links);
        let at = |from, to, kind| BoardLink { from, to, kind };
        assert_eq!(
            made,
            [
                at(me, a, LinkKind::Blocks),
                at(b, me, LinkKind::Blocks),
                at(me, c, LinkKind::Relates),
                at(me, d, LinkKind::Part),
                at(e, me, LinkKind::Part),
            ]
        );
        let focus = Focus {
            task: Some(me),
            links: made,
        };
        assert_eq!(focus.linked(), HashSet::from([a, b, c, d, e]));
    }

    #[test]
    fn the_board_says_what_each_kind_of_line_means() {
        let hint = focus_hint(3);
        assert!(hint.starts_with("3 linked"));
        for word in ["blocker", "dashed", "dotted"] {
            assert!(hint.contains(word), "{hint}");
        }
        assert!(!hint.contains("child"));
    }

    #[test]
    fn the_sub_task_mark_counts_done_over_all() {
        assert_eq!(subtask_label(2, 5), "2/5 sub-tasks");
        assert_eq!(subtask_label(0, 1), "0/1 sub-task");
        assert!(subtask_hint(2, 5).starts_with("2 of 5 sub-tasks done"));
    }

    fn card(left: f64, top: f64) -> CardBox {
        CardBox {
            left,
            right: left + 100.0,
            top,
            bottom: top + 40.0,
            lane_top: 0.0,
            lane_bottom: 500.0,
        }
    }

    #[test]
    fn a_line_leaves_the_side_that_faces_the_other_card() {
        let right = arrow_path(&card(0.0, 0.0), &card(200.0, 100.0));
        assert!(
            right.starts_with("M100.0,20.0 ") && right.ends_with(" 200.0,120.0"),
            "{right}"
        );
        let left = arrow_path(&card(200.0, 100.0), &card(0.0, 0.0));
        assert!(
            left.starts_with("M200.0,120.0 ") && left.ends_with(" 100.0,20.0"),
            "{left}"
        );
    }

    #[test]
    fn cards_of_one_column_are_joined_round_their_right_edge() {
        let d = arrow_path(&card(0.0, 0.0), &card(0.0, 200.0));
        assert_eq!(d, "M100.0,20.0 C116.0,20.0 116.0,220.0 100.0,220.0");
    }

    #[test]
    fn a_card_scrolled_out_of_its_column_is_met_at_the_column_edge() {
        let mut gone = card(0.0, 900.0);
        gone.lane_bottom = 500.0;
        assert_eq!(gone.mid(), 494.0);
        gone.top = -300.0;
        gone.bottom = -260.0;
        assert_eq!(gone.mid(), 6.0);
    }

    #[test]
    fn the_s_key_walks_to_do_in_progress_done() {
        assert_eq!(next_status(TaskStatus::Todo), TaskStatus::InProgress);
        assert_eq!(next_status(TaskStatus::InProgress), TaskStatus::Done);
        assert_eq!(next_status(TaskStatus::Done), TaskStatus::Todo);
        assert_eq!(next_status(TaskStatus::Blocked), TaskStatus::Todo);
    }

    #[test]
    fn every_column_has_a_tint_and_a_hint() {
        for s in TaskStatus::ALL.iter().copied() {
            assert!(column_tint(task_status_tone(s)).contains("border-"));
            assert!(!empty_hint(s).is_empty());
        }
    }
}
