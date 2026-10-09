//! Meetings (spec 38) in the screens: the clock that moves them on, the day / time / length
//! fields, the dialog that makes a meeting or a follow-up, and the words for "in 35 min".
//!
//! The backend has no reliable way to know the local time zone, so this file is where the clock
//! on the user's wall is read; everything else about meetings is decided in core.

use std::time::Duration;

use leptos::{ev, html, prelude::*, task::spawn_local};
use minimap_types::{
    fmt_clock, fmt_length, parse_clock, parse_length, timefmt::parse_date, AppError,
    AssigneeChoice, Clock, CreateMeeting, Date, MeetingTime, NodeRef, NodeType, Task, Uuid,
    DEFAULT_MEETING_MINUTES,
};

use crate::{
    api,
    components::{
        date_field::{today_ymd, DateField},
        form::{BUTTON, BUTTON_PRIMARY, BUTTON_SOFT, INPUT},
    },
    pages::this_week::shift_days,
    state::{finish, DataVersion, MeetingDialog, NowClock, Selection, Toasts},
};

// ------------------------------------------------------------------ the clock

/// The clock on the user's wall: today's date there, the minute of the day, and how far ahead of
/// UTC it is (read for the date in question, so summer time is right).
pub fn local_clock() -> Option<Clock> {
    let (y, m, d) = today_ymd();
    let date = parse_date(&format!("{y:04}-{m:02}-{d:02}")).ok()?;
    let now = js_sys::Date::new_0();
    let minute = u16::try_from(now.get_hours() * 60 + now.get_minutes()).ok()?;
    Some(Clock {
        date,
        minute,
        // `getTimezoneOffset` is minutes *behind* UTC.
        utc_offset_minutes: -(now.get_timezone_offset() as i32),
    })
}

/// Moves meetings on with the clock: when the screen opens (a meeting that ended while the app
/// was closed is closed now) and every half minute. Also keeps [`NowClock`] fresh. Mount once.
#[component]
pub fn MeetingClock() -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let now = expect_context::<NowClock>();
    let tick = move || {
        let Some(clock) = local_clock() else { return };
        now.0.set(Some(clock));
        spawn_local(async move {
            // A locked database or an odd clock answers with an error; the next tick tries again.
            if let Ok(done) = api::advance_meetings(clock).await {
                if done.changed > 0 {
                    version.bump();
                }
            }
        });
    };
    tick();
    let handle = set_interval_with_handle(tick, Duration::from_secs(30)).ok();
    on_cleanup(move || {
        if let Some(h) = handle {
            h.clear();
        }
    });
}

// ------------------------------------------------------------------ pure text

/// The next half hour after `now`, for a new meeting's start (09:00 when that would be tomorrow).
pub fn default_start(now: &Clock) -> u16 {
    let next = (now.minute / 30 + 1) * 30;
    if next >= 1440 {
        9 * 60
    } else {
        next
    }
}

/// What the three boxes say, as a meeting time: the date as `YYYY-MM-DD`, the start as `HH:MM`
/// and the length as `45m`, `1h`, `1h30m` (empty is the default hour). The reason when one is
/// wrong, in words for the user.
pub fn when_from_texts(date: &str, time: &str, length: &str) -> Result<MeetingTime, String> {
    let date = parse_date(date.trim())
        .map_err(|_| "A meeting needs a date, like 2027-03-31.".to_owned())?;
    let minute = parse_clock(time)?;
    let length_minutes = match length.trim() {
        "" => None,
        text => Some(parse_length(text)?),
    };
    Ok(MeetingTime {
        date,
        time: fmt_clock(minute),
        length_minutes,
    })
}

/// How long a meeting lasts as the box shows it ("1h").
pub fn length_text(task: &Task) -> String {
    fmt_length(task.meeting_minutes())
}

/// When a meeting that has not ended is, close to now: "now", "in 25 min", "in 2h 10m". Nothing
/// when it is more than six hours away or has ended (the row shows the day and the time).
pub fn starts_in_text(task: &Task, now: &Clock) -> Option<String> {
    let (to_start, to_end) = (task.minutes_to_start(now)?, task.minutes_to_end(now)?);
    if to_end <= 0 {
        return None;
    }
    if to_start <= 0 {
        return Some("now".to_owned());
    }
    match to_start {
        1..=59 => Some(format!("in {to_start} min")),
        60..=360 => Some(match to_start % 60 {
            0 => format!("in {}h", to_start / 60),
            m => format!("in {}h {m}m", to_start / 60),
        }),
        _ => None,
    }
}

/// An error in the shape the toasts show.
fn invalid(message: String) -> AppError {
    AppError {
        code: "invalid".into(),
        message,
    }
}

// ------------------------------------------------------------------ the fields

/// Date, start and length of a meeting as three boxes that write into the signals; `on_change`
/// runs after each box is committed (a meeting already made saves then).
#[component]
pub fn WhenFields(
    date: RwSignal<String>,
    time: RwSignal<String>,
    length: RwSignal<String>,
    #[prop(optional, into)] on_change: Option<Callback<()>>,
) -> impl IntoView {
    let changed = move || {
        if let Some(f) = on_change {
            f.run(());
        }
    };
    view! {
        <div class="grid grid-cols-3 gap-3">
            <DateField label="Date" current=date.get_untracked() placeholder="YYYY-MM-DD"
                on_commit=move |v: String| { date.set(v); changed(); } />
            <label class="mb-2 block">
                <span class="mb-0.5 block text-[11px] text-muted">"Starts (24-hour)"</span>
                <input class=INPUT type="text" placeholder="10:30" autocomplete="off"
                    prop:value=move || time.get()
                    on:input=move |ev| time.set(event_target_value(&ev))
                    on:change=move |_| changed() />
            </label>
            <label class="mb-2 block">
                <span class="mb-0.5 block text-[11px] text-muted">"Length (30m, 1h)"</span>
                <input class=INPUT type="text" placeholder="1h" autocomplete="off"
                    prop:value=move || length.get()
                    on:input=move |ev| length.set(event_target_value(&ev))
                    on:change=move |_| changed() />
            </label>
        </div>
    }
}

/// The time of a meeting in its pane: change a box and it saves. A meeting moved to a time that
/// has not come is open again.
#[component]
pub fn MeetingFields(task: Task) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let id = task.id;
    let date = RwSignal::new(task.due_date.map(|d| d.to_string()).unwrap_or_default());
    let time = RwSignal::new(task.start_minute.map(fmt_clock).unwrap_or_default());
    let length = RwSignal::new(length_text(&task));
    let save = Callback::new(move |()| {
        let result = when_from_texts(
            &date.get_untracked(),
            &time.get_untracked(),
            &length.get_untracked(),
        );
        match (result, local_clock()) {
            (Ok(when), Some(clock)) => spawn_local(async move {
                finish(api::set_meeting(id, when, clock).await, toasts, version);
            }),
            // Say what is wrong and put the boxes back to what is saved.
            (Err(message), _) => {
                toasts.error(&invalid(message));
                version.bump();
            }
            (Ok(_), None) => version.bump(),
        }
    });
    view! {
        <WhenFields date=date time=time length=length on_change=save />
        <p class="mb-2 text-[11px] text-muted">
            "A meeting moves to in progress when it starts and to done when it ends. To postpone it, change its time."
        </p>
    }
}

/// Turns a task into a meeting: asks for the day and time first, since a meeting can't be
/// without them. Shown in the pane when "Meeting" is picked as the type.
#[component]
pub fn MakeMeeting(
    task: Uuid,
    due: Option<Date>,
    #[prop(into)] on_cancel: Callback<()>,
) -> impl IntoView {
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let now = local_clock();
    let date = RwSignal::new(
        due.or(now.map(|c| c.date))
            .map(|d| d.to_string())
            .unwrap_or_default(),
    );
    let time = RwSignal::new(fmt_clock(now.as_ref().map_or(9 * 60, default_start)));
    let length = RwSignal::new(fmt_length(DEFAULT_MEETING_MINUTES));
    let make = move |_| {
        let result = when_from_texts(
            &date.get_untracked(),
            &time.get_untracked(),
            &length.get_untracked(),
        );
        match (result, local_clock()) {
            (Ok(when), Some(clock)) => spawn_local(async move {
                if finish(api::set_meeting(task, when, clock).await, toasts, version).is_some() {
                    on_cancel.run(());
                }
            }),
            (Err(message), _) => toasts.error(&invalid(message)),
            (Ok(_), None) => {}
        }
    };
    view! {
        <div class="mt-1 rounded-sm border border-accent/40 bg-accent/5 p-2.5">
            <p class="mb-2 text-[12px]">"A meeting happens at a time. When is it?"</p>
            <WhenFields date=date time=time length=length />
            <div class="flex gap-2">
                <button class=BUTTON_PRIMARY on:click=make>"Make it a meeting"</button>
                <button class=BUTTON on:click=move |_| on_cancel.run(())>"Cancel"</button>
            </div>
        </div>
    }
}

// ------------------------------------------------------------------ the dialog

/// The "New meeting" button in the header of the Tasks screen.
#[component]
pub fn NewMeetingButton() -> impl IntoView {
    let dialog = expect_context::<MeetingDialog>();
    view! {
        <button class=BUTTON_SOFT title="A meeting at a day and time. It starts and ends on its own"
                on:click=move |_| dialog.new_meeting()>"New meeting"</button>
    }
}

/// Mount once, inside the router.
#[component]
pub fn MeetingDialogHost() -> impl IntoView {
    let dialog = expect_context::<MeetingDialog>();
    view! {
        {move || dialog.0.get().map(|request| view! { <Dialog follow_up_of=request.follow_up_of /> })}
    }
}

/// The dialog: a title, a day, a time and a length for a new meeting; for a follow-up the title
/// is the original's with "Follow-up:" and the time starts a week on from the original's.
#[component]
fn Dialog(follow_up_of: Option<(Uuid, String)>) -> impl IntoView {
    let dialog = expect_context::<MeetingDialog>();
    let version = expect_context::<DataVersion>();
    let toasts = expect_context::<Toasts>();
    let selection = expect_context::<Selection>();
    let busy = RwSignal::new(false);
    let now = local_clock();
    let title = RwSignal::new(String::new());
    let date = RwSignal::new(now.map(|c| c.date.to_string()).unwrap_or_default());
    let time = RwSignal::new(fmt_clock(now.as_ref().map_or(9 * 60, default_start)));
    let length = RwSignal::new(fmt_length(DEFAULT_MEETING_MINUTES));
    let input = leptos::prelude::NodeRef::<html::Input>::new();
    let source = follow_up_of.as_ref().map(|(id, _)| *id);

    // A follow-up starts from its original: a week later at the same time, the same length.
    if let Some(source) = source {
        let original = LocalResource::new(move || api::get_task(source));
        Effect::new(move |_| {
            if let Some(Ok(t)) = original.get() {
                if let (Some(due), Some(start)) = (t.due_date, t.start_minute) {
                    date.set(shift_days(due, 7).to_string());
                    time.set(fmt_clock(start));
                    length.set(length_text(&t));
                }
            }
        });
    }
    Effect::new(move |_| {
        if let Some(el) = input.get() {
            let _ = el.focus();
        }
    });

    let submit = move || {
        if busy.get_untracked() {
            return;
        }
        let text = title.get_untracked();
        if source.is_none() && text.trim().is_empty() {
            toasts.error(&invalid("A meeting needs a title.".into()));
            return;
        }
        let when = match when_from_texts(
            &date.get_untracked(),
            &time.get_untracked(),
            &length.get_untracked(),
        ) {
            Ok(w) => w,
            Err(message) => {
                toasts.error(&invalid(message));
                return;
            }
        };
        busy.set(true);
        spawn_local(async move {
            let result = match source {
                Some(source) => api::schedule_follow_up(source, Some(when)).await,
                None => {
                    api::create_meeting(CreateMeeting {
                        title: text,
                        when,
                        project_id: None,
                        assignee: AssigneeChoice::Me,
                    })
                    .await
                }
            };
            busy.set(false);
            if let Some(task) = finish(result, toasts, version) {
                dialog.close();
                selection.open(NodeRef::new(NodeType::Task, task.id));
            }
        });
    };
    let on_keydown = move |e: ev::KeyboardEvent| match e.key().as_str() {
        "Escape" => {
            e.prevent_default();
            e.stop_propagation();
            dialog.close();
        }
        "Enter"
            if e.target().is_some_and(|t| {
                // Enter in the title or the time boxes makes the meeting; the date box has its own.
                wasm_bindgen::JsCast::dyn_ref::<web_sys::HtmlInputElement>(&t)
                    .is_some_and(|i| i.type_() == "text" && i.placeholder() != "YYYY-MM-DD")
            }) =>
        {
            e.prevent_default();
            submit();
        }
        _ => {}
    };

    let heading = match &follow_up_of {
        Some((_, name)) => view! {
            "Follow up on " <span class="text-accent">{name.clone()}</span>
        }
        .into_any(),
        None => view! { "New meeting" }.into_any(),
    };
    let is_follow_up = source.is_some();
    view! {
        <div class="fixed inset-0 z-[80] bg-scrim" on:mousedown=move |_| dialog.close()></div>
        <div role="dialog" aria-label="Meeting"
             class="fixed inset-x-0 mx-auto top-[14vh] z-[81] w-[34rem] max-w-[94vw] \
                    rounded-sm border border-line bg-panel p-4 text-[13px]"
             on:keydown=on_keydown>
            <h2 class="mb-3 text-[14px] font-semibold">{heading}</h2>
            {(!is_follow_up).then(|| view! {
                <label class="mb-2 block">
                    <span class="mb-0.5 block text-[11px] text-muted">"Title"</span>
                    <input node_ref=input class=INPUT type="text" autocomplete="off"
                        placeholder="What is the meeting?"
                        prop:value=move || title.get()
                        on:input=move |ev| title.set(event_target_value(&ev)) />
                </label>
            })}
            <WhenFields date=date time=time length=length />
            <p class="mb-3 text-[11px] text-muted">
                {if is_follow_up {
                    "The follow-up keeps the project, priority, assignee and links of the meeting, and is linked to it. Change the time here if next week at the same time doesn't suit."
                } else {
                    "It moves to in progress when it starts and to done when it ends."
                }}
            </p>
            <div class="flex items-center justify-end gap-2">
                <button type="button" class=BUTTON on:click=move |_| dialog.close()>"Cancel"</button>
                <button type="button" class=BUTTON_PRIMARY disabled=move || busy.get()
                    on:click=move |_| submit()>
                    {if is_follow_up { "Schedule follow-up" } else { "Create and open" }}
                </button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use minimap_types::MEETING_TYPE;
    use time::{macros::date, OffsetDateTime};

    fn clock(minute: u16) -> Clock {
        Clock {
            date: date!(2027 - 03 - 03),
            minute,
            utc_offset_minutes: 0,
        }
    }

    fn meeting(start: u16, length: Option<u32>) -> Task {
        Task {
            links: Vec::new(),
            task_type: Some(MEETING_TYPE.into()),
            focus: None,
            start_minute: Some(start),
            length_minutes: length,
            id: Uuid::nil(),
            title: "Sync".into(),
            description: String::new(),
            project_id: None,
            status: minimap_types::TaskStatus::Todo,
            estimate_days: None,
            start_date: None,
            due_date: Some(date!(2027 - 03 - 03)),
            completed_at: None,
            priority: 3,
            recurrence: None,
            created_at: OffsetDateTime::UNIX_EPOCH,
            updated_at: OffsetDateTime::UNIX_EPOCH,
            archived_at: None,
        }
    }

    #[test]
    fn a_new_meeting_starts_at_the_next_half_hour() {
        assert_eq!(default_start(&clock(9 * 60)), 9 * 60 + 30);
        assert_eq!(default_start(&clock(9 * 60 + 29)), 9 * 60 + 30);
        assert_eq!(default_start(&clock(9 * 60 + 30)), 10 * 60);
        // Late at night: tomorrow morning, not a time past midnight.
        assert_eq!(default_start(&clock(23 * 60 + 45)), 9 * 60);
    }

    #[test]
    fn the_three_boxes_make_a_meeting_time_or_say_what_is_wrong() {
        let ok = when_from_texts("2027-03-31", "9:30", "1h30m").unwrap();
        assert_eq!(ok.date, date!(2027 - 03 - 31));
        assert_eq!((ok.time.as_str(), ok.length_minutes), ("09:30", Some(90)));
        // An empty length is the default hour, decided by the backend.
        assert_eq!(
            when_from_texts("2027-03-31", "10:00", "  ")
                .unwrap()
                .length_minutes,
            None
        );
        assert!(when_from_texts("soon", "10:00", "")
            .unwrap_err()
            .contains("date"));
        assert!(when_from_texts("2027-03-31", "25:00", "")
            .unwrap_err()
            .contains("24-hour"));
        assert!(when_from_texts("2027-03-31", "10:00", "lots")
            .unwrap_err()
            .contains("length"));
    }

    #[test]
    fn a_meeting_says_how_soon_it_is_until_it_is_far_off_or_over() {
        let m = meeting(10 * 60 + 30, None);
        assert_eq!(
            starts_in_text(&m, &clock(10 * 60)).as_deref(),
            Some("in 30 min")
        );
        assert_eq!(
            starts_in_text(&m, &clock(10 * 60 + 29)).as_deref(),
            Some("in 1 min")
        );
        assert_eq!(
            starts_in_text(&m, &clock(9 * 60)).as_deref(),
            Some("in 1h 30m")
        );
        assert_eq!(
            starts_in_text(&m, &clock(8 * 60 + 30)).as_deref(),
            Some("in 2h")
        );
        assert_eq!(starts_in_text(&m, &clock(0)), None);
        // Under way, then over.
        assert_eq!(
            starts_in_text(&m, &clock(10 * 60 + 30)).as_deref(),
            Some("now")
        );
        assert_eq!(
            starts_in_text(&m, &clock(11 * 60 + 29)).as_deref(),
            Some("now")
        );
        assert_eq!(starts_in_text(&m, &clock(11 * 60 + 30)), None);
        // Its own length.
        let short = meeting(10 * 60 + 30, Some(15));
        assert_eq!(starts_in_text(&short, &clock(10 * 60 + 45)), None);
        assert_eq!(length_text(&short), "15m");
        assert_eq!(length_text(&m), "1h");
    }
}
