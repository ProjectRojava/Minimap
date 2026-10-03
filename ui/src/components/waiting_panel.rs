//! Detail-pane body for a "waiting on": status and quick actions, editable fields, archive.

use leptos::{prelude::*, task::spawn_local};
use minimap_types::{
    timefmt::parse_date, AppError, PersonRow, UpdateWaitingOn, Uuid, WaitingOnRow,
};

use crate::{
    api,
    components::{
        detail_pane::Section,
        form::{date_patch, SelectField, TextField, BUTTON, BUTTON_DANGER, BUTTON_PRIMARY},
        people_panel::error_line,
    },
    state::{finish, DataVersion, Selection, Toasts},
};

pub const SNOOZE_CHOICES: [(&str, &str); 5] = [
    ("", "Snooze…"),
    ("1", "1 day"),
    ("3", "3 days"),
    ("7", "1 week"),
    ("14", "2 weeks"),
];

pub fn snooze_options() -> Vec<(String, String)> {
    SNOOZE_CHOICES
        .iter()
        .map(|(v, l)| (v.to_string(), l.to_string()))
        .collect()
}

/// "12 days", "1 day", "today".
pub fn age_text(days: i64) -> String {
    match days {
        0 => "today".to_owned(),
        1 => "1 day".to_owned(),
        n => format!("{n} days"),
    }
}

#[component]
pub fn WaitingPanel(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    // Fields load once per opening so typing is never overwritten; status follows every write.
    let fields = LocalResource::new(move || api::get_waiting_on_detail(id));
    let status = LocalResource::new(move || {
        version.track();
        api::get_waiting_on_detail(id)
    });
    let people = LocalResource::new(api::list_people);

    view! {
        {move || match status.get() {
            Some(Ok(row)) => view! { <StatusSection row=row /> }.into_any(),
            Some(Err(e)) => view! { <Section title="Status">{error_line(e)}</Section> }.into_any(),
            None => view! { <Section title="Status"><p class="text-muted">"Loading…"</p></Section> }.into_any(),
        }}
        <Section title="Fields">
            {move || match (fields.get(), people.get()) {
                (Some(Ok(r)), Some(Ok(ps))) => view! { <WaitingFields row=r people=ps /> }.into_any(),
                (Some(Err(e)), _) | (_, Some(Err(e))) => error_line(e),
                _ => view! { <p class="text-muted">"Loading…"</p> }.into_any(),
            }}
        </Section>
        <ArchiveWaiting id=id />
    }
}

#[component]
fn StatusSection(row: WaitingOnRow) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let id = row.waiting.id;
    let resolved = row.waiting.resolved_on.is_some();

    let resolve = move |_| {
        spawn_local(async move {
            finish(api::resolve_waiting_on(id).await, toasts, version);
        });
    };
    let reopen = move |_| {
        spawn_local(async move {
            finish(api::reopen_waiting_on(id).await, toasts, version);
        });
    };
    let wake = move |_| {
        spawn_local(async move {
            finish(api::snooze_waiting_on(id, None).await, toasts, version);
        });
    };
    let snooze = move |v: String| {
        if let Ok(days) = v.parse::<u32>() {
            spawn_local(async move {
                finish(
                    api::snooze_waiting_on(id, Some(days)).await,
                    toasts,
                    version,
                );
            });
        }
    };

    let summary = if let Some(d) = row.waiting.resolved_on {
        format!("Resolved {d}")
    } else if let Some(d) = row.waiting.follow_up_on.filter(|_| row.snoozed) {
        format!("Snoozed until {d} · waiting {}", age_text(row.age_days))
    } else if row.stale {
        format!("Stale · waiting {}", age_text(row.age_days))
    } else {
        format!("Waiting {}", age_text(row.age_days))
    };

    view! {
        <Section title="Status">
            <p class=if row.stale { "mb-2 font-medium" } else { "mb-2 text-muted" }>{summary}</p>
            <div class="flex flex-wrap items-center gap-2">
                {if resolved {
                    view! { <button class=BUTTON on:click=reopen>"Reopen"</button> }.into_any()
                } else {
                    view! {
                        <button class=BUTTON_PRIMARY on:click=resolve>"Resolve"</button>
                        <SelectField compact=true options=snooze_options() current=String::new() on_change=snooze />
                        {row.snoozed.then(|| view! { <button class=BUTTON on:click=wake>"End snooze"</button> })}
                    }.into_any()
                }}
            </div>
        </Section>
    }
}

#[component]
fn WaitingFields(row: WaitingOnRow, people: Vec<PersonRow>) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let w = row.waiting;
    let id = w.id;
    let save = move |patch: UpdateWaitingOn| {
        spawn_local(async move {
            finish(api::update_waiting_on(id, patch).await, toasts, version);
        });
    };
    let bad_date = move |e: AppError| {
        toasts.error(&e);
        version.bump();
    };
    let save_expected = move |v: String| match date_patch(&v) {
        Ok(p) => save(UpdateWaitingOn {
            expected_by: p,
            ..Default::default()
        }),
        Err(e) => bad_date(e),
    };
    let save_follow_up = move |v: String| match date_patch(&v) {
        Ok(p) => save(UpdateWaitingOn {
            follow_up_on: p,
            ..Default::default()
        }),
        Err(e) => bad_date(e),
    };
    let save_resolved = move |v: String| match date_patch(&v) {
        Ok(p) => save(UpdateWaitingOn {
            resolved_on: p,
            ..Default::default()
        }),
        Err(e) => bad_date(e),
    };
    let save_asked = move |v: String| match parse_date(v.trim()) {
        Ok(d) => save(UpdateWaitingOn {
            asked_on: Some(d),
            ..Default::default()
        }),
        Err(_) => bad_date(AppError {
            code: "invalid".into(),
            message: "Asked on must be a date like 2027-03-31".into(),
        }),
    };
    let save_person = move |v: String| {
        if let Ok(p) = Uuid::parse_str(&v) {
            save(UpdateWaitingOn {
                person_id: Some(p),
                ..Default::default()
            });
        }
    };
    let people_options: Vec<(String, String)> = people
        .iter()
        .map(|p| (p.person.id.to_string(), p.person.name.clone()))
        .collect();

    view! {
        <TextField label="What are you waiting for?" multiline=true value=w.description.clone()
            on_commit=move |v: String| save(UpdateWaitingOn { description: Some(v), ..Default::default() }) />
        <SelectField label="Waiting on" options=people_options current=w.person_id.to_string() on_change=save_person />
        <div class="mt-2 grid grid-cols-2 gap-3">
            <TextField label="Asked on" kind="date" value=w.asked_on.to_string() on_commit=save_asked />
            <TextField label="Expected by" kind="date"
                value=w.expected_by.map(|d| d.to_string()).unwrap_or_default() on_commit=save_expected />
            <TextField label="Snoozed until" kind="date"
                value=w.follow_up_on.map(|d| d.to_string()).unwrap_or_default() on_commit=save_follow_up />
            <TextField label="Resolved on" kind="date"
                value=w.resolved_on.map(|d| d.to_string()).unwrap_or_default() on_commit=save_resolved />
        </div>
        <p class="mt-1 text-[11px] text-muted">"Link it to a task or project under Links (About)."</p>
    }
}

#[component]
fn ArchiveWaiting(id: Uuid) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let confirming = RwSignal::new(false);
    let confirm = move |_| {
        spawn_local(async move {
            if finish(api::archive_waiting_on(id).await, toasts, version).is_some() {
                confirming.set(false);
                selection.close();
            }
        });
    };
    view! {
        <Section title="Archive">
            {move || if confirming.get() {
                view! {
                    <div class="space-y-2">
                        <p>"Archive this waiting-on? Its links are archived with it."</p>
                        <div class="flex gap-2">
                            <button class=BUTTON_DANGER on:click=confirm>"Archive"</button>
                            <button class=BUTTON on:click=move |_| confirming.set(false)>"Cancel"</button>
                        </div>
                    </div>
                }.into_any()
            } else {
                view! { <button class=BUTTON on:click=move |_| confirming.set(true)>"Archive…"</button> }.into_any()
            }}
        </Section>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages_read_naturally() {
        assert_eq!(age_text(0), "today");
        assert_eq!(age_text(1), "1 day");
        assert_eq!(age_text(12), "12 days");
    }

    #[test]
    fn snooze_choices_are_valid_day_counts() {
        for (v, _) in SNOOZE_CHOICES.iter().skip(1) {
            assert!(v.parse::<u32>().is_ok_and(|d| d > 0), "{v}");
        }
        assert_eq!(snooze_options().len(), SNOOZE_CHOICES.len());
    }
}
