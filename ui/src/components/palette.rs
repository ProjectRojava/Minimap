//! Command palette (Ctrl/Cmd+K): go to a screen, run an action, find a node, or capture work in
//! one line. A line that starts with `task`, `project`, `wait`, `note` or `decision` and a space
//! is quick-add: the palette shows what it understood, asks about names it can't place, and
//! Enter adds it. Everything is keyboard driven. Grammar: `docs/quick-add-grammar.md`.

use leptos::{ev, prelude::*, task::spawn_local};
use leptos_router::hooks::use_navigate;
use minimap_types::{
    QuickChoice, QuickPick, QuickPreview, QuickRef, RefState, SearchFilter, SearchHit,
    WaitingOnFilter, WaitingOnRow,
};

use crate::{
    api,
    nav::{move_cursor, type_label, NAV},
    state::{finish, undo_or_redo, DataVersion, PaletteOpen, Selection, Toasts},
};

/// Words that turn the line into quick-add (when followed by a space).
const KEYWORDS: [&str; 6] = ["task", "project", "wait", "waiting", "note", "decision"];
const MAX_COMMANDS: usize = 8;
const MAX_NODES: usize = 8;

// ------------------------------------------------------------------ pure logic

/// `task Fix login`, `note 1:1 @priya`...: a keyword followed by a space.
pub fn is_quick_add(text: &str) -> bool {
    text.trim_start()
        .split_once(char::is_whitespace)
        .is_some_and(|(first, _)| KEYWORDS.contains(&first.to_lowercase().as_str()))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Go(&'static str),
    /// Start a quick-add line with this keyword.
    Prefill(&'static str),
    ResolveWaiting,
    /// Open the new-meeting dialog (spec 38).
    NewMeeting,
    /// Undo or redo the last change (spec 25).
    Undo,
    Redo,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub label: String,
    pub hint: String,
    pub action: Action,
}

/// Actions first, then every screen in the sidebar.
pub fn commands() -> Vec<Command> {
    let action = |label: &str, hint: &str, action| Command {
        label: label.into(),
        hint: hint.into(),
        action,
    };
    let mut all = vec![
        action("New task", "task …", Action::Prefill("task ")),
        action("New project", "project …", Action::Prefill("project ")),
        action("New waiting-on", "wait @name …", Action::Prefill("wait ")),
        action("New note", "note …", Action::Prefill("note ")),
        action("New decision", "decision …", Action::Prefill("decision ")),
        action("New meeting…", "a day and a time", Action::NewMeeting),
        action("Resolve waiting-on…", "", Action::ResolveWaiting),
        action("Undo last change", "Ctrl Z", Action::Undo),
        action("Redo", "Ctrl Shift Z", Action::Redo),
        action("What if this slips?", "g f", Action::Go("/what-if")),
    ];
    all.extend(NAV.iter().filter(|n| n.enabled).map(|n| Command {
        label: format!("Go to {}", n.label),
        hint: format!("g {}", n.chord),
        action: Action::Go(n.path),
    }));
    all
}

/// Commands whose label contains every word of the query (all of them for an empty query).
pub fn matching(query: &str, all: &[Command]) -> Vec<Command> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    all.iter()
        .filter(|c| {
            let label = c.label.to_lowercase();
            words.iter().all(|w| label.contains(w.as_str()))
        })
        .cloned()
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Command(Command),
    Node(SearchHit),
    /// "Add task: <text>": the way into quick-add for text without a keyword.
    AddTask(String),
}

pub fn build_rows(query: &str, commands: &[Command], hits: &[SearchHit]) -> Vec<Row> {
    let mut rows: Vec<Row> = matching(query, commands)
        .into_iter()
        .take(MAX_COMMANDS)
        .map(Row::Command)
        .collect();
    rows.extend(hits.iter().take(MAX_NODES).cloned().map(Row::Node));
    if !query.trim().is_empty() {
        rows.push(Row::AddTask(query.trim().to_owned()));
    }
    rows
}

/// One answer the user can give for a reference that needs one.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub label: String,
    pub pick: QuickPick,
}

/// The first reference still waiting for an answer (they are answered in order).
pub fn first_pending(p: &QuickPreview) -> Option<&QuickRef> {
    p.refs
        .iter()
        .find(|r| matches!(r.state, RefState::Pending { .. }))
}

/// The options for a pending reference: each candidate, then "create", then "skip".
pub fn pending_entries(r: &QuickRef) -> Vec<Entry> {
    let RefState::Pending {
        options,
        can_create,
        skip,
        ..
    } = &r.state
    else {
        return Vec::new();
    };
    let mut out: Vec<Entry> = options
        .iter()
        .map(|o| Entry {
            label: o.label.clone(),
            pick: QuickPick::Node(o.node.id),
        })
        .collect();
    if *can_create {
        out.push(Entry {
            label: format!("Create \u{201c}{}\u{201d}", r.query),
            pick: QuickPick::Create,
        });
    }
    if let Some(effect) = skip {
        out.push(Entry {
            label: format!("Skip: {effect}"),
            pick: QuickPick::Skip,
        });
    }
    out
}

/// The question to ask about a pending reference.
pub fn pending_heading(r: &QuickRef) -> String {
    match &r.state {
        RefState::Pending { options, guess, .. } if options.is_empty() => {
            format!("{}: no match for \u{201c}{}\u{201d}", r.label, r.query)
        }
        RefState::Pending { guess: true, .. } => {
            format!("{}: did you mean…? (\u{201c}{}\u{201d})", r.label, r.query)
        }
        RefState::Pending { .. } => {
            format!("{}: which one? (\u{201c}{}\u{201d})", r.label, r.query)
        }
        _ => r.label.clone(),
    }
}

fn waiting_matches(row: &WaitingOnRow, query: &str) -> bool {
    let hay = format!("{} {}", row.person.label, row.waiting.description).to_lowercase();
    query
        .split_whitespace()
        .all(|w| hay.contains(&w.to_lowercase()))
}

// ------------------------------------------------------------------- component

/// Everything the handlers share. All fields are `Copy`, so closures can capture it freely.
#[derive(Clone, Copy)]
struct Pal {
    open: RwSignal<bool>,
    input: RwSignal<String>,
    cursor: RwSignal<usize>,
    resolving: RwSignal<bool>,
    choices: RwSignal<Vec<QuickChoice>>,
    go: RwSignal<Option<&'static str>>,
    selection: Selection,
    toasts: Toasts,
    version: DataVersion,
    meetings: crate::state::MeetingDialog,
}

impl Pal {
    fn close(self) {
        self.open.set(false);
    }

    fn reset(self) {
        self.input.set(String::new());
        self.cursor.set(0);
        self.resolving.set(false);
        self.choices.set(Vec::new());
    }

    fn run(self, row: Row) {
        match row {
            Row::Command(c) => match c.action {
                Action::Go(path) => {
                    self.go.set(Some(path));
                    self.close();
                }
                Action::Prefill(keyword) => {
                    self.input.set(keyword.to_owned());
                    self.cursor.set(0);
                }
                Action::NewMeeting => {
                    self.close();
                    self.meetings.new_meeting();
                }
                Action::ResolveWaiting => {
                    self.resolving.set(true);
                    self.input.set(String::new());
                    self.cursor.set(0);
                }
                Action::Undo | Action::Redo => {
                    self.close();
                    undo_or_redo(c.action == Action::Redo, self.toasts, self.version);
                }
            },
            Row::Node(hit) => {
                self.selection.open(hit.node);
                self.close();
            }
            Row::AddTask(text) => {
                self.input.set(format!("task {text}"));
                self.cursor.set(0);
            }
        }
    }

    fn choose(self, key: String, pick: QuickPick) {
        self.choices.update(|c| {
            c.retain(|x| x.key != key);
            c.push(QuickChoice { key, pick });
        });
        self.cursor.set(0);
    }

    fn commit(self) {
        let text = self.input.get_untracked();
        let choices = self.choices.get_untracked();
        spawn_local(async move {
            if let Some(done) = finish(
                api::commit_quick_add(text, choices).await,
                self.toasts,
                self.version,
            ) {
                self.close();
                let extra = if done.created.is_empty() {
                    String::new()
                } else {
                    let names: Vec<&str> = done.created.iter().map(|c| c.label.as_str()).collect();
                    format!(" (also created {})", names.join(", "))
                };
                self.toasts.info(format!(
                    "Added {} \u{201c}{}\u{201d}{extra}",
                    type_label(done.node.node.node_type).to_lowercase(),
                    done.node.label
                ));
                self.selection.open(done.node.node);
            }
        });
    }

    fn resolve_waiting(self, id: minimap_types::Uuid, what: String) {
        spawn_local(async move {
            if finish(api::resolve_waiting_on(id).await, self.toasts, self.version).is_some() {
                self.close();
                self.toasts.info(format!("Resolved: {what}"));
            }
        });
    }
}

/// The palette overlay. Mount once, inside the router.
#[component]
pub fn PaletteHost() -> impl IntoView {
    let pal = Pal {
        open: expect_context::<PaletteOpen>().0,
        input: RwSignal::new(String::new()),
        cursor: RwSignal::new(0),
        resolving: RwSignal::new(false),
        choices: RwSignal::new(Vec::new()),
        go: RwSignal::new(None),
        selection: expect_context::<Selection>(),
        toasts: expect_context::<Toasts>(),
        version: expect_context::<DataVersion>(),
        meetings: expect_context::<crate::state::MeetingDialog>(),
    };
    let hits = RwSignal::new(Vec::<SearchHit>::new());
    let preview = RwSignal::new(Option::<QuickPreview>::None);
    let waits = RwSignal::new(Vec::<WaitingOnRow>::new());
    let all_commands = StoredValue::new(commands());
    // Answers can arrive out of order; only the newest request may update what is shown.
    let latest = StoredValue::new(0_u64);
    let input_ref = leptos::prelude::NodeRef::<leptos::html::Input>::new();

    let navigate = use_navigate();
    Effect::new(move |_| {
        if let Some(path) = pal.go.get() {
            navigate(path, Default::default());
            pal.go.set(None);
        }
    });
    // A fresh palette each time it opens, with the cursor in the box.
    Effect::new(move |was_open: Option<bool>| {
        let now = pal.open.get();
        if now && was_open != Some(true) {
            pal.reset();
            hits.set(Vec::new());
            preview.set(None);
        }
        now
    });
    Effect::new(move |_| {
        if let Some(el) = input_ref.get() {
            let _ = el.focus();
        }
    });
    // What to show follows the text: waiting-ons, a quick-add preview, or search results.
    Effect::new(move |_| {
        if !pal.open.get() {
            return;
        }
        let text = pal.input.get();
        let resolving = pal.resolving.get();
        let choices = pal.choices.get();
        let ticket = latest.get_value() + 1;
        latest.set_value(ticket);
        if resolving {
            spawn_local(async move {
                if let Ok(rows) = api::get_waiting_on(WaitingOnFilter::default()).await {
                    if latest.get_value() == ticket {
                        waits.set(rows);
                    }
                }
            });
        } else if is_quick_add(&text) {
            spawn_local(async move {
                match api::parse_quick_add(text, choices).await {
                    Ok(p) if latest.get_value() == ticket => {
                        preview.set(Some(p));
                        pal.cursor.set(0);
                    }
                    Ok(_) => {}
                    Err(e) => pal.toasts.error(&e),
                }
            });
        } else if text.trim().is_empty() {
            preview.set(None);
            hits.set(Vec::new());
        } else {
            preview.set(None);
            let filter = SearchFilter {
                limit: Some(MAX_NODES as u32),
                ..Default::default()
            };
            spawn_local(async move {
                if let Ok(found) = api::search(text, filter).await {
                    if latest.get_value() == ticket {
                        hits.set(found);
                        pal.cursor.set(0);
                    }
                }
            });
        }
    });

    let rows = move || build_rows(&pal.input.get(), &all_commands.get_value(), &hits.get());
    let open_waits = move || -> Vec<WaitingOnRow> {
        let q = pal.input.get();
        waits
            .get()
            .into_iter()
            .filter(|w| waiting_matches(w, &q))
            .collect()
    };
    let pending_len = move || {
        preview.with(|p| {
            p.as_ref()
                .and_then(|p| first_pending(p).map(|r| pending_entries(r).len()))
        })
    };
    // How many rows the arrow keys move over right now.
    let list_len = move || {
        if pal.resolving.get_untracked() {
            open_waits().len()
        } else if is_quick_add(&pal.input.get_untracked()) {
            pending_len().unwrap_or(0)
        } else {
            rows().len()
        }
    };

    let on_enter = move || {
        let cursor = pal.cursor.get_untracked();
        if pal.resolving.get_untracked() {
            if let Some(w) = open_waits().get(cursor) {
                pal.resolve_waiting(w.waiting.id, w.waiting.description.clone());
            }
        } else if is_quick_add(&pal.input.get_untracked()) {
            let Some(p) = preview.get_untracked() else {
                return;
            };
            if p.ready {
                pal.commit();
            } else if let Some(r) = first_pending(&p) {
                if let Some(e) = pending_entries(r).get(cursor) {
                    pal.choose(r.key.clone(), e.pick.clone());
                }
            }
        } else if let Some(row) = rows().get(cursor) {
            pal.run(row.clone());
        }
    };
    let on_keydown = move |ev: ev::KeyboardEvent| match ev.key().as_str() {
        "Escape" => {
            ev.prevent_default();
            if pal.resolving.get_untracked() {
                pal.resolving.set(false);
                pal.input.set(String::new());
            } else {
                pal.close();
            }
        }
        "ArrowDown" | "ArrowUp" => {
            ev.prevent_default();
            let delta = if ev.key() == "ArrowDown" { 1 } else { -1 };
            let next = move_cursor(Some(pal.cursor.get_untracked()), list_len(), delta);
            pal.cursor.set(next.unwrap_or(0));
        }
        "Enter" => {
            ev.prevent_default();
            on_enter();
        }
        _ => {}
    };

    let body = move || -> AnyView {
        if pal.resolving.get() {
            let list = open_waits();
            if list.is_empty() {
                return view! { <p class="px-3 py-2 text-muted">"Nothing is waiting on anyone."</p> }
                    .into_any();
            }
            return list
                .into_iter()
                .enumerate()
                .map(|(i, w)| {
                    let id = w.waiting.id;
                    let what = w.waiting.description.clone();
                    let who = w.person.label.clone();
                    let age = format!("{}d", w.age_days);
                    let text = what.clone();
                    view! {
                        <PaletteRow index=i cursor=pal.cursor
                            on_pick=move || pal.resolve_waiting(id, what.clone())>
                            <span class="w-28 shrink-0 truncate text-muted">{who}</span>
                            <span class="min-w-0 flex-1 truncate">{text}</span>
                            <span class="shrink-0 text-[11px] text-muted">{age}</span>
                        </PaletteRow>
                    }
                })
                .collect_view()
                .into_any();
        }
        if is_quick_add(&pal.input.get()) {
            return match preview.get() {
                Some(p) => view! { <QuickCard preview=p pal=pal /> }.into_any(),
                None => view! { <p class="px-3 py-2 text-muted">"…"</p> }.into_any(),
            };
        }
        rows()
            .into_iter()
            .enumerate()
            .map(|(i, row)| {
                let picked = row.clone();
                let (tag, label, hint) = match &row {
                    Row::Command(c) => ("Do".to_owned(), c.label.clone(), c.hint.clone()),
                    Row::Node(h) => (
                        type_label(h.node.node_type).to_owned(),
                        h.label.clone(),
                        if h.archived { "archived".to_owned() } else { h.snippet.clone() },
                    ),
                    Row::AddTask(t) => ("Add".to_owned(), format!("Add task \u{201c}{t}\u{201d}"), "task …".to_owned()),
                };
                view! {
                    <PaletteRow index=i cursor=pal.cursor on_pick=move || pal.run(picked.clone())>
                        <span class="w-16 shrink-0 text-[10px] uppercase tracking-wide text-muted">{tag}</span>
                        <span class="shrink-0 truncate font-medium">{label}</span>
                        <span class="min-w-0 flex-1 truncate text-[11px] text-muted">{hint}</span>
                    </PaletteRow>
                }
            })
            .collect_view()
            .into_any()
    };
    let footer = move || -> &'static str {
        if pal.resolving.get() {
            "↑↓ move · Enter resolve · Esc back"
        } else if is_quick_add(&pal.input.get()) {
            match preview.with(|p| p.as_ref().map(|p| (p.ready, first_pending(p).is_some()))) {
                Some((true, _)) => "Enter adds it · Esc closes",
                Some((false, true)) => "↑↓ choose · Enter confirms the choice · Esc closes",
                _ => "Fix the line above · Esc closes",
            }
        } else {
            "↑↓ move · Enter go · task / project / wait / note / decision + text adds"
        }
    };

    view! {
        {move || pal.open.get().then(|| view! {
            <div class="fixed inset-0 z-[80] bg-scrim" on:mousedown=move |_| pal.close()></div>
            <div role="dialog" aria-label="Command palette"
                 class="fixed left-1/2 top-[12vh] z-[81] w-[42rem] max-w-[94vw] -translate-x-1/2 \
                        rounded-sm border border-line bg-panel text-[13px]">
                <input node_ref=input_ref type="text" autocomplete="off" spellcheck="false"
                       class="w-full border-b border-line bg-transparent px-3 py-2 text-[14px] \
                              text-fg focus:outline-none"
                       placeholder=move || if pal.resolving.get() { "Resolve which waiting-on?" } else { "Go to…, search, or add: task Fix login @priya due:fri" }
                       prop:value=move || pal.input.get()
                       on:input=move |ev| { pal.input.set(event_target_value(&ev)); pal.cursor.set(0); }
                       on:keydown=on_keydown />
                <div class="max-h-[60vh] overflow-y-auto py-1" on:mousedown=move |ev| ev.prevent_default()>
                    {body}
                </div>
                <div class="border-t border-line px-3 py-1 text-[11px] text-muted">{footer}</div>
            </div>
        })}
    }
}

/// One selectable row. Picks on mouse-down (so the input keeps focus) and follows the pointer.
#[component]
fn PaletteRow(
    index: usize,
    cursor: RwSignal<usize>,
    on_pick: impl Fn() + 'static,
    children: Children,
) -> impl IntoView {
    view! {
        <div role="option"
             class=move || format!(
                 "flex items-baseline gap-2 px-3 h-7 cursor-default {}",
                 if cursor.get() == index { "bg-active" } else { "hover:bg-hover" })
             on:mousedown=move |ev| { ev.prevent_default(); on_pick(); }
             on:mousemove=move |_| cursor.set(index)>
            {children()}
        </div>
    }
}

/// What the line means, what is wrong with it, and the question about the first unclear name.
#[component]
fn QuickCard(preview: QuickPreview, pal: Pal) -> impl IntoView {
    let kind = match preview.kind {
        minimap_types::QuickKind::Task => "Task",
        minimap_types::QuickKind::Project => "Project",
        minimap_types::QuickKind::Wait => "Waiting on",
        minimap_types::QuickKind::Note => "Note",
        minimap_types::QuickKind::Decision => "Decision",
    };
    let title = if preview.title.is_empty() {
        "…".to_owned()
    } else {
        preview.title.clone()
    };
    let details = preview
        .details
        .iter()
        .map(|d| format!("{}: {}", d.label, d.value))
        .collect::<Vec<_>>()
        .join(" · ");
    let pending_key = first_pending(&preview).map(|r| r.key.clone());
    let refs = preview
        .refs
        .iter()
        .map(|r| {
            let is_active = pending_key.as_deref() == Some(r.key.as_str());
            let label = r.label.clone();
            let state = match &r.state {
                RefState::Resolved { node } => {
                    view! { <span>"✓ " {node.label.clone()}</span> }.into_any()
                }
                RefState::Created { name } => {
                    view! { <span>"+ new: " {name.clone()}</span> }.into_any()
                }
                RefState::Skipped { effect } => {
                    view! { <span class="text-muted">"skipped: " {effect.clone()}</span> }.into_any()
                }
                RefState::Pending { .. } if !is_active => {
                    view! { <span class="text-muted">"needs an answer (after the one above)"</span> }
                        .into_any()
                }
                RefState::Pending { .. } => {
                    let key = r.key.clone();
                    let heading = pending_heading(r);
                    let entries = pending_entries(r)
                        .into_iter()
                        .enumerate()
                        .map(|(i, e)| {
                            let key = key.clone();
                            let pick = e.pick.clone();
                            view! {
                                <PaletteRow index=i cursor=pal.cursor on_pick=move || pal.choose(key.clone(), pick.clone())>
                                    <span class="truncate">{e.label}</span>
                                </PaletteRow>
                            }
                        })
                        .collect_view();
                    view! {
                        <div class="w-full">
                            <div class="font-medium">{heading}</div>
                            <div class="-mx-3">{entries}</div>
                        </div>
                    }
                    .into_any()
                }
            };
            view! {
                <div class="flex items-baseline gap-2 py-0.5">
                    {(!is_active).then(|| view! { <span class="w-24 shrink-0 text-muted">{label}</span> })}
                    {state}
                </div>
            }
        })
        .collect_view();
    let problems = preview
        .problems
        .iter()
        .map(|p| view! { <p class="text-danger">{p.clone()}</p> })
        .collect_view();
    view! {
        <div class="px-3 py-1">
            <div class="flex items-baseline gap-2">
                <span class="text-[10px] uppercase tracking-wide text-muted">{kind}</span>
                <span class="font-medium">{title}</span>
            </div>
            {(!details.is_empty()).then(|| view! { <p class="text-[11px] text-muted">{details}</p> })}
            <div class="mt-1">{refs}</div>
            {problems}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NodeRef, NodeSummary, NodeType, Uuid};

    fn summary(label: &str, n: u128) -> NodeSummary {
        NodeSummary {
            node: NodeRef::new(NodeType::Person, Uuid::from_u128(n)),
            label: label.into(),
            archived: false,
        }
    }

    fn pending(
        options: Vec<NodeSummary>,
        guess: bool,
        create: bool,
        skip: Option<&str>,
    ) -> QuickRef {
        QuickRef {
            key: "Assignee:pri".into(),
            label: "Assignee".into(),
            query: "pri".into(),
            state: RefState::Pending {
                options,
                guess,
                can_create: create,
                skip: skip.map(str::to_owned),
            },
        }
    }

    #[test]
    fn quick_add_needs_a_keyword_and_a_space() {
        for yes in [
            "task Fix",
            "TASK x",
            "  note 1:1 @priya",
            "wait @raj on x",
            "waiting @raj",
            "project Q1",
            "decision A",
        ] {
            assert!(is_quick_add(yes), "{yes}");
        }
        for no in [
            "",
            "task",
            "note",
            "tasks Fix",
            "Fix login",
            "go to settings",
            "taskforce x",
        ] {
            assert!(!is_quick_add(no), "{no}");
        }
    }

    #[test]
    fn commands_cover_actions_and_every_visible_screen() {
        let all = commands();
        assert!(all.iter().any(|c| c.action == Action::Prefill("task ")));
        assert!(all.iter().any(|c| c.action == Action::ResolveWaiting));
        assert!(all.iter().any(|c| c.action == Action::Undo));
        assert!(all.iter().any(|c| c.action == Action::Redo));
        // Every visible screen can be reached.
        for n in NAV.iter().filter(|n| n.enabled) {
            assert!(
                all.iter().any(|c| c.action == Action::Go(n.path)),
                "{}",
                n.path
            );
        }
        // "What if this slips?" is also offered by that name.
        assert!(all.iter().any(|c| c.label == "What if this slips?"));
        assert!(all.iter().all(|c| !c.label.is_empty()));
    }

    #[test]
    fn matching_needs_every_word_in_any_order() {
        let all = commands();
        let labels =
            |q: &str| -> Vec<String> { matching(q, &all).into_iter().map(|c| c.label).collect() };
        assert_eq!(labels("").len(), all.len());
        assert!(labels("new task").contains(&"New task".to_owned()));
        assert_eq!(labels("task new"), ["New task"]);
        assert!(labels("go notes").contains(&"Go to Notes".to_owned()));
        assert!(labels("zzz").is_empty());
        assert!(labels("PROJ").contains(&"New project".to_owned()));
    }

    #[test]
    fn rows_end_with_the_way_to_add_what_was_typed() {
        let hit = SearchHit {
            node: NodeRef::new(NodeType::Task, Uuid::from_u128(1)),
            label: "Fix login timeout".into(),
            archived: false,
            snippet: String::new(),
        };
        let rows = build_rows("fix", &commands(), std::slice::from_ref(&hit));
        assert!(
            matches!(rows.first(), Some(Row::Node(h)) if *h == hit)
                || matches!(rows.first(), Some(Row::Command(_)))
        );
        assert_eq!(rows.last(), Some(&Row::AddTask("fix".into())));
        // Empty query: just the commands, nothing to add.
        let rows = build_rows("  ", &commands(), &[]);
        assert!(rows.iter().all(|r| matches!(r, Row::Command(_))));
        assert!(rows.len() <= MAX_COMMANDS);
    }

    #[test]
    fn pending_references_offer_candidates_then_create_then_skip() {
        let r = pending(
            vec![summary("Priya Shah", 2), summary("Priyanka Rao", 3)],
            false,
            true,
            Some("assigned to you instead"),
        );
        let e = pending_entries(&r);
        let labels: Vec<&str> = e.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Priya Shah",
                "Priyanka Rao",
                "Create \u{201c}pri\u{201d}",
                "Skip: assigned to you instead"
            ]
        );
        assert_eq!(e[0].pick, QuickPick::Node(Uuid::from_u128(2)));
        assert_eq!(e[2].pick, QuickPick::Create);
        assert_eq!(e[3].pick, QuickPick::Skip);
        // Required and not creatable: only the candidates.
        let only = pending(vec![summary("A", 1)], false, false, None);
        assert_eq!(pending_entries(&only).len(), 1);
        // A resolved reference has nothing to answer.
        let done = QuickRef {
            state: RefState::Created { name: "x".into() },
            ..r
        };
        assert!(pending_entries(&done).is_empty());
    }

    #[test]
    fn headings_say_whether_it_is_a_miss_a_guess_or_a_choice() {
        let miss = pending(vec![], false, true, None);
        assert!(pending_heading(&miss).contains("no match"));
        let guess = pending(vec![summary("Priya", 1)], true, true, None);
        assert!(pending_heading(&guess).contains("did you mean"));
        let many = pending(vec![summary("A", 1), summary("B", 2)], false, true, None);
        assert!(pending_heading(&many).contains("which one"));
    }

    #[test]
    fn the_first_pending_reference_is_asked_first() {
        let resolved = QuickRef {
            key: "Project:x".into(),
            label: "Project".into(),
            query: "x".into(),
            state: RefState::Resolved {
                node: summary("X", 9),
            },
        };
        let p = QuickPreview {
            kind: minimap_types::QuickKind::Task,
            title: "t".into(),
            details: vec![],
            refs: vec![resolved, pending(vec![], false, true, None)],
            problems: vec![],
            ready: false,
        };
        assert_eq!(
            first_pending(&p).map(|r| r.key.as_str()),
            Some("Assignee:pri")
        );
    }
}
