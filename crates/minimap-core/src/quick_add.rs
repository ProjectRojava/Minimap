//! Quick-add: one line of text -> a preview (what it means, what is unclear) and, once nothing
//! is unclear, a plan the store commits. Pure. The grammar is in `docs/quick-add-grammar.md`.

use minimap_types::{
    DecisionStatus, DirectoryEntry, NodeRef, NodeSummary, NodeType, NoteKind, QuickAssignee,
    QuickChoice, QuickDetail, QuickKind, QuickMain, QuickPick, QuickPlan, QuickPreview, QuickRef,
    Ref, RefState, TaskType,
};
use time::{Date, Duration, Weekday};

use crate::{
    recurrence::{first_on_or_after, parse_every},
    search::{distance, typo_budget},
    task_types,
    tasks::parse_estimate,
};

/// Most options offered for one unclear reference.
const MAX_OPTIONS: usize = 6;

pub struct Context<'a> {
    pub today: Date,
    pub hours_per_day: f64,
    pub directory: &'a [DirectoryEntry],
    pub choices: &'a [QuickChoice],
    /// The task types a `type:` can name (spec 32).
    pub task_types: &'a [TaskType],
}

pub struct Outcome {
    pub preview: QuickPreview,
    /// Present only when the preview is `ready`.
    pub plan: Option<QuickPlan>,
}

// ---------------------------------------------------------------------- dates

const DATE_HELP: &str = "Dates look like today, fri, next-wed, +3d, +2w or 2027-03-31";

fn weekday_named(name: &str) -> Option<Weekday> {
    Some(match name {
        "mon" | "monday" => Weekday::Monday,
        "tue" | "tues" | "tuesday" => Weekday::Tuesday,
        "wed" | "weds" | "wednesday" => Weekday::Wednesday,
        "thu" | "thur" | "thurs" | "thursday" => Weekday::Thursday,
        "fri" | "friday" => Weekday::Friday,
        "sat" | "saturday" => Weekday::Saturday,
        "sun" | "sunday" => Weekday::Sunday,
        _ => return None,
    })
}

/// A natural date relative to `today`.
///
/// - `today`, `tomorrow`
/// - a weekday (`fri`, `friday`): the next one, **today if it is that day**
/// - `next-fri`: that weekday **in the next calendar week** (Monday to Sunday)
/// - `next-week`: the Monday of the next calendar week
/// - `+3d`, `+2w`: days or weeks from today
/// - an ISO date `2027-03-31`
pub fn parse_when(text: &str, today: Date) -> Result<Date, String> {
    // "next week" and "next-week" are the same.
    let t = text.trim().to_lowercase().replace(' ', "-");
    let bad = || format!("\"{}\" isn't a date. {DATE_HELP}", text.trim());
    if t == "today" {
        return Ok(today);
    }
    if t == "tomorrow" {
        return Ok(today + Duration::days(1));
    }
    if t == "next-week" {
        let to_next_monday = 7 - i64::from(today.weekday().number_days_from_monday());
        return Ok(today + Duration::days(to_next_monday));
    }
    if let Some(day) = weekday_named(&t) {
        let ahead =
            (day.number_days_from_monday() + 7 - today.weekday().number_days_from_monday()) % 7;
        return Ok(today + Duration::days(i64::from(ahead)));
    }
    if let Some(day) = t.strip_prefix("next-").and_then(weekday_named) {
        let to_next_monday = 7 - i64::from(today.weekday().number_days_from_monday());
        return Ok(
            today + Duration::days(to_next_monday + i64::from(day.number_days_from_monday()))
        );
    }
    if let Some(rest) = t.strip_prefix('+') {
        let unit = rest.chars().last().ok_or_else(bad)?;
        let n: i64 = rest[..rest.len() - unit.len_utf8()]
            .parse()
            .map_err(|_| bad())?;
        let days = match unit {
            'd' => n,
            'w' => n.checked_mul(7).ok_or_else(bad)?,
            _ => return Err(bad()),
        };
        return today.checked_add(Duration::days(days)).ok_or_else(bad);
    }
    minimap_types::timefmt::parse_date(&t).map_err(|_| bad())
}

fn day_text(d: Date) -> String {
    let name = match d.weekday() {
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
        Weekday::Sunday => "Sun",
    };
    format!("{name} {d}")
}

// ---------------------------------------------------------------------- lexing

#[derive(Debug, Clone, PartialEq)]
struct Token {
    /// Quotes removed.
    text: String,
    /// The first character was inside quotes, so the token is plain text (no `@`, `#`, `key:`).
    literal: bool,
}

/// Splits on spaces outside double quotes. Returns whether a quote was left open.
fn lex(input: &str) -> (Vec<Token>, bool) {
    let mut tokens = Vec::new();
    let mut text = String::new();
    let mut literal = false;
    let mut started = false;
    let mut quoted = false;
    for c in input.chars() {
        match c {
            '"' | '\u{201c}' | '\u{201d}' => {
                if !started {
                    literal = true;
                }
                started = true;
                quoted = !quoted;
            }
            c if c.is_whitespace() && !quoted => {
                if started && !text.is_empty() {
                    tokens.push(Token {
                        text: std::mem::take(&mut text),
                        literal,
                    });
                }
                started = false;
                literal = false;
                text.clear();
            }
            c => {
                if !started {
                    literal = quoted;
                }
                started = true;
                text.push(c);
            }
        }
    }
    if started && !text.is_empty() {
        tokens.push(Token { text, literal });
    }
    (tokens, quoted)
}

const KEYS: [&str; 14] = [
    "due", "by", "target", "start", "est", "blocks", "for", "affects", "owner", "about", "status",
    "date", "every", "type",
];
const KEYS_EXTRA: [&str; 1] = ["kind"];

fn keyed(token: &Token) -> Option<(String, &str)> {
    if token.literal {
        return None;
    }
    let (k, v) = token.text.split_once(':')?;
    let k = k.to_lowercase();
    (KEYS.contains(&k.as_str()) || KEYS_EXTRA.contains(&k.as_str())).then_some((k, v))
}

fn kind_of(word: &str) -> Option<QuickKind> {
    Some(match word.to_lowercase().as_str() {
        "task" => QuickKind::Task,
        "project" => QuickKind::Project,
        "wait" | "waiting" => QuickKind::Wait,
        "note" => QuickKind::Note,
        "decision" => QuickKind::Decision,
        _ => return None,
    })
}

fn kind_name(k: QuickKind) -> &'static str {
    match k {
        QuickKind::Task => "task",
        QuickKind::Project => "project",
        QuickKind::Wait => "wait",
        QuickKind::Note => "note",
        QuickKind::Decision => "decision",
    }
}

// ------------------------------------------------------------------ references

/// What a reference is for; decides what it may match and what happens if left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Assignee,
    Owner,
    Project,
    Blocks,
    Objective,
    Affects,
    About,
    WaitPerson,
    Mention,
}

struct Request {
    slot: Slot,
    key: String,
    label: &'static str,
    query: String,
    types: Vec<NodeType>,
    create: Option<NodeType>,
    skip: Option<&'static str>,
}

/// A reference as typed, before matching: the text after `@`/`#`, and the type its sigil implies.
fn request(slot: Slot, raw: &str) -> Request {
    let (sigil, query) = match raw.chars().next() {
        Some(c @ ('@' | '#')) => (Some(c), raw[1..].trim().to_owned()),
        _ => (None, raw.trim().to_owned()),
    };
    let (label, types, create, skip): (&str, Vec<NodeType>, Option<NodeType>, Option<&str>) =
        match slot {
            Slot::Assignee => (
                "Assignee",
                vec![NodeType::Person],
                Some(NodeType::Person),
                Some("assigned to you instead"),
            ),
            Slot::Owner => (
                "Owner",
                vec![NodeType::Person],
                Some(NodeType::Person),
                Some("no owner"),
            ),
            Slot::Project => (
                "Project",
                vec![NodeType::Project],
                Some(NodeType::Project),
                Some("goes to the inbox (no project)"),
            ),
            Slot::Blocks => (
                "Blocks",
                vec![NodeType::Task],
                Some(NodeType::Task),
                Some("not linked"),
            ),
            Slot::Objective => (
                "For objective",
                vec![NodeType::Objective],
                Some(NodeType::Objective),
                Some("not linked to an objective"),
            ),
            Slot::Affects => {
                if sigil == Some('#') {
                    (
                        "Affects",
                        vec![NodeType::Project],
                        Some(NodeType::Project),
                        Some("not linked"),
                    )
                } else {
                    (
                        "Affects",
                        vec![NodeType::Project, NodeType::Task, NodeType::Objective],
                        None,
                        Some("not linked"),
                    )
                }
            }
            Slot::About => {
                if sigil == Some('#') {
                    (
                        "About",
                        vec![NodeType::Project],
                        Some(NodeType::Project),
                        Some("not linked"),
                    )
                } else {
                    (
                        "About",
                        vec![NodeType::Task, NodeType::Project],
                        None,
                        Some("not linked"),
                    )
                }
            }
            Slot::WaitPerson => (
                "Waiting on",
                vec![NodeType::Person],
                Some(NodeType::Person),
                None,
            ),
            Slot::Mention => {
                if sigil == Some('#') {
                    (
                        "Mentions",
                        vec![NodeType::Project],
                        Some(NodeType::Project),
                        Some("left out of the note"),
                    )
                } else {
                    (
                        "Mentions",
                        vec![NodeType::Person],
                        Some(NodeType::Person),
                        Some("left out of the note"),
                    )
                }
            }
        };
    Request {
        slot,
        key: format!("{label}:{}", query.to_lowercase()),
        label,
        query,
        types,
        create,
        skip,
    }
}

fn words_of(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

enum Found<'a> {
    One(&'a DirectoryEntry),
    Many(Vec<&'a DirectoryEntry>),
    Guess(Vec<&'a DirectoryEntry>),
    Nothing,
}

fn find<'a>(req: &Request, directory: &'a [DirectoryEntry]) -> Found<'a> {
    let q = req.query.to_lowercase();
    if q.is_empty() {
        return Found::Nothing;
    }
    let pool: Vec<&DirectoryEntry> = directory
        .iter()
        .filter(|e| req.types.contains(&e.node.node_type))
        .collect();
    if q == "me" && req.types == [NodeType::Person] {
        return pool
            .iter()
            .find(|e| e.is_self)
            .map_or(Found::Nothing, |e| Found::One(e));
    }
    let settle = |mut hits: Vec<&'a DirectoryEntry>| -> Option<Found<'a>> {
        hits.sort_by_key(|e| e.label.to_lowercase());
        match hits.len() {
            0 => None,
            1 => Some(Found::One(hits[0])),
            _ => {
                hits.truncate(MAX_OPTIONS);
                Some(Found::Many(hits))
            }
        }
    };
    // 1. The whole name or handle.
    let exact: Vec<&DirectoryEntry> = pool
        .iter()
        .copied()
        .filter(|e| e.label.to_lowercase() == q || e.handle.as_deref() == Some(q.as_str()))
        .collect();
    if let Some(found) = settle(exact) {
        return found;
    }
    let qwords = words_of(&q);
    if qwords.is_empty() {
        return Found::Nothing;
    }
    let entry_words = |e: &DirectoryEntry| -> Vec<String> {
        let mut w = words_of(&e.label);
        if let Some(h) = &e.handle {
            w.extend(words_of(h));
        }
        w
    };
    // 2. Every typed word is a whole word of the name (`@priya` -> Priya Shah).
    let whole: Vec<&DirectoryEntry> = pool
        .iter()
        .copied()
        .filter(|e| {
            let ew = entry_words(e);
            qwords.iter().all(|q| ew.contains(q))
        })
        .collect();
    if let Some(found) = settle(whole) {
        return found;
    }
    // 3. Every typed word starts a word of the name (`@pri`).
    let prefix: Vec<&DirectoryEntry> = pool
        .iter()
        .copied()
        .filter(|e| {
            let ew = entry_words(e);
            qwords
                .iter()
                .all(|q| ew.iter().any(|w| w.starts_with(q.as_str())))
        })
        .collect();
    if let Some(found) = settle(prefix) {
        return found;
    }
    // 4. A typo away: only ever offered as a guess, never taken silently.
    let mut guesses: Vec<(usize, &DirectoryEntry)> = pool
        .iter()
        .copied()
        .filter_map(|e| {
            let ew = entry_words(e);
            let mut total = 0;
            for q in &qwords {
                let budget = typo_budget(q.chars().count());
                let best = ew.iter().map(|w| distance(q, w)).min()?;
                if best > budget {
                    return None;
                }
                total += best;
            }
            Some((total, e))
        })
        .collect();
    guesses.sort_by_key(|g| (g.0, g.1.label.to_lowercase()));
    guesses.truncate(MAX_OPTIONS);
    if guesses.is_empty() {
        Found::Nothing
    } else {
        Found::Guess(guesses.into_iter().map(|(_, e)| e).collect())
    }
}

fn summary(e: &DirectoryEntry) -> NodeSummary {
    NodeSummary {
        node: e.node,
        label: e.label.clone(),
        archived: false,
    }
}

/// People get a capital letter on each word when created from typed text; other things keep it.
fn name_to_create(slot_type: NodeType, query: &str) -> String {
    if slot_type != NodeType::Person {
        return query.to_owned();
    }
    query
        .split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Working state while resolving: the preview rows and the nodes to create.
struct Resolver<'a> {
    cx: &'a Context<'a>,
    refs: Vec<QuickRef>,
    new_nodes: Vec<minimap_types::NewNode>,
}

impl<'a> Resolver<'a> {
    fn new(cx: &'a Context<'a>) -> Self {
        Resolver {
            cx,
            refs: Vec::new(),
            new_nodes: Vec::new(),
        }
    }

    fn new_node(&mut self, node_type: NodeType, name: String) -> Ref {
        let existing = self
            .new_nodes
            .iter()
            .position(|n| n.node_type == node_type && n.name.to_lowercase() == name.to_lowercase());
        let idx = existing.unwrap_or_else(|| {
            self.new_nodes
                .push(minimap_types::NewNode { node_type, name });
            self.new_nodes.len() - 1
        });
        Ref::New(idx as u32)
    }

    /// Resolves a reference, records it in the preview, and returns the node to use
    /// (`None` while it is pending or was skipped). A key seen twice is resolved once.
    fn resolve(&mut self, req: Request) -> Option<Ref> {
        if let Some(prev) = self.refs.iter().find(|r| r.key == req.key) {
            return match &prev.state {
                RefState::Resolved { node } => Some(Ref::Existing(node.node)),
                RefState::Created { name } => {
                    let t = req.create?;
                    Some(self.new_node(t, name.clone()))
                }
                _ => None,
            };
        }
        let choice = self
            .cx
            .choices
            .iter()
            .find(|c| c.key == req.key)
            .map(|c| &c.pick);
        let mut result: Option<Ref> = None;
        let state = match choice {
            Some(QuickPick::Node(id)) => {
                match self
                    .cx
                    .directory
                    .iter()
                    .find(|e| e.node.id == *id && req.types.contains(&e.node.node_type))
                {
                    Some(e) => {
                        result = Some(Ref::Existing(e.node));
                        Some(RefState::Resolved { node: summary(e) })
                    }
                    None => None,
                }
            }
            Some(QuickPick::Create) if req.create.is_some() => {
                let t = req.create.unwrap_or(NodeType::Person);
                let name = name_to_create(t, &req.query);
                result = Some(self.new_node(t, name.clone()));
                Some(RefState::Created { name })
            }
            Some(QuickPick::Skip) if req.skip.is_some() => Some(RefState::Skipped {
                effect: req.skip.unwrap_or_default().to_owned(),
            }),
            _ => None,
        };
        let state = state.unwrap_or_else(|| match find(&req, self.cx.directory) {
            Found::One(e) => {
                result = Some(Ref::Existing(e.node));
                RefState::Resolved { node: summary(e) }
            }
            Found::Many(list) => RefState::Pending {
                options: list.into_iter().map(summary).collect(),
                guess: false,
                can_create: req.create.is_some(),
                skip: req.skip.map(str::to_owned),
            },
            Found::Guess(list) => RefState::Pending {
                options: list.into_iter().map(summary).collect(),
                guess: true,
                can_create: req.create.is_some(),
                skip: req.skip.map(str::to_owned),
            },
            Found::Nothing => RefState::Pending {
                options: Vec::new(),
                guess: false,
                can_create: req.create.is_some(),
                skip: req.skip.map(str::to_owned),
            },
        });
        self.refs.push(QuickRef {
            key: req.key,
            label: req.label.to_owned(),
            query: req.query,
            state,
        });
        result
    }
}

// --------------------------------------------------------------------- parsing

/// Everything read from the tokens, before references are resolved.
#[derive(Default)]
struct Parsed {
    words: Vec<String>,
    requests: Vec<Request>,
    priority: Option<u8>,
    values: Vec<(String, String)>,
    note_kind: Option<NoteKind>,
    problems: Vec<String>,
}

fn parse_tokens(kind: QuickKind, tokens: &[Token]) -> Parsed {
    let mut p = Parsed::default();
    let mut person_seen = false;
    for t in tokens {
        if let Some((key, value)) = keyed(t) {
            if value.trim().is_empty() {
                p.problems.push(format!("{key}: needs a value"));
                continue;
            }
            let slot = match (key.as_str(), kind) {
                ("blocks", QuickKind::Task) => Some(Slot::Blocks),
                ("for", QuickKind::Task | QuickKind::Project) => Some(Slot::Objective),
                ("owner", QuickKind::Project) => Some(Slot::Owner),
                ("affects", QuickKind::Decision) => Some(Slot::Affects),
                ("about", QuickKind::Wait) => Some(Slot::About),
                _ => None,
            };
            if let Some(slot) = slot {
                p.requests.push(request(slot, value));
                continue;
            }
            let allowed = match kind {
                QuickKind::Task => ["due", "by", "start", "est", "every", "type"].as_slice(),
                QuickKind::Project => ["target", "due", "by", "start"].as_slice(),
                QuickKind::Wait => ["by", "due"].as_slice(),
                QuickKind::Note => ["date", "kind", "every"].as_slice(),
                QuickKind::Decision => ["date", "status"].as_slice(),
            };
            if allowed.contains(&key.as_str()) {
                p.values.push((key, value.trim().to_owned()));
            } else {
                p.problems
                    .push(format!("{key}: isn't used for a {}", kind_name(kind)));
            }
            continue;
        }
        if !t.literal {
            let first = t.text.chars().next();
            let rest = &t.text[first.map_or(0, char::len_utf8)..];
            match first {
                Some('@') | Some('#') if rest.trim().is_empty() => {
                    p.problems
                        .push(format!("Name missing after {}", first.unwrap_or('@')));
                    continue;
                }
                Some('@') => {
                    let slot = match kind {
                        QuickKind::Task => Some(Slot::Assignee),
                        QuickKind::Project => Some(Slot::Owner),
                        QuickKind::Wait => Some(Slot::WaitPerson),
                        QuickKind::Note => Some(Slot::Mention),
                        QuickKind::Decision => None,
                    };
                    match slot {
                        Some(s @ (Slot::Assignee | Slot::Owner | Slot::WaitPerson)) => {
                            if person_seen {
                                p.problems.push(format!(
                                    "Only one person can be the {}",
                                    request(s, &t.text).label.to_lowercase()
                                ));
                            } else {
                                person_seen = true;
                                p.requests.push(request(s, &t.text));
                            }
                        }
                        Some(s) => p.requests.push(request(s, &t.text)),
                        None => p.problems.push(
                            "A decision affects projects, tasks or objectives, not people".into(),
                        ),
                    }
                    continue;
                }
                Some('#') => {
                    let slot = match kind {
                        QuickKind::Task => Some(Slot::Project),
                        QuickKind::Wait => Some(Slot::About),
                        QuickKind::Note => Some(Slot::Mention),
                        QuickKind::Decision => Some(Slot::Affects),
                        QuickKind::Project => None,
                    };
                    match slot {
                        Some(s) => p.requests.push(request(s, &t.text)),
                        None => p.problems.push(
                            "A project's handle is made from its title; use for: to link an objective".into(),
                        ),
                    }
                    continue;
                }
                Some('!') if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) => {
                    match rest.parse::<u8>() {
                        Ok(n @ 1..=5) if matches!(kind, QuickKind::Task | QuickKind::Project) => {
                            p.priority = Some(n);
                        }
                        Ok(1..=5) => p
                            .problems
                            .push(format!("Priority isn't used for a {}", kind_name(kind))),
                        _ => p.problems.push("Priority is !1 (highest) to !5".into()),
                    }
                    continue;
                }
                _ => {}
            }
            if kind == QuickKind::Note && t.text == "1:1" {
                p.note_kind = Some(NoteKind::OneOnOne);
                continue;
            }
            // "wait @raj on ...": the linking word isn't part of the description.
            if kind == QuickKind::Wait && p.words.is_empty() && t.text.eq_ignore_ascii_case("on") {
                continue;
            }
        }
        p.words.push(t.text.clone());
    }
    p
}

fn date_value(
    p: &mut Parsed,
    keys: &[&str],
    label: &str,
    today: Date,
    details: &mut Vec<QuickDetail>,
) -> Option<Date> {
    let mut found = None;
    for (k, v) in &p.values.clone() {
        if keys.contains(&k.as_str()) {
            match parse_when(v, today) {
                Ok(d) => {
                    found = Some(d);
                }
                Err(e) => p.problems.push(e),
            }
        }
    }
    if let Some(d) = found {
        details.push(QuickDetail {
            label: label.into(),
            value: day_text(d),
        });
    }
    found
}

/// The `every:` value: how often it repeats ("Repeats: every Monday"). `anchor` is the date the
/// rule takes a weekday or day of the month from when the text doesn't give one.
fn every_value(
    p: &mut Parsed,
    anchor: Date,
    details: &mut Vec<QuickDetail>,
) -> Option<minimap_types::Recurrence> {
    let mut found = None;
    for (k, v) in &p.values.clone() {
        if k == "every" {
            match parse_every(v, anchor) {
                Ok(cadence) => {
                    details.push(QuickDetail {
                        label: "Repeats".into(),
                        value: cadence.describe(),
                    });
                    found = Some(minimap_types::Recurrence::from(cadence));
                }
                Err(e) => p.problems.push(e),
            }
        }
    }
    found
}

/// The `type:` value: which task type it is ("Type: Decision"). A name or id of a type that is in
/// the list and not archived; anything else is a problem that lists what is available.
fn type_value(
    p: &mut Parsed,
    types: &[TaskType],
    details: &mut Vec<QuickDetail>,
) -> Option<String> {
    let mut found = None;
    for (k, v) in &p.values.clone() {
        if k == "type" {
            match task_types::find_active(types, v) {
                Some(t) => {
                    details.push(QuickDetail {
                        label: "Type".into(),
                        value: t.name.clone(),
                    });
                    found = Some(t.id.clone());
                }
                None => {
                    let names: Vec<&str> = types
                        .iter()
                        .filter(|t| !t.archived)
                        .map(|t| t.name.as_str())
                        .collect();
                    p.problems.push(if names.is_empty() {
                        format!("type:{v} - there are no task types to choose from")
                    } else {
                        format!(
                            "type:{v} isn't a task type. Choose one of {}",
                            names.join(", ")
                        )
                    });
                }
            }
        }
    }
    found
}

/// Turns one line of quick-add into a preview and (when nothing is unclear) a plan.
pub fn plan(text: &str, cx: &Context) -> Outcome {
    let (mut tokens, unclosed) = lex(text);
    let mut kind = QuickKind::Task;
    if let Some(first) = tokens.first() {
        if !first.literal {
            if let Some(k) = kind_of(&first.text) {
                kind = k;
                tokens.remove(0);
            }
        }
    }
    let mut p = parse_tokens(kind, &tokens);
    if unclosed {
        p.problems.push("A quote isn't closed".into());
    }
    let mut details: Vec<QuickDetail> = Vec::new();
    let mut resolver = Resolver::new(cx);
    let detail = |label: &str, value: String| QuickDetail {
        label: label.into(),
        value,
    };

    let requests = std::mem::take(&mut p.requests);
    let mut title = p.words.join(" ");
    let mut main: Option<QuickMain> = None;
    let mut resolved: Vec<(Slot, Option<Ref>, String)> = Vec::new();
    let mut seen_keys: Vec<String> = Vec::new();
    for r in requests {
        let slot = r.slot;
        let query = r.query.clone();
        // The same reference typed twice is one reference.
        if seen_keys.contains(&r.key) {
            continue;
        }
        seen_keys.push(r.key.clone());
        if matches!(slot, Slot::Project | Slot::About)
            && resolved.iter().any(|(s, _, _)| *s == slot)
        {
            p.problems
                .push(format!("Only one {} can be given", r.label.to_lowercase()));
            continue;
        }
        let got = resolver.resolve(r);
        resolved.push((slot, got, query));
    }
    let of = |slot: Slot| -> Vec<Ref> {
        resolved
            .iter()
            .filter(|(s, r, _)| *s == slot && r.is_some())
            .filter_map(|(_, r, _)| *r)
            .collect()
    };
    let first_of = |slot: Slot| of(slot).into_iter().next();

    match kind {
        QuickKind::Task => {
            if title.is_empty() {
                p.problems.push("A task needs a title".into());
            }
            let mut due = date_value(&mut p, &["due", "by"], "Due", cx.today, &mut details);
            let start = date_value(&mut p, &["start"], "Start", cx.today, &mut details);
            // `every:` repeats it; without a due date the first one is on the rule's first date.
            let recurrence = every_value(&mut p, due.unwrap_or(cx.today), &mut details);
            if let (Some(rule), None) = (&recurrence, due) {
                let first = first_on_or_after(rule.cadence, cx.today);
                details.push(detail("Due", day_text(first)));
                due = Some(first);
            }
            let task_type = type_value(&mut p, cx.task_types, &mut details);
            let mut estimate = None;
            for (k, v) in &p.values.clone() {
                if k == "est" {
                    match parse_estimate(v, cx.hours_per_day) {
                        Ok(d) => {
                            estimate = Some(d);
                            details.push(detail("Estimate", format!("{d}d")));
                        }
                        Err(e) => p.problems.push(e.to_string()),
                    }
                }
            }
            if let Some(n) = p.priority {
                details.push(detail("Priority", n.to_string()));
            }
            if !resolved.iter().any(|(s, _, _)| *s == Slot::Project) {
                details.push(detail("Project", "Inbox (none)".into()));
            }
            main = Some(QuickMain::Task {
                title: title.clone(),
                priority: p.priority,
                start_date: start,
                due_date: due,
                estimate_days: estimate,
                project: first_of(Slot::Project),
                assignee: first_of(Slot::Assignee)
                    .map_or(QuickAssignee::Default, QuickAssignee::Person),
                blocks: of(Slot::Blocks),
                objectives: of(Slot::Objective),
                recurrence,
                task_type,
            });
        }
        QuickKind::Project => {
            if title.is_empty() {
                p.problems.push("A project needs a title".into());
            }
            let target = date_value(
                &mut p,
                &["target", "due", "by"],
                "Target",
                cx.today,
                &mut details,
            );
            let start = date_value(&mut p, &["start"], "Start", cx.today, &mut details);
            if let Some(n) = p.priority {
                details.push(detail("Priority", n.to_string()));
            }
            main = Some(QuickMain::Project {
                title: title.clone(),
                priority: p.priority,
                start_date: start,
                target_date: target,
                owner: first_of(Slot::Owner),
                objectives: of(Slot::Objective),
            });
        }
        QuickKind::Wait => {
            if title.is_empty() {
                p.problems.push("Say what you're waiting for".into());
            }
            let by = date_value(
                &mut p,
                &["by", "due"],
                "Expected by",
                cx.today,
                &mut details,
            );
            let wants_person = resolved.iter().any(|(s, _, _)| *s == Slot::WaitPerson);
            if !wants_person {
                p.problems
                    .push("Say who you're waiting on, e.g. @raj".into());
            }
            if let Some(person) = first_of(Slot::WaitPerson) {
                main = Some(QuickMain::Wait {
                    description: title.clone(),
                    person,
                    expected_by: by,
                    about: first_of(Slot::About),
                });
            }
        }
        QuickKind::Note => {
            let mut note_kind = p.note_kind.unwrap_or(NoteKind::General);
            for (k, v) in &p.values.clone() {
                if k == "kind" {
                    match v.to_lowercase().as_str() {
                        "1:1" | "one_on_one" | "1on1" => note_kind = NoteKind::OneOnOne,
                        "meeting" => note_kind = NoteKind::Meeting,
                        "general" => note_kind = NoteKind::General,
                        other => p.problems.push(format!(
                            "kind: \"{other}\" isn't a note kind (1:1, meeting, general)"
                        )),
                    }
                }
            }
            let mut date = date_value(&mut p, &["date"], "Date", cx.today, &mut details);
            let recurrence = every_value(&mut p, date.unwrap_or(cx.today), &mut details);
            if let (Some(rule), None) = (&recurrence, date) {
                let first = first_on_or_after(rule.cadence, cx.today);
                details.push(detail("Date", day_text(first)));
                date = Some(first);
            }
            let mentions: Vec<minimap_types::NoteMention> = resolved
                .iter()
                .filter(|(s, r, _)| *s == Slot::Mention && r.is_some())
                .filter_map(|(_, r, q)| {
                    let target = (*r)?;
                    Some(minimap_types::NoteMention {
                        label: label_of(&target, &resolver, cx, q),
                        target,
                    })
                })
                .collect();
            details.push(detail(
                "Kind",
                match note_kind {
                    NoteKind::OneOnOne => "1:1",
                    NoteKind::Meeting => "Meeting",
                    NoteKind::General => "General",
                }
                .into(),
            ));
            if title.is_empty() {
                match note_kind {
                    NoteKind::OneOnOne => {
                        let people: Vec<&str> = mentions
                            .iter()
                            .filter(|m| m.target_is_person(&resolver, cx))
                            .map(|m| m.label.as_str())
                            .collect();
                        if people.is_empty() {
                            p.problems
                                .push("Say who the 1:1 is with, e.g. @priya".into());
                        } else {
                            title = format!("1:1 with {}", people.join(" and "));
                        }
                    }
                    NoteKind::Meeting => title = "Meeting".into(),
                    NoteKind::General => p.problems.push("A note needs a title".into()),
                }
            }
            main = Some(QuickMain::Note {
                title: title.clone(),
                kind: note_kind,
                note_date: date,
                mentions,
                recurrence,
            });
        }
        QuickKind::Decision => {
            if title.is_empty() {
                p.problems.push("A decision needs a title".into());
            }
            let on = date_value(&mut p, &["date"], "Decided on", cx.today, &mut details);
            let mut status = None;
            for (k, v) in &p.values.clone() {
                if k == "status" {
                    match v.to_lowercase().as_str() {
                        "proposed" => status = Some(DecisionStatus::Proposed),
                        "decided" => status = Some(DecisionStatus::Decided),
                        other => p.problems.push(format!(
                            "status: \"{other}\" isn't one of proposed, decided"
                        )),
                    }
                }
            }
            if let Some(s) = status {
                details.push(detail("Status", s.as_str().to_owned()));
            }
            main = Some(QuickMain::Decision {
                title: title.clone(),
                status,
                decided_on: on,
                affects: of(Slot::Affects),
            });
        }
    }

    let pending = resolver
        .refs
        .iter()
        .any(|r| matches!(r.state, RefState::Pending { .. }));
    let ready = p.problems.is_empty() && !pending && main.is_some();
    let preview = QuickPreview {
        kind,
        title,
        details,
        refs: resolver.refs,
        problems: p.problems,
        ready,
    };
    let plan = main.filter(|_| ready).map(|main| QuickPlan {
        new_nodes: resolver.new_nodes,
        main,
    });
    Outcome { preview, plan }
}

/// The name a mention is written with: a matched node's label, or the name being created.
fn label_of(target: &Ref, resolver: &Resolver, cx: &Context, typed: &str) -> String {
    match target {
        Ref::Existing(n) => cx
            .directory
            .iter()
            .find(|e| e.node == *n)
            .map(|e| e.label.clone())
            .unwrap_or_else(|| typed.to_owned()),
        Ref::New(i) => resolver
            .new_nodes
            .get(*i as usize)
            .map(|n| n.name.clone())
            .unwrap_or_else(|| typed.to_owned()),
    }
}

trait MentionKind {
    fn target_is_person(&self, resolver: &Resolver, cx: &Context) -> bool;
}

impl MentionKind for minimap_types::NoteMention {
    fn target_is_person(&self, resolver: &Resolver, cx: &Context) -> bool {
        match self.target {
            Ref::Existing(NodeRef { node_type, .. }) => {
                let _ = cx;
                node_type == NodeType::Person
            }
            Ref::New(i) => resolver
                .new_nodes
                .get(i as usize)
                .is_some_and(|n| n.node_type == NodeType::Person),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{NoteMention, Uuid};
    use time::macros::date;

    // 2027-03-03 is a Wednesday.
    const TODAY: Date = date!(2027 - 03 - 03);

    fn id(n: u128) -> Uuid {
        Uuid::from_u128(n)
    }

    fn entry(
        node_type: NodeType,
        n: u128,
        label: &str,
        handle: Option<&str>,
        me: bool,
    ) -> DirectoryEntry {
        DirectoryEntry {
            node: NodeRef::new(node_type, id(n)),
            label: label.into(),
            handle: handle.map(str::to_owned),
            is_self: me,
        }
    }

    fn directory() -> Vec<DirectoryEntry> {
        use NodeType::*;
        vec![
            entry(Person, 1, "Uday Lamba", None, true),
            entry(Person, 2, "Priya Shah", None, false),
            entry(Person, 3, "Priyanka Rao", None, false),
            entry(Person, 4, "Raj Patel", None, false),
            entry(Project, 10, "API Launch", Some("api-launch"), false),
            entry(Project, 11, "Billing", Some("billing"), false),
            entry(Objective, 20, "Launch EU", None, false),
            entry(Task, 30, "Release 1.2", None, false),
        ]
    }

    fn run(text: &str) -> Outcome {
        run_with(text, &[])
    }

    /// The examples in the help pages (`docs/help/`) are lines a user will copy: each one has to
    /// be understood, whatever people and projects exist (names it can't find are asked about
    /// in the preview, which is not a problem with the line).
    #[test]
    fn the_examples_in_the_help_pages_are_understood() {
        let pages = [
            include_str!("../../../docs/help/welcome.md"),
            include_str!("../../../docs/help/quick-add.md"),
            include_str!("../../../docs/help/recurring.md"),
            include_str!("../../../docs/help/tasks.md"),
            include_str!("../../../docs/help/notes.md"),
            include_str!("../../../docs/help/waiting-on.md"),
        ];
        let mut checked = 0;
        for page in pages {
            let mut in_block = false;
            for line in page.lines() {
                if line.trim_start().starts_with("```") {
                    in_block = !in_block;
                    continue;
                }
                let first = line.split_whitespace().next().unwrap_or("");
                let is_example = in_block
                    && matches!(
                        first,
                        "task" | "project" | "wait" | "waiting" | "note" | "decision"
                    );
                if is_example {
                    let out = run(line.trim());
                    assert!(
                        out.preview.problems.is_empty(),
                        "help example {line:?}: {:?}",
                        out.preview.problems
                    );
                    checked += 1;
                }
            }
        }
        assert!(checked >= 10, "only {checked} examples were found");
    }

    #[test]
    fn every_makes_a_task_repeat_and_starts_it_on_the_rules_first_date() {
        use minimap_types::Cadence;
        // Today is a Wednesday: "every:mon" starts on the coming Monday.
        let out = run("task Board update every:mon");
        let QuickMain::Task {
            due_date,
            recurrence,
            ..
        } = out.plan.unwrap().main
        else {
            panic!("not a task")
        };
        assert_eq!(due_date, Some(date!(2027 - 03 - 08)));
        assert_eq!(
            recurrence.map(|r| r.cadence),
            Some(Cadence::Weekly {
                every: 1,
                weekday: 0
            })
        );
        let preview = run("task Board update every:mon").preview;
        let shown = |label: &str| {
            preview
                .details
                .iter()
                .find(|d| d.label == label)
                .map(|d| d.value.clone())
        };
        assert_eq!(shown("Repeats").as_deref(), Some("every Monday"));
        assert!(shown("Due").unwrap().contains("2027-03-08"));

        // A given due date is kept, and gives the weekday or day the rule needs.
        let every = |text: &str| -> Option<Cadence> {
            let QuickMain::Task { recurrence, .. } = run(text).plan.unwrap().main else {
                panic!()
            };
            recurrence.map(|r| r.cadence)
        };
        assert_eq!(
            every("task x due:2027-03-10 every:2w"),
            Some(Cadence::Weekly {
                every: 2,
                weekday: 2
            })
        );
        assert_eq!(
            every("task x due:2027-03-10 every:week"),
            Some(Cadence::Weekly {
                every: 1,
                weekday: 2
            })
        );
        assert_eq!(
            every("task x due:2027-03-15 every:month"),
            Some(Cadence::Monthly { day: 15 })
        );
        assert_eq!(every("task x every:day"), Some(Cadence::Daily));
        assert_eq!(
            every("task x every:month:1"),
            Some(Cadence::Monthly { day: 1 })
        );
        assert_eq!(every("task x"), None);
        let QuickMain::Task { due_date, .. } = run("task x due:fri every:mon").plan.unwrap().main
        else {
            panic!()
        };
        assert_eq!(due_date, Some(date!(2027 - 03 - 05)));
    }

    #[test]
    fn every_on_a_note_starts_it_on_the_rules_first_date() {
        use minimap_types::Cadence;
        let out = run("note 1:1 @priya every:wed");
        let QuickMain::Note {
            note_date,
            recurrence,
            ..
        } = out.plan.unwrap().main
        else {
            panic!("not a note")
        };
        assert_eq!(note_date, Some(date!(2027 - 03 - 03)));
        assert_eq!(
            recurrence.map(|r| r.cadence),
            Some(Cadence::Weekly {
                every: 1,
                weekday: 2
            })
        );
    }

    #[test]
    fn a_bad_every_says_what_is_accepted_and_other_kinds_do_not_take_it() {
        let problems = |t: &str| run(t).preview.problems;
        let p = problems("task x every:soon");
        assert!(p[0].contains("Repeats look like"), "{p:?}");
        assert!(problems("task x every:0w")[0].contains("from 1 to 52"));
        assert!(problems("project x every:mon")[0].contains("isn't used"));
        assert!(problems("decision x every:mon")[0].contains("isn't used"));
        assert!(problems("wait @raj on x every:mon")[0].contains("isn't used"));
        assert!(problems("task x every:")[0].contains("needs a value"));
    }

    #[test]
    fn type_names_a_task_type_by_id_or_name() {
        let task_type = |text: &str| -> Option<String> {
            let QuickMain::Task { task_type, .. } = run(text).plan.unwrap().main else {
                panic!()
            };
            task_type
        };
        assert_eq!(
            task_type("task Pick the store type:decision").as_deref(),
            Some("decision")
        );
        assert_eq!(
            task_type("task Pick the store type:Design").as_deref(),
            Some("design")
        );
        assert_eq!(task_type("task Pick the store").as_deref(), None);
        let out = run("task Pick the store type:decision");
        let shown = out
            .preview
            .details
            .iter()
            .find(|d| d.label == "Type")
            .map(|d| d.value.clone());
        assert_eq!(shown.as_deref(), Some("Decision"));
    }

    #[test]
    fn a_type_that_is_not_in_the_list_says_what_is() {
        let out = run("task x type:nonsense");
        assert!(!out.preview.ready);
        let problem = &out.preview.problems[0];
        assert!(problem.contains("isn't a task type"), "{problem}");
        assert!(
            problem.contains("Design") && problem.contains("Bug"),
            "{problem}"
        );
        assert!(run("project x type:design").preview.problems[0].contains("isn't used"));
        assert!(run("task x type:").preview.problems[0].contains("needs a value"));
        // An archived type is not offered.
        let dir = directory();
        let mut types = minimap_types::default_task_types();
        types[0].archived = true;
        let out = plan(
            "task x type:design",
            &Context {
                today: TODAY,
                hours_per_day: 8.0,
                directory: &dir,
                choices: &[],
                task_types: &types,
            },
        );
        assert!(!out.preview.ready);
        assert!(!out.preview.problems[0].contains("Design"));
    }

    fn run_with(text: &str, choices: &[QuickChoice]) -> Outcome {
        let dir = directory();
        let types = minimap_types::default_task_types();
        plan(
            text,
            &Context {
                today: TODAY,
                hours_per_day: 8.0,
                directory: &dir,
                choices,
                task_types: &types,
            },
        )
    }

    fn existing(node_type: NodeType, n: u128) -> Ref {
        Ref::Existing(NodeRef::new(node_type, id(n)))
    }

    fn pick(key: &str, n: u128) -> QuickChoice {
        QuickChoice {
            key: key.into(),
            pick: QuickPick::Node(id(n)),
        }
    }

    // ------------------------------------------------------------------ dates

    #[test]
    fn natural_dates() {
        let d = |t: &str| parse_when(t, TODAY).unwrap();
        assert_eq!(d("today"), TODAY);
        assert_eq!(d("Tomorrow"), date!(2027 - 03 - 04));
        assert_eq!(d("fri"), date!(2027 - 03 - 05));
        assert_eq!(d("friday"), date!(2027 - 03 - 05));
        assert_eq!(d("mon"), date!(2027 - 03 - 08));
        assert_eq!(d("+3d"), date!(2027 - 03 - 06));
        assert_eq!(d("+2w"), date!(2027 - 03 - 17));
        assert_eq!(d("+0d"), TODAY);
        assert_eq!(d("2027-03-31"), date!(2027 - 03 - 31));
        for bad in [
            "+3é",
            "",
            "nope",
            "+d",
            "+3é",
            "+3x",
            "2027-02-30",
            "next-",
            "next-xyz",
            "+99999999999999999999d",
        ] {
            assert!(parse_when(bad, TODAY).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_weekday_means_today_when_it_is_that_day() {
        // Wednesday: `wed` is today, `next-wed` is a week away (next calendar week's Wednesday).
        assert_eq!(parse_when("wed", TODAY).unwrap(), TODAY);
        assert_eq!(
            parse_when("next-wed", TODAY).unwrap(),
            date!(2027 - 03 - 10)
        );
        // `next-<day>` is that weekday in the next Monday-to-Sunday week, whatever today is.
        assert_eq!(
            parse_when("next-mon", TODAY).unwrap(),
            date!(2027 - 03 - 08)
        );
        assert_eq!(
            parse_when("next-sun", TODAY).unwrap(),
            date!(2027 - 03 - 14)
        );
        // On a Monday, `next-wed` is not this week's Wednesday.
        let monday = date!(2027 - 03 - 08);
        assert_eq!(parse_when("wed", monday).unwrap(), date!(2027 - 03 - 10));
        assert_eq!(
            parse_when("next-wed", monday).unwrap(),
            date!(2027 - 03 - 17)
        );
        // On a Sunday, next week starts tomorrow.
        let sunday = date!(2027 - 03 - 07);
        assert_eq!(
            parse_when("next-mon", sunday).unwrap(),
            date!(2027 - 03 - 08)
        );
        assert_eq!(parse_when("sun", sunday).unwrap(), sunday);
    }

    #[test]
    fn next_week_is_the_next_monday_from_any_day() {
        assert_eq!(
            parse_when("next-week", TODAY).unwrap(),
            date!(2027 - 03 - 08)
        );
        assert_eq!(
            parse_when("Next Week", TODAY).unwrap(),
            date!(2027 - 03 - 08)
        );
        // From a Monday it is a week away; from a Sunday, tomorrow.
        assert_eq!(
            parse_when("next-week", date!(2027 - 03 - 01)).unwrap(),
            date!(2027 - 03 - 08)
        );
        assert_eq!(
            parse_when("next-week", date!(2027 - 03 - 07)).unwrap(),
            date!(2027 - 03 - 08)
        );
    }

    #[test]
    fn date_errors_say_what_is_accepted() {
        let e = parse_when("someday", TODAY).unwrap_err();
        assert!(e.contains("someday") && e.contains("next-wed"), "{e}");
    }

    // ------------------------------------------------------------------ lexing

    #[test]
    fn lexing_respects_quotes() {
        let (t, open) = lex(r#"task "Fix the thing" @priya due:"next fri" "@not" end"#);
        assert!(!open);
        let texts: Vec<&str> = t.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "task",
                "Fix the thing",
                "@priya",
                "due:next fri",
                "@not",
                "end"
            ]
        );
        assert!(t[1].literal && t[4].literal && !t[2].literal);
        assert!(lex(r#"say "hi"#).1, "an open quote is reported");
        assert!(lex("   ").0.is_empty());
    }

    // ---------------------------------------------------- the spec's examples

    #[test]
    fn task_example() {
        let out = run(
            r#"task Fix login timeout @priya #api-launch !2 due:fri est:3d blocks:"Release 1.2""#,
        );
        assert!(out.preview.ready, "{:?}", out.preview);
        assert_eq!(out.preview.kind, QuickKind::Task);
        assert_eq!(out.preview.title, "Fix login timeout");
        assert_eq!(
            out.plan.unwrap(),
            QuickPlan {
                new_nodes: vec![],
                main: QuickMain::Task {
                    title: "Fix login timeout".into(),
                    priority: Some(2),
                    start_date: None,
                    due_date: Some(date!(2027 - 03 - 05)),
                    estimate_days: Some(3.0),
                    project: Some(existing(NodeType::Project, 10)),
                    assignee: QuickAssignee::Person(existing(NodeType::Person, 2)),
                    blocks: vec![existing(NodeType::Task, 30)],
                    objectives: vec![],
                    recurrence: None,
                    task_type: None,
                }
            }
        );
        let labels: Vec<&str> = out.preview.refs.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["Assignee", "Project", "Blocks"]);
    }

    #[test]
    fn project_example() {
        let out = run(r#"project Q1 EU region owner:@me target:2027-03-31 for:"Launch EU""#);
        assert!(out.preview.ready, "{:?}", out.preview);
        assert_eq!(
            out.plan.unwrap().main,
            QuickMain::Project {
                title: "Q1 EU region".into(),
                priority: None,
                start_date: None,
                target_date: Some(date!(2027 - 03 - 31)),
                owner: Some(existing(NodeType::Person, 1)),
                objectives: vec![existing(NodeType::Objective, 20)],
            }
        );
    }

    #[test]
    fn wait_example() {
        let out = run(r#"wait @raj on "Security review sign-off" by:next-wed"#);
        assert!(out.preview.ready, "{:?}", out.preview);
        assert_eq!(out.preview.kind, QuickKind::Wait);
        assert_eq!(
            out.plan.unwrap().main,
            QuickMain::Wait {
                description: "Security review sign-off".into(),
                person: existing(NodeType::Person, 4),
                expected_by: Some(date!(2027 - 03 - 10)),
                about: None,
            }
        );
        // The same without quotes and without "on".
        let plain = run("waiting @raj security review by:fri");
        assert!(plain.preview.ready);
        assert_eq!(plain.preview.title, "security review");
    }

    #[test]
    fn note_example() {
        let out = run("note 1:1 @priya");
        assert!(out.preview.ready, "{:?}", out.preview);
        assert_eq!(
            out.plan.unwrap().main,
            QuickMain::Note {
                title: "1:1 with Priya Shah".into(),
                kind: NoteKind::OneOnOne,
                note_date: None,
                mentions: vec![NoteMention {
                    label: "Priya Shah".into(),
                    target: existing(NodeType::Person, 2),
                }],
                recurrence: None,
            }
        );
        // A 1:1 needs a person; a general note needs a title; a meeting defaults its title.
        assert!(!run("note 1:1").preview.problems.is_empty());
        assert!(!run("note").preview.problems.is_empty());
        let meeting = run("note kind:meeting @raj");
        assert_eq!(meeting.preview.title, "Meeting");
        let titled = run("note Weekly sync @raj @priya kind:meeting date:fri");
        let QuickMain::Note {
            title,
            kind,
            note_date,
            mentions,
            ..
        } = titled.plan.unwrap().main
        else {
            panic!("not a note")
        };
        assert_eq!(title, "Weekly sync");
        assert_eq!(kind, NoteKind::Meeting);
        assert_eq!(note_date, Some(date!(2027 - 03 - 05)));
        assert_eq!(mentions.len(), 2);
    }

    #[test]
    fn decision_example() {
        let out = run(r#"decision "Postgres over Mongo" affects:#api-launch"#);
        assert!(out.preview.ready, "{:?}", out.preview);
        assert_eq!(
            out.plan.unwrap().main,
            QuickMain::Decision {
                title: "Postgres over Mongo".into(),
                status: None,
                decided_on: None,
                affects: vec![existing(NodeType::Project, 10)],
            }
        );
        let decided = run("decision Use Postgres status:decided date:today affects:billing");
        let QuickMain::Decision {
            status,
            decided_on,
            affects,
            ..
        } = decided.plan.unwrap().main
        else {
            panic!("not a decision")
        };
        assert_eq!(status, Some(DecisionStatus::Decided));
        assert_eq!(decided_on, Some(TODAY));
        assert_eq!(affects, vec![existing(NodeType::Project, 11)]);
    }

    // ------------------------------------------------------ keyword and basics

    #[test]
    fn no_keyword_means_a_task() {
        let out = run("Fix login timeout !1");
        assert_eq!(out.preview.kind, QuickKind::Task);
        assert_eq!(out.preview.title, "Fix login timeout");
        assert!(out.preview.ready);
        // A task whose first word is a keyword says so with the keyword.
        let t = run("task note taking policy");
        assert_eq!(
            (t.preview.kind, t.preview.title.as_str()),
            (QuickKind::Task, "note taking policy")
        );
        // Quoting makes it a plain word.
        let q = run(r#""note" to self"#);
        assert_eq!(q.preview.kind, QuickKind::Task);
        // Keywords are case-insensitive.
        assert_eq!(run("TASK x").preview.kind, QuickKind::Task);
        assert_eq!(run("Project x").preview.kind, QuickKind::Project);
    }

    #[test]
    fn a_plain_task_defaults_to_me_and_the_inbox() {
        let out = run("task Write the plan");
        let QuickMain::Task {
            assignee, project, ..
        } = out.plan.unwrap().main
        else {
            panic!("not a task")
        };
        assert_eq!(assignee, QuickAssignee::Default);
        assert_eq!(project, None);
        assert!(out
            .preview
            .details
            .iter()
            .any(|d| d.label == "Project" && d.value.contains("Inbox")));
        assert!(out.preview.refs.is_empty());
    }

    #[test]
    fn empty_and_incomplete_lines_have_problems_not_plans() {
        for text in [
            "",
            "   ",
            "task",
            "project",
            "decision",
            "wait",
            "wait @raj",
            "wait Review sign-off",
        ] {
            let out = run(text);
            assert!(!out.preview.ready, "{text}");
            assert!(out.plan.is_none(), "{text}");
            assert!(
                !out.preview.problems.is_empty(),
                "{text}: {:?}",
                out.preview
            );
        }
    }

    #[test]
    fn mistakes_are_reported_in_plain_words() {
        let problems = |t: &str| run(t).preview.problems;
        assert!(problems("task x due:someday")[0].contains("someday"));
        assert!(problems("task x est:lots")[0].contains("3d"));
        assert!(problems("task x !9")[0].contains("!1"));
        assert!(problems("task x due:")[0].contains("needs a value"));
        assert!(problems("task x @")[0].contains("Name missing"));
        assert!(problems(r#"task "unclosed"#)
            .iter()
            .any(|p| p.contains("quote")));
        assert!(problems("project x est:3d")[0].contains("isn't used"));
        assert!(problems("note x !2")[0].contains("Priority"));
        assert!(problems("task x @raj @priya")[0].contains("Only one"));
        assert!(problems("task x #billing #api-launch")[0].contains("Only one project"));
        assert!(problems("decision x @raj")[0].contains("not people"));
        assert!(problems("decision x status:maybe")[0].contains("maybe"));
        assert!(problems("project x #billing")[0].contains("handle"));
        // Several mistakes are all reported.
        assert_eq!(problems("task x due:x est:y").len(), 2);
    }

    #[test]
    fn unknown_keys_and_odd_tokens_stay_plain_words() {
        let out = run("task Meet at 10:30 re: budget !wow");
        assert!(out.preview.ready, "{:?}", out.preview);
        assert_eq!(out.preview.title, "Meet at 10:30 re: budget !wow");
    }

    // -------------------------------------------------------------- references

    #[test]
    fn ambiguous_people_ask_to_pick() {
        let out = run("task Review @pri");
        assert!(!out.preview.ready && out.plan.is_none());
        let r = &out.preview.refs[0];
        assert_eq!(r.key, "Assignee:pri");
        let RefState::Pending {
            options,
            guess,
            can_create,
            skip,
        } = &r.state
        else {
            panic!("expected pending: {:?}", r.state)
        };
        let labels: Vec<&str> = options.iter().map(|o| o.label.as_str()).collect();
        assert_eq!(labels, ["Priya Shah", "Priyanka Rao"]);
        assert!(!guess && *can_create);
        assert_eq!(skip.as_deref(), Some("assigned to you instead"));

        // Picking one resolves it and the plan appears.
        let done = run_with("task Review @pri", &[pick("Assignee:pri", 3)]);
        assert!(done.preview.ready, "{:?}", done.preview);
        let QuickMain::Task { assignee, .. } = done.plan.unwrap().main else {
            panic!()
        };
        assert_eq!(
            assignee,
            QuickAssignee::Person(existing(NodeType::Person, 3))
        );
        assert!(matches!(
            done.preview.refs[0].state,
            RefState::Resolved { .. }
        ));
    }

    #[test]
    fn a_whole_first_name_beats_longer_names() {
        // "priya" is a whole word of "Priya Shah" and only a prefix of "Priyanka Rao".
        let out = run("task x @priya");
        assert!(out.preview.ready);
        assert!(
            matches!(&out.preview.refs[0].state, RefState::Resolved { node } if node.label == "Priya Shah")
        );
        // Exact full names and handles win; matching ignores case.
        assert!(run("task x @PRIYA SHAH").preview.ready);
        assert!(run("task x #API-LAUNCH").preview.ready);
        assert!(run(r#"task x @"Priya Shah""#).preview.ready);
    }

    #[test]
    fn me_is_the_self_person() {
        let out = run("task x @me");
        let QuickMain::Task { assignee, .. } = out.plan.unwrap().main else {
            panic!()
        };
        assert_eq!(
            assignee,
            QuickAssignee::Person(existing(NodeType::Person, 1))
        );
        // With no self person there is nothing to match.
        let none: Vec<DirectoryEntry> = vec![entry(NodeType::Person, 2, "Priya Shah", None, false)];
        let out = plan(
            "task x @me",
            &Context {
                today: TODAY,
                hours_per_day: 8.0,
                directory: &none,
                choices: &[],
                task_types: &[],
            },
        );
        assert!(!out.preview.ready);
    }

    #[test]
    fn misspellings_are_offered_as_guesses_never_taken_silently() {
        let out = run("task x @priyaa");
        let RefState::Pending { options, guess, .. } = &out.preview.refs[0].state else {
            panic!("{:?}", out.preview.refs)
        };
        assert!(*guess);
        assert!(options.iter().any(|o| o.label == "Priya Shah"));
        assert!(!out.preview.ready);
        // Short names aren't guessed at.
        let RefState::Pending { options, .. } = &run("task x @raz").preview.refs[0].state else {
            panic!()
        };
        assert!(options.is_empty());
    }

    #[test]
    fn unresolved_names_can_be_created_or_skipped() {
        let text = "task Ship it @sam #moonshot";
        let out = run(text);
        assert!(!out.preview.ready);
        assert_eq!(out.preview.refs.len(), 2);
        for r in &out.preview.refs {
            let RefState::Pending {
                options,
                can_create,
                skip,
                ..
            } = &r.state
            else {
                panic!()
            };
            assert!(options.is_empty() && *can_create && skip.is_some(), "{r:?}");
        }
        let create = |key: &str| QuickChoice {
            key: key.into(),
            pick: QuickPick::Create,
        };
        let skip = |key: &str| QuickChoice {
            key: key.into(),
            pick: QuickPick::Skip,
        };

        let made = run_with(text, &[create("Assignee:sam"), create("Project:moonshot")]);
        assert!(made.preview.ready, "{:?}", made.preview);
        let plan = made.plan.unwrap();
        assert_eq!(
            plan.new_nodes,
            vec![
                minimap_types::NewNode {
                    node_type: NodeType::Person,
                    name: "Sam".into()
                },
                minimap_types::NewNode {
                    node_type: NodeType::Project,
                    name: "moonshot".into()
                },
            ]
        );
        let QuickMain::Task {
            assignee, project, ..
        } = plan.main
        else {
            panic!()
        };
        assert_eq!(assignee, QuickAssignee::Person(Ref::New(0)));
        assert_eq!(project, Some(Ref::New(1)));

        let skipped = run_with(text, &[skip("Assignee:sam"), skip("Project:moonshot")]);
        assert!(skipped.preview.ready);
        let plan = skipped.plan.unwrap();
        assert!(plan.new_nodes.is_empty());
        let QuickMain::Task {
            assignee, project, ..
        } = plan.main
        else {
            panic!()
        };
        assert_eq!((assignee, project), (QuickAssignee::Default, None));
    }

    #[test]
    fn required_references_cannot_be_skipped() {
        let out = run("wait @nobody Review");
        let RefState::Pending {
            skip, can_create, ..
        } = &out.preview.refs[0].state
        else {
            panic!()
        };
        assert!(skip.is_none() && *can_create);
        // A skip choice for it is ignored.
        let ignored = run_with(
            "wait @nobody Review",
            &[QuickChoice {
                key: "Waiting on:nobody".into(),
                pick: QuickPick::Skip,
            }],
        );
        assert!(!ignored.preview.ready);
        // Creating works, and the new person is who is waited on.
        let made = run_with(
            "wait @nobody Review",
            &[QuickChoice {
                key: "Waiting on:nobody".into(),
                pick: QuickPick::Create,
            }],
        );
        let QuickMain::Wait { person, .. } = made.plan.unwrap().main else {
            panic!()
        };
        assert_eq!(person, Ref::New(0));
    }

    #[test]
    fn choices_must_fit_the_reference() {
        // A project can't be picked as an assignee, and stale keys are ignored.
        let out = run_with(
            "task x @pri",
            &[pick("Assignee:pri", 10), pick("Nothing:here", 2)],
        );
        assert!(!out.preview.ready);
    }

    #[test]
    fn affects_and_about_may_point_at_several_kinds_of_node() {
        let out = run(
            "decision Cut scope affects:\"Release 1.2\" affects:#billing affects:\"Launch EU\"",
        );
        assert!(out.preview.ready, "{:?}", out.preview);
        let QuickMain::Decision { affects, .. } = out.plan.unwrap().main else {
            panic!()
        };
        assert_eq!(
            affects,
            vec![
                existing(NodeType::Task, 30),
                existing(NodeType::Project, 11),
                existing(NodeType::Objective, 20)
            ]
        );
        // Unclear kind: no creation offered; a `#` implies a project, so it is.
        let RefState::Pending { can_create, .. } =
            &run("decision x affects:zzz").preview.refs[0].state
        else {
            panic!()
        };
        assert!(!can_create);
        let RefState::Pending { can_create, .. } =
            &run("decision x affects:#zzz").preview.refs[0].state
        else {
            panic!()
        };
        assert!(can_create);
        // A wait about a project, with a `#`.
        let w = run("wait @raj Sign-off #api-launch");
        let QuickMain::Wait { about, .. } = w.plan.unwrap().main else {
            panic!()
        };
        assert_eq!(about, Some(existing(NodeType::Project, 10)));
    }

    #[test]
    fn the_same_reference_twice_is_one_reference() {
        let out = run("note Sync @raj @raj #billing");
        assert!(out.preview.ready);
        assert_eq!(out.preview.refs.len(), 2);
        let QuickMain::Note { mentions, .. } = out.plan.unwrap().main else {
            panic!()
        };
        assert_eq!(mentions.len(), 2, "one mention per distinct node");
    }

    #[test]
    fn a_name_created_twice_is_made_once() {
        let create = |k: &str| QuickChoice {
            key: k.into(),
            pick: QuickPick::Create,
        };
        let out = run_with("note Sync @sam @Sam", &[create("Mentions:sam")]);
        assert!(out.preview.ready, "{:?}", out.preview);
        assert_eq!(out.plan.unwrap().new_nodes.len(), 1);
    }

    #[test]
    fn estimates_use_hours_per_day() {
        let dir = directory();
        let out = plan(
            "task x est:4h",
            &Context {
                today: TODAY,
                hours_per_day: 4.0,
                directory: &dir,
                choices: &[],
                task_types: &[],
            },
        );
        let QuickMain::Task { estimate_days, .. } = out.plan.unwrap().main else {
            panic!()
        };
        assert_eq!(estimate_days, Some(1.0));
    }

    #[test]
    fn previews_describe_dates_with_the_weekday() {
        let out = run("task x due:fri start:today");
        let value = |l: &str| {
            out.preview
                .details
                .iter()
                .find(|d| d.label == l)
                .map(|d| d.value.clone())
        };
        assert_eq!(value("Due").as_deref(), Some("Fri 2027-03-05"));
        assert_eq!(value("Start").as_deref(), Some("Wed 2027-03-03"));
    }

    use proptest::prelude::*;

    proptest! {
        /// Whatever is typed, parsing never panics and a plan exists only for a ready preview.
        #[test]
        fn parsing_is_total_and_plans_match_readiness(text in "\\PC{0,80}") {
            let out = run(&text);
            prop_assert_eq!(out.plan.is_some(), out.preview.ready);
            if out.preview.ready {
                prop_assert!(out.preview.problems.is_empty());
                prop_assert!(!out.preview.title.trim().is_empty());
            }
        }

        #[test]
        fn dates_never_panic(text in "\\PC{0,20}") {
            let _ = parse_when(&text, TODAY);
        }
    }
}
