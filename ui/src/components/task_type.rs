//! Task types (spec 32) in the interface: the shared list, the chip that shows a task's type, the
//! options of a type dropdown, and the "planned against actual" lines of a task.

use leptos::prelude::*;
use minimap_types::{
    finish_timing, plan_history,
    timefmt::{fmt_ts, parse_date},
    Date, PlanHistory, Task, TaskStatus, TaskType, TypeBorder, Uuid,
};

use crate::{api, components::page::Tone, state::DataVersion};

/// The user's list of task types, provided once by the shell and reloaded when data changes.
#[derive(Clone, Copy)]
pub struct TaskTypes {
    list: Memo<Vec<TaskType>>,
    /// Frame board cards by their type (spec 39); a setting of this device.
    borders: Memo<bool>,
}

impl TaskTypes {
    pub fn provide() -> Self {
        let version = expect_context::<DataVersion>();
        let settings = LocalResource::new(move || {
            version.track();
            api::get_settings()
        });
        // Keep the last list while reloading, so chips never blink out after an edit.
        let list = Memo::new(move |last: Option<&Vec<TaskType>>| match settings.get() {
            Some(Ok(s)) => s.task_types,
            _ => last.cloned().unwrap_or_default(),
        });
        let borders = Memo::new(move |last: Option<&bool>| match settings.get() {
            Some(Ok(s)) => s.type_borders,
            _ => last.copied().unwrap_or(true),
        });
        let types = Self { list, borders };
        provide_context(types);
        types
    }

    /// Whether board cards are framed by their type (tracks).
    pub fn borders(&self) -> bool {
        self.borders.get()
    }

    /// Every type, archived ones included (tracks).
    pub fn all(&self) -> Vec<TaskType> {
        self.list.get()
    }

    /// The type with this id (tracks).
    pub fn find(&self, id: &str) -> Option<TaskType> {
        self.list.with(|list| TaskType::find(list, id).cloned())
    }

    /// The options for a task that has no type yet: no type and every type not archived (tracks).
    pub fn all_active_options(&self) -> Vec<(String, String)> {
        self.list.with(|list| active_options(list))
    }

    /// The dropdown options for a task whose type is `current` (tracks).
    pub fn options(&self, current: Option<&str>) -> Vec<(String, String)> {
        self.list.with(|list| type_options(list, current))
    }
}

pub fn use_task_types() -> TaskTypes {
    expect_context::<TaskTypes>()
}

/// What a type dropdown offers: no type, every type that is not archived, and the task's own type
/// when it has been archived since (marked, so it can stay).
pub fn type_options(list: &[TaskType], current: Option<&str>) -> Vec<(String, String)> {
    std::iter::once((String::new(), "No type".to_owned()))
        .chain(list.iter().filter_map(|t| {
            if !t.archived {
                Some((t.id.clone(), t.name.clone()))
            } else if Some(t.id.as_str()) == current {
                Some((t.id.clone(), format!("{} (archived)", t.name)))
            } else {
                None
            }
        }))
        .collect()
}

/// Just the options that can be chosen for a new task (no archived types, and not Meeting: a
/// meeting needs a day and a time, so it is made with *New meeting*, spec 38).
pub fn active_options(list: &[TaskType]) -> Vec<(String, String)> {
    type_options(list, None)
        .into_iter()
        .filter(|(id, _)| id != minimap_types::MEETING_TYPE)
        .collect()
}

/// The type a task made while the type filter says `filter` gets: the filtered type, except that
/// a meeting can't be made without a time.
pub fn type_for_new_task(filter: &str) -> Option<String> {
    Some(filter)
        .filter(|t| !t.is_empty() && *t != minimap_types::MEETING_TYPE)
        .map(str::to_owned)
}

/// The inline style that draws a board card's border for its type: the line style and width of
/// `TypeBorder::of` (see `article.card-frame` in `input.css`). Nothing when the setting is off or
/// the task has no type in the list: the card then has a plain line.
pub fn frame_style(kind: Option<&TaskType>, on: bool) -> Option<String> {
    let kind = kind.filter(|_| on)?;
    let border = TypeBorder::of(&kind.id);
    Some(format!(
        "--tb-style: {}; --tb-width: {}px;",
        border.css(),
        border.width_px()
    ))
}

/// The colour of a card's border: the objective's hue (what the card's left edge used to be), if
/// its project serves one.
pub fn frame_colour(hue: Option<u16>) -> Option<String> {
    hue.map(|h| format!("--frame: hsl({h} var(--obj-s) var(--obj-l));"))
}

/// A task's type as a small coloured chip (nothing for a task without one, or one that is not in
/// the list).
#[component]
pub fn TypeChip(id: Option<String>) -> impl IntoView {
    let types = use_task_types();
    move || {
        let t = id.as_deref().and_then(|id| types.find(id))?;
        let title = format!(
            "Type: {}{}",
            t.name,
            if t.archived { " (archived)" } else { "" }
        );
        Some(view! {
            <span class=format!("obj-chip {}", if t.archived { "opacity-60" } else { "" })
                  style=format!("--obj-h: {}", t.hue) title=title>
                <span>{t.name}</span>
            </span>
        })
    }
}

// ------------------------------------------------------- planned against actual

/// How a finished task did against its due date: the words and the tone they wear.
#[derive(Debug, Clone, PartialEq)]
pub struct FinishNote {
    pub text: String,
    pub tone: Tone,
}

/// The date a task was finished (UTC day).
fn finished_on(task: &Task) -> Option<Date> {
    let at = task.completed_at?;
    parse_date(fmt_ts(at).get(..10)?).ok()
}

/// "Done 2027-03-03, 2 days late" for a finished task (just the date when it had no due date).
pub fn finish_note(task: &Task) -> Option<FinishNote> {
    if task.status != TaskStatus::Done {
        return None;
    }
    let done = finished_on(task)?;
    let Some(due) = task.due_date else {
        return Some(FinishNote {
            text: format!("Done {done}"),
            tone: Tone::Neutral,
        });
    };
    let timing = finish_timing(due, done);
    let tone = match timing {
        minimap_types::Finish::Late(_) => Tone::Danger,
        _ => Tone::Success,
    };
    Some(FinishNote {
        text: format!("Done {done}, {}", timing.text()),
        tone,
    })
}

/// The short badge on a finished card: "2d late", "on time", "1d early".
pub fn finish_badge(task: &Task) -> Option<(String, Tone)> {
    let (due, done) = (task.due_date?, finished_on(task)?);
    let note = finish_note(task)?;
    let text = match finish_timing(due, done) {
        minimap_types::Finish::OnTime => "on time".to_owned(),
        minimap_types::Finish::Early(d) => format!("{d}d early"),
        minimap_types::Finish::Late(d) => format!("{d}d late"),
    };
    Some((text, note.tone))
}

/// The lines under a task's dates: how it finished against its due date and, when the date has
/// moved, what it was first and how a finished task did against that.
pub fn timing_lines(task: &Task, history: &PlanHistory) -> Vec<FinishNote> {
    let mut lines = Vec::new();
    if let Some(note) = finish_note(task) {
        lines.push(note);
    }
    if let Some(text) = history.moved_text(|d| d.to_string()) {
        let against = finished_on(task)
            .filter(|_| task.status == TaskStatus::Done)
            .and_then(|done| history.first_timing(done));
        lines.push(FinishNote {
            text: match against {
                Some(f) => format!("{text}. Done {} against that date.", f.text()),
                None => text,
            },
            tone: Tone::Neutral,
        });
    }
    lines
}

/// Under the date fields of a task panel.
#[component]
pub fn TaskTiming(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let task = LocalResource::new(move || {
        version.track();
        api::get_task(id)
    });
    let activity = LocalResource::new(move || {
        version.track();
        api::list_activity_for(id)
    });
    move || {
        let (Some(Ok(task)), Some(Ok(rows))) = (task.get(), activity.get()) else {
            return None;
        };
        let lines = timing_lines(&task, &plan_history(&rows));
        (!lines.is_empty()).then(|| {
            view! {
                <div class="mt-1 space-y-0.5 text-[11px]">
                    {lines.into_iter().map(|l| view! {
                        <p class=if l.tone == Tone::Neutral { "text-muted" } else { l.tone.text() }>{l.text}</p>
                    }).collect_view()}
                </div>
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::{default_task_types, timefmt::parse_ts};

    fn task(due: Option<&str>, done: Option<&str>) -> Task {
        Task {
            id: Uuid::nil(),
            title: "Pick the store".into(),
            description: String::new(),
            project_id: None,
            status: if done.is_some() {
                TaskStatus::Done
            } else {
                TaskStatus::Todo
            },
            estimate_days: None,
            start_date: None,
            due_date: due.map(|d| parse_date(d).unwrap()),
            completed_at: done.map(|d| parse_ts(&format!("{d}T10:00:00.000Z")).unwrap()),
            priority: 3,
            recurrence: None,
            links: Vec::new(),
            task_type: Some("decision".into()),
            focus: None,
            start_minute: None,
            length_minutes: None,
            created_at: parse_ts("2027-01-01T00:00:00.000Z").unwrap(),
            updated_at: parse_ts("2027-01-01T00:00:00.000Z").unwrap(),
            archived_at: None,
        }
    }

    #[test]
    fn options_offer_active_types_and_keep_an_archived_one_on_its_own_task() {
        let mut list = default_task_types();
        list[1].archived = true; // Build
        let none = active_options(&list);
        assert_eq!(none[0], (String::new(), "No type".to_owned()));
        assert_eq!(none.len(), 7); // No type + 6 active, and not Meeting
        assert!(none.iter().all(|(id, _)| id != "build"));
        let own = type_options(&list, Some("build"));
        assert!(own.contains(&("build".to_owned(), "Build (archived)".to_owned())));
        assert_eq!(own.len(), 9);
        // A new task made under the Meeting filter is a plain task.
        assert_eq!(type_for_new_task(""), None);
        assert_eq!(type_for_new_task("meeting"), None);
        assert_eq!(type_for_new_task("bug").as_deref(), Some("bug"));
        assert!(none.iter().all(|(id, _)| id != "meeting"));
    }

    #[test]
    fn a_cards_border_is_the_types_line_in_the_objectives_colour() {
        let types = default_task_types();
        let find = |id: &str| TaskType::find(&types, id).cloned();
        let decision = find("decision").unwrap();
        assert_eq!(
            frame_style(Some(&decision), true).as_deref(),
            Some("--tb-style: double; --tb-width: 4px;")
        );
        let design = find("design").unwrap();
        let style = frame_style(Some(&design), true).unwrap();
        assert!(style.contains("--tb-style: dashed") && style.contains("--tb-width: 3px"));
        assert!(frame_style(find("review").as_ref(), true)
            .unwrap()
            .contains("--tb-width: 3px"));
        // Build and a type of your own are a plain 2px line.
        let own = TaskType {
            id: "legal-review".into(),
            name: "Legal review".into(),
            hue: 285,
            archived: false,
        };
        for kind in [find("build").unwrap(), own] {
            let style = frame_style(Some(&kind), true).unwrap();
            assert!(style.contains("--tb-style: solid") && style.contains("--tb-width: 2px"));
        }
        // Off, or no type: the card's default (a plain line, set in the CSS).
        assert_eq!(frame_style(Some(&decision), false), None);
        assert_eq!(frame_style(None, true), None);
        // The colour is the objective's, whatever the type; none without an objective.
        assert_eq!(
            frame_colour(Some(215)).as_deref(),
            Some("--frame: hsl(215 var(--obj-s) var(--obj-l));")
        );
        assert_eq!(frame_colour(None), None);
    }

    #[test]
    fn a_finished_task_says_how_it_did_against_its_due_date() {
        let late = finish_note(&task(Some("2027-03-01"), Some("2027-03-03"))).unwrap();
        assert_eq!(late.text, "Done 2027-03-03, 2 days late");
        assert_eq!(late.tone, Tone::Danger);
        let ok = finish_note(&task(Some("2027-03-03"), Some("2027-03-03"))).unwrap();
        assert_eq!(
            (ok.text.as_str(), ok.tone),
            ("Done 2027-03-03, on time", Tone::Success)
        );
        assert_eq!(
            finish_badge(&task(Some("2027-03-05"), Some("2027-03-04"))),
            Some(("1d early".to_owned(), Tone::Success))
        );
        // No due date: only the day. Not finished: nothing to say.
        let undated = finish_note(&task(None, Some("2027-03-03"))).unwrap();
        assert_eq!(undated.text, "Done 2027-03-03");
        assert!(finish_badge(&task(None, Some("2027-03-03"))).is_none());
        assert!(finish_note(&task(Some("2027-03-01"), None)).is_none());
        assert!(finish_badge(&task(Some("2027-03-01"), None)).is_none());
    }

    #[test]
    fn a_moved_date_adds_the_first_one_and_how_it_finished_against_it() {
        let history = PlanHistory {
            first: Some(parse_date("2027-02-22").unwrap()),
            moves: 2,
        };
        let lines = timing_lines(&task(Some("2027-03-01"), Some("2027-03-01")), &history);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].text, "Done 2027-03-01, on time");
        assert_eq!(
            lines[1].text,
            "Originally planned for 2027-02-22, moved twice. Done 7 days late against that date."
        );
        // Still open: only the plan history.
        let open = timing_lines(&task(Some("2027-03-01"), None), &history);
        assert_eq!(open.len(), 1);
        assert_eq!(
            open[0].text,
            "Originally planned for 2027-02-22, moved twice"
        );
        assert!(timing_lines(&task(Some("2027-03-01"), None), &PlanHistory::default()).is_empty());
    }
}
